use crate::engine::renderer::Renderer;
use crate::engine::mesh::Mesh;
use nalgebra::{Matrix4, Point3, Vector3};
use js_sys::Date;

pub struct Body {
    pub mesh: Mesh,
    pub radius: f32,
    pub orbit_radius: f32,
    pub orbit_speed: f32,
    pub orbit_angle: f32,
    pub color: (f32, f32, f32),
    pub parent: Option<usize>,
    pub name: String,
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
}

impl SolarSystem {
    pub fn new(renderer: Renderer) -> Self {
        let mut bodies = Vec::new();

        // Sun
        bodies.push(Body {
            mesh: Mesh::sphere(1.0, 20, 20, 1.0, 1.0, 0.0),
            radius: 2.0,
            orbit_radius: 0.0,
            orbit_speed: 0.0,
            orbit_angle: 0.0,
            color: (1.0, 1.0, 0.0),
            parent: None,
            name: "Sun".to_string(),
        });

        // Mercury
        bodies.push(Body {
            mesh: Mesh::sphere(1.0, 10, 10, 0.5, 0.5, 0.5),
            radius: 0.38,
            orbit_radius: 5.0,
            orbit_speed: 4.7,
            orbit_angle: 0.0,
            color: (0.5, 0.5, 0.5),
            parent: Some(0),
            name: "Mercury".to_string(),
        });

        // Venus
        bodies.push(Body {
            mesh: Mesh::sphere(1.0, 15, 15, 0.9, 0.7, 0.2),
            radius: 0.95,
            orbit_radius: 8.0,
            orbit_speed: 3.5,
            orbit_angle: 0.0,
            color: (0.9, 0.7, 0.2),
            parent: Some(0),
            name: "Venus".to_string(),
        });

        // Earth
        bodies.push(Body {
            mesh: Mesh::sphere(1.0, 15, 15, 0.0, 0.0, 1.0),
            radius: 1.0,
            orbit_radius: 11.0,
            orbit_speed: 2.9,
            orbit_angle: 0.0,
            color: (0.0, 0.0, 1.0),
            parent: Some(0),
            name: "Earth".to_string(),
        });

        // Moon
        bodies.push(Body {
            mesh: Mesh::sphere(1.0, 8, 8, 0.6, 0.6, 0.6),
            radius: 0.27,
            orbit_radius: 2.0,
            orbit_speed: 10.0,
            orbit_angle: 0.0,
            color: (0.6, 0.6, 0.6),
            parent: Some(3),
            name: "Moon".to_string(),
        });

        // Mars
        bodies.push(Body {
            mesh: Mesh::sphere(1.0, 12, 12, 1.0, 0.0, 0.0),
            radius: 0.53,
            orbit_radius: 15.0,
            orbit_speed: 2.4,
            orbit_angle: 0.0,
            color: (1.0, 0.0, 0.0),
            parent: Some(0),
            name: "Mars".to_string(),
        });

        // Jupiter
        bodies.push(Body {
            mesh: Mesh::sphere(1.0, 20, 20, 0.8, 0.6, 0.4),
            radius: 3.0,
            orbit_radius: 25.0,
            orbit_speed: 1.3,
            orbit_angle: 0.0,
            color: (0.8, 0.6, 0.4),
            parent: Some(0),
            name: "Jupiter".to_string(),
        });

        // Saturn
        bodies.push(Body {
            mesh: Mesh::sphere(1.0, 18, 18, 0.9, 0.8, 0.5),
            radius: 2.5,
            orbit_radius: 35.0,
            orbit_speed: 0.9,
            orbit_angle: 0.0,
            color: (0.9, 0.8, 0.5),
            parent: Some(0),
            name: "Saturn".to_string(),
        });

        // Uranus
        bodies.push(Body {
            mesh: Mesh::sphere(1.0, 15, 15, 0.0, 0.8, 0.8),
            radius: 1.8,
            orbit_radius: 45.0,
            orbit_speed: 0.6,
            orbit_angle: 0.0,
            color: (0.0, 0.8, 0.8),
            parent: Some(0),
            name: "Uranus".to_string(),
        });

        // Neptune
        bodies.push(Body {
            mesh: Mesh::sphere(1.0, 15, 15, 0.0, 0.0, 0.8),
            radius: 1.7,
            orbit_radius: 55.0,
            orbit_speed: 0.5,
            orbit_angle: 0.0,
            color: (0.0, 0.0, 0.8),
            parent: Some(0),
            name: "Neptune".to_string(),
        });

        SolarSystem {
            renderer,
            bodies,
            camera_distance: 60.0,
            camera_rotation: (0.5, 0.0),
            last_time: Date::now(),
            is_dragging: false,
            last_mouse_pos: (0, 0),
            time_scale: 1.0,
        }
    }

    pub fn set_time_scale(&mut self, scale: f32) {
        self.time_scale = scale;
    }

    pub fn update(&mut self) {
        let now = Date::now();
        let dt = (now - self.last_time) / 1000.0;
        self.last_time = now;

        for body in &mut self.bodies {
            if body.parent.is_some() {
                body.orbit_angle += body.orbit_speed * dt as f32 * 0.5 * self.time_scale; 
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

        // Calculate positions
        for (i, body) in self.bodies.iter().enumerate() {
            if let Some(parent_idx) = body.parent {
                let parent_pos = positions[parent_idx];
                let x = parent_pos.x + body.orbit_radius * body.orbit_angle.cos();
                let z = parent_pos.z + body.orbit_radius * body.orbit_angle.sin();
                positions[i] = Vector3::new(x, 0.0, z);
            } else {
                positions[i] = Vector3::new(0.0, 0.0, 0.0);
            }
        }

        // Draw orbits
        for (_i, body) in self.bodies.iter().enumerate() {
            if let Some(parent_idx) = body.parent {
                let parent_pos = positions[parent_idx];
                let mut orbit_points = Vec::new();
                let segments = 64;
                for j in 0..segments {
                    let angle = j as f32 * 2.0 * std::f32::consts::PI / segments as f32;
                    let x = parent_pos.x + body.orbit_radius * angle.cos();
                    let z = parent_pos.z + body.orbit_radius * angle.sin();
                    orbit_points.push(x);
                    orbit_points.push(0.0);
                    orbit_points.push(z);
                }
                self.renderer.draw_lines(&orbit_points, 0.3, 0.3, 0.3, &projection, &view);
            }
        }

        // Draw bodies
        for (i, body) in self.bodies.iter().enumerate() {
            let pos = positions[i];
            self.renderer.draw_mesh(
                &body.mesh,
                pos.x, pos.y, pos.z,
                body.radius, body.radius, body.radius,
                0.0, 0.0, 0.0,
                &projection, &view
            );
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
