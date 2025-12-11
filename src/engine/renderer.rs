use wasm_bindgen::prelude::*;
use web_sys::{WebGlRenderingContext, WebGlProgram, WebGlBuffer, WebGlUniformLocation, HtmlCanvasElement, WebGlTexture, HtmlImageElement, AngleInstancedArrays};
use nalgebra::{Matrix4, Vector3};
use crate::engine::mesh::Mesh;
use wasm_bindgen::JsCast;

const VERTEX_SHADER: &str = r#"
    attribute vec3 aPosition;
    attribute vec3 aColor;
    attribute vec2 aTexCoord;
    attribute vec3 aNormal;
    
    uniform mat4 uModelViewProjection;
    uniform mat4 uModel;
    uniform mat3 uNormalMatrix;
    
    varying vec3 vColor;
    varying vec2 vTexCoord;
    varying vec3 vPos;
    varying vec3 vNormal;
    varying vec3 vFragPos;
    
    void main() {
        gl_Position = uModelViewProjection * vec4(aPosition, 1.0);
        vPos = aPosition;
        vColor = aColor;
        vTexCoord = aTexCoord;
        
        // Calculate world space position and normal
        vFragPos = vec3(uModel * vec4(aPosition, 1.0));
        vNormal = uNormalMatrix * aNormal; // Assuming aNormal is available in mesh
    }
"#;

const INSTANCED_VERTEX_SHADER: &str = r#"
    attribute vec3 aPosition;
    attribute vec3 aNormal;
    attribute vec2 aTexCoord;
    
    attribute vec3 aInstancePosition;
    attribute float aInstanceScale;
    attribute vec3 aInstanceColor;
    attribute float aInstanceLight;

    uniform mat4 uView;
    uniform mat4 uProjection;
    
    varying vec3 vColor;
    varying vec2 vTexCoord;
    varying vec3 vPos;
    varying vec3 vNormal;
    varying vec3 vFragPos;

    void main() {
        vec3 scaledPos = aPosition * aInstanceScale;
        vec3 worldPos = scaledPos + aInstancePosition;
        
        gl_Position = uProjection * uView * vec4(worldPos, 1.0);
        
        vPos = aPosition; 
        vColor = aInstanceColor * aInstanceLight;
        vTexCoord = aTexCoord;
        vFragPos = worldPos;
        vNormal = aNormal; 
    }
"#;

const FRAGMENT_SHADER: &str = r#"
    precision highp float;
    varying vec3 vColor;
    varying vec2 vTexCoord;
    varying vec3 vPos;
    varying vec3 vNormal;
    varying vec3 vFragPos;
    
    uniform sampler2D uTexture;
    uniform sampler2D uNightTexture;
    uniform int uUseTexture;
    uniform int uUseNightTexture;
    uniform vec3 uUniformColor;
    uniform bool uUseUniformColor;
    uniform vec3 uTimeColor;
    uniform bool uIsRing;
    uniform float uRingInnerRadius;
    
    uniform vec3 uLightPos;
    const vec3 lightColor = vec3(1.0, 1.0, 1.0);
    const float ambientStrength = 0.15;

    uniform bool uUseLighting;
    uniform bool uIsBlackHole;
    uniform bool uIsFrozen;
    uniform vec3 uCameraPos;
    uniform sampler2D uBackgroundTexture;

    vec2 dirToUV(vec3 dir) {
        float u = 0.5 + atan(dir.z, dir.x) / (2.0 * 3.14159265);
        float v = 0.5 - asin(dir.y) / 3.14159265;
        return vec2(u, v);
    }

    void main() {
        vec3 color;
        float alpha = 1.0;

        if (uIsBlackHole) {
            vec3 viewDir = normalize(vFragPos - uCameraPos); // Camera to Fragment
            vec3 normal = normalize(vNormal);
            
            // Calculate impact parameter (distance from center in screen space relative to radius)
            // For a sphere, N dot V (where V is Frag to Cam) gives us centrality.
            // Let's use V = uCameraPos - vFragPos (Cam to Frag is -V)
            vec3 V = normalize(uCameraPos - vFragPos);
            float NdotV = dot(normal, V);
            
            // r is 0 at center, 1 at edge
            float r = sqrt(1.0 - NdotV * NdotV);
            
            // Define Event Horizon radius (relative to the mesh size)
            // We will render the mesh 3x larger than the actual event horizon.
            // So EH is at r = 0.33
            float ehRadius = 0.33;
            
            if (r < ehRadius) {
                gl_FragColor = vec4(0.0, 0.0, 0.0, 1.0);
                return;
            }
            
            // Gravitational Lensing (Distortion)
            // We want to bend the view vector towards the black hole center.
            // The center direction is -normal (roughly).
            // Strength depends on 1/distance.
            
            float dist = r;
            float strength = 0.2 / (dist * dist); // Inverse square-ish
            
            // Bend the view vector
            // Original view vector is viewDir.
            // We want to pull it towards the normal (which points out from center).
            // Wait, light bends IN. So we see light that came from OUT.
            // So we should bend the lookup vector OUT (along normal).
            
            vec3 distortDir = normalize(viewDir - normal * strength);
            
            vec2 uv = dirToUV(distortDir);
            vec3 bgColor = texture2D(uBackgroundTexture, uv).rgb;
            
            gl_FragColor = vec4(bgColor, 1.0);
            return;
        }

        if (uUseUniformColor) {
            color = uUniformColor;
        } else {
            color = vColor;
        }

        vec2 texCoord = vTexCoord;
        if (uIsRing) {
            float dist = distance(vTexCoord, vec2(0.5));
            float inner = uRingInnerRadius;
            if (inner <= 0.0) inner = 0.15;

            if (dist > 0.5 || dist < inner) {
                discard;
            }
            texCoord = vec2((dist - inner) / (0.5 - inner), 0.5);
        }

        if (uUseTexture == 1) {
            vec4 texColor = texture2D(uTexture, texCoord);
            color *= texColor.rgb;
            alpha = texColor.a;
        }
        
        vec3 result;
        
        if (uUseLighting) {
            vec3 ambient = ambientStrength * lightColor;
            
            vec3 norm = normalize(vNormal);
            vec3 lightDir = normalize(uLightPos - vFragPos);
            
            float diff = max(dot(norm, lightDir), 0.0);

            if (uIsRing) {
                diff = 0.8;
                ambient = vec3(0.4);
            }

            if (uIsFrozen) {
                diff = 0.0;
                ambient *= 0.5;
            }
            
            float dist = length(vFragPos - uLightPos);
            if (dist < 1.0) {
                diff = 1.0;
                ambient = vec3(1.0);
            }
            
            vec3 diffuse = diff * lightColor;
            
            vec3 dayColor = (ambient + diffuse) * color;
            
            if (uUseNightTexture == 1) {
                vec3 nightColor = texture2D(uNightTexture, texCoord).rgb;
                float mixFactor = smoothstep(0.0, 0.2, diff);
                result = mix(nightColor, dayColor, mixFactor);
            } else {
                result = dayColor;
            }
        } else {
            result = color;
        }

        if (uIsFrozen) {
            float gray = dot(result, vec3(0.299, 0.587, 0.114));
            vec3 darkGray = vec3(0.15, 0.15, 0.18);
            result = mix(vec3(gray), darkGray, 0.7);
        }

        result *= uTimeColor;

        float luminance = dot(result, vec3(0.2126, 0.7152, 0.0722));
        vec3 gray = vec3(luminance);
        result = mix(gray, result, 1.2);
        
        result = pow(result, vec3(1.1));

        gl_FragColor = vec4(result, alpha);
    }
