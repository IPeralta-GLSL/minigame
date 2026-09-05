use wasm_bindgen::prelude::*;
use web_sys::{WebGl2RenderingContext, WebGlProgram, WebGlBuffer, WebGlUniformLocation, HtmlCanvasElement, WebGlTexture, HtmlImageElement, ImageBitmap, ImageBitmapOptions, Request, RequestInit, Response, ExtTextureFilterAnisotropic};
use nalgebra::{Matrix4, Vector3};
use crate::mesh::Mesh;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

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
    uniform float uGlobalAlpha;
    
    uniform vec3 uLightPos;
    const vec3 lightColor = vec3(0.75, 0.75, 0.75);
    const float ambientStrength = 0.3;

    uniform bool uUseLighting;
    uniform bool uIsBlackHole;
    uniform bool uIsFrozen;
    uniform vec3 uCameraPos;
    uniform sampler2D uBackgroundTexture;
    uniform bool uIsCloud;
    uniform float uTime;
    uniform float uCloudRotOffset;

    uniform int uPhotometryMode;
    uniform vec4 uPhotometryParams;
    uniform vec2 uPhotometryParams2;

    float hapkeHGPhase(float cosA, float g) {
        float d = 1.0 + g * g - 2.0 * g * cosA;
        return (1.0 - g * g) / (d * sqrt(max(d, 1e-6)));
    }

    float hapkePhotometry(float mu0, float mu, float cosA) {
        if (mu0 <= 0.0001) return 0.0;
        float w = uPhotometryParams.x;
        float g = uPhotometryParams.y;
        float h = uPhotometryParams.z;
        float b0 = uPhotometryParams.w;
        float limb = uPhotometryParams2.x;
        float beta = uPhotometryParams2.y;
        float base = pow(mu0 / (mu0 + mu + 1e-4), beta);
        float limbT = pow(mu + 1e-4, limb);
        float pRatio = min(hapkeHGPhase(cosA, g) / max(hapkeHGPhase(1.0, g), 1e-4), 8.0);
        float alpha = acos(clamp(cosA, -1.0, 1.0));
        float B = b0 / (1.0 + tan(alpha * 0.5) / max(h, 1e-3));
        float sq = sqrt(max(1.0 - w, 1e-4));
        float hm0 = (1.0 + 2.0 * mu0) / (1.0 + 2.0 * mu0 * sq);
        float hm = (1.0 + 2.0 * mu) / (1.0 + 2.0 * mu * sq);
        float multi = 0.6 * w * (hm0 * hm - 1.0);
        return base * limbT * pRatio * (1.0 + B) + multi;
    }

    // --- 3D noise (seamless on sphere, no UV seam) ---
    float cloudHash(vec3 p) {
        p = fract(p * vec3(0.1031, 0.1030, 0.0973));
        p += dot(p, p.yxz + 33.33);
        return fract((p.x + p.y) * p.z);
    }
    float cloudNoise(vec3 p) {
        vec3 i = floor(p); vec3 f = fract(p);
        f = f * f * (3.0 - 2.0 * f);
        return mix(
            mix(mix(cloudHash(i),              cloudHash(i+vec3(1,0,0)), f.x),
                mix(cloudHash(i+vec3(0,1,0)),  cloudHash(i+vec3(1,1,0)), f.x), f.y),
            mix(mix(cloudHash(i+vec3(0,0,1)),  cloudHash(i+vec3(1,0,1)), f.x),
                mix(cloudHash(i+vec3(0,1,1)),  cloudHash(i+vec3(1,1,1)), f.x), f.y), f.z);
    }
    float cloudFbm(vec3 p) {
        float v = 0.0; float a = 0.5;
        for (int i = 0; i < 6; i++) { v += a * cloudNoise(p); p = p * 2.1 + vec3(1.7, 9.2, 4.3); a *= 0.5; }
        return v;
    }
    // rotate point around Y axis (east-west wind drift)
    vec3 rotY(vec3 p, float a) {
        return vec3(p.x*cos(a)-p.z*sin(a), p.y, p.x*sin(a)+p.z*cos(a));
    }

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

        if (uIsCloud) {
            vec3 N   = normalize(vNormal);
            vec3 sph = normalize(vPos);

            // fuerte parallax – nubes claramente sobre la superficie
            vec3 viewDir = normalize(uCameraPos - vFragPos);
            vec3 tangent = viewDir - N * dot(viewDir, N);
            float cosV   = max(dot(viewDir, N), 0.05);
            vec3 lifted  = normalize(sph + tangent * (0.25 / cosV));

            float t      = uTime;
            float lat    = asin(clamp(lifted.y, -1.0, 1.0));
            float absLat = abs(lat);

            // ── CIRCULACIÓN ATMOSFÉRICA (3 celdas de Hadley/Ferrel/Polar) ──
            // Vientos alisios Hadley (0-30°): oeste, lentos
            float hadleyW  = (1.0 - smoothstep(0.0, 0.52, absLat)) * (-0.55);
            // Oestes de Ferrel (30-60°): este, fuertes (jet stream)
            float ferrelW  = smoothstep(0.52, 0.70, absLat) * (1.0 - smoothstep(0.95, 1.10, absLat)) * 1.70;
            // Polares del este (>60°): oeste, suave
            float polarWind = smoothstep(1.05, 1.57, absLat) * (-0.40);
            float drift    = hadleyW + ferrelW + polarWind; // relativo al planeta

            // ── COBERTURA ZONAL (basada en datos satelitales reales) ──
            // Cinturón subtropical claro ~25° (anticiclones subtropicales, desiertos)
            float subtrop  = max(0.0, 1.0 - abs(absLat - 0.44) / 0.19);
            // Alta cobertura en latitudes medias (frentes extra-tropicales)
            float midlat   = smoothstep(0.52, 0.78, absLat) * (1.0 - smoothstep(1.10, 1.40, absLat));
            // Alta cobertura polar (frente polar, ciclones árticos/antárticos)
            float polarCov = smoothstep(1.05, 1.40, absLat);
            float band = clamp(0.85 - 0.42 * subtrop + 0.32 * midlat + 0.35 * polarCov, 0.25, 1.15);

            // ── MÁSCARA TIERRA/OCÉANO ──
            // UV corregido al marco terrestre (descuenta la rotación diferencial de la capa de nubes)
            vec2 earthUV = vec2(fract(vTexCoord.x - uCloudRotOffset / 6.28318530), vTexCoord.y);
            vec4 earthSample = texture2D(uBackgroundTexture, earthUV);
            // Océano: canal azul dominante; tierra: rojo/verde dominantes
            float oceanness = smoothstep(0.0, 0.25, earthSample.b - max(earthSample.r, earthSample.g * 0.85));
            float terrainFactor = mix(0.68, 1.0, oceanness); // ~32 % menos nubes sobre tierra

            // ── LIFECYCLE: nacimiento/muerte de sistemas nubosos ──
            vec3 rP    = rotY(lifted, t * 0.012);
            float phase = cloudFbm(rP * 3.0 + vec3(7.3, 1.5, 4.8));
            float lifecycle = sin(t * 0.50 + phase * 9.42);
            float lifeW     = smoothstep(-0.3, 0.7, lifecycle);

            // ── CAPA 0: frentes sinópticos – elongados por el viento ──
            // Cada capa deriva a su velocidad según la celda atmosférica
            vec3 r0  = rotY(lifted, drift * 0.055 * t);
            // estira en dirección E-O para simular bandas de frente
            r0 = vec3(r0.x * 0.68, r0.y, r0.z);
            vec3 q0a = vec3(cloudFbm(r0*2.8), cloudFbm(r0*2.8+vec3(5.2,1.3,2.1)), cloudFbm(r0*2.8+vec3(3.7,8.1,0.5)));
            // doble domain-warp para formas orgánicas
            vec3 q0b = vec3(cloudFbm(r0*2.8 + q0a), cloudFbm(r0*2.8 + q0a.yzx), 0.0);
            float base = cloudFbm(r0*2.8 + 0.90*q0b);

            // ── CAPA 1: cúmulos y sistemas mesoescala ──
            vec3 r1  = rotY(lifted, drift * 0.10 * t);
            r1 = vec3(r1.x * 0.82, r1.y, r1.z);
            vec3 q1  = vec3(cloudFbm(r1*5.5+vec3(3.1,7.4,2.9)), cloudFbm(r1*5.5+vec3(1.8,0.4,6.3)), 0.0);
            float cumulus = cloudFbm(r1*5.5 + 0.65*q1);

            // ── CAPA 2: cirros – siguen el jet stream ──
            // Más rápido en latitudes medias (corriente en chorro)
            float jetBoost = 1.0 + 1.5 * ferrelW;
            vec3 r2  = rotY(lifted, drift * jetBoost * 0.18 * t);
            float wisp = cloudFbm(r2*11.0 + vec3(8.3,2.9,6.1));

            float density = (base*0.50 + cumulus*0.30 + wisp*0.20) * band * terrainFactor;
            density += (lifeW - 0.5) * 0.13;

            // ~55% cobertura global como textura 8K
            float cloudA = smoothstep(0.27, 0.46, density);
            if (cloudA < 0.01) discard;

            // sombreado volumétrico: núcleo blanco, bordes azul-gris
            float thick = smoothstep(0.35, 0.70, density);
            color = mix(vec3(0.58, 0.71, 0.87), vec3(0.97, 0.98, 1.00), thick);
            alpha = cloudA * 0.85;
        } else if (uUseTexture == 1) {
            vec4 texColor = texture2D(uTexture, texCoord);
            if (texColor.a < 0.1) {
                discard;
            }
            color *= texColor.rgb;
            alpha = texColor.a;
        }
        
        vec3 result;
        
        if (uUseLighting) {
            vec3 ambient = ambientStrength * lightColor;
            
            vec3 norm = normalize(vNormal);
            vec3 lightDir = normalize(uLightPos - vFragPos);

            float diff;
            if (uPhotometryMode == 1) {
                vec3 vDir = normalize(uCameraPos - vFragPos);
                diff = hapkePhotometry(dot(norm, lightDir), max(dot(norm, vDir), 0.0), clamp(dot(lightDir, vDir), -1.0, 1.0));
            } else {
                diff = max(dot(norm, lightDir), 0.0);
            }

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

        gl_FragColor = vec4(result, alpha * uGlobalAlpha);
    }
