use crate::engine::renderer::Renderer;
use crate::engine::mesh::Mesh;
use nalgebra::{Matrix4, Point3, Vector3, Vector4};
use js_sys::Date;
use web_sys::{HtmlElement, WebGlTexture};
use wasm_bindgen::JsCast;
use rand::Rng;

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
    pub longitude_of_ascending_node: f32,
    pub argument_of_periapsis: f32,
    pub last_trail_angle: f32,
    pub eccentricity: f32,
    pub mass: String,
    pub temperature: f32,
    pub description: String,
    pub ring_texture: Option<WebGlTexture>,
    pub ring_radius: f32,
    pub ring_inner_radius: Option<f32>,
    pub is_frozen: bool,
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
    asteroid_mesh: Mesh,
    ring_mesh: Mesh,
    is_black_hole: bool,
    sun_texture: Option<WebGlTexture>,
    use_celsius: bool,
}

impl SolarSystem {
    pub fn new(renderer: Renderer, is_black_hole_mode: bool) -> Self {
        let mut bodies = Vec::new();
        let sphere_mesh = Mesh::sphere(1.0, 20, 20, 1.0, 1.0, 1.0);
        let asteroid_mesh = Mesh::sphere(1.0, 6, 6, 1.0, 1.0, 1.0);
        let ring_mesh = Mesh::quad(2.0, 2.0);
        
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
        
        // Clear existing labels
        if let Some(container) = &labels_container {
            container.set_inner_html("");
        }

        // Update sidebar list first item name
        if let Ok(Some(list)) = document.query_selector(".body-list") {
            if let Some(first_child) = list.first_element_child() {
                first_child.set_text_content(Some(if is_black_hole_mode { "Black Hole" } else { "Sun" }));
            }
        }

        let create_body = |name: &str, radius: f32, orbit_radius: f32, orbit_speed: f32, orbit_angle: f32, color: (f32, f32, f32), parent: Option<usize>, mesh_fn: fn(f32, u16, u16, f32, f32, f32) -> Mesh, texture_url: Option<&str>, night_texture_url: Option<&str>, cloud_texture_url: Option<&str>, ring_texture_url: Option<&str>, ring_radius: f32, rotation_period: f32, axial_tilt: f32, orbit_inclination: f32, longitude_of_ascending_node: f32, argument_of_periapsis: f32, eccentricity: f32, mass: &str, temperature: f32, description: &str, ring_inner_radius: Option<f32>| {
            let mut label_element = None;
            if let Some(container) = &labels_container {
                if !name.starts_with("Asteroid") && !name.starts_with("Kuiper") && !name.starts_with("Oort") {
                    let el = document.create_element("div").unwrap();
                    el.set_class_name("solar-label");
                    el.set_text_content(Some(name));
                    container.append_child(&el).unwrap();
                    if let Ok(html_el) = el.dyn_into::<HtmlElement>() {
                        label_element = Some(html_el);
                    }
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

            let ring_texture = if let Some(url) = ring_texture_url {
                match renderer.create_texture(url) {
                    Ok(t) => Some(t),
                    Err(e) => {
                        web_sys::console::error_1(&format!("Failed to create ring texture for {}: {:?}", name, e).into());
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

            let (slices, stacks) = if name.starts_with("Asteroid") || name.starts_with("Kuiper") || name.starts_with("Oort") {
                (6, 6)
            } else {
                (40, 40)
            };

            let (final_temp, is_frozen) = if is_black_hole_mode && name != "Black Hole" {
                (30.0, true)
            } else {
                (temperature, false)
            };

            Body {
                mesh: mesh_fn(1.0, slices, stacks, mesh_r, mesh_g, mesh_b),
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
                longitude_of_ascending_node: longitude_of_ascending_node.to_radians(),
                argument_of_periapsis: argument_of_periapsis.to_radians(),
                last_trail_angle: orbit_angle,
                eccentricity,
                mass: mass.to_string(),
                temperature: final_temp,
                description: description.to_string(),
                ring_texture,
                ring_radius,
                ring_inner_radius,
                is_frozen,
            }
        };



        if is_black_hole_mode {
            // 3km radius. Earth (6371km) is 0.0042.
            // 3km = 3 * (0.0042 / 6371) = 0.0000019777
            let bh_radius = 0.0000019777;
            bodies.push(create_body("Black Hole", bh_radius, 0.0, 0.0, 0.0, (0.0, 0.0, 0.0), None, Mesh::sphere, None, None, None, None, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, "1.989 × 10^30 kg", 0.0, "A black hole with the same mass as the Sun. Event Horizon: 3km.", None));
        } else {
            bodies.push(create_body("Sun", 0.465, 0.0, 0.0, 0.0, (1.0, 1.0, 0.0), None, Mesh::sphere, Some("assets/textures/2k_sun.jpg"), None, None, None, 0.0, 25.0, 7.25, 0.0, 0.0, 0.0, 0.0, "1.989 × 10^30 kg", 5778.0, "The star at the center of our Solar System.", None));
        }

        let p_mercury = 87.969;

        bodies.push(create_body("Mercury", 0.0016, 39.0, get_orbit_speed(p_mercury), get_initial_angle(252.25, p_mercury), (0.5, 0.5, 0.5), Some(0), Mesh::sphere, Some("assets/textures/2k_mercury.jpg"), None, None, None, 0.0, 58.6, 0.03, 7.0, 0.0, 0.0, 0.205, "3.285 × 10^23 kg", 440.0, "The smallest planet in the Solar System and the closest to the Sun.", None));

        let p_venus = 224.701;

        bodies.push(create_body("Venus", 0.004, 72.0, get_orbit_speed(p_venus), get_initial_angle(181.98, p_venus), (0.9, 0.7, 0.2), Some(0), Mesh::sphere, Some("assets/textures/2k_venus_surface.jpg"), None, Some("assets/textures/2k_venus_atmosphere.jpg"), None, 0.0, -243.0, 177.3, 3.4, 0.0, 0.0, 0.007, "4.867 × 10^24 kg", 737.0, "The second planet from the Sun. It has a dense atmosphere.", None));

        let p_earth = 365.256;

        if is_black_hole_mode {
            bodies.push(create_body("Earth", 0.0042, 100.0, get_orbit_speed(p_earth), get_initial_angle(100.46, p_earth), (0.8, 0.9, 1.0), Some(0), Mesh::sphere, Some("assets/textures/2k_earth_daymap.jpg"), None, None, None, 0.0, 1.0, 23.4, 0.0, 0.0, 0.0, 0.017, "5.972 × 10^24 kg", 30.0, "A frozen wasteland orbiting a black hole.", None));
        } else {
            bodies.push(create_body("Earth", 0.0042, 100.0, get_orbit_speed(p_earth), get_initial_angle(100.46, p_earth), (0.0, 0.0, 1.0), Some(0), Mesh::sphere, Some("assets/textures/2k_earth_daymap.jpg"), Some("assets/textures/2k_earth_nightmap.jpg"), Some("assets/textures/2k_earth_clouds.jpg"), None, 0.0, 1.0, 23.4, 0.0, 0.0, 0.0, 0.017, "5.972 × 10^24 kg", 288.0, "Our home planet, the third from the Sun.", None));
        }

        let p_moon = 27.322;

        bodies.push(create_body("Moon", 0.0011, 0.257, get_orbit_speed(p_moon), get_initial_angle(0.0, p_moon), (0.6, 0.6, 0.6), Some(3), Mesh::sphere, Some("assets/textures/2k_moon.jpg"), None, None, None, 0.0, 27.3, 6.7, 5.1, 0.0, 0.0, 0.055, "7.342 × 10^22 kg", 220.0, "Earth's only natural satellite.", None));

        let p_mars = 686.980;

        bodies.push(create_body("Mars", 0.0022, 152.0, get_orbit_speed(p_mars), get_initial_angle(355.45, p_mars), (1.0, 0.0, 0.0), Some(0), Mesh::sphere, Some("assets/textures/2k_mars.jpg"), None, None, None, 0.0, 1.03, 25.2, 1.85, 0.0, 0.0, 0.094, "6.39 × 10^23 kg", 210.0, "The fourth planet from the Sun, known as the Red Planet.", None));
        let mars_idx = bodies.len() - 1;

        // Mars Moons
        bodies.push(create_body("Phobos", 0.00008, 0.006, get_orbit_speed(0.3189), get_initial_angle(0.0, 0.3189), (0.6, 0.5, 0.4), Some(mars_idx), Mesh::sphere, Some("assets/textures/phobos.webp"), None, None, None, 0.0, 0.3189, 0.0, 1.0, 0.0, 0.0, 0.015, "1.06 × 10^16 kg", 233.0, "The larger and inner of the two natural satellites of Mars.", None));
        bodies.push(create_body("Deimos", 0.00004, 0.015, get_orbit_speed(1.262), get_initial_angle(0.0, 1.262), (0.7, 0.6, 0.5), Some(mars_idx), Mesh::sphere, Some("assets/textures/deimos.webp"), None, None, None, 0.0, 1.262, 0.0, 0.9, 0.0, 0.0, 0.0002, "1.47 × 10^15 kg", 233.0, "The smaller and outer of the two natural satellites of Mars.", None));


        let p_ceres = 1681.6;
        bodies.push(create_body("Ceres", 0.00029, 277.0, get_orbit_speed(p_ceres), get_initial_angle(0.0, p_ceres), (0.4, 0.4, 0.4), Some(0), Mesh::sphere, Some("assets/textures/2k_ceres_fictional.jpg"), None, None, None, 0.0, 0.375, 4.0, 10.6, 0.0, 0.0, 0.076, "9.393 × 10^20 kg", 168.0, "The largest object in the asteroid belt.", None));

        let mut rng = rand::thread_rng();
        for i in 0..1500 {
            let angle: f32 = rng.gen_range(0.0..360.0);
            let dist: f32 = rng.gen_range(220.0..320.0);
            let size: f32 = rng.gen_range(0.00001..0.00005);
            let period = (dist / 100.0).powf(1.5) * 365.256;
            
            bodies.push(create_body(
                &format!("Asteroid {}", i),
                size,
                dist,
                get_orbit_speed(period),
                angle.to_radians(),
                (0.5, 0.5, 0.5),
                Some(0),
                Mesh::sphere,
                None,
                None,
                None,
                None,
                0.0,
                rng.gen_range(5.0..20.0),
                rng.gen_range(0.0..30.0),
                rng.gen_range(-10.0..10.0),
                rng.gen_range(0.0..360.0),
                rng.gen_range(0.0..360.0),
                rng.gen_range(0.0..0.2),
                "Unknown",
                150.0,
                "Asteroid Belt Object",
                None
            ));
        }

        let p_jupiter = 4332.589;

        bodies.push(create_body("Jupiter", 0.047, 520.0, get_orbit_speed(p_jupiter), get_initial_angle(34.40, p_jupiter), (0.8, 0.6, 0.4), Some(0), Mesh::sphere, Some("assets/textures/2k_jupiter.jpg"), None, None, None, 0.0, 0.41, 3.1, 1.3, 0.0, 0.0, 0.049, "1.898 × 10^27 kg", 165.0, "The largest planet in the Solar System.", None));
        let jupiter_idx = bodies.len() - 1;

        // Jupiter Moons
        bodies.push(create_body("Io", 0.0012, 0.28, get_orbit_speed(1.769), get_initial_angle(0.0, 1.769), (0.8, 0.7, 0.2), Some(jupiter_idx), Mesh::sphere, Some("assets/textures/io.webp"), None, None, None, 0.0, 1.769, 0.0, 0.0, 0.0, 0.0, 0.004, "8.93 × 10^22 kg", 110.0, "Jupiter's innermost Galilean moon.", None));
        bodies.push(create_body("Europa", 0.0010, 0.45, get_orbit_speed(3.55), get_initial_angle(0.0, 3.55), (0.9, 0.9, 0.8), Some(jupiter_idx), Mesh::sphere, Some("assets/textures/Europa.webp"), None, None, None, 0.0, 3.55, 0.1, 0.47, 0.0, 0.0, 0.009, "4.8 × 10^22 kg", 102.0, "Jupiter's icy moon.", None));
        bodies.push(create_body("Ganymede", 0.0017, 0.71, get_orbit_speed(7.15), get_initial_angle(0.0, 7.15), (0.6, 0.6, 0.6), Some(jupiter_idx), Mesh::sphere, Some("assets/textures/Ganymede.webp"), None, None, None, 0.0, 7.15, 0.2, 0.2, 0.0, 0.0, 0.001, "1.48 × 10^23 kg", 110.0, "The largest moon in the Solar System.", None));
        bodies.push(create_body("Callisto", 0.0016, 1.25, get_orbit_speed(16.69), get_initial_angle(0.0, 16.69), (0.4, 0.4, 0.4), Some(jupiter_idx), Mesh::sphere, Some("assets/textures/Callisto.webp"), None, None, None, 0.0, 16.69, 0.0, 0.2, 0.0, 0.0, 0.007, "1.08 × 10^23 kg", 134.0, "Jupiter's heavily cratered moon.", None));

        let p_saturn = 10759.22;

        bodies.push(create_body("Saturn", 0.039, 958.0, get_orbit_speed(p_saturn), get_initial_angle(49.94, p_saturn), (0.9, 0.8, 0.5), Some(0), Mesh::sphere, Some("assets/textures/2k_saturn.jpg"), None, None, Some("assets/textures/2k_saturn_ring_alpha.png"), 0.09, 0.45, 26.7, 2.48, 0.0, 0.0, 0.057, "5.683 × 10^26 kg", 134.0, "The sixth planet from the Sun, famous for its rings.", Some(0.15)));
        let saturn_idx = bodies.len() - 1;

        // Saturn Moon
        bodies.push(create_body("Titan", 0.0017, 0.81, get_orbit_speed(15.94), get_initial_angle(0.0, 15.94), (0.9, 0.7, 0.2), Some(saturn_idx), Mesh::sphere, None, None, None, None, 0.0, 15.94, 0.0, 0.3, 0.0, 0.0, 0.028, "1.345 × 10^23 kg", 94.0, "Saturn's largest moon.", None));

        // Chariklo (Centaur)
        let p_chariklo = 22911.0; // ~62.7 years
        bodies.push(create_body("Chariklo", 0.00008, 1500.0, get_orbit_speed(p_chariklo), get_initial_angle(0.0, p_chariklo), (0.5, 0.4, 0.5), Some(0), Mesh::sphere, Some("assets/textures/chariklo.webp"), None, None, Some("assets/textures/2k_saturn_ring_alpha.png"), 0.0002, 0.3, 0.0, 23.4, 0.0, 0.0, 0.17, "Unknown", 50.0, "A centaur with rings between Saturn and Uranus.", Some(0.4)));

        let p_uranus = 30685.4;

        bodies.push(create_body("Uranus", 0.017, 1920.0, get_orbit_speed(p_uranus), get_initial_angle(313.23, p_uranus), (0.0, 0.8, 0.8), Some(0), Mesh::sphere, Some("assets/textures/2k_uranus.jpg"), None, None, None, 0.0, -0.72, 97.8, 0.77, 0.0, 0.0, 0.046, "8.681 × 10^25 kg", 76.0, "The seventh planet from the Sun.", None));

        let p_neptune = 60189.0;

        bodies.push(create_body("Neptune", 0.016, 3005.0, get_orbit_speed(p_neptune), get_initial_angle(304.88, p_neptune), (0.0, 0.0, 0.8), Some(0), Mesh::sphere, Some("assets/textures/2k_neptune.jpg"), None, None, None, 0.0, 0.67, 28.3, 1.77, 0.0, 0.0, 0.011, "1.024 × 10^26 kg", 72.0, "The eighth and farthest-known Solar planet from the Sun.", None));


        let p_pluto = 90560.0;
        bodies.push(create_body("Pluto", 0.00075, 3948.0, get_orbit_speed(p_pluto), get_initial_angle(0.0, p_pluto), (0.6, 0.5, 0.4), Some(0), Mesh::sphere, Some("assets/textures/Pluto.webp"), None, None, None, 0.0, -6.39, 122.5, 17.16, 0.0, 0.0, 0.244, "1.309 × 10^22 kg", 44.0, "A dwarf planet in the Kuiper belt.", None));
        let pluto_idx = bodies.len() - 1;

        // Charon
        bodies.push(create_body("Charon", 0.00038, 0.013, get_orbit_speed(6.387), get_initial_angle(0.0, 6.387), (0.5, 0.5, 0.5), Some(pluto_idx), Mesh::sphere, Some("assets/textures/Charon.webp"), None, None, None, 0.0, 6.387, 0.0, 0.0, 0.0, 0.0, 0.0, "1.586 × 10^21 kg", 53.0, "Pluto's largest moon.", None));


        let p_haumea = 103368.0;
        bodies.push(create_body("Haumea", 0.00055, 4313.0, get_orbit_speed(p_haumea), get_initial_angle(0.0, p_haumea), (0.7, 0.7, 0.7), Some(0), Mesh::sphere, Some("assets/textures/2k_haumea_fictional.jpg"), None, None, None, 0.0, 0.16, 0.0, 28.2, 0.0, 0.0, 0.191, "4.006 × 10^21 kg", 50.0, "A dwarf planet located beyond Neptune's orbit.", None));


        let p_makemake = 112862.0;
        bodies.push(create_body("Makemake", 0.00046, 4579.0, get_orbit_speed(p_makemake), get_initial_angle(0.0, p_makemake), (0.8, 0.6, 0.5), Some(0), Mesh::sphere, Some("assets/textures/2k_makemake_fictional.jpg"), None, None, None, 0.0, 0.95, 0.0, 29.0, 0.0, 0.0, 0.159, "3.1 × 10^21 kg", 30.0, "A dwarf planet in the Kuiper belt.", None));


        let p_eris = 203443.0;
        bodies.push(create_body("Eris", 0.00075, 6767.0, get_orbit_speed(p_eris), get_initial_angle(0.0, p_eris), (0.9, 0.9, 0.9), Some(0), Mesh::sphere, Some("assets/textures/2k_eris_fictional.jpg"), None, None, None, 0.0, 1.08, 78.0, 44.0, 0.0, 0.0, 0.441, "1.66 × 10^22 kg", 30.0, "The most massive and second-largest known dwarf planet.", None));

        for i in 0..2000 {
            let angle: f32 = rng.gen_range(0.0..360.0);
            let dist: f32 = rng.gen_range(3000.0..5500.0);
            let size: f32 = rng.gen_range(0.0002..0.0006);
            let period = (dist / 100.0).powf(1.5) * 365.256;
            
            bodies.push(create_body(
                &format!("Kuiper Object {}", i),
                size,
                dist,
                get_orbit_speed(period),
                angle.to_radians(),
                (0.6, 0.6, 0.7),
                Some(0),
                Mesh::sphere,
                None,
                None,
                None,
                None,
                0.0,
                rng.gen_range(5.0..20.0),
                rng.gen_range(0.0..30.0),
                rng.gen_range(-20.0..20.0),
                rng.gen_range(0.0..360.0),
                rng.gen_range(0.0..360.0),
                rng.gen_range(0.0..0.3),
                "Unknown",
                40.0,
                "Kuiper Belt Object",
                None
            ));
        }

        for i in 0..10000 {
            let angle: f32 = rng.gen_range(0.0..360.0);
            // Real scale: Inner Oort ~2,000 AU (200,000 units) to Outer Oort ~50,000 AU (5,000,000 units)
            // Using a logarithmic distribution to have more objects in the inner part would be better, 
            // but linear is fine for now, maybe biased towards inner.
            // Let's use a power distribution to concentrate more density closer to the center
            let r = rng.gen_range(0.0f32..1.0f32);
            let dist_au = 2000.0 + (50000.0 - 2000.0) * r.powf(2.0); // Bias towards outer? No, r^2 biases towards 0 (inner) if r is 0..1? 
            // If r is 0..1, r^2 is smaller, so it biases towards 0.
            // Wait, if I want more density inside, I want smaller distances more often.
            // If r is uniform 0..1. r^2 is clustered near 0.
            // So dist = min + (max-min) * r^2 will cluster near min. Correct.
            
            let dist = dist_au * 100.0; // Convert AU to game units
            
            let size: f32 = rng.gen_range(0.00005..0.00015); 
            let period = (dist / 100.0).powf(1.5) * 365.256;
            
            bodies.push(create_body(
                &format!("Oort Object {}", i),
                size,
                dist,
                get_orbit_speed(period),
                angle.to_radians(),
                (0.8, 0.8, 0.9), 
                Some(0),
                Mesh::sphere,
                None,
                None,
                None,
                None,
                0.0,
                rng.gen_range(5.0..20.0),
                rng.gen_range(0.0..30.0),
                rng.gen_range(-90.0..90.0), 
                rng.gen_range(0.0..360.0), 
                rng.gen_range(0.0..360.0), 
                rng.gen_range(0.0..0.5),
                "Unknown",
                10.0,
                "Oort Cloud Object",
                None
            ));
        }

        let background_texture = renderer.create_texture("assets/textures/8k_stars.jpg").ok();
        let background_mesh = Mesh::sphere(1.0, 40, 40, 1.0, 1.0, 1.0);


        let trail_points = 1000;
        for i in 0..bodies.len() {
            let body = &mut bodies[i];
            if body.name.starts_with("Asteroid") || body.name.starts_with("Kuiper") || body.name.starts_with("Oort") { continue; }
            if body.orbit_radius > 0.0 && body.orbit_speed != 0.0 {
                let full_circle = 2.0 * std::f32::consts::PI;
                let angle_step = full_circle / trail_points as f32;
                



                
                for j in 0..trail_points {
                    let angle_offset = -full_circle + (j as f32 * angle_step);
                    let angle = body.orbit_angle + angle_offset;
                    
                    // Kepler for initial trail
                    let m = angle;
                    let e = body.eccentricity;
                    let big_e = m + e * m.sin();
                    
                    let x_orb_raw = body.orbit_radius * (big_e.cos() - e);
                    let z_orb_raw = body.orbit_radius * (1.0 - e*e).sqrt() * big_e.sin();
                    
                    // Apply Argument of Periapsis
                    let w = body.argument_of_periapsis;
                    let (sin_w, cos_w) = w.sin_cos();
                    let x_orb = x_orb_raw * cos_w + z_orb_raw * sin_w;
                    let z_orb = -x_orb_raw * sin_w + z_orb_raw * cos_w;
                    
                    let y_incl = z_orb * body.orbit_inclination.sin();
                    let z_incl = z_orb * body.orbit_inclination.cos();
                    
                    // Apply Longitude of Ascending Node
                    let omega = body.longitude_of_ascending_node;
                    let (sin_o, cos_o) = omega.sin_cos();
                    
                    let x_final = x_orb * cos_o + z_incl * sin_o;
                    let y_final = y_incl;
                    let z_final = -x_orb * sin_o + z_incl * cos_o;
                    
                    let pos = Vector3::new(x_final, y_final, z_final);
                    
                    body.trail.push(pos.x);
                    body.trail.push(pos.y);
                    body.trail.push(pos.z);
                }
            }
        }


        if let Some(list) = document.query_selector(".body-list").unwrap() {
            list.set_inner_html(""); // Clear existing
            
            for (i, body) in bodies.iter().enumerate() {
                if body.name.starts_with("Asteroid") || body.name.starts_with("Kuiper") || body.name.starts_with("Oort") { continue; }
                let li = document.create_element("li").unwrap();
                li.set_text_content(Some(&body.name));

                li.set_attribute("onclick", &format!("window.selectSolarBody({})", i)).unwrap();
                li.set_attribute("style", "cursor: pointer; padding: 5px; transition: background 0.2s;").unwrap();
                li.set_class_name("solar-list-item");
                
                list.append_child(&li).unwrap();
            }
        }

        let sun_texture = if !is_black_hole_mode { bodies[0].texture.clone() } else { None };

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
            asteroid_mesh,
            ring_mesh,
            is_black_hole: is_black_hole_mode,
            sun_texture,
            use_celsius: true,
        }
    }

    pub fn select_body(&mut self, index: usize) {
        if index < self.bodies.len() {
            self.focused_body_index = Some(index);
            let body = &self.bodies[index];

            let window = web_sys::window().unwrap();
            let document = window.document().unwrap();
            
            if let Some(panel) = document.get_element_by_id("solar-info-panel") {
                panel.set_attribute("style", "position: absolute; top: 20px; right: 20px; width: 280px; display: block; pointer-events: auto; padding: 20px;").unwrap();
                panel.set_class_name("panel-glass");
                
                if let Some(el) = document.get_element_by_id("info-name") { el.set_text_content(Some(&body.name)); }
                if let Some(el) = document.get_element_by_id("info-mass") { el.set_text_content(Some(&body.mass)); }
                if let Some(el) = document.get_element_by_id("info-radius") { el.set_text_content(Some(&format!("{:.1} km", body.radius * 6371.0 / 0.0042))); } // Approx scale based on Earth
                if let Some(el) = document.get_element_by_id("info-temp") {
                    let temp_str = if self.use_celsius {
                        format!("{:.0} °C", body.temperature - 273.15)
                    } else {
                        format!("{:.0} K", body.temperature)
                    };
                    el.set_text_content(Some(&temp_str));
                }
                if let Some(el) = document.get_element_by_id("info-speed") {
                    if body.name.trim() == "Sun" || body.name.trim() == "Black Hole" {
                         el.set_text_content(Some("230 km/s (Galactic)"));
                    } else {
                        let speed_km_s = body.orbit_speed.abs() * body.orbit_radius * 1496000.0;
                        el.set_text_content(Some(&format!("{:.2} km/s", speed_km_s)));
                    }
                }
                if let Some(el) = document.get_element_by_id("info-period") { 
                    if body.name.trim() == "Sun" || body.name.trim() == "Black Hole" {
                        el.set_text_content(Some("230,000,000 years (Galactic)"));
                    } else {
                        let period = if body.orbit_speed.abs() > 0.0 {
                            (2.0 * std::f32::consts::PI / body.orbit_speed) / (24.0 * 3600.0)
                        } else {
                            0.0
                        };
                        el.set_text_content(Some(&format!("{:.2} days", period))); 
                    }
                }
                if let Some(el) = document.get_element_by_id("info-eccentricity") { el.set_text_content(Some(&format!("{:.4}", body.eccentricity))); }
                if let Some(el) = document.get_element_by_id("info-desc") { el.set_text_content(Some(&body.description)); }
            }

            let radius = self.bodies[index].radius;
            self.camera_distance = radius * 5.0;
            self.camera_distance = self.camera_distance.max(radius * 1.5);
        } else {
            self.focused_body_index = None;
            let window = web_sys::window().unwrap();
            let document = window.document().unwrap();
            if let Some(panel) = document.get_element_by_id("solar-info-panel") {
                panel.set_attribute("style", "display: none;").unwrap();
            }
        }
    }

    pub fn toggle_temperature_unit(&mut self) {
        self.use_celsius = !self.use_celsius;
        if let Some(index) = self.focused_body_index {
            self.select_body(index);
        }
    }

    pub fn set_time_scale(&mut self, scale: f32) {
        self.time_scale = scale;
    }

    pub fn update(&mut self) {
        let now = Date::now();
        let dt = (now - self.last_time) / 1000.0;
        self.last_time = now;
        
        // Prevent huge time jumps if dt is too large (e.g. tab inactive)
        let safe_dt = if dt > 0.1 { 0.1 } else { dt };
        
        self.current_time += safe_dt * 1000.0 * self.time_scale as f64;
        
        let date = Date::new(&wasm_bindgen::JsValue::from_f64(self.current_time));
        let window = web_sys::window().unwrap();
        let document = window.document().unwrap();
            if let Some(element) = document.get_element_by_id("solar-date") {
                let year = date.get_full_year();
                let month = date.get_month() + 1;
                let day = date.get_date();
                let hours = date.get_hours();
                let minutes = date.get_minutes();
                let seconds = date.get_seconds();
                let date_str = format!("{:02}/{:02}/{} {:02}:{:02}:{:02}", month, day, year, hours, minutes, seconds);
                element.set_text_content(Some(&date_str));
            }

        // Update speed info if a body is selected
        if let Some(idx) = self.focused_body_index {
            if idx < self.bodies.len() {
                let body = &self.bodies[idx];
                if let Some(el) = document.get_element_by_id("info-speed") {
                    let speed_kmh = if body.orbit_radius > 0.0 {
                        // Calculate current distance r
                        let m = body.orbit_angle;
                        let e = body.eccentricity;
                        let big_e = m + e * m.sin();
                        let x_orb = body.orbit_radius * (big_e.cos() - e);
                        let z_orb = body.orbit_radius * (1.0 - e*e).sqrt() * big_e.sin();
                        let r = (x_orb*x_orb + z_orb*z_orb).sqrt();

                        // Vis-viva equation: v = sqrt(mu * (2/r - 1/a))
                        // mu = n^2 * a^3
                        // v = n * a * sqrt(2a/r - 1)
                        let n = body.orbit_speed.abs();
                        let a = body.orbit_radius;
                        
                        if r > 0.0 {
                            let v_sim = n * a * ((2.0 * a / r) - 1.0).abs().sqrt();
                            // Convert to km/h
                            // Scale: 1 unit = 6371.0 / 0.0042 km
                            let scale = 6371.0 / 0.0042;
                            v_sim * scale * 3600.0
                        } else {
                            0.0
                        }
                    } else {
                        0.0
                    };
                    el.set_text_content(Some(&format!("{:.0} km/h", speed_kmh)));
                }
            }
        }

        let mut positions = vec![Vector3::new(0.0, 0.0, 0.0); self.bodies.len()];        for i in 0..self.bodies.len() {

            let body = &mut self.bodies[i];
            if body.parent.is_some() {
                body.orbit_angle += body.orbit_speed * safe_dt as f32 * self.time_scale;
                body.orbit_angle %= 2.0 * std::f32::consts::PI;
            }
            

            if body.rotation_period != 0.0 {



                let period_seconds = body.rotation_period.abs() * 24.0 * 3600.0;
                let rotation_speed = (2.0 * std::f32::consts::PI) / period_seconds;
                

                body.current_rotation += rotation_speed * safe_dt as f32 * self.time_scale;
                body.current_rotation %= 2.0 * std::f32::consts::PI;

                if body.cloud_texture.is_some() {

                    body.cloud_rotation += rotation_speed * 0.2 * safe_dt as f32 * self.time_scale;
                    body.cloud_rotation %= 2.0 * std::f32::consts::PI;
                }
            }

            // Calculate position using Kepler's equation approximation
            // M = orbit_angle (Mean Anomaly)
            // E approx M + e*sin(M) (Eccentric Anomaly)
            // x = a * (cos(E) - e)
            // z = a * sqrt(1 - e^2) * sin(E)
            
            let m = body.orbit_angle;
            let e = body.eccentricity;
            // Simple approximation for E (Eccentric Anomaly)
            let big_e = m + e * m.sin(); 
            
            let x_orb_raw = body.orbit_radius * (big_e.cos() - e);
            let z_orb_raw = body.orbit_radius * (1.0 - e*e).sqrt() * big_e.sin();
            
            // Apply Argument of Periapsis
            let w = body.argument_of_periapsis;
            let (sin_w, cos_w) = w.sin_cos();
            let x_orb = x_orb_raw * cos_w + z_orb_raw * sin_w;
            let z_orb = -x_orb_raw * sin_w + z_orb_raw * cos_w;
            
            // Apply inclination
            // Rotate around X axis by inclination
            let y_incl = z_orb * body.orbit_inclination.sin();
            let z_incl = z_orb * body.orbit_inclination.cos();
            
            // Apply Longitude of Ascending Node (Rotation around Y axis)
            let omega = body.longitude_of_ascending_node;
            let (sin_o, cos_o) = omega.sin_cos();
            
            let x_final = x_orb * cos_o + z_incl * sin_o;
            let y_final = y_incl;
            let z_final = -x_orb * sin_o + z_incl * cos_o;
            
            let mut pos = Vector3::new(x_final, y_final, z_final);
            
            if let Some(parent_idx) = body.parent {
                pos += positions[parent_idx];
            }
            
            positions[i] = pos;
            
            if body.orbit_radius > 0.0 {
                if body.name.starts_with("Asteroid") || body.name.starts_with("Kuiper") || body.name.starts_with("Oort") { continue; }

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
                        let a_angle = body.last_trail_angle + (k as f32 * angle_step);
                        
                        // Same Kepler calculation for trail
                        let m_t = a_angle;
                        let big_e_t = m_t + e * m_t.sin();
                        
                        let x_t_raw = body.orbit_radius * (big_e_t.cos() - e);
                        let z_t_raw = body.orbit_radius * (1.0 - e*e).sqrt() * big_e_t.sin();
                        
                        // Apply Argument of Periapsis
                        let w = body.argument_of_periapsis;
                        let (sin_w, cos_w) = w.sin_cos();
                        let x_t = x_t_raw * cos_w + z_t_raw * sin_w;
                        let z_t = -x_t_raw * sin_w + z_t_raw * cos_w;
                        
                        let y_incl = z_t * body.orbit_inclination.sin();
                        let z_incl = z_t * body.orbit_inclination.cos();
                        
                        // Apply Longitude of Ascending Node
                        let omega = body.longitude_of_ascending_node;
                        let (sin_o, cos_o) = omega.sin_cos();
                        
                        let x_final = x_t * cos_o + z_incl * sin_o;
                        let y_final = y_incl;
                        let z_final = -x_t * sin_o + z_incl * cos_o;
                        
                        let p = Vector3::new(x_final, y_final, z_final);
                        
                        body.trail.push(p.x);
                        body.trail.push(p.y);
                        body.trail.push(p.z);
                    }
                    
                    body.last_trail_angle += steps as f32 * angle_step;
                    body.last_trail_angle %= two_pi;
                    

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
            
            let m = body.orbit_angle;
            let e = body.eccentricity;
            let big_e = m + e * m.sin();
            
            let x_orb_raw = body.orbit_radius * (big_e.cos() - e);
            let z_orb_raw = body.orbit_radius * (1.0 - e*e).sqrt() * big_e.sin();
            
            // Apply Argument of Periapsis
            let w = body.argument_of_periapsis;
            let (sin_w, cos_w) = w.sin_cos();
            let x_orb = x_orb_raw * cos_w + z_orb_raw * sin_w;
            let z_orb = -x_orb_raw * sin_w + z_orb_raw * cos_w;
            
            let y_incl = z_orb * body.orbit_inclination.sin();
            let z_incl = z_orb * body.orbit_inclination.cos();

            // Apply Longitude of Ascending Node
            let omega = body.longitude_of_ascending_node;
            let (sin_o, cos_o) = omega.sin_cos();
            
            let x_final = x_orb * cos_o + z_incl * sin_o;
            let y_final = y_incl;
            let z_final = -x_orb * sin_o + z_incl * cos_o;

            let mut pos = Vector3::new(x_final, y_final, z_final);
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
        let projection = Matrix4::new_perspective(aspect, 45.0 * std::f32::consts::PI / 180.0, 0.001, 200000000.0); // Increased far plane significantly
        




        
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
                None,
                false,
                None,
                false,
                false,
                false,
                None,
                None
            );        // Re-enable lighting for planets
        self.renderer.gl.uniform1i(Some(&self.renderer.u_use_lighting_location), 1);
        
        self.renderer.enable_depth_test();

        let mut instance_data = Vec::with_capacity(self.bodies.len() * 7);
        let mut asteroid_count = 0;
        
        struct BodyScreenData {
            index: usize,
            screen_x: f32,
            screen_y: f32,
            label_y: f32,
            radius_px: f32,
            depth: f32,
        }
        let mut screen_data = Vec::new();

        for (i, body) in self.bodies.iter().enumerate() {
            let abs_pos = positions[i];
            let pos = abs_pos - target;
            
            if !body.trail.is_empty() && !body.name.starts_with("Asteroid") && !body.name.starts_with("Kuiper") && !body.name.starts_with("Oort") {
                let parent_pos = if let Some(pidx) = body.parent {
                    positions[pidx]
                } else {
                    Vector3::new(0.0, 0.0, 0.0)
                };

                let relative_trail: Vec<f32> = body.trail.chunks(3).flat_map(|p| {
                    vec![p[0] + parent_pos.x - target.x, p[1] + parent_pos.y - target.y, p[2] + parent_pos.z - target.z]
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
            
            let is_small_body = body.name.starts_with("Asteroid") || body.name.starts_with("Kuiper") || body.name.starts_with("Oort");
            
            if is_small_body {
                let scale_factor = 0.0005;
                let min_size = dist * scale_factor; 
                let render_radius = if min_size > body.radius { min_size } else { body.radius };
                
                instance_data.push(pos.x);
                instance_data.push(pos.y);
                instance_data.push(pos.z);
                instance_data.push(render_radius);
                instance_data.push(body.color.0);
                instance_data.push(body.color.1);
                instance_data.push(body.color.2);
                asteroid_count += 1;
                continue;
            }

            let scale_factor = 0.002;
            let min_size = dist * scale_factor; 
            
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

            let should_use_lighting = use_texture && body.name != "Sun" && body.name != "Black Hole";
            let is_black_hole = body.name == "Black Hole";
            
            // If black hole, we want it to be visible despite its tiny physical radius.
            // We scale it up for rendering so the lensing effect is visible.
            // 3km is invisible. Let's make the visual effect roughly Sun-sized (0.5) or slightly smaller.
            let final_render_radius = if is_black_hole { 0.3 } else { render_radius };

            self.renderer.draw_mesh(
                mesh_to_use,
                pos.x, pos.y, pos.z,
                final_render_radius, final_render_radius, final_render_radius,
                body.axial_tilt, body.current_rotation, 0.0,
                &projection,
                &view,
                texture_to_use,
                night_texture_to_use,
                color_override,
                false,
                None,
                should_use_lighting,
                is_black_hole,
                body.is_frozen,
                Some((rel_cam_x, rel_cam_y, rel_cam_z)),
                if is_black_hole { self.background_texture.as_ref() } else { None }
            );

            if use_texture {
                if let Some(ring_tex) = &body.ring_texture {
                    self.renderer.gl.enable(web_sys::WebGlRenderingContext::BLEND);
                    self.renderer.gl.blend_func(web_sys::WebGlRenderingContext::SRC_ALPHA, web_sys::WebGlRenderingContext::ONE_MINUS_SRC_ALPHA);
                    
                    // Rings are usually equatorial.
                    // We rotate 90 deg around X to make the quad horizontal (XZ plane).
                    // Then apply axial tilt (X rotation).
                    // So total X rotation = axial_tilt + 90 deg.
                    
                    self.renderer.draw_mesh(
                        &self.ring_mesh,
                        pos.x, pos.y, pos.z,
                        body.ring_radius, body.ring_radius, body.ring_radius,
                        body.axial_tilt + std::f32::consts::FRAC_PI_2, 0.0, 0.0,
                        &projection,
                        &view,
                        Some(ring_tex),
                        None,
                        None,
                        true,
                        body.ring_inner_radius,
                        true,
                        false,
                        body.is_frozen,
                        None,
                        None
                    );
                    
                    self.renderer.gl.disable(web_sys::WebGlRenderingContext::BLEND);
                }

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
                        None,
                        false,
                        None,
                        true,
                        false,
                        body.is_frozen,
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
                        
                        // Store for second pass
                        screen_data.push(BodyScreenData {
                            index: i,
                            screen_x,
                            screen_y: screen_cy,
                            label_y,
                            radius_px,
                            depth: dist, // dist calculated earlier
                        });
                    } else {
                        element.style().set_property("display", "none").unwrap();
                    }
                } else {
                    element.style().set_property("display", "none").unwrap();
                }
            }
        }

        // Second pass: Occlusion Culling for Labels
        for data in &screen_data {
            let mut is_occluded = false;
            
            // Check against all other bodies
            for other in &screen_data {
                if data.index == other.index { continue; }
                
                // If other body is closer and overlaps
                if other.depth < data.depth {
                    let dx = data.screen_x - other.screen_x;
                    let dy = data.screen_y - other.screen_y; // Use center of planet, not label pos
                    let dist_sq = dx*dx + dy*dy;
                    
                    // Check if label center (approx) is inside the other planet's visual radius
                    // Actually, we should check if the PLANET center is behind the other planet.
                    // If the planet is hidden, the label should be hidden too.
                    
                    if dist_sq < (other.radius_px * other.radius_px) {
                        is_occluded = true;
                        break;
                    }
                }
            }
            
            if let Some(element) = &self.bodies[data.index].label_element {
                if is_occluded {
                    element.style().set_property("display", "none").unwrap();
                } else {
                    let style = element.style();
                    style.set_property("display", "block").unwrap();
                    style.set_property("left", &format!("{}px", data.screen_x)).unwrap();
                    style.set_property("top", &format!("{}px", data.label_y)).unwrap();
                }
            }
        }

        if asteroid_count > 0 {
             self.renderer.draw_instanced_mesh(
                &self.asteroid_mesh,
                &instance_data,
                asteroid_count,
                &projection,
                &view,
                &Vector3::new(0.0, 0.0, 0.0)
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
            
            self.camera_rotation.0 = self.camera_rotation.0.max(-1.5).min(1.5);
            
            self.last_mouse_pos = (x, y);
        }
    }

    pub fn handle_wheel(&mut self, delta: f32) {
        let zoom_sensitivity = 0.001;
        let factor = (delta * zoom_sensitivity).exp();
        self.camera_distance *= factor;
        self.camera_distance = self.camera_distance.max(0.0001).min(100000000.0);
    }
}
