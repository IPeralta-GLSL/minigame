use wasm_bindgen::prelude::*;
use web_sys::{WebGlRenderingContext, WebGlProgram, WebGlBuffer, WebGlUniformLocation, HtmlCanvasElement, KeyboardEvent, Request, RequestInit, RequestMode, Response};
use std::cell::RefCell;
use std::rc::Rc;
use nalgebra::{Matrix4, Vector3, Perspective3};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct ModelConfig {
    path: String,
    scale: f32,
    rotation_offset_x: f32,
    rotation_offset_y: f32,
    rotation_offset_z: f32,
    position_offset_y: f32,
}

#[derive(Serialize, Deserialize)]
struct AppConfig {
    car_model: ModelConfig,
}

const VERTEX_SHADER: &str = r#"
    attribute vec3 aPosition;
    attribute vec3 aColor;
    attribute vec2 aTexCoord;
    uniform mat4 uModelViewProjection;
    varying vec3 vColor;
    varying vec2 vTexCoord;
    varying vec3 vPos;
    void main() {
        gl_Position = uModelViewProjection * vec4(aPosition, 1.0);
        vPos = aPosition;
        vColor = aColor;
        vTexCoord = aTexCoord;
    }
"#;

const FRAGMENT_SHADER: &str = r#"
    precision mediump float;
    varying vec3 vColor;
    varying vec2 vTexCoord;
    varying vec3 vPos;
    uniform sampler2D uTexture;
    uniform int uUseTexture;
    void main() {
        vec3 color = vColor;
        if (uUseTexture == 1) {
            vec4 texColor = texture2D(uTexture, vTexCoord);
            color *= texColor.rgb;
        }
        
        // Simple ambient occlusion based on height (darker at bottom)
        float ao = smoothstep(-0.5, 0.5, vPos.y + 0.5);
        ao = mix(0.7, 1.0, ao);
        color *= ao;

        // Color grading
        // Increase saturation slightly
        float luminance = dot(color, vec3(0.2126, 0.7152, 0.0722));
        vec3 gray = vec3(luminance);
        color = mix(gray, color, 1.2);
        
        // Slight contrast
        color = pow(color, vec3(1.1));

        gl_FragColor = vec4(color, 1.0);
    }
"#;

struct Mesh {
    vertices: Vec<f32>,
    indices: Vec<u16>,
}

impl Mesh {
    fn cube(size: f32, r: f32, g: f32, b: f32) -> Self {
        let s = size / 2.0;
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let mut add_face = |
            x1: f32, y1: f32, z1: f32,
            x2: f32, y2: f32, z2: f32,
            x3: f32, y3: f32, z3: f32,
            x4: f32, y4: f32, z4: f32,
            brightness: f32
        | {
            let base = (vertices.len() / 6) as u16;
            let br = r * brightness;
            let bg = g * brightness;
            let bb = b * brightness;
            
            vertices.extend_from_slice(&[
                x1, y1, z1, br, bg, bb,
                x2, y2, z2, br, bg, bb,
                x3, y3, z3, br, bg, bb,
                x4, y4, z4, br, bg, bb,
            ]);
            
            indices.extend_from_slice(&[
                base, base + 1, base + 2,
                base, base + 2, base + 3,
            ]);
        };

        add_face(-s, -s, s, s, -s, s, s, s, s, -s, s, s, 0.9);
        add_face(s, -s, -s, -s, -s, -s, -s, s, -s, s, s, -s, 0.7);
        add_face(-s, s, s, s, s, s, s, s, -s, -s, s, -s, 1.1);
        add_face(-s, -s, -s, s, -s, -s, s, -s, s, -s, -s, s, 0.4);
        add_face(s, -s, s, s, -s, -s, s, s, -s, s, s, s, 0.8);
        add_face(-s, -s, -s, -s, -s, s, -s, s, s, -s, s, -s, 0.6);

        Mesh { vertices, indices }
    }

    fn from_gltf(bytes: &[u8]) -> Result<Self, String> {
        let (document, buffers, _) = gltf::import_slice(bytes).map_err(|e| e.to_string())?;
        
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        
        for mesh in document.meshes() {
            for primitive in mesh.primitives() {
                let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
                
                let positions: Vec<[f32; 3]> = reader.read_positions().ok_or("No positions")?.collect();
                let colors: Vec<[f32; 3]> = if let Some(iter) = reader.read_colors(0) {
                    iter.into_rgb_f32().collect()
                } else {
                    vec![[1.0, 1.0, 1.0]; positions.len()]
                };
                
                let base_index = (vertices.len() / 6) as u16;
                
                for (pos, color) in positions.iter().zip(colors.iter()) {
                    vertices.extend_from_slice(&[
                        pos[0], pos[1], pos[2],
                        color[0], color[1], color[2]
                    ]);
                }
                
                if let Some(iter) = reader.read_indices() {
                    for index in iter.into_u32() {
                        indices.push(base_index + index as u16);
                    }
                }
            }
        }
        
        Ok(Mesh { vertices, indices })
    }

