//! Save session data structures
//!
//! Defines the serializable session format for saving and loading
//! fractal exploration state.

use serde::{Deserialize, Serialize};

use crate::camera::Camera;
use crate::fractals::FractalParams;
use crate::sdf::{ColorConfig, LightingConfig, RayMarchConfig};

/// A saved fractal exploration session.
///
/// All config structs use `#[serde(default)]` so that future fields
/// added to any config are automatically filled with defaults when
/// loading older save files (backward compatibility).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedSession {
    /// Schema version for forward-compatible migrations
    pub version: String,
    /// ISO 8601 timestamp of when the session was saved
    pub timestamp: String,
    /// User-provided name for this save
    pub name: String,
    /// Human-readable fractal type name (for display without parsing params)
    pub fractal_type_name: String,
    /// PNG thumbnail encoded as base64
    pub thumbnail_base64: String,
    /// Thumbnail width in pixels
    pub thumbnail_width: u32,
    /// Thumbnail height in pixels
    pub thumbnail_height: u32,
    /// Fractal parameters
    pub fractal_params: FractalParams,
    /// Ray marching configuration
    pub ray_march_config: RayMarchConfig,
    /// Lighting configuration
    pub lighting_config: LightingConfig,
    /// Color configuration
    pub color_config: ColorConfig,
    /// Camera state
    pub camera: Camera,
}

impl Default for SavedSession {
    fn default() -> Self {
        Self {
            version: "1".to_string(),
            timestamp: String::new(),
            name: String::new(),
            fractal_type_name: String::new(),
            thumbnail_base64: String::new(),
            thumbnail_width: 320,
            thumbnail_height: 180,
            fractal_params: FractalParams::default(),
            ray_march_config: RayMarchConfig::default(),
            lighting_config: LightingConfig::default(),
            color_config: ColorConfig::default(),
            camera: Camera::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serde_roundtrip() {
        let mut s = SavedSession::default();
        s.name = "Test Session".to_string();
        s.timestamp = "2026-03-20T12:00:00Z".to_string();
        s.fractal_type_name = "Mandelbulb".to_string();
        s.fractal_params = FractalParams::for_type(crate::fractals::FractalType::Julia3D);
        let json = serde_json::to_string(&s).unwrap();
        let s2: SavedSession = serde_json::from_str(&json).unwrap();
        assert_eq!(s.name, s2.name);
        assert_eq!(s.version, s2.version);
        assert_eq!(s.fractal_params.iterations, s2.fractal_params.iterations);
    }

    #[test]
    fn test_backward_compat_missing_fields() {
        let json = r#"{"version":"1"}"#;
        let s: SavedSession = serde_json::from_str(json).unwrap();
        assert_eq!(s.version, "1");
        // All other fields should be defaults
        assert_eq!(s.thumbnail_width, 320);
        assert_eq!(s.fractal_params.fractal_type, crate::fractals::FractalType::Mandelbulb);
    }

    #[test]
    fn test_unknown_fields_ignored() {
        let json = r#"{"version":"1","some_future_field":"hello","another_one":42}"#;
        let s: SavedSession = serde_json::from_str(json).unwrap();
        assert_eq!(s.version, "1");
    }

    #[test]
    fn test_nested_config_defaults() {
        // Missing nested config fields should get defaults
        let json = r#"{"version":"1","ray_march_config":{"max_steps":256}}"#;
        let s: SavedSession = serde_json::from_str(json).unwrap();
        assert_eq!(s.ray_march_config.max_steps, 256);
        // Other fields in ray_march_config should be defaults
        assert_eq!(s.ray_march_config.epsilon, 0.001);
        assert_eq!(s.ray_march_config.sample_count, 1);
    }
}