"#;

pub struct Renderer {
    pub gl: WebGlRenderingContext,
    program: WebGlProgram,
    mvp_location: WebGlUniformLocation,
    model_location: WebGlUniformLocation,
    normal_matrix_location: WebGlUniformLocation,
    u_uniform_color_location: WebGlUniformLocation,
    u_use_uniform_color_location: WebGlUniformLocation,
    u_time_color_location: WebGlUniformLocation,
    u_use_texture_location: WebGlUniformLocation,
    u_texture_location: WebGlUniformLocation,
    u_use_night_texture_location: WebGlUniformLocation,
    u_night_texture_location: WebGlUniformLocation,
    pub u_use_lighting_location: WebGlUniformLocation,
    pub u_light_pos_location: WebGlUniformLocation,
    pub u_is_ring_location: WebGlUniformLocation,
    pub u_ring_inner_radius_location: WebGlUniformLocation,
    pub u_is_black_hole_location: WebGlUniformLocation,
    pub u_is_frozen_location: WebGlUniformLocation,
    pub u_camera_pos_location: WebGlUniformLocation,
    pub u_background_texture_location: WebGlUniformLocation,
    unit_cube_vertex_buffer: WebGlBuffer,
    unit_cube_index_buffer: WebGlBuffer,
    unit_cube_index_count: i32,
    dynamic_vertex_buffer: WebGlBuffer,
    dynamic_index_buffer: WebGlBuffer,
    
    // Instancing
    instanced_ext: Option<AngleInstancedArrays>,
    instanced_program: WebGlProgram,
    u_instanced_view_loc: WebGlUniformLocation,
    u_instanced_proj_loc: WebGlUniformLocation,
    u_instanced_light_pos_loc: WebGlUniformLocation,
    u_instanced_use_lighting_loc: WebGlUniformLocation,
    u_instanced_time_color_loc: WebGlUniformLocation,
    instance_data_buffer: WebGlBuffer,
}