    fn car(body_r: f32, body_g: f32, body_b: f32) -> Self {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        
        let mut add_box = |verts: &mut Vec<f32>, idxs: &mut Vec<u16>, 
                       ox: f32, oy: f32, oz: f32, 
                       sx: f32, sy: f32, sz: f32, 
                       r: f32, g: f32, b: f32| {
            let hx = sx / 2.0;
            let hy = sy / 2.0;
            let hz = sz / 2.0;
            
            let mut add_face = |
                x1: f32, y1: f32, z1: f32,
                x2: f32, y2: f32, z2: f32,
                x3: f32, y3: f32, z3: f32,
                x4: f32, y4: f32, z4: f32,
                brightness: f32
            | {
                let base = (verts.len() / 6) as u16;
                let br = r * brightness;
                let bg = g * brightness;
                let bb = b * brightness;
                
                verts.extend_from_slice(&[
                    ox + x1, oy + y1, oz + z1, br, bg, bb,
                    ox + x2, oy + y2, oz + z2, br, bg, bb,
                    ox + x3, oy + y3, oz + z3, br, bg, bb,
                    ox + x4, oy + y4, oz + z4, br, bg, bb,
                ]);
                
                idxs.extend_from_slice(&[
                    base, base + 1, base + 2,
                    base, base + 2, base + 3,
                ]);
            };

            add_face(-hx, -hy, hz, hx, -hy, hz, hx, hy, hz, -hx, hy, hz, 0.9);
            add_face(hx, -hy, -hz, -hx, -hy, -hz, -hx, hy, -hz, hx, hy, -hz, 0.7);
            add_face(-hx, hy, hz, hx, hy, hz, hx, hy, -hz, -hx, hy, -hz, 1.1);
            add_face(-hx, -hy, -hz, hx, -hy, -hz, hx, -hy, hz, -hx, -hy, hz, 0.4);
            add_face(hx, -hy, hz, hx, -hy, -hz, hx, hy, -hz, hx, hy, hz, 0.8);
            add_face(-hx, -hy, -hz, -hx, -hy, hz, -hx, hy, hz, -hx, hy, -hz, 0.6);
        };
        
        add_box(&mut vertices, &mut indices, 0.0, -0.1, 0.0, 0.55, 0.25, 0.9, body_r, body_g, body_b);
        add_box(&mut vertices, &mut indices, 0.0, -0.18, 0.0, 0.5, 0.08, 0.8, body_r * 0.7, body_g * 0.7, body_b * 0.7);
        add_box(&mut vertices, &mut indices, 0.0, 0.08, 0.02, 0.35, 0.2, 0.6, body_r * 0.9, body_g * 0.9, body_b * 0.9);
        add_box(&mut vertices, &mut indices, 0.0, 0.12, 0.02, 0.28, 0.1, 0.45, 0.55, 0.7, 0.85);
        add_box(&mut vertices, &mut indices, 0.0, -0.03, -0.43, 0.5, 0.06, 0.06, 0.15, 0.15, 0.15);
        add_box(&mut vertices, &mut indices, 0.0, -0.03, 0.43, 0.5, 0.05, 0.06, 0.15, 0.15, 0.15);
        add_box(&mut vertices, &mut indices, -0.18, -0.2, -0.3, 0.08, 0.22, 0.22, 0.1, 0.1, 0.1);
        add_box(&mut vertices, &mut indices, 0.18, -0.2, -0.3, 0.08, 0.22, 0.22, 0.1, 0.1, 0.1);
        add_box(&mut vertices, &mut indices, -0.18, -0.2, 0.3, 0.08, 0.22, 0.22, 0.1, 0.1, 0.1);
        add_box(&mut vertices, &mut indices, 0.18, -0.2, 0.3, 0.08, 0.22, 0.22, 0.1, 0.1, 0.1);
        add_box(&mut vertices, &mut indices, -0.18, -0.2, -0.3, 0.1, 0.12, 0.12, 0.35, 0.35, 0.35);
        add_box(&mut vertices, &mut indices, 0.18, -0.2, -0.3, 0.1, 0.12, 0.12, 0.35, 0.35, 0.35);
        add_box(&mut vertices, &mut indices, -0.18, -0.2, 0.3, 0.1, 0.12, 0.12, 0.35, 0.35, 0.35);
        add_box(&mut vertices, &mut indices, 0.18, -0.2, 0.3, 0.1, 0.12, 0.12, 0.35, 0.35, 0.35);
        add_box(&mut vertices, &mut indices, -0.12, 0.0, -0.45, 0.08, 0.08, 0.04, 1.0, 1.0, 0.7);
        add_box(&mut vertices, &mut indices, 0.12, 0.0, -0.45, 0.08, 0.08, 0.04, 1.0, 1.0, 0.7);
        add_box(&mut vertices, &mut indices, 0.0, -0.06, -0.45, 0.15, 0.03, 0.02, 0.85, 0.85, 0.85);
        add_box(&mut vertices, &mut indices, -0.12, -0.02, 0.45, 0.1, 0.1, 0.04, 0.9, 0.1, 0.1);
        add_box(&mut vertices, &mut indices, 0.12, -0.02, 0.45, 0.1, 0.1, 0.04, 0.9, 0.1, 0.1);
        add_box(&mut vertices, &mut indices, 0.0, -0.06, 0.45, 0.12, 0.03, 0.02, 0.85, 0.85, 0.85);
        add_box(&mut vertices, &mut indices, 0.0, 0.0, -0.4, 0.1, 0.05, 0.04, 0.15, 0.15, 0.15);
        add_box(&mut vertices, &mut indices, 0.0, 0.0, -0.4, 0.06, 0.03, 0.02, 0.4, 0.5, 0.6);
        
        Mesh { vertices, indices }
    }
}

struct GameObject {
    x: f32,
    y: f32,
    z: f32,
    width: f32,
    height: f32,
    depth: f32,
    velocity_x: f32,
    color: (f32, f32, f32),
    is_car: bool,
}

const CAR_COLORS: [(f32, f32, f32); 8] = [
    (0.9, 0.2, 0.2),
    (0.2, 0.5, 0.9),
    (0.2, 0.8, 0.3),
    (0.95, 0.8, 0.2),
    (0.9, 0.4, 0.1),
    (0.7, 0.2, 0.8),
    (0.1, 0.8, 0.8),
    (0.95, 0.95, 0.95),
];

impl GameObject {
    fn new(x: f32, y: f32, z: f32, width: f32, height: f32, depth: f32, color: (f32, f32, f32)) -> Self {
        GameObject { x, y, z, width, height, depth, velocity_x: 0.0, color, is_car: false }
    }

    fn new_car(x: f32, y: f32, z: f32, width: f32, height: f32, depth: f32, color_idx: usize) -> Self {
        let color = CAR_COLORS[color_idx % CAR_COLORS.len()];
        GameObject { x, y, z, width, height, depth, velocity_x: 0.0, color, is_car: true }
    }

    #[allow(dead_code)]
    fn collides_with(&self, other: &GameObject) -> bool {
        let dx = (self.x - other.x).abs();
        let dy = (self.y - other.y).abs();
        let dz = (self.z - other.z).abs();
        dx < (self.width + other.width) / 2.0 &&
        dy < (self.height + other.height) / 2.0 &&
        dz < (self.depth + other.depth) / 2.0
    }

    fn collides_horizontal(&self, other: &GameObject) -> bool {
        let dx = (self.x - other.x).abs();
        let dz = (self.z - other.z).abs();
        dx < (self.width + other.width) / 2.0 &&
        dz < (self.depth + other.depth) / 2.0
    }
}

