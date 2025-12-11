mod engine;
mod game;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{WebGlRenderingContext, HtmlCanvasElement, KeyboardEvent, MouseEvent, WheelEvent, Request, RequestInit, RequestMode, Response};
use std::cell::RefCell;
use std::rc::Rc;
use crate::engine::renderer::Renderer;
use crate::engine::mesh::Mesh;
use crate::game::{Game, AppConfig};
use crate::game::solar_system::{SolarSystem, SystemType};
use crate::game::minecraft::Minecraft;

enum ActiveGame {
    Crossy(Game),
    Solar(SolarSystem),
    Minecraft(Minecraft),
}

thread_local! {
    static CURRENT_GAME: RefCell<Option<ActiveGame>> = RefCell::new(None);
}

fn get_gl() -> Result<WebGlRenderingContext, JsValue> {
    let window = web_sys::window().ok_or("No window")?;
    let document = window.document().ok_or("No document")?;
    let canvas = document.get_element_by_id("canvas")
        .ok_or("No canvas")?
        .dyn_into::<HtmlCanvasElement>()?;

    let gl = canvas
        .get_context("webgl")?
        .ok_or("No WebGL")?
        .dyn_into::<WebGlRenderingContext>()?;
    Ok(gl)
}

fn start_game_loop() -> Result<(), JsValue> {
    let closure = Closure::wrap(Box::new(move |event: KeyboardEvent| {
        CURRENT_GAME.with(|g| {
            if let Some(active_game) = g.borrow_mut().as_mut() {
                match active_game {
                    ActiveGame::Crossy(game) => {
                        let handled = match event.key().as_str() {
                            " " => { game.move_forward(); true },
                            "ArrowLeft" | "d" | "D" => { game.move_left(); true },
                            "ArrowRight" | "a" | "A" => { game.move_right(); true },
                            "r" | "R" => { game.restart(); true },
                            _ => false,
                        };
                        if handled {
                            event.prevent_default();
                        }
                    },
                    ActiveGame::Solar(game) => {
                        game.handle_input(&event.key());
                    },
                    ActiveGame::Minecraft(game) => {
                        game.handle_input(&event.key());
                    }
                }
            }
        });
    }) as Box<dyn FnMut(_)>);

    web_sys::window().unwrap().add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref())?;
    closure.forget();

    let closure_keyup = Closure::wrap(Box::new(move |event: KeyboardEvent| {
        CURRENT_GAME.with(|g| {
            if let Some(ActiveGame::Minecraft(game)) = g.borrow_mut().as_mut() {
                game.handle_keyup(&event.key());
            }
        });
    }) as Box<dyn FnMut(_)>);
    web_sys::window().unwrap().add_event_listener_with_callback("keyup", closure_keyup.as_ref().unchecked_ref())?;
    closure_keyup.forget();

    let closure_down = Closure::wrap(Box::new(move |event: MouseEvent| {
        CURRENT_GAME.with(|g| {
            if let Some(active_game) = g.borrow_mut().as_mut() {
                match active_game {
                    ActiveGame::Solar(game) => game.handle_mouse_down(event.client_x(), event.client_y()),
                    ActiveGame::Minecraft(game) => game.handle_mouse_down(event.client_x(), event.client_y(), event.button() as i32),
                    _ => {}
                }
            }
        });
    }) as Box<dyn FnMut(_)>);
    web_sys::window().unwrap().document().unwrap().get_element_by_id("canvas").unwrap()
        .add_event_listener_with_callback("mousedown", closure_down.as_ref().unchecked_ref())?;
    closure_down.forget();

    let closure_up = Closure::wrap(Box::new(move |_event: MouseEvent| {
        CURRENT_GAME.with(|g| {
            if let Some(ActiveGame::Solar(game)) = g.borrow_mut().as_mut() {
                game.handle_mouse_up();
            }
        });
    }) as Box<dyn FnMut(_)>);
    web_sys::window().unwrap().add_event_listener_with_callback("mouseup", closure_up.as_ref().unchecked_ref())?;
    closure_up.forget();

    let closure_move = Closure::wrap(Box::new(move |event: MouseEvent| {
        CURRENT_GAME.with(|g| {
            if let Some(active_game) = g.borrow_mut().as_mut() {
                match active_game {
                    ActiveGame::Solar(game) => game.handle_mouse_move(event.client_x(), event.client_y()),
                    ActiveGame::Minecraft(game) => game.handle_mouse_move(event.movement_x(), event.movement_y()),
                    _ => {}
                }
            }
        });
    }) as Box<dyn FnMut(_)>);
    web_sys::window().unwrap().add_event_listener_with_callback("mousemove", closure_move.as_ref().unchecked_ref())?;
    closure_move.forget();

    let closure_wheel = Closure::wrap(Box::new(move |event: WheelEvent| {
        CURRENT_GAME.with(|g| {
            if let Some(ActiveGame::Solar(game)) = g.borrow_mut().as_mut() {
                game.handle_wheel(event.delta_y() as f32);
                event.prevent_default();
            }
        });
    }) as Box<dyn FnMut(_)>);
    web_sys::window().unwrap().document().unwrap().get_element_by_id("canvas").unwrap()
        .add_event_listener_with_callback("wheel", closure_wheel.as_ref().unchecked_ref())?;
    closure_wheel.forget();

    let closure_lock = Closure::wrap(Box::new(move || {
        let document = web_sys::window().unwrap().document().unwrap();
        let is_locked = document.pointer_lock_element().is_some();
        
        CURRENT_GAME.with(|g| {
            if let Some(ActiveGame::Minecraft(game)) = g.borrow_mut().as_mut() {
                game.set_locked(is_locked);
            }
        });
    }) as Box<dyn FnMut()>);
    web_sys::window().unwrap().document().unwrap()
        .add_event_listener_with_callback("pointerlockchange", closure_lock.as_ref().unchecked_ref())?;
    closure_lock.forget();

    let f = Rc::new(RefCell::new(None));
    let g = f.clone();

    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        CURRENT_GAME.with(|game| {
            if let Some(active_game) = game.borrow_mut().as_mut() {
                match active_game {
                    ActiveGame::Crossy(game) => {
                        game.update();
                        game.render();
                        update_ui(game.score, game.coins, game.game_over);
                    },
                    ActiveGame::Solar(game) => {
                        game.update();
                        let window = web_sys::window().unwrap();
                        let width = window.inner_width().unwrap().as_f64().unwrap() as i32;
                        let height = window.inner_height().unwrap().as_f64().unwrap() as i32;
                        game.render(width, height);
                    },
                    ActiveGame::Minecraft(game) => {
                        game.update();
                        let window = web_sys::window().unwrap();
                        let width = window.inner_width().unwrap().as_f64().unwrap() as i32;
                        let height = window.inner_height().unwrap().as_f64().unwrap() as i32;
                        game.render(width, height);
                    }
                }
            }
        });
        request_animation_frame(f.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut()>));

    request_animation_frame(g.borrow().as_ref().unwrap());
    Ok(())
}