impl Renderer {
    pub fn new(gl: WebGlRenderingContext) -> Result<Self, JsValue> {
        let program = create_program(&gl)?;
        gl.use_program(Some(&program));

        let dynamic_vertex_buffer = gl.create_buffer().ok_or("Failed to create buffer")?;
        let dynamic_index_buffer = gl.create_buffer().ok_or("Failed to create buffer")?;

        let mvp_location = gl.get_uniform_location(&program, "uModelViewProjection")
            .ok_or("Failed to get uniform location")?;
        let model_location = gl.get_uniform_location(&program, "uModel")
            .ok_or("Failed to get uModel location")?;
        let normal_matrix_location = gl.get_uniform_location(&program, "uNormalMatrix")
            .ok_or("Failed to get uNormalMatrix location")?;
        let u_uniform_color_location = gl.get_uniform_location(&program, "uUniformColor")
            .ok_or("Failed to get uUniformColor location")?;
        let u_use_uniform_color_location = gl.get_uniform_location(&program, "uUseUniformColor")
            .ok_or("Failed to get uUseUniformColor location")?;
        let u_time_color_location = gl.get_uniform_location(&program, "uTimeColor")
            .ok_or("Failed to get uTimeColor location")?;
        let u_use_texture_location = gl.get_uniform_location(&program, "uUseTexture")
            .ok_or("Failed to get uUseTexture location")?;
        let u_texture_location = gl.get_uniform_location(&program, "uTexture")
            .ok_or("Failed to get uTexture location")?;
        let u_use_night_texture_location = gl.get_uniform_location(&program, "uUseNightTexture")
            .ok_or("Failed to get uUseNightTexture location")?;
        let u_night_texture_location = gl.get_uniform_location(&program, "uNightTexture")
            .ok_or("Failed to get uNightTexture location")?;
        let u_use_lighting_location = gl.get_uniform_location(&program, "uUseLighting")
            .ok_or("Failed to get uUseLighting location")?;
        let u_light_pos_location = gl.get_uniform_location(&program, "uLightPos")
            .ok_or("Failed to get uLightPos location")?;
        let u_is_ring_location = gl.get_uniform_location(&program, "uIsRing")
            .ok_or("Failed to get uIsRing location")?;
        let u_ring_inner_radius_location = gl.get_uniform_location(&program, "uRingInnerRadius")
            .ok_or("Failed to get uRingInnerRadius location")?;
        let u_is_black_hole_location = gl.get_uniform_location(&program, "uIsBlackHole")
            .ok_or("Failed to get uIsBlackHole location")?;
        let u_is_frozen_location = gl.get_uniform_location(&program, "uIsFrozen")
            .ok_or("Failed to get uIsFrozen location")?;
        let u_camera_pos_location = gl.get_uniform_location(&program, "uCameraPos")
            .ok_or("Failed to get uCameraPos location")?;
        let u_background_texture_location = gl.get_uniform_location(&program, "uBackgroundTexture")
            .ok_or("Failed to get uBackgroundTexture location")?;

        // Instancing setup
        let instanced_ext = gl.get_extension("ANGLE_instanced_arrays")?.map(|e| e.unchecked_into::<AngleInstancedArrays>());
        let instanced_program = create_instanced_program(&gl)?;
        let u_instanced_view_loc = gl.get_uniform_location(&instanced_program, "uView").ok_or("Failed to get uView")?;
        let u_instanced_proj_loc = gl.get_uniform_location(&instanced_program, "uProjection").ok_or("Failed to get uProjection")?;
        let u_instanced_light_pos_loc = gl.get_uniform_location(&instanced_program, "uLightPos").ok_or("Failed to get uLightPos")?;
        let u_instanced_use_lighting_loc = gl.get_uniform_location(&instanced_program, "uUseLighting").ok_or("Failed to get uUseLighting instanced")?;
        let u_instanced_time_color_loc = gl.get_uniform_location(&instanced_program, "uTimeColor").ok_or("Failed to get uTimeColor")?;
        let instance_data_buffer = gl.create_buffer().ok_or("Failed to create instance buffer")?;

        // Create unit cube buffers
        let unit_cube_vertex_buffer = gl.create_buffer().ok_or("Failed to create unit cube buffer")?;
        let unit_cube_index_buffer = gl.create_buffer().ok_or("Failed to create unit cube index buffer")?;
        
        let unit_cube = Mesh::cube(1.0, 1.0, 1.0, 1.0); // White unit cube
        
        gl.bind_buffer(WebGlRenderingContext::ARRAY_BUFFER, Some(&unit_cube_vertex_buffer));
        unsafe {
            let vert_array = js_sys::Float32Array::view(&unit_cube.vertices);
            gl.buffer_data_with_array_buffer_view(
                WebGlRenderingContext::ARRAY_BUFFER,
                &vert_array,
                WebGlRenderingContext::STATIC_DRAW
            );
        }

        gl.bind_buffer(WebGlRenderingContext::ELEMENT_ARRAY_BUFFER, Some(&unit_cube_index_buffer));
        unsafe {
            let idx_array = js_sys::Uint16Array::view(&unit_cube.indices);
            gl.buffer_data_with_array_buffer_view(
                WebGlRenderingContext::ELEMENT_ARRAY_BUFFER,
                &idx_array,
                WebGlRenderingContext::STATIC_DRAW
            );
        }
        let unit_cube_index_count = unit_cube.indices.len() as i32;

        // Initialize time color to white (no filter)
        gl.uniform3f(Some(&u_time_color_location), 1.0, 1.0, 1.0);
        // Initialize light pos to 0,0,0
        gl.uniform3f(Some(&u_light_pos_location), 0.0, 0.0, 0.0);

        Ok(Renderer {
            gl,
            program,
            mvp_location,
            model_location,
            normal_matrix_location,
            u_uniform_color_location,
            u_use_uniform_color_location,
            u_time_color_location,
            u_use_texture_location,
            u_texture_location,
            u_use_night_texture_location,
            u_night_texture_location,
            unit_cube_vertex_buffer,
            unit_cube_index_buffer,
            unit_cube_index_count,
            dynamic_vertex_buffer,
            dynamic_index_buffer,
            u_use_lighting_location,
            u_light_pos_location,
            u_is_ring_location,
            u_ring_inner_radius_location,
            u_is_black_hole_location,
            u_is_frozen_location,
            u_camera_pos_location,
            u_background_texture_location,
            instanced_ext,
            instanced_program,
            u_instanced_view_loc,
            u_instanced_proj_loc,
            u_instanced_light_pos_loc,
            u_instanced_use_lighting_loc,
            u_instanced_time_color_loc,
            instance_data_buffer,
        })
    }