struct Lane {
    z: f32,
    lane_type: LaneType,
    obstacles: Vec<GameObject>,
    coins: Vec<GameObject>,
}

enum LaneType {
    Grass,
    Road,
    Water,
}

struct Game {
    gl: WebGlRenderingContext,
    program: WebGlProgram,
    vertex_buffer: WebGlBuffer,
    index_buffer: WebGlBuffer,
    mvp_location: WebGlUniformLocation,
    player: GameObject,
    lanes: Vec<Lane>,
    score: i32,
    coins: i32,
    game_over: bool,
    moving: bool,
    target_z: f32,
    target_x: f32,
    move_direction: i32,
    jump_progress: f32,
    base_y: f32,
    world_seed: u32,
    furthest_lane: i32,
    time: f32,
    car_mesh: Option<Mesh>,
    config: Option<AppConfig>,
}

impl Game {
    fn new(gl: WebGlRenderingContext, car_mesh: Option<Mesh>, config: Option<AppConfig>) -> Result<Self, JsValue> {
        let program = create_program(&gl)?;
        gl.use_program(Some(&program));

        let vertex_buffer = gl.create_buffer().ok_or("Failed to create buffer")?;
        let index_buffer = gl.create_buffer().ok_or("Failed to create buffer")?;

        let mvp_location = gl.get_uniform_location(&program, "uModelViewProjection")
            .ok_or("Failed to get uniform location")?;

        let player = GameObject::new(0.0, 0.5, 0.0, 0.8, 1.0, 0.8, (0.2, 0.6, 1.0));

        // Generate random world seed
        let world_seed = (js_sys::Math::random() * 1000000.0) as u32;
        
        // Generate initial lanes
        let mut lanes = Vec::new();
        for i in -5..25 {
            lanes.push(create_lane_procedural(i as f32 * 2.0, i, world_seed));
        }

        Ok(Game {
            gl,
            program,
            vertex_buffer,
            index_buffer,
            mvp_location,
            player,
            lanes,
            score: 0,
            coins: 0,
            game_over: false,
            moving: false,
            target_z: 0.0,
            target_x: 0.0,
            move_direction: 0,
            jump_progress: 0.0,
            base_y: 0.5,
            world_seed,
            furthest_lane: 24,
            time: 0.0,
            car_mesh,
            config,
        })
    }

    fn update(&mut self) {
        // Always update time for animations
        self.time += 0.016; // ~60fps
        
        if self.game_over {
            return;
        }

        if self.moving {
            let speed = 0.15;
            self.jump_progress += speed / 2.0;
            
            let jump_height = 1.5;
            let jump_y = (self.jump_progress * std::f32::consts::PI).sin() * jump_height;
            self.player.y = self.base_y + jump_y;
            
            match self.move_direction {
                0 => {
                    self.player.z += speed;
                    if self.player.z >= self.target_z {
                        self.player.z = self.target_z;
                        self.moving = false;
                        self.jump_progress = 0.0;
                        self.player.y = self.base_y;
                    }
                }
                2 => {
                    self.player.x -= speed;
                    if self.player.x <= self.target_x {
                        self.player.x = self.target_x;
                        self.moving = false;
                        self.jump_progress = 0.0;
                        self.player.y = self.base_y;
                    }
                }
                3 => {
                    self.player.x += speed;
                    if self.player.x >= self.target_x {
                        self.player.x = self.target_x;
                        self.moving = false;
                        self.jump_progress = 0.0;
                        self.player.y = self.base_y;
                    }
                }
                _ => {}
            }
        }

        // Generate new lanes ahead as player advances (infinite world)
        let player_lane_idx = (self.player.z / 2.0).round() as i32;
        while self.furthest_lane < player_lane_idx + 20 {
            self.furthest_lane += 1;
            let new_lane = create_lane_procedural(
                self.furthest_lane as f32 * 2.0,
                self.furthest_lane,
                self.world_seed
            );
            self.lanes.push(new_lane);
        }
        
        // Remove lanes far behind the player to save memory
        self.lanes.retain(|lane| {
            let lane_idx = (lane.z / 2.0).round() as i32;
            lane_idx > player_lane_idx - 10
        });

        for lane in &mut self.lanes {
            for obstacle in &mut lane.obstacles {
                obstacle.x += obstacle.velocity_x;
                if obstacle.x > 15.0 {
                    obstacle.x = -15.0;
                }
                if obstacle.x < -15.0 {
                    obstacle.x = 15.0;
                }
            }

            for coin in &mut lane.coins {
                coin.x += coin.velocity_x;
                if coin.x > 15.0 {
                    coin.x = -15.0;
                }
                if coin.x < -15.0 {
                    coin.x = 15.0;
                }
            }

            // Check coin collisions
            let mut coins_collected = 0;
            lane.coins.retain(|coin| {
                if self.player.collides_horizontal(coin) {
                    coins_collected += 1;
                    false
                } else {
                    true
                }
            });
            self.coins += coins_collected;
        }

        // Find the lane at player's position
        let player_lane = self.lanes.iter().find(|lane| {
            let lane_idx = (lane.z / 2.0).round() as i32;
            lane_idx == player_lane_idx
        });

        if let Some(lane) = player_lane {
            if !self.moving {
                match lane.lane_type {
                    LaneType::Road => {
                        for obstacle in &lane.obstacles {
                            if self.player.collides_horizontal(obstacle) {
                                self.game_over = true;
                            }
                        }
                    }
                    LaneType::Water => {
                        let on_log = lane.obstacles.iter()
                            .any(|o| self.player.collides_horizontal(o));
                        if !on_log {
                            self.game_over = true;
                        }
                    }
                    _ => {}
                }
            }
            
            // Move player with log
            if let LaneType::Water = lane.lane_type {
                for obstacle in &lane.obstacles {
                    if self.player.collides_horizontal(obstacle) {
                        self.player.x += obstacle.velocity_x;
                    }
                }
            }
        }

        self.player.x = self.player.x.clamp(-10.0, 10.0);

        let new_score = (self.player.z / 2.0) as i32;
        if new_score > self.score {
            self.score = new_score;
        }
    }