"#;

const SKYBOX_VERTEX_SHADER: &str = r#"
    attribute vec3 aPosition;
    varying vec3 vTexCoord;
    uniform mat4 uProjection;
    uniform mat4 uView;
    
    void main() {
        vTexCoord = aPosition;
        vec4 pos = uProjection * uView * vec4(aPosition, 1.0);
        gl_Position = pos.xyww; 
    }
"#;

const SKYBOX_FRAGMENT_SHADER: &str = r#"
    precision mediump float;
    varying vec3 vTexCoord;
    uniform sampler2D uSkybox;
    
    const vec2 invAtan = vec2(0.1591, 0.3183);
    vec2 SampleSphericalMap(vec3 v)
    {
        vec2 uv = vec2(atan(v.z, v.x), asin(v.y));
        uv *= invAtan;
        uv += 0.5;
        return uv;
    }
    
    void main() {
        vec2 uv = SampleSphericalMap(normalize(vTexCoord));
        gl_FragColor = texture2D(uSkybox, uv); 
    }
"#;

pub struct Renderer {
    pub gl: WebGl2RenderingContext,
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
    pub u_global_alpha_location: WebGlUniformLocation,
    pub u_is_cloud_location: Option<WebGlUniformLocation>,
    pub u_time_location: Option<WebGlUniformLocation>,
    pub u_cloud_rot_offset: Option<WebGlUniformLocation>,
    u_photometry_mode_location: WebGlUniformLocation,
    u_photometry_params_location: WebGlUniformLocation,
    u_photometry_params2_location: WebGlUniformLocation,
    unit_cube_vertex_buffer: WebGlBuffer,
    unit_cube_index_buffer: WebGlBuffer,
    unit_cube_index_count: i32,
    dynamic_vertex_buffer: WebGlBuffer,
    dynamic_index_buffer: WebGlBuffer,
    
    // Instancing
    instanced_program: WebGlProgram,
    u_instanced_view_loc: WebGlUniformLocation,
    u_instanced_proj_loc: WebGlUniformLocation,
    u_instanced_light_pos_loc: WebGlUniformLocation,
    u_instanced_use_lighting_loc: WebGlUniformLocation,
    u_instanced_time_color_loc: WebGlUniformLocation,
    u_instanced_use_texture_loc: WebGlUniformLocation,
    u_instanced_texture_loc: WebGlUniformLocation,
    pub u_instanced_global_alpha_loc: WebGlUniformLocation,
    u_instanced_photometry_mode_loc: WebGlUniformLocation,
    u_instanced_photometry_params_loc: WebGlUniformLocation,
    u_instanced_photometry_params2_loc: WebGlUniformLocation,
    u_instanced_camera_pos_loc: WebGlUniformLocation,
    instance_data_buffer: WebGlBuffer,
    max_anisotropy: Option<(u32, f32)>,

    // Skybox
    skybox_program: WebGlProgram,
    u_skybox_view_loc: WebGlUniformLocation,
    u_skybox_proj_loc: WebGlUniformLocation,
    u_skybox_texture_loc: WebGlUniformLocation,
}