    pub fn set_light_position(&self, x: f32, y: f32, z: f32) {
        self.gl.uniform3f(Some(&self.u_light_pos_location), x, y, z);
    }

    pub fn clear(&self, r: f32, g: f32, b: f32) {
        self.gl.clear_color(r, g, b, 1.0);
        self.gl.clear(WebGlRenderingContext::COLOR_BUFFER_BIT | WebGlRenderingContext::DEPTH_BUFFER_BIT);
    }

    pub fn set_time_color(&self, r: f32, g: f32, b: f32) {
        self.gl.uniform3f(Some(&self.u_time_color_location), r, g, b);
    }

    pub fn enable_depth_test(&self) {
        self.gl.enable(WebGlRenderingContext::DEPTH_TEST);
    }

    pub fn enable_face_culling(&self) {
        self.gl.enable(WebGlRenderingContext::CULL_FACE);
        self.gl.cull_face(WebGlRenderingContext::BACK);
    }

    pub fn enable_blend(&self) {
        self.gl.enable(WebGlRenderingContext::BLEND);
        self.gl.blend_func(WebGlRenderingContext::SRC_ALPHA, WebGlRenderingContext::ONE_MINUS_SRC_ALPHA);
    }

    pub fn disable_blend(&self) {
        self.gl.disable(WebGlRenderingContext::BLEND);
    }

    pub fn resize(&self, width: i32, height: i32) {
        self.gl.viewport(0, 0, width, height);
    }

    pub fn clear_screen(&self, r: f32, g: f32, b: f32) {
        self.gl.clear_color(r, g, b, 1.0);
        self.gl.clear(WebGlRenderingContext::COLOR_BUFFER_BIT | WebGlRenderingContext::DEPTH_BUFFER_BIT);
    }

    pub fn canvas(&self) -> Option<HtmlCanvasElement> {
        self.gl.canvas().unwrap().dyn_into::<HtmlCanvasElement>().ok()
    }

    pub fn draw_cube(&self, x: f32, y: f32, z: f32, w: f32, h: f32, d: f32, r: f32, g: f32, b: f32, projection: &Matrix4<f32>, view: &Matrix4<f32>) {
        self.gl.bind_buffer(WebGlRenderingContext::ARRAY_BUFFER, Some(&self.unit_cube_vertex_buffer));
        self.gl.bind_buffer(WebGlRenderingContext::ELEMENT_ARRAY_BUFFER, Some(&self.unit_cube_index_buffer));

        let pos_loc = self.gl.get_attrib_location(&self.program, "aPosition") as u32;
        let col_loc = self.gl.get_attrib_location(&self.program, "aColor") as u32;
        let tex_loc = self.gl.get_attrib_location(&self.program, "aTexCoord") as u32;
        let norm_loc = self.gl.get_attrib_location(&self.program, "aNormal") as u32;

        self.gl.vertex_attrib_pointer_with_i32(pos_loc, 3, WebGlRenderingContext::FLOAT, false, 44, 0);
        self.gl.enable_vertex_attrib_array(pos_loc);

        // We need to set these pointers even if unused, to avoid using pointers from other buffers
        self.gl.vertex_attrib_pointer_with_i32(col_loc, 3, WebGlRenderingContext::FLOAT, false, 44, 12);
        self.gl.enable_vertex_attrib_array(col_loc);

        self.gl.vertex_attrib_pointer_with_i32(tex_loc, 2, WebGlRenderingContext::FLOAT, false, 44, 24);
        self.gl.enable_vertex_attrib_array(tex_loc);
        
        self.gl.vertex_attrib_pointer_with_i32(norm_loc, 3, WebGlRenderingContext::FLOAT, false, 44, 32);
        self.gl.enable_vertex_attrib_array(norm_loc);

        self.gl.uniform1i(Some(&self.u_use_uniform_color_location), 1);
        self.gl.uniform1i(Some(&self.u_use_texture_location), 0);
        self.gl.uniform1i(Some(&self.u_use_lighting_location), 0); // Disable lighting
        self.gl.uniform1i(Some(&self.u_is_black_hole_location), 0); // Disable black hole shader
        self.gl.uniform3f(Some(&self.u_uniform_color_location), r, g, b);

        let model = Matrix4::new_translation(&Vector3::new(x, y, z)) *
                    Matrix4::new_nonuniform_scaling(&Vector3::new(w, h, d));
        let mvp = projection * view * model;

        let mvp_array: [f32; 16] = mvp.as_slice().try_into().unwrap();
        self.gl.uniform_matrix4fv_with_f32_array(Some(&self.mvp_location), false, &mvp_array);

        self.gl.draw_elements_with_i32(
            WebGlRenderingContext::TRIANGLES,
            self.unit_cube_index_count,
            WebGlRenderingContext::UNSIGNED_SHORT,
            0
        );
    }

