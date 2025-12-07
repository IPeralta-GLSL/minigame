use gltf;

pub struct Mesh {
    pub vertices: Vec<f32>,
    pub indices: Vec<u16>,
}

impl Mesh {
    pub fn cube(size: f32, r: f32, g: f32, b: f32) -> Self {
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
            let base = (vertices.len() / 8) as u16;
            let br = r * brightness;
            let bg = g * brightness;
            let bb = b * brightness;
            
            // Normals for cube faces
            // We can infer normal from face orientation, but here we need to pass it explicitly or calculate it.
            // For simplicity, let's calculate face normal from first 3 points (cross product)
            // Or just hardcode based on which face it is.
            // But add_face is generic.
            // Let's compute normal:
            let ux = x2 - x1; let uy = y2 - y1; let uz = z2 - z1;
            let vx = x3 - x1; let vy = y3 - y1; let vz = z3 - z1;
            let nx = uy * vz - uz * vy;
            let ny = uz * vx - ux * vz;
            let nz = ux * vy - uy * vx;
            let len = (nx*nx + ny*ny + nz*nz).sqrt();
            let nx = nx / len; let ny = ny / len; let nz = nz / len;

            vertices.extend_from_slice(&[
                x1, y1, z1, br, bg, bb, 0.0, 0.0, nx, ny, nz,
                x2, y2, z2, br, bg, bb, 1.0, 0.0, nx, ny, nz,
                x3, y3, z3, br, bg, bb, 1.0, 1.0, nx, ny, nz,
                x4, y4, z4, br, bg, bb, 0.0, 1.0, nx, ny, nz,
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

    pub fn from_gltf(bytes: &[u8]) -> Result<Self, String> {
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
                
                let base_index = (vertices.len() / 8) as u16;
                
                for (pos, color) in positions.iter().zip(colors.iter()) {
                    // Default normal pointing up if not present
                    // Ideally we should read normals from GLTF
                    vertices.extend_from_slice(&[
                        pos[0], pos[1], pos[2],
                        color[0], color[1], color[2],
                        0.0, 0.0,
                        0.0, 1.0, 0.0 
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

    pub fn sphere(radius: f32, lat_segments: u16, long_segments: u16, r: f32, g: f32, b: f32) -> Self {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        for i in 0..=lat_segments {
            let theta = i as f32 * std::f32::consts::PI / lat_segments as f32;
            let sin_theta = theta.sin();
            let cos_theta = theta.cos();

            for j in 0..=long_segments {
                let phi = j as f32 * 2.0 * std::f32::consts::PI / long_segments as f32;
                let sin_phi = phi.sin();
                let cos_phi = phi.cos();

                let x = cos_phi * sin_theta;
                let y = cos_theta;
                let z = sin_phi * sin_theta;
                
                let u = 1.0 - (j as f32 / long_segments as f32);
                let v = 1.0 - (i as f32 / lat_segments as f32);

                // For a sphere centered at origin, the normal is just the normalized position vector.
                // Since radius is constant, (x,y,z) normalized is just (x/radius, y/radius, z/radius)
                // But x,y,z here are already normalized (unit sphere coords) * radius.
                // So the normal is just (x/radius, y/radius, z/radius) which is (cos_phi*sin_theta, cos_theta, sin_phi*sin_theta)
                
                let nx = x;
                let ny = y;
                let nz = z;

                vertices.extend_from_slice(&[
                    x * radius, y * radius, z * radius,
                    r, g, b,
                    u, v,
                    nx, ny, nz
                ]);
            }
        }

        for i in 0..lat_segments {
            for j in 0..long_segments {
                let first = (i * (long_segments + 1)) + j;
                let second = first + long_segments + 1;

                indices.extend_from_slice(&[
                    first, second, first + 1,
                    second, second + 1, first + 1
                ]);
            }
        }

        Mesh { vertices, indices }
    }

    pub fn quad(width: f32, height: f32) -> Self {
        let hw = width / 2.0;
        let hh = height / 2.0;
        
        let vertices = vec![
            -hw, -hh, 0.0,  1.0, 1.0, 1.0,  0.0, 1.0, 0.0, 0.0, 1.0,
             hw, -hh, 0.0,  1.0, 1.0, 1.0,  1.0, 1.0, 0.0, 0.0, 1.0,
             hw,  hh, 0.0,  1.0, 1.0, 1.0,  1.0, 0.0, 0.0, 0.0, 1.0,
            -hw,  hh, 0.0,  1.0, 1.0, 1.0,  0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        
        let indices = vec![
            0, 1, 2,
            0, 2, 3,
        ];
        
        Mesh { vertices, indices }
    }
}