    fn render(&self) {
        self.gl.clear_color(0.2, 0.6, 1.0, 1.0);
        self.gl.clear(WebGlRenderingContext::COLOR_BUFFER_BIT | WebGlRenderingContext::DEPTH_BUFFER_BIT);
        self.gl.enable(WebGlRenderingContext::DEPTH_TEST);
        self.gl.enable(WebGlRenderingContext::BLEND);
        self.gl.blend_func(WebGlRenderingContext::SRC_ALPHA, WebGlRenderingContext::ONE_MINUS_SRC_ALPHA);

        let canvas = self.gl.canvas().unwrap().dyn_into::<HtmlCanvasElement>().unwrap();
        let width = canvas.width();
        let height = canvas.height();
        self.gl.viewport(0, 0, width as i32, height as i32);
        
        let aspect = width as f32 / height as f32;
        let projection = Perspective3::new(aspect, 0.8, 0.1, 100.0).to_homogeneous();
        
        let zoom_offset = if self.moving {
            (self.jump_progress * std::f32::consts::PI).sin() * 0.5
        } else {
            0.0
        };

        let eye = Vector3::new(self.player.x, 15.0 + zoom_offset, self.player.z - 10.0 - zoom_offset);
        let target = Vector3::new(self.player.x, 0.0, self.player.z + 5.0);
        let up = Vector3::new(0.0, 1.0, 0.0);
        let view = Matrix4::look_at_rh(&eye.into(), &target.into(), &up);

        for lane in &self.lanes {
            match lane.lane_type {
                LaneType::Grass => {
                    // Draw grass base
                    self.draw_cube(
                        0.0, -0.5, lane.z,
                        24.0, 0.5, 2.0,
                        0.22, 0.5, 0.22,
                        &projection, &view
                    );
                    // Draw procedural grass details
                    self.draw_grass_details(lane.z, &projection, &view);
                }
                LaneType::Road => {
                    self.draw_cube(
                        0.0, -0.5, lane.z,
                        24.0, 0.5, 2.0,
                        0.3, 0.3, 0.3,
                        &projection, &view
                    );
                    self.draw_road_markings(lane.z, &projection, &view);
                }
                LaneType::Water => {
                    self.draw_cube(
                        0.0, -0.5, lane.z,
                        24.0, 0.5, 2.0,
                        0.2, 0.4, 0.8,
                        &projection, &view
                    );
                    // Add water details
                    self.draw_water_details(lane.z, &projection, &view);
                }
            }
        }

        for lane in &self.lanes {
            for obstacle in &lane.obstacles {
                self.draw_shadow(
                    obstacle.x, obstacle.z,
                    obstacle.width * 0.8, obstacle.depth * 0.8,
                    0.3,
                    &projection, &view
                );
            }
            for coin in &lane.coins {
                self.draw_shadow(
                    coin.x, coin.z,
                    coin.width * 0.6, coin.depth * 0.6,
                    0.2,
                    &projection, &view
                );
            }
        }
        
        let shadow_scale = 1.0 + (self.player.y - self.base_y) * 0.3;
        let shadow_alpha = 0.4 - (self.player.y - self.base_y) * 0.1;
        self.draw_shadow(
            self.player.x, self.player.z,
            self.player.width * shadow_scale, self.player.depth * shadow_scale,
            shadow_alpha.max(0.1),
            &projection, &view
        );

        for lane in &self.lanes {
            for obstacle in &lane.obstacles {
                if obstacle.is_car {
                    self.draw_car(
                        obstacle.x, obstacle.y, obstacle.z,
                        obstacle.width, obstacle.height, obstacle.depth,
                        obstacle.color.0, obstacle.color.1, obstacle.color.2,
                        obstacle.velocity_x,
                        &projection, &view
                    );
                } else {
                    self.draw_cube(
                        obstacle.x, obstacle.y, obstacle.z,
                        obstacle.width, obstacle.height, obstacle.depth,
                        obstacle.color.0, obstacle.color.1, obstacle.color.2,
                        &projection, &view
                    );
                }
            }
            
            for coin in &lane.coins {
                let pulse = (self.time * 5.0).sin() * 0.1 + 1.0;
                self.draw_cube(
                    coin.x, coin.y + 0.2 + (self.time * 3.0).sin() * 0.1, coin.z,
                    coin.width * pulse, coin.height * pulse, coin.depth * pulse,
                    1.0, 0.84, 0.0, // Gold
                    &projection, &view
                );
            }
        }

        let player_color = if self.game_over { (1.0, 0.2, 0.2) } else { self.player.color };
        self.draw_cube(
            self.player.x, self.player.y, self.player.z,
            self.player.width, self.player.height, self.player.depth,
            player_color.0, player_color.1, player_color.2,
            &projection, &view
        );
        
        self.gl.disable(WebGlRenderingContext::BLEND);
    }