impl Renderer {
    pub fn new(gl: WebGl2RenderingContext) -> Result<Self, JsValue> {
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
        let u_global_alpha_location = gl.get_uniform_location(&program, "uGlobalAlpha")
            .ok_or("Failed to get uGlobalAlpha location")?;
        let u_is_cloud_location = gl.get_uniform_location(&program, "uIsCloud");
        let u_time_location = gl.get_uniform_location(&program, "uTime");
        let u_cloud_rot_offset = gl.get_uniform_location(&program, "uCloudRotOffset");
        let u_photometry_mode_location = gl.get_uniform_location(&program, "uPhotometryMode")
            .ok_or("Failed to get uPhotometryMode location")?;
        let u_photometry_params_location = gl.get_uniform_location(&program, "uPhotometryParams")
            .ok_or("Failed to get uPhotometryParams location")?;
        let u_photometry_params2_location = gl.get_uniform_location(&program, "uPhotometryParams2")
            .ok_or("Failed to get uPhotometryParams2 location")?;

        // Instancing setup
        let instanced_program = create_instanced_program(&gl)?;
        let u_instanced_view_loc = gl.get_uniform_location(&instanced_program, "uView").ok_or("Failed to get uView")?;
        let u_instanced_proj_loc = gl.get_uniform_location(&instanced_program, "uProjection").ok_or("Failed to get uProjection")?;
        let u_instanced_light_pos_loc = gl.get_uniform_location(&instanced_program, "uLightPos").ok_or("Failed to get uLightPos")?;
        let u_instanced_use_lighting_loc = gl.get_uniform_location(&instanced_program, "uUseLighting").ok_or("Failed to get uUseLighting instanced")?;
        let u_instanced_time_color_loc = gl.get_uniform_location(&instanced_program, "uTimeColor").ok_or("Failed to get uTimeColor")?;
        let u_instanced_use_texture_loc = gl.get_uniform_location(&instanced_program, "uUseTexture").ok_or("Failed to get uUseTexture instanced")?;
        let u_instanced_texture_loc = gl.get_uniform_location(&instanced_program, "uTexture").ok_or("Failed to get uTexture instanced")?;
        let u_instanced_global_alpha_loc = gl.get_uniform_location(&instanced_program, "uGlobalAlpha").ok_or("Failed to get uGlobalAlpha instanced")?;
        let u_instanced_photometry_mode_loc = gl.get_uniform_location(&instanced_program, "uPhotometryMode").ok_or("Failed to get uPhotometryMode instanced")?;
        let u_instanced_photometry_params_loc = gl.get_uniform_location(&instanced_program, "uPhotometryParams").ok_or("Failed to get uPhotometryParams instanced")?;
        let u_instanced_photometry_params2_loc = gl.get_uniform_location(&instanced_program, "uPhotometryParams2").ok_or("Failed to get uPhotometryParams2 instanced")?;
        let u_instanced_camera_pos_loc = gl.get_uniform_location(&instanced_program, "uCameraPos").ok_or("Failed to get uCameraPos instanced")?;
        let instance_data_buffer = gl.create_buffer().ok_or("Failed to create instance buffer")?;

        let max_anisotropy = gl
            .get_extension("EXT_texture_filter_anisotropic")
            .ok()
            .flatten()
            .and_then(|_| {
                let param = ExtTextureFilterAnisotropic::TEXTURE_MAX_ANISOTROPY_EXT;
                let max = gl
                    .get_parameter(ExtTextureFilterAnisotropic::MAX_TEXTURE_MAX_ANISOTROPY_EXT)
                    .ok()?
                    .as_f64()? as f32;
                if max > 0.0 { Some((param, max.min(8.0))) } else { None }
            });

        // Skybox setup
        let skybox_program = create_skybox_program(&gl)?;
        let u_skybox_view_loc = gl.get_uniform_location(&skybox_program, "uView").ok_or("Failed to get uView skybox")?;
        let u_skybox_proj_loc = gl.get_uniform_location(&skybox_program, "uProjection").ok_or("Failed to get uProjection skybox")?;
        let u_skybox_texture_loc = gl.get_uniform_location(&skybox_program, "uSkybox").ok_or("Failed to get uSkybox")?;

        // Create unit cube buffers
        let unit_cube_vertex_buffer = gl.create_buffer().ok_or("Failed to create unit cube buffer")?;
        let unit_cube_index_buffer = gl.create_buffer().ok_or("Failed to create unit cube index buffer")?;
        
        let unit_cube = Mesh::cube(1.0, 1.0, 1.0, 1.0); // White unit cube
        
        gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&unit_cube_vertex_buffer));
        unsafe {
            let vert_array = js_sys::Float32Array::view(&unit_cube.vertices);
            gl.buffer_data_with_array_buffer_view(
                WebGl2RenderingContext::ARRAY_BUFFER,
                &vert_array,
                WebGl2RenderingContext::STATIC_DRAW
            );
        }

        gl.bind_buffer(WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER, Some(&unit_cube_index_buffer));
        unsafe {
            let idx_array = js_sys::Uint16Array::view(&unit_cube.indices);
            gl.buffer_data_with_array_buffer_view(
                WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER,
                &idx_array,
                WebGl2RenderingContext::STATIC_DRAW
            );
        }
        let unit_cube_index_count = unit_cube.indices.len() as i32;

        // Initialize time color to white (no filter)
        gl.uniform3f(Some(&u_time_color_location), 1.0, 1.0, 1.0);
        // Initialize light pos to 0,0,0
        gl.uniform3f(Some(&u_light_pos_location), 0.0, 0.0, 0.0);
        // Initialize global alpha to 1.0
        gl.uniform1f(Some(&u_global_alpha_location), 1.0);

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
            u_global_alpha_location,
            u_is_cloud_location,
            u_time_location,
            u_cloud_rot_offset,
            u_photometry_mode_location,
            u_photometry_params_location,
            u_photometry_params2_location,
            instanced_program,
            u_instanced_view_loc,
            u_instanced_proj_loc,
            u_instanced_light_pos_loc,
            u_instanced_use_lighting_loc,
            u_instanced_time_color_loc,
            u_instanced_use_texture_loc,
            u_instanced_texture_loc,
            u_instanced_global_alpha_loc,
            u_instanced_photometry_mode_loc,
            u_instanced_photometry_params_loc,
            u_instanced_photometry_params2_loc,
            u_instanced_camera_pos_loc,
            instance_data_buffer,
            max_anisotropy,
            skybox_program,
            u_skybox_view_loc,
            u_skybox_proj_loc,
            u_skybox_texture_loc,
        })
    }

    pub fn set_light_position(&self, x: f32, y: f32, z: f32) {
        self.gl.uniform3f(Some(&self.u_light_pos_location), x, y, z);
    }

    pub fn clear(&self, r: f32, g: f32, b: f32) {
        self.gl.clear_color(r, g, b, 1.0);
        self.gl.clear(WebGl2RenderingContext::COLOR_BUFFER_BIT | WebGl2RenderingContext::DEPTH_BUFFER_BIT);
    }

    pub fn set_time_color(&self, r: f32, g: f32, b: f32) {
        self.gl.uniform3f(Some(&self.u_time_color_location), r, g, b);
    }

    pub fn enable_depth_test(&self) {
        self.gl.enable(WebGl2RenderingContext::DEPTH_TEST);
    }

    pub fn enable_face_culling(&self) {
        self.gl.enable(WebGl2RenderingContext::CULL_FACE);
        self.gl.cull_face(WebGl2RenderingContext::BACK);
    }

    pub fn enable_blend(&self) {
        self.gl.enable(WebGl2RenderingContext::BLEND);
        self.gl.blend_func(WebGl2RenderingContext::SRC_ALPHA, WebGl2RenderingContext::ONE_MINUS_SRC_ALPHA);
    }

    pub fn disable_blend(&self) {
        self.gl.disable(WebGl2RenderingContext::BLEND);
    }

    pub fn resize(&self, width: i32, height: i32) {
        self.gl.viewport(0, 0, width, height);
    }

    pub fn clear_screen(&self, r: f32, g: f32, b: f32) {
        self.gl.clear_color(r, g, b, 1.0);
        self.gl.clear(WebGl2RenderingContext::COLOR_BUFFER_BIT | WebGl2RenderingContext::DEPTH_BUFFER_BIT);
    }

    pub fn canvas(&self) -> Option<HtmlCanvasElement> {
        self.gl.canvas().unwrap().dyn_into::<HtmlCanvasElement>().ok()
    }

    pub fn draw_cube(&self, x: f32, y: f32, z: f32, w: f32, h: f32, d: f32, r: f32, g: f32, b: f32, projection: &Matrix4<f32>, view: &Matrix4<f32>) {
        self.gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&self.unit_cube_vertex_buffer));
        self.gl.bind_buffer(WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER, Some(&self.unit_cube_index_buffer));

        let pos_loc = self.gl.get_attrib_location(&self.program, "aPosition") as u32;
        let col_loc = self.gl.get_attrib_location(&self.program, "aColor") as u32;
        let tex_loc = self.gl.get_attrib_location(&self.program, "aTexCoord") as u32;
        let norm_loc = self.gl.get_attrib_location(&self.program, "aNormal") as u32;

        self.gl.vertex_attrib_pointer_with_i32(pos_loc, 3, WebGl2RenderingContext::FLOAT, false, 44, 0);
        self.gl.enable_vertex_attrib_array(pos_loc);

        // We need to set these pointers even if unused, to avoid using pointers from other buffers
        self.gl.vertex_attrib_pointer_with_i32(col_loc, 3, WebGl2RenderingContext::FLOAT, false, 44, 12);
        self.gl.enable_vertex_attrib_array(col_loc);

        self.gl.vertex_attrib_pointer_with_i32(tex_loc, 2, WebGl2RenderingContext::FLOAT, false, 44, 24);
        self.gl.enable_vertex_attrib_array(tex_loc);
        
        self.gl.vertex_attrib_pointer_with_i32(norm_loc, 3, WebGl2RenderingContext::FLOAT, false, 44, 32);
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
            WebGl2RenderingContext::TRIANGLES,
            self.unit_cube_index_count,
            WebGl2RenderingContext::UNSIGNED_SHORT,
            0
        );
    }

    pub fn draw_textured_cube(&self, x: f32, y: f32, z: f32, w: f32, h: f32, d: f32, texture: Option<&WebGlTexture>, projection: &Matrix4<f32>, view: &Matrix4<f32>) {
        self.gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&self.unit_cube_vertex_buffer));
        self.gl.bind_buffer(WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER, Some(&self.unit_cube_index_buffer));

        let pos_loc = self.gl.get_attrib_location(&self.program, "aPosition") as u32;
        let col_loc = self.gl.get_attrib_location(&self.program, "aColor") as u32;
        let tex_loc = self.gl.get_attrib_location(&self.program, "aTexCoord") as u32;
        let norm_loc = self.gl.get_attrib_location(&self.program, "aNormal") as u32;

        self.gl.vertex_attrib_pointer_with_i32(pos_loc, 3, WebGl2RenderingContext::FLOAT, false, 44, 0);
        self.gl.enable_vertex_attrib_array(pos_loc);
        self.gl.vertex_attrib_pointer_with_i32(col_loc, 3, WebGl2RenderingContext::FLOAT, false, 44, 12);
        self.gl.enable_vertex_attrib_array(col_loc);
        self.gl.vertex_attrib_pointer_with_i32(tex_loc, 2, WebGl2RenderingContext::FLOAT, false, 44, 24);
        self.gl.enable_vertex_attrib_array(tex_loc);
        self.gl.vertex_attrib_pointer_with_i32(norm_loc, 3, WebGl2RenderingContext::FLOAT, false, 44, 32);
        self.gl.enable_vertex_attrib_array(norm_loc);

        self.gl.uniform1i(Some(&self.u_use_uniform_color_location), 0);
        self.gl.uniform1i(Some(&self.u_use_lighting_location), 0);
        self.gl.uniform1i(Some(&self.u_is_black_hole_location), 0);

        if let Some(tex) = texture {
            self.gl.active_texture(WebGl2RenderingContext::TEXTURE0);
            self.gl.bind_texture(WebGl2RenderingContext::TEXTURE_2D, Some(tex));
            self.gl.uniform1i(Some(&self.u_use_texture_location), 1);
            self.gl.uniform1i(Some(&self.u_texture_location), 0);
        } else {
            self.gl.uniform1i(Some(&self.u_use_texture_location), 0);
        }

        let model = Matrix4::new_translation(&Vector3::new(x, y, z)) *
                    Matrix4::new_nonuniform_scaling(&Vector3::new(w, h, d));
        let mvp = projection * view * model;

        let mvp_array: [f32; 16] = mvp.as_slice().try_into().unwrap();
        self.gl.uniform_matrix4fv_with_f32_array(Some(&self.mvp_location), false, &mvp_array);

        self.gl.draw_elements_with_i32(
            WebGl2RenderingContext::TRIANGLES,
            self.unit_cube_index_count,
            WebGl2RenderingContext::UNSIGNED_SHORT,
            0
        );
    }

    pub fn draw_skybox(&self, mesh: &Mesh, projection: &Matrix4<f32>, view: &Matrix4<f32>, texture: Option<&WebGlTexture>) {
        self.gl.use_program(Some(&self.skybox_program));
        
        // Disable depth write so skybox is always behind
        self.gl.depth_mask(false);
        
        // Bind uniforms
        // Remove translation from view matrix for skybox
        let mut view_no_trans = *view;
        view_no_trans[(0, 3)] = 0.0;
        view_no_trans[(1, 3)] = 0.0;
        view_no_trans[(2, 3)] = 0.0;

        let view_slice: &[f32] = view_no_trans.as_slice();
        self.gl.uniform_matrix4fv_with_f32_array(Some(&self.u_skybox_view_loc), false, view_slice);
        
        let proj_slice: &[f32] = projection.as_slice();
        self.gl.uniform_matrix4fv_with_f32_array(Some(&self.u_skybox_proj_loc), false, proj_slice);
        
        // Bind texture
        if let Some(tex) = texture {
            self.gl.active_texture(WebGl2RenderingContext::TEXTURE0);
            self.gl.bind_texture(WebGl2RenderingContext::TEXTURE_2D, Some(tex));
            self.gl.uniform1i(Some(&self.u_skybox_texture_loc), 0);
        }
        
        // Upload mesh
        self.gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&self.dynamic_vertex_buffer));
        unsafe {
            let vert_array = js_sys::Float32Array::view(&mesh.vertices);
            self.gl.buffer_data_with_array_buffer_view(
                WebGl2RenderingContext::ARRAY_BUFFER,
                &vert_array,
                WebGl2RenderingContext::STATIC_DRAW
            );
        }

        self.gl.bind_buffer(WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER, Some(&self.dynamic_index_buffer));
        unsafe {
            let idx_array = js_sys::Uint16Array::view(&mesh.indices);
            self.gl.buffer_data_with_array_buffer_view(
                WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER,
                &idx_array,
                WebGl2RenderingContext::STATIC_DRAW
            );
        }
        
        // Attributes
        let pos_loc = self.gl.get_attrib_location(&self.skybox_program, "aPosition");
        if pos_loc != -1 {
            self.gl.vertex_attrib_pointer_with_i32(pos_loc as u32, 3, WebGl2RenderingContext::FLOAT, false, 44, 0);
            self.gl.enable_vertex_attrib_array(pos_loc as u32);
        }
        
        self.gl.draw_elements_with_i32(
            WebGl2RenderingContext::TRIANGLES,
            mesh.indices.len() as i32,
            WebGl2RenderingContext::UNSIGNED_SHORT,
            0
        );
        
        // Re-enable depth mask
        self.gl.depth_mask(true);
    }

    pub fn draw_instanced_mesh(
        &self,
        mesh: &Mesh,
        instance_data: &[f32],
        count: i32,
        projection: &Matrix4<f32>,
        view: &Matrix4<f32>,
        light_pos: &Vector3<f32>,
        texture: Option<&WebGlTexture>,
        alpha: f32,
        camera_pos: Option<(f32, f32, f32)>,
        photometry: Option<[f32; 6]>,
    ) {
        self.gl.use_program(Some(&self.instanced_program));

        // web_sys::console::log_1(&format!("Drawing instanced: {} instances", count).into());

        self.gl.uniform_matrix4fv_with_f32_array(Some(&self.u_instanced_view_loc), false, view.as_slice());
        self.gl.uniform_matrix4fv_with_f32_array(Some(&self.u_instanced_proj_loc), false, projection.as_slice());
        self.gl.uniform3f(Some(&self.u_instanced_light_pos_loc), light_pos.x, light_pos.y, light_pos.z);
        self.gl.uniform1i(Some(&self.u_instanced_use_lighting_loc), 1); // Enable lighting for instanced
        self.gl.uniform3f(Some(&self.u_instanced_time_color_loc), 1.0, 1.0, 1.0);
        self.gl.uniform1f(Some(&self.u_instanced_global_alpha_loc), alpha);

        match camera_pos {
            Some((cx, cy, cz)) => self.gl.uniform3f(Some(&self.u_instanced_camera_pos_loc), cx, cy, cz),
            None => self.gl.uniform3f(Some(&self.u_instanced_camera_pos_loc), 0.0, 0.0, 0.0),
        }

        match photometry {
            Some(p) => {
                self.gl.uniform1i(Some(&self.u_instanced_photometry_mode_loc), 1);
                self.gl.uniform4f(Some(&self.u_instanced_photometry_params_loc), p[0], p[1], p[2], p[3]);
                self.gl.uniform2f(Some(&self.u_instanced_photometry_params2_loc), p[4], p[5]);
            }
            None => {
                self.gl.uniform1i(Some(&self.u_instanced_photometry_mode_loc), 0);
            }
        }

        if let Some(tex) = texture {
            self.gl.active_texture(WebGl2RenderingContext::TEXTURE0);
            self.gl.bind_texture(WebGl2RenderingContext::TEXTURE_2D, Some(tex));
            self.gl.uniform1i(Some(&self.u_instanced_use_texture_loc), 1);
            self.gl.uniform1i(Some(&self.u_instanced_texture_loc), 0);
        } else {
            self.gl.uniform1i(Some(&self.u_instanced_use_texture_loc), 0);
        }

        self.gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&self.dynamic_vertex_buffer));
        unsafe {
            let vert_array = js_sys::Float32Array::view(&mesh.vertices);
            self.gl.buffer_data_with_array_buffer_view(
                WebGl2RenderingContext::ARRAY_BUFFER,
                &vert_array,
                WebGl2RenderingContext::STATIC_DRAW
            );
        }

        self.gl.bind_buffer(WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER, Some(&self.dynamic_index_buffer));
        unsafe {
            let idx_array = js_sys::Uint16Array::view(&mesh.indices);
            self.gl.buffer_data_with_array_buffer_view(
                WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER,
                &idx_array,
                WebGl2RenderingContext::STATIC_DRAW
            );
        }

        let pos_loc = self.gl.get_attrib_location(&self.instanced_program, "aPosition");
        let norm_loc = self.gl.get_attrib_location(&self.instanced_program, "aNormal");
        let tex_loc = self.gl.get_attrib_location(&self.instanced_program, "aTexCoord");

        if pos_loc != -1 {
            self.gl.vertex_attrib_pointer_with_i32(pos_loc as u32, 3, WebGl2RenderingContext::FLOAT, false, 44, 0);
            self.gl.enable_vertex_attrib_array(pos_loc as u32);
        }

        if tex_loc != -1 {
            self.gl.vertex_attrib_pointer_with_i32(tex_loc as u32, 2, WebGl2RenderingContext::FLOAT, false, 44, 24);
            self.gl.enable_vertex_attrib_array(tex_loc as u32);
        }

        if norm_loc != -1 {
            self.gl.vertex_attrib_pointer_with_i32(norm_loc as u32, 3, WebGl2RenderingContext::FLOAT, false, 44, 32);
            self.gl.enable_vertex_attrib_array(norm_loc as u32);
        }

        self.gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&self.instance_data_buffer));
        unsafe {
            let data_array = js_sys::Float32Array::view(instance_data);
            self.gl.buffer_data_with_array_buffer_view(
                WebGl2RenderingContext::ARRAY_BUFFER,
                &data_array,
                WebGl2RenderingContext::DYNAMIC_DRAW
            );
        }

        let i_pos_loc = self.gl.get_attrib_location(&self.instanced_program, "aInstancePosition");
        let i_scale_loc = self.gl.get_attrib_location(&self.instanced_program, "aInstanceScale");
        let i_col_loc = self.gl.get_attrib_location(&self.instanced_program, "aInstanceColor");
        let i_light_loc = self.gl.get_attrib_location(&self.instanced_program, "aInstanceLight");

        let stride = 32; // 3+1+3+1 = 8 floats * 4 bytes = 32 bytes

        if i_pos_loc != -1 {
            self.gl.vertex_attrib_pointer_with_i32(i_pos_loc as u32, 3, WebGl2RenderingContext::FLOAT, false, stride, 0);
            self.gl.enable_vertex_attrib_array(i_pos_loc as u32);
            self.gl.vertex_attrib_divisor(i_pos_loc as u32, 1);
        }

        if i_scale_loc != -1 {
            self.gl.vertex_attrib_pointer_with_i32(i_scale_loc as u32, 1, WebGl2RenderingContext::FLOAT, false, stride, 12);
            self.gl.enable_vertex_attrib_array(i_scale_loc as u32);
            self.gl.vertex_attrib_divisor(i_scale_loc as u32, 1);
        }

        if i_col_loc != -1 {
            self.gl.vertex_attrib_pointer_with_i32(i_col_loc as u32, 3, WebGl2RenderingContext::FLOAT, false, stride, 16);
            self.gl.enable_vertex_attrib_array(i_col_loc as u32);
            self.gl.vertex_attrib_divisor(i_col_loc as u32, 1);
        }

        if i_light_loc != -1 {
            self.gl.vertex_attrib_pointer_with_i32(i_light_loc as u32, 1, WebGl2RenderingContext::FLOAT, false, stride, 28);
            self.gl.enable_vertex_attrib_array(i_light_loc as u32);
            self.gl.vertex_attrib_divisor(i_light_loc as u32, 1);
        }

        self.gl.draw_elements_instanced_with_i32(
            WebGl2RenderingContext::TRIANGLES,
            mesh.indices.len() as i32,
            WebGl2RenderingContext::UNSIGNED_SHORT,
            0,
            count
        );

        if i_pos_loc != -1 {
            self.gl.vertex_attrib_divisor(i_pos_loc as u32, 0);
            self.gl.disable_vertex_attrib_array(i_pos_loc as u32);
        }
        if i_scale_loc != -1 {
            self.gl.vertex_attrib_divisor(i_scale_loc as u32, 0);
            self.gl.disable_vertex_attrib_array(i_scale_loc as u32);
        }
        if i_col_loc != -1 {
            self.gl.vertex_attrib_divisor(i_col_loc as u32, 0);
            self.gl.disable_vertex_attrib_array(i_col_loc as u32);
        }
        if i_light_loc != -1 {
            self.gl.vertex_attrib_divisor(i_light_loc as u32, 0);
            self.gl.disable_vertex_attrib_array(i_light_loc as u32);
        }
    }

    pub fn draw_mesh(&self, mesh: &Mesh, x: f32, y: f32, z: f32, w: f32, h: f32, d: f32, rotation_x: f32, rotation_y: f32, rotation_z: f32, projection: &Matrix4<f32>, view: &Matrix4<f32>, texture: Option<&WebGlTexture>, night_texture: Option<&WebGlTexture>, color_override: Option<(f32, f32, f32)>, is_ring: bool, ring_inner_radius: Option<f32>, use_lighting: bool, is_black_hole: bool, is_frozen: bool, camera_pos: Option<(f32, f32, f32)>, background_texture: Option<&WebGlTexture>, photometry: Option<[f32; 6]>) {
        self.gl.use_program(Some(&self.program));
        
        self.gl.uniform1i(Some(&self.u_use_lighting_location), if use_lighting { 1 } else { 0 });
        match photometry {
            Some(p) => {
                self.gl.uniform1i(Some(&self.u_photometry_mode_location), 1);
                self.gl.uniform4f(Some(&self.u_photometry_params_location), p[0], p[1], p[2], p[3]);
                self.gl.uniform2f(Some(&self.u_photometry_params2_location), p[4], p[5]);
            }
            None => {
                self.gl.uniform1i(Some(&self.u_photometry_mode_location), 0);
            }
        }
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
            self.gl.active_texture(WebGl2RenderingContext::TEXTURE2);
            self.gl.bind_texture(WebGl2RenderingContext::TEXTURE_2D, Some(bg_tex));
            self.gl.uniform1i(Some(&self.u_background_texture_location), 2);
        }

        if let Some(tex) = texture {
            self.gl.active_texture(WebGl2RenderingContext::TEXTURE0);
            self.gl.bind_texture(WebGl2RenderingContext::TEXTURE_2D, Some(tex));
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
            self.gl.active_texture(WebGl2RenderingContext::TEXTURE1);
            self.gl.bind_texture(WebGl2RenderingContext::TEXTURE_2D, Some(night_tex));
            self.gl.uniform1i(Some(&self.u_use_night_texture_location), 1);
            self.gl.uniform1i(Some(&self.u_night_texture_location), 1);
        } else {
            self.gl.uniform1i(Some(&self.u_use_night_texture_location), 0);
        }

        self.gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&self.dynamic_vertex_buffer));
        unsafe {
            let vert_array = js_sys::Float32Array::view(&mesh.vertices);
            self.gl.buffer_data_with_array_buffer_view(
                WebGl2RenderingContext::ARRAY_BUFFER,
                &vert_array,
                WebGl2RenderingContext::STATIC_DRAW
            );
        }

        self.gl.bind_buffer(WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER, Some(&self.dynamic_index_buffer));
        unsafe {
            let idx_array = js_sys::Uint16Array::view(&mesh.indices);
            self.gl.buffer_data_with_array_buffer_view(
                WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER,
                &idx_array,
                WebGl2RenderingContext::STATIC_DRAW
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
        
        self.gl.vertex_attrib_pointer_with_i32(pos_loc, 3, WebGl2RenderingContext::FLOAT, false, 44, 0);
        self.gl.enable_vertex_attrib_array(pos_loc);

        self.gl.vertex_attrib_pointer_with_i32(col_loc, 3, WebGl2RenderingContext::FLOAT, false, 44, 12);
        self.gl.enable_vertex_attrib_array(col_loc);

        self.gl.vertex_attrib_pointer_with_i32(tex_loc, 2, WebGl2RenderingContext::FLOAT, false, 44, 24);
        self.gl.enable_vertex_attrib_array(tex_loc);
        
        self.gl.vertex_attrib_pointer_with_i32(norm_loc, 3, WebGl2RenderingContext::FLOAT, false, 44, 32);
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
            WebGl2RenderingContext::TRIANGLES,
            mesh.indices.len() as i32,
            WebGl2RenderingContext::UNSIGNED_SHORT,
            0
        );
    }

    pub fn draw_lines(&self, vertices: &[f32], r: f32, g: f32, b: f32, projection: &Matrix4<f32>, view: &Matrix4<f32>) {
        self.gl.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&self.dynamic_vertex_buffer));
        unsafe {
            let vert_array = js_sys::Float32Array::view(vertices);
            self.gl.buffer_data_with_array_buffer_view(
                WebGl2RenderingContext::ARRAY_BUFFER,
                &vert_array,
                WebGl2RenderingContext::DYNAMIC_DRAW
            );
        }

        let pos_loc = self.gl.get_attrib_location(&self.program, "aPosition") as u32;
        let col_loc = self.gl.get_attrib_location(&self.program, "aColor") as u32;
        let tex_loc = self.gl.get_attrib_location(&self.program, "aTexCoord") as u32;
        let norm_loc = self.gl.get_attrib_location(&self.program, "aNormal") as u32;

        self.gl.vertex_attrib_pointer_with_i32(pos_loc, 3, WebGl2RenderingContext::FLOAT, false, 0, 0);
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
            WebGl2RenderingContext::LINE_STRIP,
            0,
            (vertices.len() / 3) as i32
        );
    }
    pub fn create_texture(&self, url: &str) -> Result<WebGlTexture, JsValue> {
        let texture = self.create_placeholder_texture()?;
        let gl = self.gl.clone();
        let target = texture.clone();
        let aniso = self.max_anisotropy;
        let url_owned = url.to_string();
        wasm_bindgen_futures::spawn_local(async move {
            let _permit = AcquireStreamPermit.await;
            match load_texture_source(&url_owned).await {
                Ok(source) => upload_texture_source(&gl, &target, &source, aniso),
                Err(_) => web_sys::console::error_1(&format!("Failed to load texture: {}", url_owned).into()),
            }
        });
        Ok(texture)
    }

    pub fn create_streamed_texture(&self, low_url: Option<&str>, high_url: &str) -> Result<WebGlTexture, JsValue> {
        let texture = self.create_placeholder_texture()?;
        let gl = self.gl.clone();
        let target = texture.clone();
        let aniso = self.max_anisotropy;
        let low = low_url.map(|u| u.to_string());
        let high = high_url.to_string();
        wasm_bindgen_futures::spawn_local(async move {
            let _permit = AcquireStreamPermit.await;
            if let Some(low) = &low {
                if let Ok(source) = load_texture_source(low).await {
                    upload_texture_source(&gl, &target, &source, aniso);
                }
            }
            match load_texture_source(&high).await {
                Ok(source) => upload_texture_source(&gl, &target, &source, aniso),
                Err(_) => web_sys::console::error_1(&format!("Failed to load texture: {}", high).into()),
            }
        });
        Ok(texture)
    }

    fn create_placeholder_texture(&self) -> Result<WebGlTexture, JsValue> {
        let texture = self.gl.create_texture().ok_or("Failed to create texture")?;
        self.gl.bind_texture(WebGl2RenderingContext::TEXTURE_2D, Some(&texture));
        let _ = self.gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
            WebGl2RenderingContext::TEXTURE_2D,
            0,
            WebGl2RenderingContext::RGBA as i32,
            1,
            1,
            0,
            WebGl2RenderingContext::RGBA,
            WebGl2RenderingContext::UNSIGNED_BYTE,
            Some(&[24u8, 26, 34, 255]),
        );
        Ok(texture)
    }
}

