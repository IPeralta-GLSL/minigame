use crate::engine::renderer::Renderer;
use crate::engine::mesh::Mesh;
use nalgebra::{Matrix4, Point3, Vector3, Vector4};
use js_sys::Date;
use web_sys::{HtmlElement, WebGlTexture};
use wasm_bindgen::JsCast;

pub struct Body {
    pub mesh: Mesh,
    pub radius: f32,
    pub orbit_radius: f32,
    pub orbit_speed: f32,
    pub orbit_angle: f32,
    pub color: (f32, f32, f32),
    pub parent: Option<usize>,
    pub name: String,
    pub trail: Vec<f32>,
    pub label_element: Option<HtmlElement>,
    pub texture: Option<WebGlTexture>,
}

pub struct SolarSystem {
    renderer: Renderer,
    bodies: Vec<Body>,
    camera_distance: f32,
    camera_rotation: (f32, f32),
    last_time: f64,
    is_dragging: bool,
    last_mouse_pos: (i32, i32),
    time_scale: f32,
    current_time: f64,
}

impl SolarSystem {
    pub fn new(renderer: Renderer) -> Self {
        let mut bodies = Vec::new();
        
        let now_ms = Date::now();
        let j2000_ms = 946728000000.0;
        let days_since_j2000 = (now_ms - j2000_ms) / (1000.0 * 60.0 * 60.0 * 24.0);
        
        let get_initial_angle = |l0: f32, p: f32| -> f32 {
            let n = 360.0 / p;
            let angle_deg = (l0 + n * days_since_j2000 as f32) % 360.0;
            angle_deg.to_radians()
        };
        
        let get_orbit_speed = |p: f32| -> f32 {
            let p_seconds = p * 24.0 * 3600.0;
            (2.0 * std::f32::consts::PI) / p_seconds
        };

        let window = web_sys::window().unwrap();
        let document = window.document().unwrap();
        let labels_container = document.get_element_by_id("solar-labels");

        let create_body = |name: &str, radius: f32, orbit_radius: f32, orbit_speed: f32, orbit_angle: f32, color: (f32, f32, f32), parent: Option<usize>, mesh_fn: fn(f32, u16, u16, f32, f32, f32) -> Mesh, texture_url: Option<&str>| {
            let mut label_element = None;
            if let Some(container) = &labels_container {
                let el = document.create_element("div").unwrap();
                el.set_class_name("solar-label");
                el.set_text_content(Some(name));
                container.append_child(&el).unwrap();
                if let Ok(html_el) = el.dyn_into::<HtmlElement>() {
                    label_element = Some(html_el);
                }
            }

            let texture = if let Some(url) = texture_url {
                renderer.create_texture(url).ok()
            } else {
                None
            };

            Body {
                mesh: mesh_fn(1.0, 40, 40, color.0, color.1, color.2),
                radius,
                orbit_radius,
                orbit_speed,
                orbit_angle,
                color,
                parent,
                name: name.to_string(),
                trail: Vec::new(),
                label_element,
                texture,
            }
        };

        // Textures from Solar System Scope (Creative Commons Attribution 4.0 International)
        // Using low res versions for performance/bandwidth
        bodies.push(create_body("Sun", 2.0, 0.0, 0.0, 0.0, (1.0, 1.0, 0.0), None, Mesh::sphere, Some("https://upload.wikimedia.org/wikipedia/commons/c/c0/Sun_texture.jpg")));

        let p_mercury = 87.969;
        bodies.push(create_body("Mercury", 0.38, 5.0, get_orbit_speed(p_mercury), get_initial_angle(252.25, p_mercury), (0.5, 0.5, 0.5), Some(0), Mesh::sphere, Some("https://upload.wikimedia.org/wikipedia/commons/3/30/Mercury_in_color_-_Prockter07-edit1.jpg")));

        let p_venus = 224.701;
        bodies.push(create_body("Venus", 0.95, 8.0, get_orbit_speed(p_venus), get_initial_angle(181.98, p_venus), (0.9, 0.7, 0.2), Some(0), Mesh::sphere, Some("https://upload.wikimedia.org/wikipedia/commons/0/02/Venus_globe_-_Magellan_image.jpg")));

        let p_earth = 365.256;
        bodies.push(create_body("Earth", 1.0, 11.0, get_orbit_speed(p_earth), get_initial_angle(100.46, p_earth), (0.0, 0.0, 1.0), Some(0), Mesh::sphere, Some("https://upload.wikimedia.org/wikipedia/commons/thumb/c/c3/Solarsystemscope_texture_2k_earth_daymap.jpg/1024px-Solarsystemscope_texture_2k_earth_daymap.jpg")));

        let p_moon = 27.322;
        bodies.push(create_body("Moon", 0.27, 2.0, get_orbit_speed(p_moon), get_initial_angle(0.0, p_moon), (0.6, 0.6, 0.6), Some(3), Mesh::sphere, Some("https://upload.wikimedia.org/wikipedia/commons/thumb/e/e1/FullMoon2010.jpg/1024px-FullMoon2010.jpg")));

        let p_mars = 686.980;
        bodies.push(create_body("Mars", 0.53, 15.0, get_orbit_speed(p_mars), get_initial_angle(355.45, p_mars), (1.0, 0.0, 0.0), Some(0), Mesh::sphere, Some("https://upload.wikimedia.org/wikipedia/commons/thumb/0/02/OSIRIS_Mars_true_color.jpg/1024px-OSIRIS_Mars_true_color.jpg")));

        let p_jupiter = 4332.589;
        bodies.push(create_body("Jupiter", 3.0, 25.0, get_orbit_speed(p_jupiter), get_initial_angle(34.40, p_jupiter), (0.8, 0.6, 0.4), Some(0), Mesh::sphere, Some("https://upload.wikimedia.org/wikipedia/commons/e/e2/Jupiter.jpg")));

        let p_saturn = 10759.22;
        bodies.push(create_body("Saturn", 2.5, 35.0, get_orbit_speed(p_saturn), get_initial_angle(49.94, p_saturn), (0.9, 0.8, 0.5), Some(0), Mesh::sphere, Some("https://upload.wikimedia.org/wikipedia/commons/c/c7/Saturn_during_Equinox.jpg")));

        let p_uranus = 30685.4;
        bodies.push(create_body("Uranus", 1.8, 45.0, get_orbit_speed(p_uranus), get_initial_angle(313.23, p_uranus), (0.0, 0.8, 0.8), Some(0), Mesh::sphere, Some("https://upload.wikimedia.org/wikipedia/commons/3/3d/Uranus2.jpg")));

        let p_neptune = 60189.0;
        bodies.push(create_body("Neptune", 1.7, 55.0, get_orbit_speed(p_neptune), get_initial_angle(304.88, p_neptune), (0.0, 0.0, 0.8), Some(0), Mesh::sphere, Some("https://upload.wikimedia.org/wikipedia/commons/5/56/Neptune_Full.jpg")));

        SolarSystem {
            renderer,
            bodies,
            camera_distance: 60.0,
            camera_rotation: (0.5, 0.0),
            last_time: now_ms,
            is_dragging: false,
            last_mouse_pos: (0, 0),
            time_scale: 1.0,
            current_time: now_ms,
        }
    }