    fn draw_grass_details(&self, z: f32, projection: &Matrix4<f32>, view: &Matrix4<f32>) {
        // Create procedural grass texture with small patches of different green shades
        let seed = (z * 100.0) as i32;
        
        // Simple pseudo-random function
        let rand = |s: i32, offset: i32| -> f32 {
            let n = ((s.wrapping_add(offset)).wrapping_mul(1103515245).wrapping_add(12345)) as u32;
            (n % 10000) as f32 / 10000.0
        };
        
        // Draw varied grass base patches for texture
        for i in 0..20 {
            let r1 = rand(seed, i * 7);
            let r2 = rand(seed, i * 13);
            let r3 = rand(seed, i * 23);
            
            let x = -11.5 + (i as f32 * 1.2) + r1 * 0.6;
            let z_offset = (r2 - 0.5) * 1.6;
            
            // Vary green shades
            let base_g = 0.45 + r3 * 0.25;
            let base_r = 0.18 + r1 * 0.12;
            
            // Grass patch
            self.draw_cube(
                x, -0.23, z + z_offset,
                0.5 + r2 * 0.3, 0.04, 0.5 + r1 * 0.3,
                base_r, base_g, 0.12,
                projection, view
            );
        }
        
        // Draw small grass blades (vertical rectangles)
        for i in 0..30 {
            let r1 = rand(seed, i * 11 + 100);
            let r2 = rand(seed, i * 17 + 100);
            let r3 = rand(seed, i * 29 + 100);
            let r4 = rand(seed, i * 37 + 100);
            
            let x = -11.0 + (i as f32 * 0.75) + r1 * 0.5;
            let z_offset = (r2 - 0.5) * 1.7;
            
            // Height variation
            let height = 0.08 + r3 * 0.12;
            
            // Color variation - different greens
            let g = 0.4 + r4 * 0.35;
            let r = 0.15 + r1 * 0.15;
            let b = 0.05 + r2 * 0.1;
            
            // Grass blade
            self.draw_cube(
                x, -0.22 + height / 2.0, z + z_offset,
                0.06, height, 0.06,
                r, g, b,
                projection, view
            );
        }
        
        // Add darker dirt/ground patches
        for i in 0..6 {
            let r1 = rand(seed, i * 43 + 200);
            let r2 = rand(seed, i * 47 + 200);
            
            let x = -10.0 + (i as f32 * 4.0) + r1 * 2.0;
            let z_offset = (r2 - 0.5) * 1.0;
            
            // Dark ground patch
            self.draw_cube(
                x, -0.24, z + z_offset,
                0.3 + r1 * 0.4, 0.02, 0.25 + r2 * 0.3,
                0.25, 0.35, 0.15,
                projection, view
            );
        }
        
        // Add flowers and small plants
        let num_flowers = ((seed.abs() % 4) + 1) as i32;
        for i in 0..num_flowers {
            let r1 = rand(seed, i * 53 + 300);
            let r2 = rand(seed, i * 59 + 300);
            let r3 = rand(seed, i * 67 + 300);
            
            let fx = -10.0 + r1 * 20.0;
            let fz = z + (r2 - 0.5) * 1.5;
            
            // Flower stem
            self.draw_cube(
                fx, -0.18, fz,
                0.03, 0.1, 0.03,
                0.15, 0.5, 0.1,
                projection, view
            );
            
            // Shadow for flower
            self.draw_shadow(fx, fz, 0.12, 0.12, 0.2, projection, view);
            
            // Flower head - different colors
            let flower_type = (r3 * 5.0) as i32;
            let (fr, fg, fb) = match flower_type {
                0 => (0.95, 0.95, 0.3),  // Yellow
                1 => (0.95, 0.4, 0.4),   // Red
                2 => (1.0, 1.0, 1.0),    // White
                3 => (0.8, 0.5, 0.9),    // Purple
                _ => (0.95, 0.6, 0.7),   // Pink
            };
            
            self.draw_cube(
                fx, -0.12, fz,
                0.1, 0.08, 0.1,
                fr, fg, fb,
                projection, view
            );
        }
        
        // Add small rocks occasionally
        if seed % 7 == 0 {
            let r1 = rand(seed, 400);
            let r2 = rand(seed, 401);
            let rx = -8.0 + r1 * 16.0;
            let rz = z + (r2 - 0.5) * 1.2;
            
            self.draw_cube(
                rx, -0.2, rz,
                0.15 + r1 * 0.1, 0.1, 0.12 + r2 * 0.08,
                0.5, 0.5, 0.48,
                projection, view
            );
            
            // Shadow for rock
            self.draw_shadow(rx, rz, 0.2 + r1 * 0.1, 0.18 + r2 * 0.08, 0.3, projection, view);
        }
        
        // Add small mushrooms rarely
        if seed % 11 == 0 {
            let r1 = rand(seed, 500);
            let r2 = rand(seed, 501);
            let mx = -6.0 + r1 * 12.0;
            let mz = z + (r2 - 0.5) * 1.0;
            
            // Stem
            self.draw_cube(
                mx, -0.2, mz,
                0.04, 0.08, 0.04,
                0.9, 0.85, 0.75,
                projection, view
            );
            // Cap
            self.draw_cube(
                mx, -0.14, mz,
                0.1, 0.05, 0.1,
                0.85, 0.2, 0.15,
                projection, view
            );
            
            // Shadow for mushroom
            self.draw_shadow(mx, mz, 0.12, 0.12, 0.2, projection, view);
        }
    }

    fn draw_road_markings(&self, z: f32, projection: &Matrix4<f32>, view: &Matrix4<f32>) {
        // Draw edge lines only (no center dashed lines)
        self.draw_cube(
            0.0, -0.24, z + 0.9,
            24.0, 0.02, 0.08,
            0.85, 0.85, 0.5,
            projection, view
        );
        self.draw_cube(
            0.0, -0.24, z - 0.9,
            24.0, 0.02, 0.08,
            0.85, 0.85, 0.5,
            projection, view
        );
    }

    fn draw_water_details(&self, z: f32, projection: &Matrix4<f32>, view: &Matrix4<f32>) {
        let seed = (z * 100.0) as i32;
        let time = self.time;
        
        let rand = |s: i32, offset: i32| -> f32 {
            let n = ((s.wrapping_add(offset)).wrapping_mul(1103515245).wrapping_add(12345)) as u32;
            (n % 10000) as f32 / 10000.0
        };
        
        // Animated water ripples/waves - move horizontally
        for i in 0..12 {
            let r1 = rand(seed, i * 7 + 600);
            let r2 = rand(seed, i * 11 + 600);
            let r3 = rand(seed, i * 17 + 600);
            
            // Wave motion - different speeds and phases for each wave
            let phase = r1 * 6.28;
            let wave_speed = 0.8 + r2 * 0.6;
            let wave_offset = (time * wave_speed + phase).sin() * 0.3;
            
            let base_x = -11.0 + (i as f32 * 2.0) + r1 * 1.0;
            let x = base_x + wave_offset;
            let z_offset = (r2 - 0.5) * 1.6 + (time * 0.5 + r3 * 6.28).sin() * 0.1;
            
            // Animated wave height
            let y_offset = (time * 1.5 + phase).sin() * 0.02;
            
            // Light blue highlight (wave crest)
            self.draw_cube(
                x, -0.23 + y_offset, z + z_offset,
                0.8 + r3 * 0.4, 0.02, 0.15,
                0.4, 0.6, 0.95,
                projection, view
            );
        }
        
        // Animated darker water patches (depth) - subtle movement
        for i in 0..8 {
            let r1 = rand(seed, i * 13 + 700);
            let r2 = rand(seed, i * 19 + 700);
            
            // Slow drift
            let drift = (time * 0.3 + r1 * 6.28).sin() * 0.2;
            
            let x = -10.0 + (i as f32 * 2.8) + r1 * 1.5 + drift;
            let z_offset = (r2 - 0.5) * 1.2 + (time * 0.4 + r2 * 6.28).cos() * 0.15;
            
            self.draw_cube(
                x, -0.24, z + z_offset,
                0.6 + r1 * 0.4, 0.01, 0.4 + r2 * 0.3,
                0.15, 0.3, 0.6,
                projection, view
            );
        }
        
        // Animated foam/bubbles - flowing movement
        for i in 0..6 {
            let r1 = rand(seed, i * 23 + 800);
            let r2 = rand(seed, i * 29 + 800);
            
            // Foam flows along edges
            let flow = (time * 0.6 + r1 * 6.28).sin() * 0.4;
            
            let x = -10.0 + (i as f32 * 4.0) + r1 * 2.0 + flow;
            let z_offset = if i % 2 == 0 { 0.85 } else { -0.85 };
            
            // Pulsing size
            let size_pulse = 1.0 + (time * 2.0 + r2 * 6.28).sin() * 0.15;
            
            // White foam
            self.draw_cube(
                x, -0.22, z + z_offset + (r2 - 0.5) * 0.1,
                (0.3 + r1 * 0.2) * size_pulse, 0.03, 0.1,
                0.85, 0.9, 0.95,
                projection, view
            );
        }
        
        // Lily pads - gentle bobbing
        if seed % 5 == 0 {
            let r1 = rand(seed, 900);
            let r2 = rand(seed, 901);
            let lx = -6.0 + r1 * 12.0;
            let lz = z + (r2 - 0.5) * 1.0;
            
            // Gentle bobbing motion
            let bob = (time * 1.2 + r1 * 6.28).sin() * 0.02;
            let sway_x = (time * 0.8 + r2 * 6.28).sin() * 0.05;
            
            // Lily pad
            self.draw_cube(
                lx + sway_x, -0.21 + bob, lz,
                0.35, 0.03, 0.35,
                0.2, 0.55, 0.25,
                projection, view
            );
            
            // Small flower on lily pad
            if seed % 10 == 0 {
                self.draw_cube(
                    lx + sway_x + 0.05, -0.15 + bob, lz,
                    0.08, 0.08, 0.08,
                    0.95, 0.7, 0.8,
                    projection, view
                );
            }
        }
    }