const MAX_ACTIVE_STREAMS: usize = 3;

enum TextureSource {
    Bitmap(ImageBitmap),
    Image(HtmlImageElement),
}

struct StreamSemaphore {
    active: usize,
    waiters: VecDeque<Waker>,
}

thread_local! {
    static STREAM_SEMAPHORE: RefCell<StreamSemaphore> = RefCell::new(StreamSemaphore { active: 0, waiters: VecDeque::new() });
    static RETAINED_CLOSURES: RefCell<Vec<Closure<dyn FnMut()>>> = RefCell::new(Vec::new());
}

struct StreamPermit;

impl Drop for StreamPermit {
    fn drop(&mut self) {
        let waker = STREAM_SEMAPHORE.with(|s| {
            let mut s = s.borrow_mut();
            s.active = s.active.saturating_sub(1);
            s.waiters.pop_front()
        });
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

struct AcquireStreamPermit;

impl Future for AcquireStreamPermit {
    type Output = StreamPermit;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        STREAM_SEMAPHORE.with(|s| {
            let mut s = s.borrow_mut();
            if s.active < MAX_ACTIVE_STREAMS {
                s.active += 1;
                Poll::Ready(StreamPermit)
            } else {
                s.waiters.push_back(cx.waker().clone());
                Poll::Pending
            }
        })
    }
}

async fn load_texture_source(url: &str) -> Result<TextureSource, JsValue> {
    match fetch_image_bitmap(url).await {
        Ok(bitmap) => Ok(TextureSource::Bitmap(bitmap)),
        Err(_) => load_image_element(url.to_string()).await.map(TextureSource::Image),
    }
}

async fn fetch_image_bitmap(url: &str) -> Result<ImageBitmap, JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window"))?;
    let opts = RequestInit::new();
    opts.set_method("GET");
    let request = Request::new_with_str_and_init(url, &opts)?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    if !resp.ok() {
        return Err(JsValue::from_str("Texture request failed"));
    }
    let blob_value = JsFuture::from(resp.blob()?).await?;
    let blob: web_sys::Blob = blob_value.dyn_into()?;
    let bitmap_opts = ImageBitmapOptions::new();
    bitmap_opts.set_image_orientation(web_sys::ImageOrientation::FlipY);
    let bitmap_value = JsFuture::from(window.create_image_bitmap_with_blob_and_image_bitmap_options(&blob, &bitmap_opts)?).await?;
    let bitmap: ImageBitmap = bitmap_value.dyn_into()?;
    Ok(bitmap)
}

