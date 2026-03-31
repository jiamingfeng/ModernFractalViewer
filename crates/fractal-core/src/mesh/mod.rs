//! Mesh extraction and export from SDF data.

pub mod decimation;
pub mod dual_contouring;
pub mod gltf_export;
pub mod marching_cubes;
mod mc_tables;
pub mod obj_export;
pub mod palette;
pub mod ply_export;
mod qef;
pub mod smoothing;
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

/// Mesh smoothing method applied as post-processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SmoothMethod {
    /// No smoothing applied.
    None,
    /// Laplacian smoothing — simple averaging, causes slight mesh shrinkage.
    Laplacian,
    /// Taubin smoothing — alternating positive/negative lambda preserves volume.
    Taubin,
}

impl Default for SmoothMethod {
    fn default() -> Self {
        SmoothMethod::None
    }
}

impl std::fmt::Display for SmoothMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SmoothMethod::None => write!(f, "None"),
            SmoothMethod::Laplacian => write!(f, "Laplacian"),
            SmoothMethod::Taubin => write!(f, "Taubin"),
        }
    }
}

/// Export file format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportFormat {
    /// glTF 2.0 Binary (.glb) — PBR material, vertex colors, widely supported.
    Glb,
    /// Wavefront OBJ (.obj) — text-based, universal compatibility.
    Obj,
    /// Stanford PLY (.ply) — binary, per-vertex colors, good for 3D printing.
    Ply,
}

impl Default for ExportFormat {
    fn default() -> Self {
        ExportFormat::Glb
    }
}

impl std::fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportFormat::Glb => write!(f, "glTF Binary (.glb)"),
            ExportFormat::Obj => write!(f, "Wavefront OBJ (.obj)"),
            ExportFormat::Ply => write!(f, "Stanford PLY (.ply)"),
        }
    }
}

impl ExportFormat {
    /// File extension (without dot).
    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Glb => "glb",
            ExportFormat::Obj => "obj",
            ExportFormat::Ply => "ply",
        }
    }

    /// Default filename incorporating the fractal type name and current timestamp.
    /// Pattern: `{fractal_type}_{YYYYMMDD}_{HHMMSS}.{ext}`, e.g. `mandelbulb_20260331_143022.glb`.
    /// Spaces in the fractal type name are replaced with underscores and lowercased.
    pub fn default_filename(&self, fractal_type_name: &str) -> String {
        let snake = fractal_type_name
            .chars()
            .map(|c| if c == ' ' { '_' } else { c.to_ascii_lowercase() })
            .collect::<String>();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let (date, time) = unix_to_datetime(ts);
        format!("{snake}_{date}_{time}.{}", self.extension())
    }

    /// MIME type for this export format.
    pub fn mime_type(&self) -> &'static str {
        match self {
            ExportFormat::Glb => "model/gltf-binary",
            ExportFormat::Obj => "text/plain",
            ExportFormat::Ply => "application/octet-stream",
        }
    }

    /// Filter label for file dialogs.
    pub fn filter_label(&self) -> &'static str {
        match self {
            ExportFormat::Glb => "glTF Binary",
            ExportFormat::Obj => "Wavefront OBJ",
            ExportFormat::Ply => "Stanford PLY",
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
    /// Whether to auto-compute iso-level from voxel size
    pub adaptive_iso: bool,
    /// Factor for adaptive iso-level: `iso = factor * voxel_diagonal`
    pub adaptive_iso_factor: f32,
    /// Whether to extend bounds by one voxel to capture edge features
    pub boundary_extension: bool,
    /// Mesh smoothing method
    pub smooth_method: SmoothMethod,
    /// Number of smoothing iterations (0 = disabled)
    pub smooth_iterations: u32,
    /// Smoothing strength (lambda parameter)
    pub smooth_lambda: f32,
    /// Export file format
    pub export_format: ExportFormat,
    /// Whether to apply mesh decimation
    pub decimate: bool,
    /// Target triangle ratio for decimation (0.01–1.0, where 1.0 = no reduction)
    pub decimate_target_ratio: f32,
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
            adaptive_iso: true,
            adaptive_iso_factor: 0.1,
            boundary_extension: true,
            smooth_method: SmoothMethod::Taubin,
            smooth_iterations: 3,
            smooth_lambda: 0.5,
            export_format: ExportFormat::default(),
            decimate: false,
            decimate_target_ratio: 0.5,
        }
    }
}

/// Converts Unix epoch seconds to `(YYYYMMDD, HHMMSS)` string pair without external crates.
fn unix_to_datetime(secs: u64) -> (String, String) {
    // Time-of-day components
    let s = (secs % 60) as u32;
    let m = ((secs / 60) % 60) as u32;
    let h = ((secs / 3600) % 24) as u32;

    // Gregorian calendar date from days since 1970-01-01
    // Algorithm: https://howardhinnant.github.io/date_algorithms.html
    let z = (secs / 86400) as i64 + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };

    (
        format!("{:04}{:02}{:02}", y, mo, d),
        format!("{:02}{:02}{:02}", h, m, s),
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_filename_pattern() {
        let name = ExportFormat::Glb.default_filename("Mandelbulb");
        // Should match: mandelbulb_YYYYMMDD_HHMMSS.glb
        assert!(name.starts_with("mandelbulb_"), "got: {name}");
        assert!(name.ends_with(".glb"), "got: {name}");
        // Date+time segment: mandelbulb_20260331_143022.glb  →  17 chars + prefix + ext
        let parts: Vec<&str> = name.trim_end_matches(".glb").split('_').collect();
        assert_eq!(parts.len(), 3, "expected 3 underscore-separated segments, got: {name}");
        assert_eq!(parts[1].len(), 8, "date segment should be 8 chars: {name}");
        assert_eq!(parts[2].len(), 6, "time segment should be 6 chars: {name}");
    }

    #[test]
    fn default_filename_space_to_underscore() {
        let name = ExportFormat::Obj.default_filename("Menger Sponge");
        assert!(name.starts_with("menger_sponge_"), "got: {name}");
        assert!(name.ends_with(".obj"), "got: {name}");
    }

    #[test]
    fn mime_types() {
        assert_eq!(ExportFormat::Glb.mime_type(), "model/gltf-binary");
        assert_eq!(ExportFormat::Obj.mime_type(), "text/plain");
        assert_eq!(ExportFormat::Ply.mime_type(), "application/octet-stream");
    }

    #[test]
    fn unix_to_datetime_known_value() {
        // 2025-03-31 14:10:22 UTC = 1743430222 seconds
        let (date, time) = unix_to_datetime(1743430222);
        assert_eq!(date, "20250331");
        assert_eq!(time, "141022");

        // 2000-01-01 00:00:00 UTC = 946684800 seconds
        let (date2, time2) = unix_to_datetime(946684800);
        assert_eq!(date2, "20000101");
        assert_eq!(time2, "000000");
    }
}