    fn draw_shadow(&self, x: f32, z: f32, w: f32, d: f32, alpha: f32, projection: &Matrix4<f32>, view: &Matrix4<f32>) {
        let dark = 0.05 * alpha;
        self.draw_cube(
            x, -0.24, z,
            w, 0.02, d,
            dark, dark, dark,
            projection, view
        );
    }

    fn draw_cube(&self, x: f32, y: f32, z: f32, w: f32, h: f32, d: f32, r: f32, g: f32, b: f32, projection: &Matrix4<f32>, view: &Matrix4<f32>) {
        let mesh = Mesh::cube(1.0, r, g, b);
        self.draw_mesh(&mesh, x, y, z, w, h, d, projection, view);
    }

    fn draw_car(&self, x: f32, y: f32, z: f32, w: f32, h: f32, d: f32, r: f32, g: f32, b: f32, velocity_x: f32, projection: &Matrix4<f32>, view: &Matrix4<f32>) {
        let rotation = if velocity_x >= 0.0 {
            std::f32::consts::FRAC_PI_2
        } else {
            -std::f32::consts::FRAC_PI_2
        };

        if let Some(mesh) = &self.car_mesh {
            // Use loaded mesh with config
            let (scale, rot_offset_x, rot_offset_y, rot_offset_z, pos_offset) = if let Some(ref c) = self.config {
                (c.car_model.scale, c.car_model.rotation_offset_x, c.car_model.rotation_offset_y, c.car_model.rotation_offset_z, c.car_model.position_offset_y)
            } else {
                (0.5, 0.0, 0.0, 0.0, 0.0)
            };
            
            self.draw_mesh_rotated(
                mesh, 
                x, y + pos_offset, z, 
                scale, scale, scale, 
                rot_offset_x,
                rotation + rot_offset_y, 
                rot_offset_z,
                projection, view
            );
        } else {
            // Fallback to procedural car
            let mesh = Mesh::car(r, g, b);
            self.draw_mesh_rotated(&mesh, x, y, z, w, h, d, 0.0, rotation, 0.0, projection, view);
        }
    }

    fn draw_mesh(&self, mesh: &Mesh, x: f32, y: f32, z: f32, w: f32, h: f32, d: f32, projection: &Matrix4<f32>, view: &Matrix4<f32>) {
        self.draw_mesh_internal(mesh, x, y, z, w, h, d, 0.0, 0.0, 0.0, projection, view);
    }

    fn draw_mesh_rotated(&self, mesh: &Mesh, x: f32, y: f32, z: f32, w: f32, h: f32, d: f32, rotation_x: f32, rotation_y: f32, rotation_z: f32, projection: &Matrix4<f32>, view: &Matrix4<f32>) {
        self.draw_mesh_internal(mesh, x, y, z, w, h, d, rotation_x, rotation_y, rotation_z, projection, view);
    }

    fn draw_mesh_internal(&self, mesh: &Mesh, x: f32, y: f32, z: f32, w: f32, h: f32, d: f32, rotation_x: f32, rotation_y: f32, rotation_z: f32, projection: &Matrix4<f32>, view: &Matrix4<f32>) {

        self.gl.bind_buffer(WebGlRenderingContext::ARRAY_BUFFER, Some(&self.vertex_buffer));
        unsafe {
            let vert_array = js_sys::Float32Array::view(&mesh.vertices);
            self.gl.buffer_data_with_array_buffer_view(
                WebGlRenderingContext::ARRAY_BUFFER,
                &vert_array,
                WebGlRenderingContext::STATIC_DRAW
            );
        }

        self.gl.bind_buffer(WebGlRenderingContext::ELEMENT_ARRAY_BUFFER, Some(&self.index_buffer));
        unsafe {
            let idx_array = js_sys::Uint16Array::view(&mesh.indices);
            self.gl.buffer_data_with_array_buffer_view(
                WebGlRenderingContext::ELEMENT_ARRAY_BUFFER,
                &idx_array,
                WebGlRenderingContext::STATIC_DRAW
            );
        }

        let pos_loc = self.gl.get_attrib_location(&self.program, "aPosition") as u32;
        let col_loc = self.gl.get_attrib_location(&self.program, "aColor") as u32;

        self.gl.vertex_attrib_pointer_with_i32(pos_loc, 3, WebGlRenderingContext::FLOAT, false, 24, 0);
        self.gl.enable_vertex_attrib_array(pos_loc);

        self.gl.vertex_attrib_pointer_with_i32(col_loc, 3, WebGlRenderingContext::FLOAT, false, 24, 12);
        self.gl.enable_vertex_attrib_array(col_loc);

        let model = Matrix4::new_translation(&Vector3::new(x, y, z)) *
                    Matrix4::from_euler_angles(rotation_x, rotation_y, rotation_z) *
                    Matrix4::new_nonuniform_scaling(&Vector3::new(w, h, d));
        let mvp = projection * view * model;

        let mvp_array: [f32; 16] = mvp.as_slice().try_into().unwrap();
        self.gl.uniform_matrix4fv_with_f32_array(Some(&self.mvp_location), false, &mvp_array);

        self.gl.draw_elements_with_i32(
            WebGlRenderingContext::TRIANGLES,
            mesh.indices.len() as i32,
            WebGlRenderingContext::UNSIGNED_SHORT,
            0
        );
    }