async fn load_image_element(url: String) -> Result<HtmlImageElement, JsValue> {
    let img = HtmlImageElement::new()?;
    let img_for_handler = img.clone();
    let url_ref = url.as_str();
    let mut executor = move |resolve: js_sys::Function, reject: js_sys::Function| {
        let onload = Closure::wrap(Box::new(move || {
            let _ = resolve.call0(&JsValue::NULL);
        }) as Box<dyn FnMut()>);
        let onerror = Closure::wrap(Box::new(move || {
            let _ = reject.call0(&JsValue::NULL);
        }) as Box<dyn FnMut()>);
        img_for_handler.set_onload(Some(onload.as_ref().unchecked_ref()));
        img_for_handler.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        RETAINED_CLOSURES.with(|v| {
            v.borrow_mut().push(onload);
            v.borrow_mut().push(onerror);
        });
        img_for_handler.set_src(url_ref);
    };
    let promise = js_sys::Promise::new(&mut executor);
    JsFuture::from(promise).await?;
    Ok(img)
}

fn upload_texture_source(gl: &WebGl2RenderingContext, texture: &WebGlTexture, source: &TextureSource, anisotropy: Option<(u32, f32)>) {
    gl.bind_texture(WebGl2RenderingContext::TEXTURE_2D, Some(texture));
    match source {
        TextureSource::Bitmap(bitmap) => {
            let _ = gl.tex_image_2d_with_u32_and_u32_and_image_bitmap(
                WebGl2RenderingContext::TEXTURE_2D,
                0,
                WebGl2RenderingContext::RGBA as i32,
                WebGl2RenderingContext::RGBA,
                WebGl2RenderingContext::UNSIGNED_BYTE,
                bitmap,
            );
        }
        TextureSource::Image(image) => {
            gl.pixel_storei(WebGl2RenderingContext::UNPACK_FLIP_Y_WEBGL, 1);
            let _ = gl.tex_image_2d_with_u32_and_u32_and_html_image_element(
                WebGl2RenderingContext::TEXTURE_2D,
                0,
                WebGl2RenderingContext::RGBA as i32,
                WebGl2RenderingContext::RGBA,
                WebGl2RenderingContext::UNSIGNED_BYTE,
                image,
            );
            gl.pixel_storei(WebGl2RenderingContext::UNPACK_FLIP_Y_WEBGL, 0);
        }
    }
    gl.tex_parameteri(WebGl2RenderingContext::TEXTURE_2D, WebGl2RenderingContext::TEXTURE_MAG_FILTER, WebGl2RenderingContext::LINEAR as i32);
    gl.generate_mipmap(WebGl2RenderingContext::TEXTURE_2D);
    gl.tex_parameteri(WebGl2RenderingContext::TEXTURE_2D, WebGl2RenderingContext::TEXTURE_MIN_FILTER, WebGl2RenderingContext::LINEAR_MIPMAP_LINEAR as i32);
    gl.tex_parameteri(WebGl2RenderingContext::TEXTURE_2D, WebGl2RenderingContext::TEXTURE_WRAP_S, WebGl2RenderingContext::REPEAT as i32);
    gl.tex_parameteri(WebGl2RenderingContext::TEXTURE_2D, WebGl2RenderingContext::TEXTURE_WRAP_T, WebGl2RenderingContext::CLAMP_TO_EDGE as i32);
    if let Some((param, max)) = anisotropy {
        gl.tex_parameterf(WebGl2RenderingContext::TEXTURE_2D, param, max);
    }
}

