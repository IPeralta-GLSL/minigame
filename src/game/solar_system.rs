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
    pub night_texture: Option<WebGlTexture>,
    pub cloud_texture: Option<WebGlTexture>,
    pub cloud_rotation: f32,
    pub rotation_period: f32,
    pub axial_tilt: f32,
    pub current_rotation: f32,
    pub orbit_inclination: f32,
    pub last_trail_angle: f32,
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
    background_mesh: Mesh,
    background_texture: Option<WebGlTexture>,
    focused_body_index: Option<usize>,
    sphere_mesh: Mesh,
}

impl SolarSystem {
    pub fn new(renderer: Renderer) -> Self {
        let mut bodies = Vec::new();
        let sphere_mesh = Mesh::sphere(1.0, 20, 20, 1.0, 1.0, 1.0);
        
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

        let create_body = |name: &str, radius: f32, orbit_radius: f32, orbit_speed: f32, orbit_angle: f32, color: (f32, f32, f32), parent: Option<usize>, mesh_fn: fn(f32, u16, u16, f32, f32, f32) -> Mesh, texture_url: Option<&str>, night_texture_url: Option<&str>, cloud_texture_url: Option<&str>, rotation_period: f32, axial_tilt: f32, orbit_inclination: f32| {
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
                match renderer.create_texture(url) {
                    Ok(t) => Some(t),
                    Err(e) => {
                        web_sys::console::error_1(&format!("Failed to create texture for {}: {:?}", name, e).into());
                        None
                    }
                }
            } else {
                None
            };

            let night_texture = if let Some(url) = night_texture_url {
                match renderer.create_texture(url) {
                    Ok(t) => Some(t),
                    Err(e) => {
                        web_sys::console::error_1(&format!("Failed to create night texture for {}: {:?}", name, e).into());
                        None
                    }
                }
            } else {
                None
            };

            let cloud_texture = if let Some(url) = cloud_texture_url {
                match renderer.create_texture(url) {
                    Ok(t) => Some(t),
                    Err(e) => {
                        web_sys::console::error_1(&format!("Failed to create cloud texture for {}: {:?}", name, e).into());
                        None
                    }
                }
            } else {
                None
            };

            let (mesh_r, mesh_g, mesh_b) = if texture.is_some() {
                (1.0, 1.0, 1.0)
            } else {
                color
            };

            Body {
                mesh: mesh_fn(1.0, 40, 40, mesh_r, mesh_g, mesh_b),
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
                night_texture,
                cloud_texture,
                cloud_rotation: 0.0,
                rotation_period,
                axial_tilt: axial_tilt.to_radians(),
                current_rotation: 0.0,
                orbit_inclination: orbit_inclination.to_radians(),
                last_trail_angle: orbit_angle,
            }
        };



        bodies.push(create_body("Sun", 0.465, 0.0, 0.0, 0.0, (1.0, 1.0, 0.0), None, Mesh::sphere, Some("assets/textures/2k_sun.jpg"), None, None, 25.0, 7.25, 0.0));

        let p_mercury = 87.969;

        bodies.push(create_body("Mercury", 0.0016, 39.0, get_orbit_speed(p_mercury), get_initial_angle(252.25, p_mercury), (0.5, 0.5, 0.5), Some(0), Mesh::sphere, Some("https://upload.wikimedia.org/wikipedia/commons/3/30/Mercury_in_color_-_Prockter07-edit1.jpg"), None, None, 58.6, 0.03, 7.0));

        let p_venus = 224.701;

        bodies.push(create_body("Venus", 0.004, 72.0, get_orbit_speed(p_venus), get_initial_angle(181.98, p_venus), (0.9, 0.7, 0.2), Some(0), Mesh::sphere, Some("assets/textures/2k_venus_surface.jpg"), None, Some("assets/textures/2k_venus_atmosphere.jpg"), -243.0, 177.3, 3.4));

        let p_earth = 365.256;