    pub fn set_time_scale(&mut self, scale: f32) {
        self.time_scale = scale;
    }

    pub fn update(&mut self) {
        let now = Date::now();
        let dt = (now - self.last_time) / 1000.0;
        self.last_time = now;
        
        self.current_time += dt * 1000.0 * self.time_scale as f64;
        
        let date = Date::new(&wasm_bindgen::JsValue::from_f64(self.current_time));
        let window = web_sys::window().unwrap();
        let document = window.document().unwrap();
        if let Some(element) = document.get_element_by_id("solar-date") {
            let date_str: String = date.to_locale_string("en-US", &wasm_bindgen::JsValue::UNDEFINED).into();
            element.set_text_content(Some(&date_str));
        }

        let mut positions = vec![Vector3::new(0.0, 0.0, 0.0); self.bodies.len()];

        for i in 0..self.bodies.len() {
            let body = &mut self.bodies[i];
            if body.parent.is_some() {
                body.orbit_angle += body.orbit_speed * dt as f32 * self.time_scale; 
            }
            
            let x = body.orbit_radius * body.orbit_angle.cos();
            let z = body.orbit_radius * body.orbit_angle.sin();
            
            let mut pos = Vector3::new(x, 0.0, z);
            
            if let Some(parent_idx) = body.parent {
                pos += positions[parent_idx];
            }
            
            positions[i] = pos;
            
            if body.orbit_radius > 0.0 {
                let should_add_point = if body.trail.len() >= 3 {
                    let last_x = body.trail[body.trail.len() - 3];
                    let last_y = body.trail[body.trail.len() - 2];
                    let last_z = body.trail[body.trail.len() - 1];
                    
                    let dx = pos.x - last_x;
                    let dy = pos.y - last_y;
                    let dz = pos.z - last_z;
                    
                    let dist_sq = dx*dx + dy*dy + dz*dz;
                    dist_sq > 0.05 // Only add point if moved enough
                } else {
                    true
                };

                if should_add_point {
                    body.trail.push(pos.x);
                    body.trail.push(pos.y);
                    body.trail.push(pos.z);
                    
                    // Keep trail long enough for a full orbit (approx)
                    // 5000 points is plenty for smooth circles
                    if body.trail.len() > 5000 {
                        body.trail.drain(0..3);
                    }
                }
            }
        }
    }

