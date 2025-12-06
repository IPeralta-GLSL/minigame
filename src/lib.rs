mod engine;
mod game;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{WebGlRenderingContext, HtmlCanvasElement, KeyboardEvent, Request, RequestInit, RequestMode, Response};
use std::cell::RefCell;
use std::rc::Rc;
use crate::engine::renderer::Renderer;
use crate::engine::mesh::Mesh;
use crate::game::{Game, AppConfig};

thread_local! {
    static GAME: RefCell<Option<Game>> = RefCell::new(None);
}

#[wasm_bindgen]
pub async fn init_game() -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("No window")?;
    let document = window.document().ok_or("No document")?;
    let canvas = document.get_element_by_id("canvas")
        .ok_or("No canvas")?
        .dyn_into::<HtmlCanvasElement>()?;

    let gl = canvas
        .get_context("webgl")?
        .ok_or("No WebGL")?
        .dyn_into::<WebGlRenderingContext>()?;

    let renderer = Renderer::new(gl)?;

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

    // Load assets
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
    GAME.with(|g| *g.borrow_mut() = Some(game));

    // Input handling
    let closure = Closure::wrap(Box::new(move |event: KeyboardEvent| {
        GAME.with(|g| {
            if let Some(game) = g.borrow_mut().as_mut() {
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
            }
        });
    }) as Box<dyn FnMut(_)>);

    window.add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref())?;
    closure.forget();

    // Game loop
    let f = Rc::new(RefCell::new(None));
    let g = f.clone();

    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        GAME.with(|game| {
            if let Some(game) = game.borrow_mut().as_mut() {
                game.update();
                game.render();
                
                // Update UI
                update_ui(game.score, game.coins, game.game_over);
            }
        });
        request_animation_frame(f.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut()>));

    request_animation_frame(g.borrow().as_ref().unwrap());

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
    GAME.with(|g| {
        if let Some(game) = g.borrow_mut().as_mut() {
            game.move_left();
        }
    });
}

#[wasm_bindgen]
pub fn touch_right() {
    GAME.with(|g| {
        if let Some(game) = g.borrow_mut().as_mut() {
            game.move_right();
        }
    });
}

#[wasm_bindgen]
pub fn touch_forward() {
    GAME.with(|g| {
        if let Some(game) = g.borrow_mut().as_mut() {
            game.move_forward();
        }
    });
}

#[wasm_bindgen]
pub fn touch_restart() {
    GAME.with(|g| {
        if let Some(game) = g.borrow_mut().as_mut() {
            game.restart();
        }
    });
}

#[wasm_bindgen]
pub fn activate_god_mode() {
    GAME.with(|g| {
        if let Some(game) = g.borrow_mut().as_mut() {
            game.debug_advance();
        }
    });
}
