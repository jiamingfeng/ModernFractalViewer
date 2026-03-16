//! GPU uniform buffer types

use bytemuck::{Pod, Zeroable};
use fractal_core::{Camera, FractalParams};
use fractal_core::sdf::{ColorConfig, LightingConfig, RayMarchConfig};

/// Main uniforms sent to the GPU shader
///
/// IMPORTANT: This struct must match the WGSL Uniforms struct exactly.
/// We avoid vec3<f32> in WGSL because it has 16-byte alignment in structs,
/// which creates implicit padding gaps that are hard to match from Rust.
/// Instead, we use individual f32 fields for padding.
/// WGSL alignment rules:
/// - vec4<f32>: 16 bytes, align 16
/// - vec2<f32>: 8 bytes, align 8
/// - f32/u32: 4 bytes, align 4
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct Uniforms {
    // Camera (48 bytes at offset 0)
    pub camera_pos: [f32; 4],      // 16 bytes, offset 0
    pub camera_target: [f32; 4],   // 16 bytes, offset 16
    pub camera_up: [f32; 4],       // 16 bytes, offset 32
    
    // Camera params (16 bytes at offset 48)
    pub camera_fov: f32,           // 4 bytes, offset 48
    pub aspect_ratio: f32,         // 4 bytes, offset 52
    pub _pad1: [f32; 2],           // 8 bytes, offset 56

    // Resolution and time (16 bytes at offset 64)
    pub resolution: [f32; 2],      // 8 bytes, offset 64
    pub time: f32,                 // 4 bytes, offset 72
    pub _pad2: f32,                // 4 bytes, offset 76

    // Fractal parameters part 1 (16 bytes at offset 80)
    pub fractal_type: u32,         // 4 bytes, offset 80
    pub power: f32,                // 4 bytes, offset 84
    pub iterations: u32,           // 4 bytes, offset 88
    pub bailout: f32,              // 4 bytes, offset 92
    
    // Fractal parameters part 2 (16 bytes at offset 96)
    pub scale: f32,                // 4 bytes, offset 96
    pub fold_limit: f32,           // 4 bytes, offset 100
    pub min_radius_sq: f32,        // 4 bytes, offset 104
    pub _pad3: f32,                // 4 bytes, offset 108
    
    // Julia C (16 bytes at offset 112)
    pub julia_c: [f32; 4],         // 16 bytes, offset 112

    // Ray marching config (32 bytes at offset 128)
    pub max_steps: u32,            // 4 bytes, offset 128
    pub epsilon: f32,              // 4 bytes, offset 132
    pub max_distance: f32,         // 4 bytes, offset 136
    pub ao_steps: u32,             // 4 bytes, offset 140
    
    // ao_intensity + 3x f32 padding (16 bytes at offset 144)
    pub ao_intensity: f32,         // 4 bytes, offset 144
    pub _pad4: [f32; 3],           // 12 bytes, offset 148
    
    // Lighting (32 bytes at offset 160)
    pub light_dir: [f32; 4],       // 16 bytes, offset 160
    pub ambient: f32,              // 4 bytes, offset 176
    pub diffuse: f32,              // 4 bytes, offset 180
    pub specular: f32,             // 4 bytes, offset 184
    pub shininess: f32,            // 4 bytes, offset 188

    // Colors (48 bytes at offset 192)
    pub base_color: [f32; 4],      // 16 bytes, offset 192
    pub secondary_color: [f32; 4], // 16 bytes, offset 208
    pub background_color: [f32; 4],// 16 bytes, offset 224
    
    // Color mode + padding (16 bytes at offset 240)
    pub color_mode: u32,           // 4 bytes, offset 240
    pub _pad5: [f32; 3],           // 12 bytes, offset 244
    
    // Total: 256 bytes
}

impl Default for Uniforms {
    fn default() -> Self {
        Self::new()
    }
}