    fn move_forward(&mut self) {
        if !self.moving && !self.game_over {
            self.moving = true;
            self.target_z = self.player.z + 2.0;
            self.move_direction = 0;
        }
    }

    fn move_left(&mut self) {
        if !self.moving && !self.game_over {
            let new_target = self.player.x - 2.0;
            if new_target >= -10.0 {
                self.moving = true;
                self.target_x = new_target;
                self.move_direction = 2;
            }
        }
    }

    fn move_right(&mut self) {
        if !self.moving && !self.game_over {
            let new_target = self.player.x + 2.0;
            if new_target <= 10.0 {
                self.moving = true;
                self.target_x = new_target;
                self.move_direction = 3;
            }
        }
    }

    fn restart(&mut self) {
        self.player.x = 0.0;
        self.player.y = self.base_y;
        self.player.z = 0.0;
        self.score = 0;
        self.coins = 0;
        self.game_over = false;
        self.moving = false;
        self.jump_progress = 0.0;
        
        // New random seed for new world
        self.world_seed = (js_sys::Math::random() * 1000000.0) as u32;
        self.furthest_lane = 24;
        
        self.lanes.clear();
        for i in -5..25 {
            self.lanes.push(create_lane_procedural(i as f32 * 2.0, i, self.world_seed));
        }
    }
}

// Procedural pseudo-random number generator
fn proc_rand(seed: u32, x: i32, y: i32) -> f32 {
    let n = seed.wrapping_add((x as u32).wrapping_mul(374761393))
        .wrapping_add((y as u32).wrapping_mul(668265263));
    let n = n ^ (n >> 13);
    let n = n.wrapping_mul(1274126177);
    let n = n ^ (n >> 16);
    (n % 10000) as f32 / 10000.0
}

fn create_lane_procedural(z: f32, index: i32, world_seed: u32) -> Lane {
    // Use procedural randomness based on index and world seed
    let r = proc_rand(world_seed, index, 0);
    let abs_index = index.unsigned_abs() as usize;
    
    // First lanes are always safe
    let lane_type = if index <= 0 {
        LaneType::Grass
    } else if index < 3 {
        LaneType::Grass
    } else {
        // Procedural lane type selection
        let type_rand = proc_rand(world_seed, index, 1);
        if type_rand < 0.35 {
            LaneType::Grass
        } else if type_rand < 0.7 {
            LaneType::Road
        } else {
            LaneType::Water
        }
    };

    let mut obstacles = Vec::new();
    let mut coins = Vec::new();
    
    // Difficulty increases with distance
    let difficulty = (abs_index as f32 / 20.0).min(1.5);
    
    match lane_type {
        LaneType::Road => {
            // Number of cars based on difficulty and randomness
            let num_cars = 1 + (proc_rand(world_seed, index, 2) * (2.0 + difficulty)) as usize;
            let direction = if proc_rand(world_seed, index, 3) > 0.5 { 1.0 } else { -1.0 };
            
            // Speed increases with difficulty
            let base_speed = 0.02 + difficulty * 0.03;
            let speed_variation = proc_rand(world_seed, index, 4) * 0.02;
            let speed = base_speed + speed_variation;
            
            for i in 0..num_cars {
                let offset = proc_rand(world_seed, index, 10 + i as i32) * 6.0;
                let color_idx = ((proc_rand(world_seed, index, 20 + i as i32) * 8.0) as usize) % CAR_COLORS.len();
                let mut car = GameObject::new_car(
                    -12.0 + (i as f32 * 7.0) + offset,
                    0.5,
                    z,
                    2.0, 1.0, 1.5,
                    color_idx
                );
                car.velocity_x = speed * direction;
                obstacles.push(car);
            }

            // Chance to spawn coin on road (risky!)
            if proc_rand(world_seed, index, 15) > 0.7 {
                let coin_x = -8.0 + proc_rand(world_seed, index, 16) * 16.0;
                let coin = GameObject::new(coin_x, 0.5, z, 0.4, 0.4, 0.4, (1.0, 0.8, 0.0));
                coins.push(coin);
            }
        }
        LaneType::Water => {
            // More logs when easier (beginning), fewer when harder
            let base_logs = if abs_index < 10 { 3 } else { 2 };
            let num_logs = base_logs + (proc_rand(world_seed, index, 5) * 2.0) as usize;
            let direction = if proc_rand(world_seed, index, 6) > 0.5 { 1.0 } else { -1.0 };
            
            let base_speed = 0.015 + difficulty * 0.02;
            let speed = base_speed + proc_rand(world_seed, index, 7) * 0.01;
            
            // Log size variation
            let log_size = 3.0 + proc_rand(world_seed, index, 8) * 2.0;
            
            for i in 0..num_logs {
                let offset = proc_rand(world_seed, index, 30 + i as i32) * 4.0;
                let mut log = GameObject::new(
                    -10.0 + (i as f32 * 6.0) + offset,
                    0.3,
                    z,
                    log_size, 0.6, 1.5,
                    (0.45 + r * 0.1, 0.25 + r * 0.1, 0.1)
                );
                log.velocity_x = speed * direction;
                obstacles.push(log);

                // Chance to spawn coin on log
                if proc_rand(world_seed, index, 35 + i as i32) > 0.7 {
                    let mut coin = GameObject::new(
                        -10.0 + (i as f32 * 6.0) + offset,
                        0.8, // Higher on log
                        z,
                        0.4, 0.4, 0.4,
                        (1.0, 0.8, 0.0)
                    );
                    coin.velocity_x = speed * direction;
                    coins.push(coin);
                }
            }
        }
        LaneType::Grass => {
            // Trees and rocks procedurally placed
            let num_obstacles = (proc_rand(world_seed, index, 9) * 3.0) as usize;
            for i in 0..num_obstacles {
                let x_pos = -10.0 + proc_rand(world_seed, index, 40 + i as i32) * 20.0;
                let is_tree = proc_rand(world_seed, index, 50 + i as i32) > 0.3;
                
                if is_tree {
                    // Tree
                    let tree_height = 1.5 + proc_rand(world_seed, index, 60 + i as i32) * 1.5;
                    let tree = GameObject::new(
                        x_pos,
                        tree_height / 2.0,
                        z,
                        0.8, tree_height, 0.8,
                        (0.15 + proc_rand(world_seed, index, 70 + i as i32) * 0.1, 
                         0.4 + proc_rand(world_seed, index, 80 + i as i32) * 0.2, 
                         0.15)
                    );
                    obstacles.push(tree);
                } else {
                    // Rock
                    let rock = GameObject::new(
                        x_pos,
                        0.3,
                        z,
                        0.6 + proc_rand(world_seed, index, 90 + i as i32) * 0.4,
                        0.5,
                        0.5 + proc_rand(world_seed, index, 100 + i as i32) * 0.3,
                        (0.5, 0.5, 0.5)
                    );
                    obstacles.push(rock);
                }
            }

            // Chance to spawn coin on grass
            if proc_rand(world_seed, index, 95) > 0.6 {
                let coin_x = -9.0 + proc_rand(world_seed, index, 96) * 18.0;
                // Check collision with obstacles roughly
                let mut collides = false;
                for obs in &obstacles {
                    if (obs.x - coin_x).abs() < 1.0 {
                        collides = true;
                        break;
                    }
                }
                if !collides {
                    let coin = GameObject::new(coin_x, 0.5, z, 0.4, 0.4, 0.4, (1.0, 0.8, 0.0));
                    coins.push(coin);
                }
            }
        }
    }

    Lane { z, lane_type, obstacles, coins }
}