    pub fn draw_instanced_mesh(
        &self,
        mesh: &Mesh,
        instance_data: &[f32],
        count: i32,
        projection: &Matrix4<f32>,
        view: &Matrix4<f32>,
        light_pos: &Vector3<f32>,
    ) {
        let ext = match &self.instanced_ext {
            Some(e) => e,
            None => {
                web_sys::console::log_1(&"Instanced extension not found".into());
                return;
            },
        };

        self.gl.use_program(Some(&self.instanced_program));

        // web_sys::console::log_1(&format!("Drawing instanced: {} instances", count).into());

        self.gl.uniform_matrix4fv_with_f32_array(Some(&self.u_instanced_view_loc), false, view.as_slice());
        self.gl.uniform_matrix4fv_with_f32_array(Some(&self.u_instanced_proj_loc), false, projection.as_slice());
        self.gl.uniform3f(Some(&self.u_instanced_light_pos_loc), light_pos.x, light_pos.y, light_pos.z);
        self.gl.uniform1i(Some(&self.u_instanced_use_lighting_loc), 1); // Enable lighting for instanced
        self.gl.uniform3f(Some(&self.u_instanced_time_color_loc), 1.0, 1.0, 1.0);

        self.gl.bind_buffer(WebGlRenderingContext::ARRAY_BUFFER, Some(&self.dynamic_vertex_buffer));
        unsafe {
            let vert_array = js_sys::Float32Array::view(&mesh.vertices);
            self.gl.buffer_data_with_array_buffer_view(
                WebGlRenderingContext::ARRAY_BUFFER,
                &vert_array,
                WebGlRenderingContext::STATIC_DRAW
            );
        }

        self.gl.bind_buffer(WebGlRenderingContext::ELEMENT_ARRAY_BUFFER, Some(&self.dynamic_index_buffer));
        unsafe {
            let idx_array = js_sys::Uint16Array::view(&mesh.indices);
            self.gl.buffer_data_with_array_buffer_view(
                WebGlRenderingContext::ELEMENT_ARRAY_BUFFER,
                &idx_array,
                WebGlRenderingContext::STATIC_DRAW
            );
        }

        let pos_loc = self.gl.get_attrib_location(&self.instanced_program, "aPosition");
        let norm_loc = self.gl.get_attrib_location(&self.instanced_program, "aNormal");
        let tex_loc = self.gl.get_attrib_location(&self.instanced_program, "aTexCoord");

        if pos_loc != -1 {
            self.gl.vertex_attrib_pointer_with_i32(pos_loc as u32, 3, WebGlRenderingContext::FLOAT, false, 44, 0);
            self.gl.enable_vertex_attrib_array(pos_loc as u32);
        }

        if tex_loc != -1 {
            self.gl.vertex_attrib_pointer_with_i32(tex_loc as u32, 2, WebGlRenderingContext::FLOAT, false, 44, 24);
            self.gl.enable_vertex_attrib_array(tex_loc as u32);
        }

        if norm_loc != -1 {
            self.gl.vertex_attrib_pointer_with_i32(norm_loc as u32, 3, WebGlRenderingContext::FLOAT, false, 44, 32);
            self.gl.enable_vertex_attrib_array(norm_loc as u32);
        }

        self.gl.bind_buffer(WebGlRenderingContext::ARRAY_BUFFER, Some(&self.instance_data_buffer));
        unsafe {
            let data_array = js_sys::Float32Array::view(instance_data);
            self.gl.buffer_data_with_array_buffer_view(
                WebGlRenderingContext::ARRAY_BUFFER,
                &data_array,
                WebGlRenderingContext::DYNAMIC_DRAW
            );
        }

        let i_pos_loc = self.gl.get_attrib_location(&self.instanced_program, "aInstancePosition");
        let i_scale_loc = self.gl.get_attrib_location(&self.instanced_program, "aInstanceScale");
        let i_col_loc = self.gl.get_attrib_location(&self.instanced_program, "aInstanceColor");
        let i_light_loc = self.gl.get_attrib_location(&self.instanced_program, "aInstanceLight");

        let stride = 32; 

        if i_pos_loc != -1 {
            self.gl.vertex_attrib_pointer_with_i32(i_pos_loc as u32, 3, WebGlRenderingContext::FLOAT, false, stride, 0);
            self.gl.enable_vertex_attrib_array(i_pos_loc as u32);
            ext.vertex_attrib_divisor_angle(i_pos_loc as u32, 1);
        }

        if i_scale_loc != -1 {
            self.gl.vertex_attrib_pointer_with_i32(i_scale_loc as u32, 1, WebGlRenderingContext::FLOAT, false, stride, 12);
            self.gl.enable_vertex_attrib_array(i_scale_loc as u32);
            ext.vertex_attrib_divisor_angle(i_scale_loc as u32, 1);
        }

        if i_col_loc != -1 {
            self.gl.vertex_attrib_pointer_with_i32(i_col_loc as u32, 3, WebGlRenderingContext::FLOAT, false, stride, 16);
            self.gl.enable_vertex_attrib_array(i_col_loc as u32);
            ext.vertex_attrib_divisor_angle(i_col_loc as u32, 1);
        }

        if i_light_loc != -1 {
            self.gl.vertex_attrib_pointer_with_i32(i_light_loc as u32, 1, WebGlRenderingContext::FLOAT, false, stride, 28);
            self.gl.enable_vertex_attrib_array(i_light_loc as u32);
            ext.vertex_attrib_divisor_angle(i_light_loc as u32, 1);
        }

        ext.draw_elements_instanced_angle_with_i32(
            WebGlRenderingContext::TRIANGLES,
            mesh.indices.len() as i32,
            WebGlRenderingContext::UNSIGNED_SHORT,
            0,
            count
        );

        if i_pos_loc != -1 {
            ext.vertex_attrib_divisor_angle(i_pos_loc as u32, 0);
            self.gl.disable_vertex_attrib_array(i_pos_loc as u32);
        }
        if i_scale_loc != -1 {
            ext.vertex_attrib_divisor_angle(i_scale_loc as u32, 0);
            self.gl.disable_vertex_attrib_array(i_scale_loc as u32);
        }
        if i_col_loc != -1 {
            ext.vertex_attrib_divisor_angle(i_col_loc as u32, 0);
            self.gl.disable_vertex_attrib_array(i_col_loc as u32);
        }
        if i_light_loc != -1 {
            ext.vertex_attrib_divisor_angle(i_light_loc as u32, 0);
            self.gl.disable_vertex_attrib_array(i_light_loc as u32);
        }
    }

