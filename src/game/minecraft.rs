use crate::engine::renderer::Renderer;
use crate::engine::mesh::Mesh;
use nalgebra::{Matrix4, Vector3, Point3};
use std::collections::HashMap;
use web_sys::WebGlTexture;
use wasm_bindgen::JsCast;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockType {
    Grass,
    Dirt,
    Stone,
    Wood,
    Leaves,
    Bedrock,
    Sand,
    Water,
}

impl BlockType {
    pub fn color(&self) -> (f32, f32, f32) {
        match self {
            BlockType::Grass => (0.2, 0.8, 0.2),
            BlockType::Dirt => (0.5, 0.3, 0.1),
            BlockType::Stone => (0.5, 0.5, 0.5),
            BlockType::Wood => (0.4, 0.2, 0.0),
            BlockType::Leaves => (0.1, 0.6, 0.1),
            BlockType::Bedrock => (0.1, 0.1, 0.1),
            BlockType::Sand => (0.76, 0.7, 0.5),
            BlockType::Water => (0.2, 0.4, 0.8),
        }
    }
}

pub struct Minecraft {
    renderer: Renderer,
    blocks: HashMap<(i32, i32, i32), BlockType>,
    player_pos: Vector3<f32>,
    player_rot: (f32, f32), // yaw, pitch
    cube_mesh: Mesh,
    top_mesh: Mesh,
    bottom_mesh: Mesh,
    side_mesh: Mesh,
    is_locked: bool,
    velocity: Vector3<f32>,
    on_ground: bool,
    selected_block_type: BlockType,
    input_state: InputState,
    
    // Textures
    grass_top_texture: Option<WebGlTexture>,
    grass_side_texture: Option<WebGlTexture>,
    dirt_texture: Option<WebGlTexture>,
    leaves_texture: Option<WebGlTexture>,
    stone_texture: Option<WebGlTexture>,
    wood_side_texture: Option<WebGlTexture>,
    wood_top_texture: Option<WebGlTexture>,
    bedrock_texture: Option<WebGlTexture>,
    sand_texture: Option<WebGlTexture>,
    water_texture: Option<WebGlTexture>,
    skybox_texture: Option<WebGlTexture>,
    sun_texture: Option<WebGlTexture>,
    moon_texture: Option<WebGlTexture>,
    time_of_day: f32,
}

struct InputState {
    forward: bool,
    backward: bool,
    left: bool,
    right: bool,
}