#[wasm_bindgen]
pub async fn start_crossy_road() -> Result<(), JsValue> {
    let gl = get_gl()?;
    let renderer = Renderer::new(gl)?;

    let window = web_sys::window().unwrap();
    let mut config: Option<AppConfig> = None;
    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);

    let config_request = Request::new_with_str_and_init("/assets/config.json", &opts)?;
    let config_resp_value = JsFuture::from(window.fetch_with_request(&config_request)).await;

    if let Ok(resp_value) = config_resp_value {
        let resp: Response = resp_value.dyn_into().unwrap();
        if resp.ok() {
            let json_promise = resp.json()?;
            let json = JsFuture::from(json_promise).await?;
            if let Ok(c) = serde_wasm_bindgen::from_value(json) {
                config = Some(c);
            }
        }
    }

    let mut car_mesh = None;
    let model_path = if let Some(ref c) = config {
        c.car_model.path.clone()
    } else {
        "/assets/models/grey_voxel_car.glb".to_string()
    };

    let request = Request::new_with_str_and_init(&model_path, &opts)?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await;
    
    if let Ok(resp_value) = resp_value {
        let resp: Response = resp_value.dyn_into().unwrap();
        if resp.ok() {
            let buffer_promise = resp.array_buffer()?;
            let buffer = JsFuture::from(buffer_promise).await?;
            let array = js_sys::Uint8Array::new(&buffer);
            let bytes = array.to_vec();
            
            if let Ok(mesh) = Mesh::from_gltf(&bytes) {
                car_mesh = Some(mesh);
            }
        }
    }

    let game = Game::new(renderer, car_mesh, config);
    CURRENT_GAME.with(|g| *g.borrow_mut() = Some(ActiveGame::Crossy(game)));
    
    start_game_loop()?;
    Ok(())
}

#[wasm_bindgen]
pub fn load_solar_system(sim_type: &str) -> Result<(), JsValue> {
    let gl = get_gl()?;
    let renderer = Renderer::new(gl)?;
    
    let system_type = match sim_type {
        "black_hole" => SystemType::BlackHole,
        "sirius" => SystemType::Sirius,
        _ => SystemType::Solar,
    };
    let game = SolarSystem::new(renderer, system_type);
    
    CURRENT_GAME.with(|g| {
        *g.borrow_mut() = Some(ActiveGame::Solar(game));
    });
    
    // Ensure loop is running (idempotent)
    start_game_loop()?;
    
    Ok(())
}

