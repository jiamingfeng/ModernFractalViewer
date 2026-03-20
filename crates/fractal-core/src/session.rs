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