impl Minecraft {
    pub fn new(renderer: Renderer) -> Self {
        let mut blocks = HashMap::new();
        let cube_mesh = Mesh::cube(1.0, 1.0, 1.0, 1.0);
        let top_mesh = Mesh::face_top(1.0);
        let bottom_mesh = Mesh::face_bottom(1.0);
        let side_mesh = Mesh::face_sides(1.0);

        // Load textures
        let grass_top_texture = renderer.create_texture("assets/textures/TinyCraft/tiles/grass_top.png").ok();
        let grass_side_texture = renderer.create_texture("assets/textures/TinyCraft/tiles/dirt_grass.png").ok();
        let dirt_texture = renderer.create_texture("assets/textures/TinyCraft/tiles/dirt.png").ok();
        let leaves_texture = renderer.create_texture("assets/textures/TinyCraft/tiles/leaves_transparent.png").ok();
        let stone_texture = renderer.create_texture("assets/textures/TinyCraft/tiles/stone.png").ok();
        let wood_side_texture = renderer.create_texture("assets/textures/TinyCraft/tiles/trunk_side.png").ok();
        let wood_top_texture = renderer.create_texture("assets/textures/TinyCraft/tiles/trunk_top.png").ok();
        let bedrock_texture = renderer.create_texture("assets/textures/TinyCraft/tiles/greystone.png").ok();
        let sand_texture = renderer.create_texture("assets/textures/TinyCraft/tiles/sand.png").ok();
        let water_texture = renderer.create_texture("assets/textures/TinyCraft/tiles/water.png").ok();
        
        // Converted from EXR to JPG for browser compatibility
        let skybox_texture = renderer.create_texture("assets/textures/cloudy_bright_day.jpg").ok();
        let sun_texture = renderer.create_texture("assets/textures/TinyCraft/Sky/sun.png").ok();
        let moon_texture = renderer.create_texture("assets/textures/2k_moon.jpg").ok();

        // Generate terrain with mountains and lakes
        let size = 32;
        let water_level = 3;
        
        for x in -size..size {
            for z in -size..size {
                // Simple heightmap generation
                let h_base = (
                    (x as f32 * 0.1).sin() * 2.0 + 
                    (z as f32 * 0.15).cos() * 2.0 +
                    ((x as f32 * 0.3).sin() * (z as f32 * 0.3).cos()) * 1.0
                );
                
                // Add some "mountains"
                let mountain = if h_base > 2.0 {
                    (h_base - 2.0).powf(1.5) * 2.0
                } else {
                    0.0
                };
                
                let h = (h_base + mountain).round() as i32 + 4; // Base height shifted up

                blocks.insert((x, 0, z), BlockType::Bedrock);
                
                for y in 1..=h {
                    if y == h {
                        if y > 8 {
                            blocks.insert((x, y, z), BlockType::Stone); // Mountain peaks
                        } else if y <= water_level + 1 {
                            blocks.insert((x, y, z), BlockType::Sand); // Beach
                        } else {
                            blocks.insert((x, y, z), BlockType::Grass);
                        }
                    } else if y > h - 3 {
                        blocks.insert((x, y, z), BlockType::Dirt);
                    } else {
                        blocks.insert((x, y, z), BlockType::Stone);
                    }
                }
                
                // Water
                if h < water_level {
                    for y in (h+1)..=water_level {
                        blocks.insert((x, y, z), BlockType::Water);
                    }
                }
            }
        }

        // Trees
        // Simple pseudo-random placement
        for x in -size..size {
            for z in -size..size {
                if (x * x * z * z + x * 7 + z * 3) % 100 < 2 { // 2% chance
                    // Find ground height
                    let mut y = 20;
                    while y > 0 {
                        if let Some(block) = blocks.get(&(x, y, z)) {
                            if *block == BlockType::Grass {
                                // Plant tree
                                for ty in 1..5 {
                                    blocks.insert((x, y + ty, z), BlockType::Wood);
                                }
                                // Leaves
                                for lx in -2i32..=2 {
                                    for lz in -2i32..=2 {
                                        for ly in 3..5 {
                                            if lx.abs() == 2 && lz.abs() == 2 { continue; } // Round corners
                                            if lx == 0 && lz == 0 && ly < 5 { continue; } // Trunk space
                                            blocks.insert((x + lx, y + ly, z), BlockType::Leaves);
                                        }
                                    }
                                }
                                // Top leaves
                                for lx in -1i32..=1 {
                                    for lz in -1i32..=1 {
                                        if lx.abs() == 1 && lz.abs() == 1 { continue; } // Cross shape
                                        if lx == 0 && lz == 0 { continue; } // Trunk space
                                        blocks.insert((x + lx, y + 5, z), BlockType::Leaves);
                                    }
                                }
                                // Cap the top
                                blocks.insert((x, y + 6, z), BlockType::Leaves);
                                
                                break;
                            } else if *block == BlockType::Water || *block == BlockType::Stone || *block == BlockType::Sand {
                                break; // Don't plant on water, stone or sand
                            }
                        }
                        y -= 1;
                    }
                }
            }
        }

        Minecraft {
            renderer,
            blocks,
            player_pos: Vector3::new(0.0, 10.0, 0.0), // Start higher
            player_rot: (0.0, 0.0),
            cube_mesh,
            top_mesh,
            bottom_mesh,
            side_mesh,
            is_locked: false,
            velocity: Vector3::new(0.0, 0.0, 0.0),
            on_ground: false,
            selected_block_type: BlockType::Grass,
            input_state: InputState {
                forward: false,
                backward: false,
                left: false,
                right: false,
            },
            grass_top_texture,
            grass_side_texture,
            dirt_texture,
            leaves_texture,
            stone_texture,
            wood_side_texture,
            wood_top_texture,
            bedrock_texture,
            sand_texture,
            water_texture,
            skybox_texture,
            sun_texture,
            moon_texture,
            time_of_day: 0.5,
        }
    }