#[wasm_bindgen]
pub fn start_solar_system() -> Result<(), JsValue> {
    load_solar_system("sun")
}

#[wasm_bindgen]
pub fn start_minecraft() -> Result<(), JsValue> {
    let gl = get_gl()?;
    let renderer = Renderer::new(gl)?;
    let game = Minecraft::new(renderer);
    
    CURRENT_GAME.with(|g| {
        *g.borrow_mut() = Some(ActiveGame::Minecraft(game));
    });
    
    start_game_loop()?;
    Ok(())
}

fn request_animation_frame(f: &Closure<dyn FnMut()>) {
    web_sys::window()
        .unwrap()
        .request_animation_frame(f.as_ref().unchecked_ref())
        .unwrap();
}

fn update_ui(score: i32, coins: i32, game_over: bool) {
    if let Some(window) = web_sys::window() {
        if let Some(document) = window.document() {
            if let Some(score_el) = document.get_element_by_id("score") {
                score_el.set_inner_html(&format!("Score: {} | Coins: {}", score, coins));
            }
            if let Some(gameover_el) = document.get_element_by_id("gameover") {
                if game_over {
                    gameover_el.set_attribute("style", "display: block;").ok();
                } else {
                    gameover_el.set_attribute("style", "display: none;").ok();
                }
            }
        }
    }
}

#[wasm_bindgen]
pub fn touch_left() {
    CURRENT_GAME.with(|g| {
        if let Some(active_game) = g.borrow_mut().as_mut() {
            match active_game {
                ActiveGame::Crossy(game) => game.move_left(),
                ActiveGame::Solar(game) => game.handle_input("ArrowLeft"),
                ActiveGame::Minecraft(game) => game.handle_input("a"),
            }
        }
    });
}

#[wasm_bindgen]
pub fn touch_right() {
    CURRENT_GAME.with(|g| {
        if let Some(active_game) = g.borrow_mut().as_mut() {
            match active_game {
                ActiveGame::Crossy(game) => game.move_right(),
                ActiveGame::Solar(game) => game.handle_input("ArrowRight"),
                ActiveGame::Minecraft(game) => game.handle_input("d"),
            }
        }
    });
}

#[wasm_bindgen]
pub fn touch_forward() {
    CURRENT_GAME.with(|g| {
        if let Some(active_game) = g.borrow_mut().as_mut() {
            match active_game {
                ActiveGame::Crossy(game) => game.move_forward(),
                ActiveGame::Solar(game) => game.handle_input("ArrowUp"),
                ActiveGame::Minecraft(game) => game.handle_input("w"),
            }
        }
    });
}

#[wasm_bindgen]
pub fn touch_restart() {
    CURRENT_GAME.with(|g| {
        if let Some(active_game) = g.borrow_mut().as_mut() {
            match active_game {
                ActiveGame::Crossy(game) => game.restart(),
                ActiveGame::Solar(game) => game.handle_input("ArrowDown"),
                ActiveGame::Minecraft(game) => game.handle_input("s"),
            }
        }
    });
}

#[wasm_bindgen]
pub fn activate_god_mode() {
    CURRENT_GAME.with(|g| {
        if let Some(active_game) = g.borrow_mut().as_mut() {
            if let ActiveGame::Crossy(game) = active_game {
                game.debug_advance();
            }
        }
    });
}

#[wasm_bindgen]
pub fn set_solar_date(timestamp: f64) {
    CURRENT_GAME.with(|g| {
        if let Some(active_game) = g.borrow_mut().as_mut() {
            if let ActiveGame::Solar(game) = active_game {
                game.set_date_from_timestamp(timestamp);
            }
        }
    });
}

#[wasm_bindgen]
pub fn set_solar_time_scale(scale: f32) {
    CURRENT_GAME.with(|g| {
        if let Some(active_game) = g.borrow_mut().as_mut() {
            if let ActiveGame::Solar(game) = active_game {
                game.set_time_scale(scale);
            }
        }
    });
}

#[wasm_bindgen]
pub fn select_solar_body(index: usize) {
    CURRENT_GAME.with(|g| {
        if let Some(active_game) = g.borrow_mut().as_mut() {
            if let ActiveGame::Solar(game) = active_game {
                game.select_body(index);
            }
        }
    });
}

#[wasm_bindgen]
pub fn toggle_solar_temperature_unit() {
    CURRENT_GAME.with(|g| {
        if let Some(active_game) = g.borrow_mut().as_mut() {
            if let ActiveGame::Solar(game) = active_game {
                game.toggle_temperature_unit();
            }
        }
    });
}