    pub fn draw_mesh(&self, mesh: &Mesh, x: f32, y: f32, z: f32, w: f32, h: f32, d: f32, rotation_x: f32, rotation_y: f32, rotation_z: f32, projection: &Matrix4<f32>, view: &Matrix4<f32>, texture: Option<&WebGlTexture>, night_texture: Option<&WebGlTexture>, color_override: Option<(f32, f32, f32)>, is_ring: bool, ring_inner_radius: Option<f32>, use_lighting: bool, is_black_hole: bool, is_frozen: bool, camera_pos: Option<(f32, f32, f32)>, background_texture: Option<&WebGlTexture>) {
        self.gl.use_program(Some(&self.program));
        
        // Enable lighting by default for meshes
        self.gl.uniform1i(Some(&self.u_use_lighting_location), if use_lighting { 1 } else { 0 });
        self.gl.uniform1i(Some(&self.u_is_ring_location), if is_ring { 1 } else { 0 });
        self.gl.uniform1f(Some(&self.u_ring_inner_radius_location), ring_inner_radius.unwrap_or(0.0));
        self.gl.uniform1i(Some(&self.u_is_black_hole_location), if is_black_hole { 1 } else { 0 });
        self.gl.uniform1i(Some(&self.u_is_frozen_location), if is_frozen { 1 } else { 0 });
        
        if let Some((cx, cy, cz)) = camera_pos {
            self.gl.uniform3f(Some(&self.u_camera_pos_location), cx, cy, cz);
        } else {
            self.gl.uniform3f(Some(&self.u_camera_pos_location), 0.0, 0.0, 0.0);
        }

        if let Some(bg_tex) = background_texture {
            self.gl.active_texture(WebGlRenderingContext::TEXTURE2);
            self.gl.bind_texture(WebGlRenderingContext::TEXTURE_2D, Some(bg_tex));
            self.gl.uniform1i(Some(&self.u_background_texture_location), 2);
        }

        if let Some(tex) = texture {
            self.gl.active_texture(WebGlRenderingContext::TEXTURE0);
            self.gl.bind_texture(WebGlRenderingContext::TEXTURE_2D, Some(tex));
            self.gl.uniform1i(Some(&self.u_use_texture_location), 1);
            self.gl.uniform1i(Some(&self.u_texture_location), 0);
            self.gl.uniform1i(Some(&self.u_use_uniform_color_location), 0);
        } else {
            self.gl.uniform1i(Some(&self.u_use_texture_location), 0);
            if let Some((r, g, b)) = color_override {
                self.gl.uniform1i(Some(&self.u_use_uniform_color_location), 1);
                self.gl.uniform3f(Some(&self.u_uniform_color_location), r, g, b);
            } else {
                self.gl.uniform1i(Some(&self.u_use_uniform_color_location), 0);
            }
        }

        if let Some(night_tex) = night_texture {
            self.gl.active_texture(WebGlRenderingContext::TEXTURE1);
            self.gl.bind_texture(WebGlRenderingContext::TEXTURE_2D, Some(night_tex));
            self.gl.uniform1i(Some(&self.u_use_night_texture_location), 1);
            self.gl.uniform1i(Some(&self.u_night_texture_location), 1);
        } else {
            self.gl.uniform1i(Some(&self.u_use_night_texture_location), 0);
        }

        self.gl.bind_buffer(WebGlRenderingContext::ARRAY_BUFFER, Some(&self.dynamic_vertex_buffer));
        unsafe {
            let vert_array = js_sys::Float32Array::view(&mesh.vertices);
            self.gl.buffer_data_with_array_buffer_view(
                WebGlRenderingContext::ARRAY_BUFFER,
                &vert_array,
                WebGlRenderingContext::STATIC_DRAW
            );
        }

        self.gl.bind_buffer(WebGlRenderingContext::ELEMENT_ARRAY_BUFFER, Some(&self.dynamic_index_buffer));
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
        let tex_loc = self.gl.get_attrib_location(&self.program, "aTexCoord") as u32;
        let norm_loc = self.gl.get_attrib_location(&self.program, "aNormal") as u32;

        // Stride is now 32 + 12 = 44 bytes (3 pos + 3 col + 2 tex + 3 norm) * 4 bytes/float
        // Wait, Mesh struct needs to be updated to include normals in the vertex buffer.
        // Currently Mesh::vertices is just a Vec<f32>.
        // Let's check Mesh implementation.
        // Assuming we update Mesh to include normals:
        // Position (3) + Color (3) + TexCoord (2) + Normal (3) = 11 floats = 44 bytes.
        
        // For now, let's assume the mesh data is updated.
        // If not, we need to update Mesh generation first.
        
        // Actually, let's check Mesh first.
        
        self.gl.vertex_attrib_pointer_with_i32(pos_loc, 3, WebGlRenderingContext::FLOAT, false, 44, 0);
        self.gl.enable_vertex_attrib_array(pos_loc);

        self.gl.vertex_attrib_pointer_with_i32(col_loc, 3, WebGlRenderingContext::FLOAT, false, 44, 12);
        self.gl.enable_vertex_attrib_array(col_loc);

        self.gl.vertex_attrib_pointer_with_i32(tex_loc, 2, WebGlRenderingContext::FLOAT, false, 44, 24);
        self.gl.enable_vertex_attrib_array(tex_loc);
        
        self.gl.vertex_attrib_pointer_with_i32(norm_loc, 3, WebGlRenderingContext::FLOAT, false, 44, 32);
        self.gl.enable_vertex_attrib_array(norm_loc);

        let model = Matrix4::new_translation(&Vector3::new(x, y, z)) *
                    Matrix4::from_axis_angle(&Vector3::z_axis(), rotation_z) *
                    Matrix4::from_axis_angle(&Vector3::x_axis(), rotation_x) *
                    Matrix4::from_axis_angle(&Vector3::y_axis(), rotation_y) *
                    Matrix4::new_nonuniform_scaling(&Vector3::new(w, h, d));
        let mvp = projection * view * model;

        let mvp_array: [f32; 16] = mvp.as_slice().try_into().unwrap();
        self.gl.uniform_matrix4fv_with_f32_array(Some(&self.mvp_location), false, &mvp_array);
        
        let model_array: [f32; 16] = model.as_slice().try_into().unwrap();
        self.gl.uniform_matrix4fv_with_f32_array(Some(&self.model_location), false, &model_array);
        
        // Normal matrix is the transpose of the inverse of the upper-left 3x3 part of the model matrix.
        // For uniform scaling and rotation, it's just the upper-left 3x3 of the model matrix.
        // But we have non-uniform scaling potentially.
        // nalgebra doesn't have a direct normal matrix helper for 4x4.
        // We can extract the 3x3 rotation part if scaling is uniform.
        // Or compute inverse transpose.
        
        let model_3x3 = model.fixed_view::<3, 3>(0, 0);
        let normal_matrix = model_3x3.try_inverse().unwrap_or_else(|| model_3x3.clone_owned()).transpose();
        
        let normal_matrix_array: [f32; 9] = normal_matrix.as_slice().try_into().unwrap();
        self.gl.uniform_matrix3fv_with_f32_array(Some(&self.normal_matrix_location), false, &normal_matrix_array);

        self.gl.draw_elements_with_i32(
            WebGlRenderingContext::TRIANGLES,
            mesh.indices.len() as i32,
            WebGlRenderingContext::UNSIGNED_SHORT,
            0
        );
    }