fn create_program(gl: &WebGlRenderingContext) -> Result<WebGlProgram, JsValue> {
    let vert_shader = compile_shader(gl, WebGlRenderingContext::VERTEX_SHADER, VERTEX_SHADER)?;
    let frag_shader = compile_shader(gl, WebGlRenderingContext::FRAGMENT_SHADER, FRAGMENT_SHADER)?;

    let program = gl.create_program().ok_or("Unable to create program")?;
    gl.attach_shader(&program, &vert_shader);
    gl.attach_shader(&program, &frag_shader);
    gl.link_program(&program);

    if gl.get_program_parameter(&program, WebGlRenderingContext::LINK_STATUS).as_bool().unwrap_or(false) {
        Ok(program)
    } else {
        Err(JsValue::from_str(&gl.get_program_info_log(&program).unwrap_or_default()))
    }
}

fn compile_shader(gl: &WebGlRenderingContext, shader_type: u32, source: &str) -> Result<web_sys::WebGlShader, JsValue> {
    let shader = gl.create_shader(shader_type).ok_or("Unable to create shader")?;
    gl.shader_source(&shader, source);
    gl.compile_shader(&shader);

    if gl.get_shader_parameter(&shader, WebGlRenderingContext::COMPILE_STATUS).as_bool().unwrap_or(false) {
        Ok(shader)
    } else {
        Err(JsValue::from_str(&gl.get_shader_info_log(&shader).unwrap_or_default()))
    }
}

thread_local! {
    static GAME: RefCell<Option<Game>> = RefCell::new(None);
}

fn start_config_reloader() {
    let f = Closure::wrap(Box::new(move || {
        wasm_bindgen_futures::spawn_local(async move {
            let window = web_sys::window().unwrap();
            let mut opts = RequestInit::new();
            opts.method("GET");
            opts.mode(RequestMode::Cors);
            
            let url = format!("/assets/config.json?t={}", js_sys::Date::now());

            if let Ok(request) = Request::new_with_str_and_init(&url, &opts) {
                if let Ok(resp_value) = JsFuture::from(window.fetch_with_request(&request)).await {
                    let resp: Response = resp_value.dyn_into().unwrap();
                    if resp.ok() {
                        if let Ok(json_promise) = resp.json() {
                            if let Ok(json) = JsFuture::from(json_promise).await {
                                if let Ok(new_config) = serde_wasm_bindgen::from_value::<AppConfig>(json) {
                                    GAME.with(|g| {
                                        if let Some(game) = g.borrow_mut().as_mut() {
                                            game.config = Some(new_config);
                                        }
                                    });
                                }
                            }
                        }
                    }
                }
            }
        });
    }) as Box<dyn FnMut()>);

    web_sys::window()
        .unwrap()
        .set_interval_with_callback_and_timeout_and_arguments_0(
            f.as_ref().unchecked_ref(),
            1000,
        )
        .unwrap();

    f.forget();
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

    let mut config: Option<AppConfig> = None;
    let mut opts = RequestInit::new();
    opts.method("GET");
    opts.mode(RequestMode::Cors);

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

    let game = Game::new(gl, car_mesh, config)?;
    GAME.with(|g| *g.borrow_mut() = Some(game));

    let closure = Closure::wrap(Box::new(move |event: KeyboardEvent| {
        event.prevent_default();
        GAME.with(|g| {
            if let Some(game) = g.borrow_mut().as_mut() {
                match event.key().as_str() {
                    " " => game.move_forward(),
                    "ArrowLeft" | "d" | "D" => game.move_left(),
                    "ArrowRight" | "a" | "A" => game.move_right(),
                    "r" | "R" => game.restart(),
                    _ => {}
                }
            }
        });
    }) as Box<dyn FnMut(_)>);

    window.add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref())?;
    closure.forget();

    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();

    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        GAME.with(|game| {
            if let Some(game) = game.borrow_mut().as_mut() {
                game.update();
                game.render();
                update_ui(game.score, game.coins, game.game_over);
            }
        });
        request_animation_frame(f.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut()>));

    request_animation_frame(g.borrow().as_ref().unwrap());

    start_config_reloader();

    Ok(())
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