        bodies.push(create_body("Earth", 0.0042, 100.0, get_orbit_speed(p_earth), get_initial_angle(100.46, p_earth), (0.0, 0.0, 1.0), Some(0), Mesh::sphere, Some("assets/textures/2k_earth_daymap.jpg"), Some("assets/textures/2k_earth_nightmap.jpg"), Some("assets/textures/2k_earth_clouds.jpg"), 1.0, 23.4, 0.0));

        let p_moon = 27.322;

        bodies.push(create_body("Moon", 0.0011, 0.257, get_orbit_speed(p_moon), get_initial_angle(0.0, p_moon), (0.6, 0.6, 0.6), Some(3), Mesh::sphere, Some("assets/textures/2k_moon.jpg"), None, None, 27.3, 6.7, 5.1));

        let p_mars = 686.980;

        bodies.push(create_body("Mars", 0.0022, 152.0, get_orbit_speed(p_mars), get_initial_angle(355.45, p_mars), (1.0, 0.0, 0.0), Some(0), Mesh::sphere, Some("assets/textures/2k_mars.jpg"), None, None, 1.03, 25.2, 1.85));


        let p_ceres = 1681.6;
        bodies.push(create_body("Ceres", 0.00029, 277.0, get_orbit_speed(p_ceres), get_initial_angle(0.0, p_ceres), (0.4, 0.4, 0.4), Some(0), Mesh::sphere, Some("assets/textures/2k_ceres_fictional.jpg"), None, None, 0.375, 4.0, 10.6));

        let p_jupiter = 4332.589;

        bodies.push(create_body("Jupiter", 0.047, 520.0, get_orbit_speed(p_jupiter), get_initial_angle(34.40, p_jupiter), (0.8, 0.6, 0.4), Some(0), Mesh::sphere, Some("assets/textures/2k_jupiter.jpg"), None, None, 0.41, 3.1, 1.3));

        let p_saturn = 10759.22;

        bodies.push(create_body("Saturn", 0.039, 958.0, get_orbit_speed(p_saturn), get_initial_angle(49.94, p_saturn), (0.9, 0.8, 0.5), Some(0), Mesh::sphere, Some("assets/textures/2k_saturn.jpg"), None, None, 0.45, 26.7, 2.48));

        let p_uranus = 30685.4;

        bodies.push(create_body("Uranus", 0.017, 1920.0, get_orbit_speed(p_uranus), get_initial_angle(313.23, p_uranus), (0.0, 0.8, 0.8), Some(0), Mesh::sphere, Some("assets/textures/2k_uranus.jpg"), None, None, -0.72, 97.8, 0.77));

        let p_neptune = 60189.0;

        bodies.push(create_body("Neptune", 0.016, 3005.0, get_orbit_speed(p_neptune), get_initial_angle(304.88, p_neptune), (0.0, 0.0, 0.8), Some(0), Mesh::sphere, Some("assets/textures/2k_neptune.jpg"), None, None, 0.67, 28.3, 1.77));


        let p_pluto = 90560.0;
        bodies.push(create_body("Pluto", 0.00075, 3948.0, get_orbit_speed(p_pluto), get_initial_angle(0.0, p_pluto), (0.6, 0.5, 0.4), Some(0), Mesh::sphere, Some("https://upload.wikimedia.org/wikipedia/commons/e/ef/Pluto_in_True_Color_-_High-Res.jpg"), None, None, -6.39, 122.5, 17.16));


        let p_haumea = 103368.0;
        bodies.push(create_body("Haumea", 0.00055, 4313.0, get_orbit_speed(p_haumea), get_initial_angle(0.0, p_haumea), (0.7, 0.7, 0.7), Some(0), Mesh::sphere, Some("assets/textures/2k_haumea_fictional.jpg"), None, None, 0.16, 0.0, 28.2));