impl Uniforms {
    pub fn new() -> Self {
        let camera = Camera::default();
        let fractal = FractalParams::default();
        let ray_march = RayMarchConfig::default();
        let lighting = LightingConfig::default();
        let color = ColorConfig::default();

        Self {
            camera_pos: [camera.position.x, camera.position.y, camera.position.z, 0.0],
            camera_target: [camera.target.x, camera.target.y, camera.target.z, 0.0],
            camera_up: [camera.up.x, camera.up.y, camera.up.z, 0.0],
            camera_fov: camera.fov,
            aspect_ratio: 1.0,
            _pad1: [0.0; 2],

            resolution: [800.0, 600.0],
            time: 0.0,
            _pad2: 0.0,

            fractal_type: fractal.fractal_type as u32,
            power: fractal.power,
            iterations: fractal.iterations,
            bailout: fractal.bailout,
            
            scale: fractal.scale,
            fold_limit: fractal.fold_limit,
            min_radius_sq: fractal.min_radius_sq,
            _pad3: 0.0,
            
            julia_c: fractal.julia_c,

            max_steps: ray_march.max_steps,
            epsilon: ray_march.epsilon,
            max_distance: ray_march.max_distance,
            ao_steps: ray_march.ao_steps,
            ao_intensity: ray_march.ao_intensity,
            _pad4: [0.0; 3],

            light_dir: [lighting.light_dir[0], lighting.light_dir[1], lighting.light_dir[2], 0.0],
            ambient: lighting.ambient,
            diffuse: lighting.diffuse,
            specular: lighting.specular,
            shininess: lighting.shininess,

            base_color: [color.base_color[0], color.base_color[1], color.base_color[2], 1.0],
            secondary_color: [color.secondary_color[0], color.secondary_color[1], color.secondary_color[2], 1.0],
            background_color: [color.background_color[0], color.background_color[1], color.background_color[2], 1.0],
            color_mode: color.color_mode,
            _pad5: [0.0; 3],
        }
    }

    /// Update camera uniforms
    pub fn update_camera(&mut self, camera: &Camera, aspect_ratio: f32) {
        self.camera_pos = [camera.position.x, camera.position.y, camera.position.z, 0.0];
        self.camera_target = [camera.target.x, camera.target.y, camera.target.z, 0.0];
        self.camera_up = [camera.up.x, camera.up.y, camera.up.z, 0.0];
        self.camera_fov = camera.fov;
        self.aspect_ratio = aspect_ratio;
    }

    /// Update fractal parameters
    pub fn update_fractal(&mut self, params: &FractalParams) {
        self.fractal_type = params.fractal_type as u32;
        self.power = params.power;
        self.iterations = params.iterations;
        self.bailout = params.bailout;
        self.scale = params.scale;
        self.fold_limit = params.fold_limit;
        self.min_radius_sq = params.min_radius_sq;
        self.julia_c = params.julia_c;
    }

    /// Update ray marching config
    pub fn update_ray_march(&mut self, config: &RayMarchConfig) {
        self.max_steps = config.max_steps;
        self.epsilon = config.epsilon;
        self.max_distance = config.max_distance;
        self.ao_steps = config.ao_steps;
        self.ao_intensity = config.ao_intensity;
    }

    /// Update lighting config
    pub fn update_lighting(&mut self, config: &LightingConfig) {
        self.light_dir = [config.light_dir[0], config.light_dir[1], config.light_dir[2], 0.0];
        self.ambient = config.ambient;
        self.diffuse = config.diffuse;
        self.specular = config.specular;
        self.shininess = config.shininess;
    }

    /// Update color config
    pub fn update_color(&mut self, config: &ColorConfig) {
        self.base_color = [config.base_color[0], config.base_color[1], config.base_color[2], 1.0];
        self.secondary_color = [config.secondary_color[0], config.secondary_color[1], config.secondary_color[2], 1.0];
        self.background_color = [config.background_color[0], config.background_color[1], config.background_color[2], 1.0];
        self.color_mode = config.color_mode;
    }

    /// Update resolution
    pub fn update_resolution(&mut self, width: u32, height: u32) {
        self.resolution = [width as f32, height as f32];
        self.aspect_ratio = width as f32 / height as f32;
    }

    /// Update time
    pub fn update_time(&mut self, time: f32) {
        self.time = time;
    }
}

// Compile-time size check
const _: () = assert!(std::mem::size_of::<Uniforms>() == 256);