fn create_program(gl: &WebGl2RenderingContext) -> Result<WebGlProgram, JsValue> {
    let vert_shader = compile_shader(gl, WebGl2RenderingContext::VERTEX_SHADER, VERTEX_SHADER)?;
    let frag_shader = compile_shader(gl, WebGl2RenderingContext::FRAGMENT_SHADER, FRAGMENT_SHADER)?;

    let program = gl.create_program().ok_or("Unable to create program")?;
    gl.attach_shader(&program, &vert_shader);
    gl.attach_shader(&program, &frag_shader);
    gl.link_program(&program);

    if gl.get_program_parameter(&program, WebGl2RenderingContext::LINK_STATUS).as_bool().unwrap_or(false) {
        Ok(program)
    } else {
        Err(JsValue::from_str(&gl.get_program_info_log(&program).unwrap_or_default()))
    }
}

fn create_instanced_program(gl: &WebGl2RenderingContext) -> Result<WebGlProgram, JsValue> {
    let vert_shader = compile_shader(gl, WebGl2RenderingContext::VERTEX_SHADER, INSTANCED_VERTEX_SHADER)?;
    let frag_shader = compile_shader(gl, WebGl2RenderingContext::FRAGMENT_SHADER, FRAGMENT_SHADER)?;

    let program = gl.create_program().ok_or("Unable to create program")?;
    gl.attach_shader(&program, &vert_shader);
    gl.attach_shader(&program, &frag_shader);
    gl.link_program(&program);

    if gl.get_program_parameter(&program, WebGl2RenderingContext::LINK_STATUS).as_bool().unwrap_or(false) {
        Ok(program)
    } else {
        Err(JsValue::from_str(&gl.get_program_info_log(&program).unwrap_or_default()))
    }
}