        let p_makemake = 112862.0;
        bodies.push(create_body("Makemake", 0.00046, 4579.0, get_orbit_speed(p_makemake), get_initial_angle(0.0, p_makemake), (0.8, 0.6, 0.5), Some(0), Mesh::sphere, Some("assets/textures/2k_makemake_fictional.jpg"), None, None, 0.95, 0.0, 29.0));


        let p_eris = 203443.0;
        bodies.push(create_body("Eris", 0.00075, 6767.0, get_orbit_speed(p_eris), get_initial_angle(0.0, p_eris), (0.9, 0.9, 0.9), Some(0), Mesh::sphere, Some("assets/textures/2k_eris_fictional.jpg"), None, None, 1.08, 78.0, 44.0));

        let background_texture = renderer.create_texture("assets/textures/8k_stars.jpg").ok();
        let background_mesh = Mesh::sphere(1.0, 40, 40, 1.0, 1.0, 1.0);


        let trail_points = 1000;
        for i in 0..bodies.len() {
            let body = &mut bodies[i];
            if body.orbit_radius > 0.0 && body.orbit_speed != 0.0 {
                let full_circle = 2.0 * std::f32::consts::PI;
                let angle_step = full_circle / trail_points as f32;
                



                
                for j in 0..trail_points {
                    let angle_offset = -full_circle + (j as f32 * angle_step);
                    let angle = body.orbit_angle + angle_offset;
                    
                    let x = body.orbit_radius * angle.cos();
                    let z = body.orbit_radius * angle.sin();
                    let y = z * body.orbit_inclination.sin();
                    let z = z * body.orbit_inclination.cos();
                    
                    let pos = Vector3::new(x, y, z);
                    
                    if body.parent.is_some() && body.parent.unwrap() == 0 {
                         body.trail.push(pos.x);
                         body.trail.push(pos.y);
                         body.trail.push(pos.z);
                    }
                }
            }
        }