    pub fn render(&self, width: i32, height: i32) {
        self.renderer.clear(0.0, 0.0, 0.0);
        self.renderer.resize(width, height);
        self.renderer.enable_depth_test();

        let aspect = width as f32 / height as f32;
        let projection = Matrix4::new_perspective(aspect, 45.0 * std::f32::consts::PI / 180.0, 0.1, 1000.0);
        
        let cam_x = self.camera_distance * self.camera_rotation.0.cos() * self.camera_rotation.1.sin();
        let cam_y = self.camera_distance * self.camera_rotation.0.sin();
        let cam_z = self.camera_distance * self.camera_rotation.0.cos() * self.camera_rotation.1.cos();

        let view = Matrix4::look_at_rh(
            &Point3::new(cam_x, cam_y, cam_z),
            &Point3::new(0.0, 0.0, 0.0),
            &Vector3::y(),
        );

        let mut positions = vec![Vector3::new(0.0, 0.0, 0.0); self.bodies.len()];

        for i in 0..self.bodies.len() {
            let body = &self.bodies[i];
            let x = body.orbit_radius * body.orbit_angle.cos();
            let z = body.orbit_radius * body.orbit_angle.sin();
            let mut pos = Vector3::new(x, 0.0, z);
            if let Some(parent_idx) = body.parent {
                pos += positions[parent_idx];
            }
            positions[i] = pos;
        }

        for (i, body) in self.bodies.iter().enumerate() {
            let pos = positions[i];
            
            if !body.trail.is_empty() {
                self.renderer.draw_lines(
                    &body.trail,
                    body.color.0 * 0.5,
                    body.color.1 * 0.5,
                    body.color.2 * 0.5,
                    &projection,
                    &view
                );
            }

            self.renderer.draw_mesh(
                &body.mesh,
                pos.x, pos.y, pos.z,
                body.radius, body.radius, body.radius,
                0.0, 0.0, 0.0,
                &projection,
                &view,
                body.texture.as_ref()
            );
            
            if let Some(element) = &body.label_element {
                let pos_vec4 = Vector4::new(pos.x, pos.y + body.radius + 0.5, pos.z, 1.0);
                let clip_space = projection * view * pos_vec4;
                
                if clip_space.w > 0.0 {
                    let ndc_x = clip_space.x / clip_space.w;
                    let ndc_y = clip_space.y / clip_space.w;
                    
                    if ndc_x >= -1.0 && ndc_x <= 1.0 && ndc_y >= -1.0 && ndc_y <= 1.0 {
                        let screen_x = (ndc_x + 1.0) * width as f32 / 2.0;
                        let screen_y = (1.0 - ndc_y) * height as f32 / 2.0;
                        
                        let style = element.style();
                        style.set_property("display", "block").unwrap();
                        style.set_property("left", &format!("{}px", screen_x)).unwrap();
                        style.set_property("top", &format!("{}px", screen_y)).unwrap();
                    } else {
                        element.style().set_property("display", "none").unwrap();
                    }
                } else {
                    element.style().set_property("display", "none").unwrap();
                }
            }
        }
    }

    pub fn handle_input(&mut self, key: &str) {
        match key {
            "ArrowUp" => self.camera_distance -= 1.0,
            "ArrowDown" => self.camera_distance += 1.0,
            "ArrowLeft" => self.camera_rotation.1 -= 0.1,
            "ArrowRight" => self.camera_rotation.1 += 0.1,
            _ => {}
        }
    }

    pub fn handle_mouse_down(&mut self, x: i32, y: i32) {
        self.is_dragging = true;
        self.last_mouse_pos = (x, y);
    }

    pub fn handle_mouse_up(&mut self) {
        self.is_dragging = false;
    }

    pub fn handle_mouse_move(&mut self, x: i32, y: i32) {
        if self.is_dragging {
            let dx = x - self.last_mouse_pos.0;
            let dy = y - self.last_mouse_pos.1;
            
            self.camera_rotation.1 += dx as f32 * 0.01;
            self.camera_rotation.0 += dy as f32 * 0.01;
            
            // Clamp elevation to avoid flipping
            self.camera_rotation.0 = self.camera_rotation.0.max(-1.5).min(1.5);
            
            self.last_mouse_pos = (x, y);
        }
    }

    pub fn handle_wheel(&mut self, delta: f32) {
        self.camera_distance += delta * 0.05;
        self.camera_distance = self.camera_distance.max(5.0).min(200.0);
    }
}