    pub fn draw_lines(&self, vertices: &[f32], r: f32, g: f32, b: f32, projection: &Matrix4<f32>, view: &Matrix4<f32>) {
        self.gl.bind_buffer(WebGlRenderingContext::ARRAY_BUFFER, Some(&self.dynamic_vertex_buffer));
        unsafe {
            let vert_array = js_sys::Float32Array::view(vertices);
            self.gl.buffer_data_with_array_buffer_view(
                WebGlRenderingContext::ARRAY_BUFFER,
                &vert_array,
                WebGlRenderingContext::DYNAMIC_DRAW
            );
        }

        let pos_loc = self.gl.get_attrib_location(&self.program, "aPosition") as u32;
        let col_loc = self.gl.get_attrib_location(&self.program, "aColor") as u32;
        let tex_loc = self.gl.get_attrib_location(&self.program, "aTexCoord") as u32;
        let norm_loc = self.gl.get_attrib_location(&self.program, "aNormal") as u32;

        self.gl.vertex_attrib_pointer_with_i32(pos_loc, 3, WebGlRenderingContext::FLOAT, false, 0, 0);
        self.gl.enable_vertex_attrib_array(pos_loc);
        
        self.gl.disable_vertex_attrib_array(col_loc);
        self.gl.disable_vertex_attrib_array(tex_loc);
        self.gl.disable_vertex_attrib_array(norm_loc);

        self.gl.uniform1i(Some(&self.u_use_uniform_color_location), 1);
        self.gl.uniform1i(Some(&self.u_use_texture_location), 0);
        // Disable lighting for lines
        self.gl.uniform1i(Some(&self.u_use_lighting_location), 0);
        self.gl.uniform1i(Some(&self.u_is_black_hole_location), 0);
        self.gl.uniform3f(Some(&self.u_uniform_color_location), r, g, b);

        let mvp = projection * view;
        let mvp_array: [f32; 16] = mvp.as_slice().try_into().unwrap();
        self.gl.uniform_matrix4fv_with_f32_array(Some(&self.mvp_location), false, &mvp_array);

        self.gl.draw_arrays(
            WebGlRenderingContext::LINE_STRIP,
            0,
            (vertices.len() / 3) as i32
        );
    }
    pub fn create_texture(&self, url: &str) -> Result<WebGlTexture, JsValue> {
        let texture = self.gl.create_texture().ok_or("Failed to create texture")?;
        self.gl.bind_texture(WebGlRenderingContext::TEXTURE_2D, Some(&texture));

        // Put a single pixel in the texture so we can use it immediately.
        let level = 0;
        let internal_format = WebGlRenderingContext::RGBA as i32;
        let width = 1;
        let height = 1;
        let border = 0;
        let src_format = WebGlRenderingContext::RGBA;
        let src_type = WebGlRenderingContext::UNSIGNED_BYTE;
        let pixel = [0u8, 0, 255, 255]; // Blue
        // We ignore the result of the initial pixel upload to ensure we return the texture object
        // even if this step fails (though it shouldn't).
        let _ = self.gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
            WebGlRenderingContext::TEXTURE_2D, level, internal_format, width, height, border, src_format, src_type, Some(&pixel)
        );