        if let Some(list) = document.query_selector(".body-list").unwrap() {
            list.set_inner_html(""); // Clear existing
            
            for (i, body) in bodies.iter().enumerate() {
                let li = document.create_element("li").unwrap();
                li.set_text_content(Some(&body.name));

                li.set_attribute("onclick", &format!("window.selectSolarBody({})", i)).unwrap();
                li.set_attribute("style", "cursor: pointer; padding: 5px; transition: background 0.2s;").unwrap();
                li.set_class_name("solar-list-item");
                
                list.append_child(&li).unwrap();
            }
        }

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
            background_mesh,
            background_texture,
            focused_body_index: Some(3),
            sphere_mesh,
        }
    }

    pub fn select_body(&mut self, index: usize) {
        if index < self.bodies.len() {
            self.focused_body_index = Some(index);


            let radius = self.bodies[index].radius;



            self.camera_distance = radius * 5.0;
            

            self.camera_distance = self.camera_distance.max(radius * 1.5);
        } else {
            self.focused_body_index = None;
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
            

            if body.rotation_period != 0.0 {



                let period_seconds = body.rotation_period.abs() * 24.0 * 3600.0;
                let rotation_speed = (2.0 * std::f32::consts::PI) / period_seconds;
                

                body.current_rotation += rotation_speed * dt as f32 * self.time_scale;

                if body.cloud_texture.is_some() {

                    body.cloud_rotation += rotation_speed * 0.2 * dt as f32 * self.time_scale;
                }
            }

            let x = body.orbit_radius * body.orbit_angle.cos();
            let z = body.orbit_radius * body.orbit_angle.sin();
            
            let y = z * body.orbit_inclination.sin();
            let z = z * body.orbit_inclination.cos();
            
            let mut pos = Vector3::new(x, y, z);
            
            if let Some(parent_idx) = body.parent {
                pos += positions[parent_idx];
            }
            
            positions[i] = pos;
            
            if body.orbit_radius > 0.0 {

                let two_pi = 2.0 * std::f32::consts::PI;
                let angle_step = two_pi / 1000.0; // 1000 points per orbit
                

                let current_angle = body.orbit_angle % two_pi;
                let last_angle = body.last_trail_angle % two_pi;
                
                let mut diff = current_angle - last_angle;
                if diff < 0.0 {
                    diff += two_pi;
                }
                

                if diff >= angle_step {
                    let steps = (diff / angle_step).floor() as usize;
                    


                    let steps_to_add = steps.min(1000);
                    
                    for k in 1..=steps_to_add {
                        let a = body.last_trail_angle + (k as f32 * angle_step);
                        
                        let x = body.orbit_radius * a.cos();
                        let z = body.orbit_radius * a.sin();
                        let y = z * body.orbit_inclination.sin();
                        let z = z * body.orbit_inclination.cos();
                        
                        let mut p = Vector3::new(x, y, z);
                        
















                        
                        if let Some(parent_idx) = body.parent {
                             p += positions[parent_idx];
                        }
                        
                        body.trail.push(p.x);
                        body.trail.push(p.y);
                        body.trail.push(p.z);
                    }
                    
                    body.last_trail_angle += steps as f32 * angle_step;
                    

                    while body.trail.len() > 3000 {
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


        let mut positions = vec![Vector3::new(0.0, 0.0, 0.0); self.bodies.len()];
        for i in 0..self.bodies.len() {
            let body = &self.bodies[i];
            let x = body.orbit_radius * body.orbit_angle.cos();
            let z = body.orbit_radius * body.orbit_angle.sin();
            
            let y = z * body.orbit_inclination.sin();
            let z = z * body.orbit_inclination.cos();

            let mut pos = Vector3::new(x, y, z);
            if let Some(parent_idx) = body.parent {
                pos += positions[parent_idx];
            }
            positions[i] = pos;
        }

        let target = if let Some(idx) = self.focused_body_index {
            positions[idx]
        } else {
            Vector3::new(0.0, 0.0, 0.0)
        };

        let aspect = width as f32 / height as f32;
        let projection = Matrix4::new_perspective(aspect, 45.0 * std::f32::consts::PI / 180.0, 0.0001, 50000.0); // Reduced near plane for close zoom
        




        
        let rel_cam_x = self.camera_distance * self.camera_rotation.0.cos() * self.camera_rotation.1.sin();
        let rel_cam_y = self.camera_distance * self.camera_rotation.0.sin();
        let rel_cam_z = self.camera_distance * self.camera_rotation.0.cos() * self.camera_rotation.1.cos();

        let view = Matrix4::look_at_rh(
            &Point3::new(rel_cam_x, rel_cam_y, rel_cam_z),
            &Point3::new(0.0, 0.0, 0.0),
            &Vector3::y(),
        );




        let rel_light_pos = Vector3::new(0.0, 0.0, 0.0) - target;
        self.renderer.set_light_position(rel_light_pos.x, rel_light_pos.y, rel_light_pos.z);

        self.renderer.gl.disable(web_sys::WebGlRenderingContext::DEPTH_TEST);
        

        self.renderer.gl.uniform1i(Some(&self.renderer.u_use_lighting_location), 0);


            self.renderer.draw_mesh(
                &self.background_mesh,
                rel_cam_x, rel_cam_y, rel_cam_z,
                5000.0, 5000.0, 5000.0,
                0.0, 0.0, 0.0,
                &projection,
                &view,
                self.background_texture.as_ref(),
                None,
                None
            );        // Re-enable lighting for planets
        self.renderer.gl.uniform1i(Some(&self.renderer.u_use_lighting_location), 1);
        
        self.renderer.enable_depth_test();





        for (i, body) in self.bodies.iter().enumerate() {
            let abs_pos = positions[i];

            let pos = abs_pos - target;
            
            if !body.trail.is_empty() {

                let relative_trail: Vec<f32> = body.trail.chunks(3).flat_map(|p| {
                    vec![p[0] - target.x, p[1] - target.y, p[2] - target.z]
                }).collect();

                self.renderer.draw_lines(
                    &relative_trail,
                    body.color.0 * 0.5,
                    body.color.1 * 0.5,
                    body.color.2 * 0.5,
                    &projection,
                    &view
                );
            }



            let dx = rel_cam_x - pos.x;
            let dy = rel_cam_y - pos.y;
            let dz = rel_cam_z - pos.z;
            let dist = (dx*dx + dy*dy + dz*dz).sqrt();
            


            let min_size = dist * 0.002; 
            
            let (render_radius, use_texture) = if min_size > body.radius {
                (min_size, false)
            } else {
                (body.radius, true)
            };
            
            let texture_to_use = if use_texture {
                body.texture.as_ref()
            } else {
                None
            };

            let night_texture_to_use = if use_texture {
                body.night_texture.as_ref()
            } else {
                None
            };
            
            let color_override = if !use_texture {
                Some(body.color)
            } else {
                None
            };
            

            let mesh_to_use = if !use_texture {
                &self.sphere_mesh
            } else {
                &body.mesh
            };

            self.renderer.draw_mesh(
                mesh_to_use,
                pos.x, pos.y, pos.z,
                render_radius, render_radius, render_radius,
                body.axial_tilt, body.current_rotation, 0.0,
                &projection,
                &view,
                texture_to_use,
                night_texture_to_use,
                color_override
            );

            if use_texture {
                if let Some(cloud_tex) = &body.cloud_texture {
                    self.renderer.gl.enable(web_sys::WebGlRenderingContext::BLEND);

                    self.renderer.gl.blend_func(web_sys::WebGlRenderingContext::ONE, web_sys::WebGlRenderingContext::ONE);
                    
                    self.renderer.draw_mesh(
                        &body.mesh,
                        pos.x, pos.y, pos.z,
                        render_radius * 1.02, render_radius * 1.02, render_radius * 1.02,
                        body.axial_tilt, body.current_rotation + body.cloud_rotation, 0.0,
                        &projection,
                        &view,
                        Some(cloud_tex),
                        None,
                        None
                    );
                    
                    self.renderer.gl.disable(web_sys::WebGlRenderingContext::BLEND);
                }
            }
            
            if let Some(element) = &body.label_element {





                
                let center_world = Vector4::new(pos.x, pos.y, pos.z, 1.0);
                let view_pos = view * center_world;

                let top_view = view_pos + Vector4::new(0.0, render_radius, 0.0, 0.0);
                
                let clip_center = projection * view_pos;
                let clip_top = projection * top_view;
                
                if clip_center.w > 0.0 {
                    let ndc_center_x = clip_center.x / clip_center.w;
                    let ndc_center_y = clip_center.y / clip_center.w;
                    let ndc_top_y = clip_top.y / clip_top.w;
                    
                    if ndc_center_x >= -1.0 && ndc_center_x <= 1.0 && ndc_center_y >= -1.0 && ndc_center_y <= 1.0 {
                        let screen_x = (ndc_center_x + 1.0) * width as f32 / 2.0;
                        let screen_cy = (1.0 - ndc_center_y) * height as f32 / 2.0;
                        let screen_ty = (1.0 - ndc_top_y) * height as f32 / 2.0;
                        

                        let radius_px = (screen_cy - screen_ty).abs();
                        let label_y = screen_cy - radius_px - 20.0;
                        
                        let style = element.style();
                        style.set_property("display", "block").unwrap();
                        style.set_property("left", &format!("{}px", screen_x)).unwrap();
                        style.set_property("top", &format!("{}px", label_y)).unwrap();
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
            
            self.camera_rotation.0 = self.camera_rotation.0.max(-1.5).min(1.5);
            
            self.last_mouse_pos = (x, y);
        }
    }

    pub fn handle_wheel(&mut self, delta: f32) {
        let zoom_sensitivity = 0.001;
        let factor = (delta * zoom_sensitivity).exp();
        self.camera_distance *= factor;
        self.camera_distance = self.camera_distance.max(0.0001).min(50000.0);
    }
}