    pub fn update(&mut self) {
        let speed = 0.02;
        let max_speed = 0.15;
        
        let (yaw, _) = self.player_rot;
        let forward = Vector3::new(yaw.cos(), 0.0, yaw.sin()).normalize();
        let right = Vector3::new(-yaw.sin(), 0.0, yaw.cos()).normalize();
        
        let mut move_dir = Vector3::new(0.0, 0.0, 0.0);
        if self.input_state.forward { move_dir += forward; }
        if self.input_state.backward { move_dir -= forward; }
        if self.input_state.right { move_dir += right; }
        if self.input_state.left { move_dir -= right; }

        if move_dir.norm() > 0.0 {
            move_dir = move_dir.normalize();
            self.velocity.x += move_dir.x * speed;
            self.velocity.z += move_dir.z * speed;
        }

        let h_vel = Vector3::new(self.velocity.x, 0.0, self.velocity.z);
        if h_vel.norm() > max_speed {
            let clamped = h_vel.normalize() * max_speed;
            self.velocity.x = clamped.x;
            self.velocity.z = clamped.z;
        }

        self.velocity.y -= 0.02;

        self.player_pos.x += self.velocity.x;
        self.resolve_collisions(0); 
        
        self.player_pos.z += self.velocity.z;
        self.resolve_collisions(2); 

        self.player_pos.y += self.velocity.y;
        self.on_ground = false;
        self.resolve_collisions(1); 

        self.velocity.x *= 0.8;
        self.velocity.z *= 0.8;

        self.update_time_ui();
    }

