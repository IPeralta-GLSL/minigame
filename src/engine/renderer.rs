use wasm_bindgen::prelude::*;
use web_sys::{WebGlRenderingContext, WebGlProgram, WebGlBuffer, WebGlUniformLocation, HtmlCanvasElement, WebGlTexture, HtmlImageElement};
use nalgebra::{Matrix4, Vector3};
use crate::engine::mesh::Mesh;
use wasm_bindgen::JsCast;

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
    uniform vec3 uUniformColor;
    uniform bool uUseUniformColor;
    uniform vec3 uTimeColor;

    void main() {
        vec3 color;
        if (uUseUniformColor) {
            color = uUniformColor;
        } else {
            color = vColor;
        }

        // Apply time of day filter
        color *= uTimeColor;

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

pub struct Renderer {
    pub gl: WebGlRenderingContext,
    program: WebGlProgram,
    mvp_location: WebGlUniformLocation,
    u_uniform_color_location: WebGlUniformLocation,
    u_use_uniform_color_location: WebGlUniformLocation,
    u_time_color_location: WebGlUniformLocation,
    u_use_texture_location: WebGlUniformLocation,
    unit_cube_vertex_buffer: WebGlBuffer,
    unit_cube_index_buffer: WebGlBuffer,
    unit_cube_index_count: i32,
    dynamic_vertex_buffer: WebGlBuffer,
    dynamic_index_buffer: WebGlBuffer,
}

impl Renderer {
    pub fn new(gl: WebGlRenderingContext) -> Result<Self, JsValue> {
        let program = create_program(&gl)?;
        gl.use_program(Some(&program));

        let dynamic_vertex_buffer = gl.create_buffer().ok_or("Failed to create buffer")?;
        let dynamic_index_buffer = gl.create_buffer().ok_or("Failed to create buffer")?;

        let mvp_location = gl.get_uniform_location(&program, "uModelViewProjection")
            .ok_or("Failed to get uniform location")?;
        let u_uniform_color_location = gl.get_uniform_location(&program, "uUniformColor")
            .ok_or("Failed to get uUniformColor location")?;
        let u_use_uniform_color_location = gl.get_uniform_location(&program, "uUseUniformColor")
            .ok_or("Failed to get uUseUniformColor location")?;
        let u_time_color_location = gl.get_uniform_location(&program, "uTimeColor")
            .ok_or("Failed to get uTimeColor location")?;
        let u_use_texture_location = gl.get_uniform_location(&program, "uUseTexture")
            .ok_or("Failed to get uUseTexture location")?;

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

        Ok(Renderer {
            gl,
            program,
            mvp_location,
            u_uniform_color_location,
            u_use_uniform_color_location,
            u_time_color_location,
            u_use_texture_location,
            unit_cube_vertex_buffer,
            unit_cube_index_buffer,
            unit_cube_index_count,
            dynamic_vertex_buffer,
            dynamic_index_buffer,
        })
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

    pub fn canvas(&self) -> Option<HtmlCanvasElement> {
        self.gl.canvas().unwrap().dyn_into::<HtmlCanvasElement>().ok()
    }

    pub fn draw_cube(&self, x: f32, y: f32, z: f32, w: f32, h: f32, d: f32, r: f32, g: f32, b: f32, projection: &Matrix4<f32>, view: &Matrix4<f32>) {
        self.gl.bind_buffer(WebGlRenderingContext::ARRAY_BUFFER, Some(&self.unit_cube_vertex_buffer));
        self.gl.bind_buffer(WebGlRenderingContext::ELEMENT_ARRAY_BUFFER, Some(&self.unit_cube_index_buffer));

        let pos_loc = self.gl.get_attrib_location(&self.program, "aPosition") as u32;
        let col_loc = self.gl.get_attrib_location(&self.program, "aColor") as u32;
        let tex_loc = self.gl.get_attrib_location(&self.program, "aTexCoord") as u32;

        self.gl.vertex_attrib_pointer_with_i32(pos_loc, 3, WebGlRenderingContext::FLOAT, false, 32, 0);
        self.gl.enable_vertex_attrib_array(pos_loc);

        // We need to set these pointers even if unused, to avoid using pointers from other buffers
        self.gl.vertex_attrib_pointer_with_i32(col_loc, 3, WebGlRenderingContext::FLOAT, false, 32, 12);
        self.gl.enable_vertex_attrib_array(col_loc);

        self.gl.vertex_attrib_pointer_with_i32(tex_loc, 2, WebGlRenderingContext::FLOAT, false, 32, 24);
        self.gl.enable_vertex_attrib_array(tex_loc);

        self.gl.uniform1i(Some(&self.u_use_uniform_color_location), 1);
        self.gl.uniform1i(Some(&self.u_use_texture_location), 0);
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

    pub fn draw_mesh(&self, mesh: &Mesh, x: f32, y: f32, z: f32, w: f32, h: f32, d: f32, rotation_x: f32, rotation_y: f32, rotation_z: f32, projection: &Matrix4<f32>, view: &Matrix4<f32>, texture: Option<&WebGlTexture>) {
        if let Some(tex) = texture {
            self.gl.active_texture(WebGlRenderingContext::TEXTURE0);
            self.gl.bind_texture(WebGlRenderingContext::TEXTURE_2D, Some(tex));
            self.gl.uniform1i(Some(&self.u_use_texture_location), 1);
            self.gl.uniform1i(Some(&self.u_use_uniform_color_location), 0);
        } else {
            self.gl.uniform1i(Some(&self.u_use_texture_location), 0);
            self.gl.uniform1i(Some(&self.u_use_uniform_color_location), 0);
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

        self.gl.vertex_attrib_pointer_with_i32(pos_loc, 3, WebGlRenderingContext::FLOAT, false, 32, 0);
        self.gl.enable_vertex_attrib_array(pos_loc);

        self.gl.vertex_attrib_pointer_with_i32(col_loc, 3, WebGlRenderingContext::FLOAT, false, 32, 12);
        self.gl.enable_vertex_attrib_array(col_loc);

        self.gl.vertex_attrib_pointer_with_i32(tex_loc, 2, WebGlRenderingContext::FLOAT, false, 32, 24);
        self.gl.enable_vertex_attrib_array(tex_loc);

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

        self.gl.vertex_attrib_pointer_with_i32(pos_loc, 3, WebGlRenderingContext::FLOAT, false, 0, 0);
        self.gl.enable_vertex_attrib_array(pos_loc);
        
        self.gl.disable_vertex_attrib_array(col_loc);
        self.gl.disable_vertex_attrib_array(tex_loc);

        self.gl.uniform1i(Some(&self.u_use_uniform_color_location), 1);
        self.gl.uniform1i(Some(&self.u_use_texture_location), 0);
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
        self.gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
            WebGlRenderingContext::TEXTURE_2D, level, internal_format, width, height, border, src_format, src_type, Some(&pixel)
        )?;

        let img = HtmlImageElement::new().unwrap();
        img.set_cross_origin(Some("anonymous"));
        
        let gl = self.gl.clone();
        let texture_clone = texture.clone();
        let img_clone = img.clone();
        
        let onload = Closure::wrap(Box::new(move || {
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

        img.set_onload(Some(onload.as_ref().unchecked_ref()));
        onload.forget();
        
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