        let img = HtmlImageElement::new().unwrap();
        img.set_cross_origin(Some("anonymous"));
        
        let gl = self.gl.clone();
        let texture_clone = texture.clone();
        let img_clone = img.clone();
        let url_string = url.to_string();
        
        let onload = Closure::wrap(Box::new(move || {
            web_sys::console::log_1(&format!("Texture loaded: {}", url_string).into());
            gl.bind_texture(WebGlRenderingContext::TEXTURE_2D, Some(&texture_clone));
            gl.tex_image_2d_with_u32_and_u32_and_image(
                WebGlRenderingContext::TEXTURE_2D, 0, WebGlRenderingContext::RGBA as i32, WebGlRenderingContext::RGBA, WebGlRenderingContext::UNSIGNED_BYTE, &img_clone
            ).unwrap();
            
            // Check if power of 2
            if is_power_of_2(img_clone.width()) && is_power_of_2(img_clone.height()) {
                gl.generate_mipmap(WebGlRenderingContext::TEXTURE_2D);
            } else {
                gl.tex_parameteri(WebGlRenderingContext::TEXTURE_2D, WebGlRenderingContext::TEXTURE_WRAP_S, WebGlRenderingContext::CLAMP_TO_EDGE as i32);
                gl.tex_parameteri(WebGlRenderingContext::TEXTURE_2D, WebGlRenderingContext::TEXTURE_WRAP_T, WebGlRenderingContext::CLAMP_TO_EDGE as i32);
                gl.tex_parameteri(WebGlRenderingContext::TEXTURE_2D, WebGlRenderingContext::TEXTURE_MIN_FILTER, WebGlRenderingContext::LINEAR as i32);
            }
        }) as Box<dyn FnMut()>);

        let onerror = Closure::wrap(Box::new(move || {
            web_sys::console::error_1(&"Failed to load texture".into());
        }) as Box<dyn FnMut()>);

        img.set_onload(Some(onload.as_ref().unchecked_ref()));
        img.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        onload.forget();
        onerror.forget();
        
        img.set_src(url);

        Ok(texture)
    }
}

fn is_power_of_2(value: u32) -> bool {
    (value & (value - 1)) == 0
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

fn create_instanced_program(gl: &WebGlRenderingContext) -> Result<WebGlProgram, JsValue> {
    let vert_shader = compile_shader(gl, WebGlRenderingContext::VERTEX_SHADER, INSTANCED_VERTEX_SHADER)?;
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
