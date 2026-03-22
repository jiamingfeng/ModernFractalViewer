//! Mesh extraction and export from SDF data.

pub mod dual_contouring;
pub mod gltf_export;
pub mod marching_cubes;
mod mc_tables;
pub mod palette;
mod qef;
pub mod surface_nets;

use serde::{Deserialize, Serialize};

/// Intermediate mesh representation, decoupled from export format.
#[derive(Debug, Clone)]
pub struct MeshData {
    /// Vertex positions [x, y, z] in SDF-space (approximately metres)
    pub positions: Vec<[f32; 3]>,
    /// Per-vertex normals [nx, ny, nz]
    pub normals: Vec<[f32; 3]>,
    /// Per-vertex RGBA colors [r, g, b, a] in [0, 1]
    pub colors: Vec<[f32; 4]>,
    /// Triangle indices (every 3 form a triangle)
    pub indices: Vec<u32>,
}

/// Mesh extraction algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MeshMethod {
    /// Classic Marching Cubes — fast, but non-watertight and no sharp features.
    MarchingCubes,
    /// Dual Contouring — watertight quads, sharp feature preservation via QEF.
    DualContouring,
    /// Surface Nets — smooth, averaged vertex placement; ideal for fractal SDFs.
    SurfaceNets,
}

impl Default for MeshMethod {
    fn default() -> Self {
        MeshMethod::DualContouring
    }
}

impl std::fmt::Display for MeshMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MeshMethod::MarchingCubes => write!(f, "Marching Cubes"),
            MeshMethod::DualContouring => write!(f, "Dual Contouring"),
            MeshMethod::SurfaceNets => write!(f, "Surface Nets"),
        }
    }
}

/// Export configuration set by the UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportConfig {
    /// Mesh extraction algorithm
    pub method: MeshMethod,
    /// Grid cells per axis (uniform resolution)
    pub resolution: u32,
    /// Bounding box minimum [x, y, z] in centimetres (converted to SDF-space at dispatch time)
    pub bounds_min: [f32; 3],
    /// Bounding box maximum [x, y, z] in centimetres (converted to SDF-space at dispatch time)
    pub bounds_max: [f32; 3],
    /// Iso-level for surface extraction (typically 0.0 for SDFs)
    pub iso_level: f32,
    /// Whether to compute smooth normals from the SDF gradient
    pub compute_normals: bool,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            method: MeshMethod::default(),
            resolution: 128,
            bounds_min: [-150.0, -150.0, -150.0],
            bounds_max: [150.0, 150.0, 150.0],
            iso_level: 0.0,
            compute_normals: true,
        }
    }
}

/// Material properties for glTF PBR export, derived from the app's lighting config.
///
/// When passed to [`gltf_export::export_glb`], a glTF `pbrMetallicRoughness`
/// material is emitted so that viewers render the mesh with physically-based
/// lighting instead of a flat default material.
#[derive(Debug, Clone)]
pub struct ExportMaterial {
    /// Base color factor `[r, g, b, a]` — multiplied with `COLOR_0` vertex attribute.
    /// Set to `[1,1,1,1]` to let vertex colors pass through unchanged.
    pub base_color_factor: [f32; 4],
    /// PBR metallic factor `0.0` (dielectric) to `1.0` (metal).
    pub metallic_factor: f32,
    /// PBR roughness factor `0.0` (smooth) to `1.0` (rough).
    pub roughness_factor: f32,
    /// Emissive factor `[r, g, b]` — adds a subtle self-illumination (ambient glow).
    pub emissive_factor: [f32; 3],
    /// Whether the material is double-sided (backface culling disabled).
    pub double_sided: bool,
}

impl Default for ExportMaterial {
    fn default() -> Self {
        Self {
            base_color_factor: [1.0, 1.0, 1.0, 1.0],
            metallic_factor: 0.0,
            roughness_factor: 0.5,
            emissive_factor: [0.0, 0.0, 0.0],
            double_sided: true,
        }
    }
}

impl ExportMaterial {
    /// Build an `ExportMaterial` from the app's lighting and color configs.
    ///
    /// When `lighting_model == 1` (PBR Cook-Torrance), the metallic/roughness
    /// values map directly. When `lighting_model == 0` (Blinn-Phong), we
    /// convert shininess → roughness and specular → metallic.
    pub fn from_lighting(
        lighting: &crate::sdf::LightingConfig,
        color: &crate::sdf::ColorConfig,
    ) -> Self {
        let (metallic, roughness) = if lighting.lighting_model == 1 {
            // PBR model — direct mapping
            (lighting.metallic, lighting.roughness)
        } else {
            // Blinn-Phong — approximate conversion
            let roughness = 1.0 - (lighting.shininess / 128.0).clamp(0.0, 0.95);
            let metallic = lighting.specular.clamp(0.0, 1.0);
            (metallic, roughness)
        };

        // Use base_color as the color factor only in solid color mode (mode 0);
        // otherwise let vertex colors define the appearance (factor = white).
        let base_color_factor = if color.color_mode == 0 {
            [color.base_color[0], color.base_color[1], color.base_color[2], 1.0]
        } else {
            [1.0, 1.0, 1.0, 1.0]
        };

        // Map ambient to a subtle emissive glow so the mesh isn't pitch-black
        // in viewers that don't have scene lighting.  We scale it down so
        // 0.1 ambient → faint glow rather than a neon look.
        let ambient_scale = lighting.ambient * 0.3;
        let emissive_factor = [
            base_color_factor[0] * ambient_scale,
            base_color_factor[1] * ambient_scale,
            base_color_factor[2] * ambient_scale,
        ];

        Self {
            base_color_factor,
            metallic_factor: metallic.clamp(0.0, 1.0),
            roughness_factor: roughness.clamp(0.0, 1.0),
            emissive_factor,
            double_sided: true,
        }
    }
}

/// Returns the default bounding box (in cm) for a given fractal type.
pub fn default_bounds(fractal_type: crate::FractalType) -> ([f32; 3], [f32; 3]) {
    use crate::FractalType::*;
    match fractal_type {
        Mandelbulb => ([-150.0, -150.0, -150.0], [150.0, 150.0, 150.0]),
        Menger => ([-150.0, -150.0, -150.0], [150.0, 150.0, 150.0]),
        Julia3D => ([-200.0, -200.0, -200.0], [200.0, 200.0, 200.0]),
        Mandelbox => ([-300.0, -300.0, -300.0], [300.0, 300.0, 300.0]),
        Sierpinski => ([-200.0, -200.0, -200.0], [200.0, 200.0, 200.0]),
        Apollonian => ([-200.0, -200.0, -200.0], [200.0, 200.0, 200.0]),
    }
}
