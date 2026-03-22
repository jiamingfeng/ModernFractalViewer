//! Mesh extraction and export from SDF data.

pub mod dual_contouring;
pub mod gltf_export;
pub mod marching_cubes;
mod mc_tables;
pub mod palette;
mod qef;

use serde::{Deserialize, Serialize};

/// Intermediate mesh representation, decoupled from export format.
#[derive(Debug, Clone)]
pub struct MeshData {
    /// Vertex positions [x, y, z] in centimetres
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
    /// Bounding box minimum [x, y, z] in centimetres
    pub bounds_min: [f32; 3],
    /// Bounding box maximum [x, y, z] in centimetres
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