    fn update_time_ui(&mut self) {
        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                if let Some(element) = document.get_element_by_id("time-slider") {
                    if let Ok(input) = element.dyn_into::<web_sys::HtmlInputElement>() {
                        // Check if user is interacting (active element)
                        let is_active = if let Some(active) = document.active_element() {
                            &active == input.as_ref() as &web_sys::Element
                        } else {
                            false
                        };

                        if is_active {
                            // User is dragging, read value
                            let val_str = input.value();
                            if let Ok(val) = val_str.parse::<f32>() {
                                self.time_of_day = val;
                            }
                        } else {
                            // Game is running, update slider
                            // Auto-increment time
                            // self.time_of_day += 0.0001;
                            if self.time_of_day > 1.0 { self.time_of_day -= 1.0; }
                            
                            input.set_value(&self.time_of_day.to_string());
                        }
                    }
                }
            }
        }
    }

    fn resolve_collisions(&mut self, axis: usize) {
        let px = self.player_pos.x.round() as i32;
        let py = self.player_pos.y.round() as i32;
        let pz = self.player_pos.z.round() as i32;

        for y in (py - 2)..=(py + 2) {
            for x in (px - 1)..=(px + 1) {
                for z in (pz - 1)..=(pz + 1) {
                    if self.blocks.contains_key(&(x, y, z)) {
                        // Skip collision for water
                        if let Some(block) = self.blocks.get(&(x, y, z)) {
                            if *block == BlockType::Water {
                                continue;
                            }
                        }

                        let block_min = Vector3::new(x as f32 - 0.5, y as f32 - 0.5, z as f32 - 0.5);
                        let block_max = Vector3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);

                        let player_width = 0.6;
                        let _player_height = 1.8;
                        let player_min = Vector3::new(
                            self.player_pos.x - player_width / 2.0,
                            self.player_pos.y - 1.5,
                            self.player_pos.z - player_width / 2.0
                        );
                        let player_max = Vector3::new(
                            self.player_pos.x + player_width / 2.0,
                            self.player_pos.y + 0.3,
                            self.player_pos.z + player_width / 2.0
                        );

                        if self.aabb_intersect(player_min, player_max, block_min, block_max) {
                            match axis {
                                0 => { 
                                    if self.velocity.x > 0.0 {
                                        self.player_pos.x = block_min.x - player_width / 2.0 - 0.001;
                                    } else if self.velocity.x < 0.0 {
                                        self.player_pos.x = block_max.x + player_width / 2.0 + 0.001;
                                    }
                                    self.velocity.x = 0.0;
                                },
                                1 => { 
                                    if self.velocity.y > 0.0 {
                                        self.player_pos.y = block_min.y - 0.3 - 0.001;
                                        self.velocity.y = 0.0;
                                    } else if self.velocity.y < 0.0 {
                                        self.player_pos.y = block_max.y + 1.5; 
                                        self.velocity.y = 0.0;
                                        self.on_ground = true;
                                    }
                                },
                                2 => { 
                                    if self.velocity.z > 0.0 {
                                        self.player_pos.z = block_min.z - player_width / 2.0 - 0.001;
                                    } else if self.velocity.z < 0.0 {
                                        self.player_pos.z = block_max.z + player_width / 2.0 + 0.001;
                                    }
                                    self.velocity.z = 0.0;
                                },
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }

    fn aabb_intersect(&self, min1: Vector3<f32>, max1: Vector3<f32>, min2: Vector3<f32>, max2: Vector3<f32>) -> bool {
        min1.x < max2.x && max1.x > min2.x &&
        min1.y < max2.y && max1.y > min2.y &&
        min1.z < max2.z && max1.z > min2.z
    }

    fn calculate_shadow(&self, x: i32, y: i32, z: i32, light_dir: Vector3<f32>) -> f32 {
        // Start slightly above the top face center to avoid self-shadowing from the block itself
        // and to avoid shadowing from neighbor ground blocks when sun is low.
        let origin = Vector3::new(x as f32, y as f32 + 0.6, z as f32);
        let mut ray_pos = origin;
        
        let max_steps = 100;
        let step_size = 0.2;
        
        for _ in 0..max_steps {
            // Step first
            ray_pos += light_dir * step_size;
            
            let check_x = ray_pos.x.round() as i32;
            let check_y = ray_pos.y.round() as i32;
            let check_z = ray_pos.z.round() as i32;
            
            // Removed the column check to allow shadows from blocks directly above (e.g. trees at noon)
            // if check_x == x && check_z == z {
            //    continue;
            // }

            if let Some(block) = self.blocks.get(&(check_x, check_y, check_z)) {
                if matches!(block, BlockType::Leaves) {
                    return 0.6; 
                } else {
                    return 0.3; 
                }
            }
            
            if ray_pos.y > 20.0 { break; } 
        }
        
        1.0 
    }

    pub fn render(&mut self, width: i32, height: i32) {
        self.renderer.resize(width, height);
        self.renderer.clear_screen(0.5, 0.7, 1.0); // Sky blue
        self.renderer.enable_depth_test();
        self.renderer.enable_face_culling();

        let aspect = width as f32 / height as f32;
        let projection = Matrix4::new_perspective(aspect, 45.0f32.to_radians(), 0.1, 500.0);
        
        // Camera view matrix
        let (yaw, pitch) = self.player_rot;
        let front = Vector3::new(
            yaw.cos() * pitch.cos(),
            pitch.sin(),
            yaw.sin() * pitch.cos()
        ).normalize();
        
        let target = self.player_pos + front;
        let view = Matrix4::look_at_rh(
            &Point3::from(self.player_pos),
            &Point3::from(target),
            &Vector3::y(),
        );

        // Draw Skybox
        self.renderer.draw_skybox(&self.cube_mesh, &projection, &view, self.skybox_texture.as_ref());
        self.renderer.gl.depth_mask(true); // Re-enable depth writing

        // Calculate Sun Position again for shadows
        let sun_angle = (self.time_of_day - 0.25) * std::f32::consts::PI * 2.0;
        let sun_dist = 100.0;
        
        // Sun position for rendering (relative to player so it's always visible)
        let sun_pos = self.player_pos + Vector3::new(sun_angle.cos() * sun_dist, sun_angle.sin() * sun_dist, 0.0);
        let moon_pos = self.player_pos + Vector3::new(-sun_angle.cos() * sun_dist, -sun_angle.sin() * sun_dist, 0.0);
        
        // Light direction for shadows (Global, independent of player)
        let light_dir = Vector3::new(sun_angle.cos(), sun_angle.sin(), 0.0).normalize();
        
        // Light position for shader (Far away to simulate directional light)
        let light_pos_uniform = light_dir * 10000.0; 

        // Draw Sun
        if self.sun_texture.is_some() {
            self.renderer.draw_textured_cube(sun_pos.x, sun_pos.y, sun_pos.z, 8.0, 8.0, 8.0, self.sun_texture.as_ref(), &projection, &view);
        }

        // Draw Moon
        if self.moon_texture.is_some() {
            self.renderer.draw_textured_cube(moon_pos.x, moon_pos.y, moon_pos.z, 6.0, 6.0, 6.0, self.moon_texture.as_ref(), &projection, &view);
        }

        // Collect instance data grouped by block type
        let mut instance_data_map: HashMap<BlockType, Vec<f32>> = HashMap::new();
        let mut count_map: HashMap<BlockType, i32> = HashMap::new();

        for ((x, y, z), block_type) in &self.blocks {
            let (r, g, b) = (1.0, 1.0, 1.0); // Use white for all blocks as they are all textured now
            
            // Shadow logic: Raycast to sun
            let light_level = self.calculate_shadow(*x, *y, *z, light_dir);

            let data = instance_data_map.entry(*block_type).or_insert(Vec::new());
            data.extend_from_slice(&[
                *x as f32, *y as f32, *z as f32, // Position
                1.0, // Scale
                r, g, b, // Color
                light_level // Light level
            ]);
            *count_map.entry(*block_type).or_insert(0) += 1;
        }

        // Draw each group
        // Separate water to draw last for transparency
        let mut water_data: Option<Vec<f32>> = None;
        let mut water_count = 0;

        for (block_type, data) in instance_data_map {
            if block_type == BlockType::Water {
                water_data = Some(data);
                water_count = count_map[&block_type];
                continue;
            }

            let count = count_map[&block_type];
            
            match block_type {
                BlockType::Grass => {
                    // Top
                    self.renderer.draw_instanced_mesh(
                        &self.top_mesh, &data, count, &projection, &view, &light_pos_uniform, self.grass_top_texture.as_ref()
                    );
                    // Bottom
                    self.renderer.draw_instanced_mesh(
                        &self.bottom_mesh, &data, count, &projection, &view, &light_pos_uniform, self.dirt_texture.as_ref()
                    );
                    // Sides
                    self.renderer.draw_instanced_mesh(
                        &self.side_mesh, &data, count, &projection, &view, &light_pos_uniform, self.grass_side_texture.as_ref()
                    );
                },
                BlockType::Wood => {
                    // Top & Bottom
                    self.renderer.draw_instanced_mesh(
                        &self.top_mesh, &data, count, &projection, &view, &light_pos_uniform, self.wood_top_texture.as_ref()
                    );
                    self.renderer.draw_instanced_mesh(
                        &self.bottom_mesh, &data, count, &projection, &view, &light_pos_uniform, self.wood_top_texture.as_ref()
                    );
                    // Sides
                    self.renderer.draw_instanced_mesh(
                        &self.side_mesh, &data, count, &projection, &view, &light_pos_uniform, self.wood_side_texture.as_ref()
                    );
                },
                _ => {
                    let texture = match block_type {
                        BlockType::Dirt => self.dirt_texture.as_ref(),
                        BlockType::Leaves => self.leaves_texture.as_ref(),
                        BlockType::Stone => self.stone_texture.as_ref(),
                        BlockType::Bedrock => self.bedrock_texture.as_ref(),
                        BlockType::Sand => self.sand_texture.as_ref(),
                        // Water handled separately
                        _ => None,
                    };
                    
                    self.renderer.draw_instanced_mesh(
                        &self.cube_mesh, &data, count, &projection, &view, &light_pos_uniform, texture
                    );
                }
            }
        }

        // Draw Water Last
        if let Some(data) = water_data {
            self.renderer.gl.enable(web_sys::WebGlRenderingContext::BLEND);
            self.renderer.gl.blend_func(web_sys::WebGlRenderingContext::SRC_ALPHA, web_sys::WebGlRenderingContext::ONE_MINUS_SRC_ALPHA);
            
            // Set global alpha for water
            self.renderer.gl.uniform1f(Some(&self.renderer.u_instanced_global_alpha_loc), 0.6);

            self.renderer.draw_instanced_mesh(
                &self.cube_mesh, &data, water_count, &projection, &view, &light_pos_uniform, self.water_texture.as_ref()
            );
            
            // Reset global alpha
            self.renderer.gl.uniform1f(Some(&self.renderer.u_instanced_global_alpha_loc), 1.0);
            self.renderer.gl.disable(web_sys::WebGlRenderingContext::BLEND);
        }
        
        // Render selection highlight (raycast)
        if let Some((_bx, _by, _bz, _face)) = self.raycast() {
             // Draw a wireframe or slightly larger transparent cube
             // For now, just draw a marker
        }
    }

    pub fn handle_input(&mut self, key: &str) {
        match key {
            "w" | "W" => self.input_state.forward = true,
            "s" | "S" => self.input_state.backward = true,
            "a" | "A" => self.input_state.left = true,
            "d" | "D" => self.input_state.right = true,
            " " => {
                if self.on_ground {
                    self.velocity.y = 0.4;
                }
            },
            "1" => { self.selected_block_type = BlockType::Grass; self.update_block_ui(); },
            "2" => { self.selected_block_type = BlockType::Dirt; self.update_block_ui(); },
            "3" => { self.selected_block_type = BlockType::Stone; self.update_block_ui(); },
            "4" => { self.selected_block_type = BlockType::Wood; self.update_block_ui(); },
            "5" => { self.selected_block_type = BlockType::Leaves; self.update_block_ui(); },
            _ => {}
        }
    }

    fn update_block_ui(&self) {
        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                let selected_index = match self.selected_block_type {
                    BlockType::Grass => 1,
                    BlockType::Dirt => 2,
                    BlockType::Stone => 3,
                    BlockType::Wood => 4,
                    BlockType::Leaves => 5,
                    _ => 1,
                };

                for i in 1..=5 {
                    if let Some(element) = document.get_element_by_id(&format!("slot-{}", i)) {
                        let class_name = if i == selected_index {
                            "hotbar-slot selected"
                        } else {
                            "hotbar-slot"
                        };
                        element.set_class_name(class_name);
                    }
                }
            }
        }
    }

    pub fn handle_keyup(&mut self, key: &str) {
        match key {
            "w" | "W" => self.input_state.forward = false,
            "s" | "S" => self.input_state.backward = false,
            "a" | "A" => self.input_state.left = false,
            "d" | "D" => self.input_state.right = false,
            _ => {}
        }
    }

    pub fn set_locked(&mut self, locked: bool) {
        self.is_locked = locked;
    }

    pub fn handle_mouse_move(&mut self, dx: i32, dy: i32) {
        if self.is_locked {
            let sensitivity = 0.005;
            self.player_rot.0 += dx as f32 * sensitivity; // Yaw (Inverted from -= to +=)
            self.player_rot.1 -= dy as f32 * sensitivity; // Pitch
            
            // Clamp pitch
            self.player_rot.1 = self.player_rot.1.max(-1.5).min(1.5);
        }
    }

    pub fn handle_mouse_down(&mut self, _x: i32, _y: i32, button: i32) {
        if !self.is_locked {
            self.is_locked = true;
            // Request pointer lock in JS side ideally
            return;
        }

        if let Some((bx, by, bz, face)) = self.raycast() {
            if button == 0 { // Left click: Break
                self.blocks.remove(&(bx, by, bz));
            } else if button == 2 { // Right click: Place
                let (nx, ny, nz) = match face {
                    0 => (bx + 1, by, bz),
                    1 => (bx - 1, by, bz),
                    2 => (bx, by + 1, bz),
                    3 => (bx, by - 1, bz),
                    4 => (bx, by, bz + 1),
                    5 => (bx, by, bz - 1),
                    _ => (bx, by, bz),
                };
                // Don't place inside player
                let block_center = Vector3::new(nx as f32, ny as f32, nz as f32);
                if (self.player_pos - block_center).norm() > 1.5 {
                    self.blocks.insert((nx, ny, nz), self.selected_block_type);
                }
            }
        }
    }
    
    fn raycast(&self) -> Option<(i32, i32, i32, usize)> {
        let (yaw, pitch) = self.player_rot;
        let dir = Vector3::new(
            yaw.cos() * pitch.cos(),
            pitch.sin(),
            yaw.sin() * pitch.cos()
        ).normalize();
        
        let mut t = 0.0;
        let step = 0.1;
        let max_dist = 5.0;
        
        while t < max_dist {
            let pos = self.player_pos + dir * t;
            let bx = pos.x.round() as i32;
            let by = pos.y.round() as i32;
            let bz = pos.z.round() as i32;
            
            if self.blocks.contains_key(&(bx, by, bz)) {
                // Determine face (very simple approximation)
                // A better way is to use a proper DDA algorithm for voxel raycasting
                // But for now, let's just return the block.
                // To get the face, we can check the previous position
                let prev_pos = self.player_pos + dir * (t - step);
                let pbx = prev_pos.x.round() as i32;
                let pby = prev_pos.y.round() as i32;
                let pbz = prev_pos.z.round() as i32;
                
                let face = if pbx > bx { 0 } else if pbx < bx { 1 }
                           else if pby > by { 2 } else if pby < by { 3 }
                           else if pbz > bz { 4 } else { 5 };
                           
                return Some((bx, by, bz, face));
            }
            t += step;
        }
        None
    }
}