fn create_skybox_program(gl: &WebGl2RenderingContext) -> Result<WebGlProgram, JsValue> {
    let vert_shader = compile_shader(gl, WebGl2RenderingContext::VERTEX_SHADER, SKYBOX_VERTEX_SHADER)?;
    let frag_shader = compile_shader(gl, WebGl2RenderingContext::FRAGMENT_SHADER, SKYBOX_FRAGMENT_SHADER)?;

    let program = gl.create_program().ok_or("Unable to create program")?;
    gl.attach_shader(&program, &vert_shader);
    gl.attach_shader(&program, &frag_shader);
    gl.link_program(&program);

    if gl.get_program_parameter(&program, WebGl2RenderingContext::LINK_STATUS).as_bool().unwrap_or(false) {
        Ok(program)
    } else {
        Err(JsValue::from_str(&gl.get_program_info_log(&program).unwrap_or_default()))
    }
}

fn compile_shader(gl: &WebGl2RenderingContext, shader_type: u32, source: &str) -> Result<web_sys::WebGlShader, JsValue> {
    let shader = gl.create_shader(shader_type).ok_or("Unable to create shader")?;
    gl.shader_source(&shader, source);
    gl.compile_shader(&shader);

    if gl.get_shader_parameter(&shader, WebGl2RenderingContext::COMPILE_STATUS).as_bool().unwrap_or(false) {
        Ok(shader)
    } else {
        Err(JsValue::from_str(&gl.get_shader_info_log(&shader).unwrap_or_default()))
    }
}
