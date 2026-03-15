//! UI state management

use fractal_core::{Camera, FractalParams, FractalType};
use fractal_core::sdf::{ColorConfig, LightingConfig, RayMarchConfig};

/// Main UI state structure
#[derive(Debug, Clone)]
pub struct UiState {
    /// Current fractal parameters
    pub fractal_params: FractalParams,
    /// Ray marching configuration
    pub ray_march_config: RayMarchConfig,
    /// Lighting configuration
    pub lighting_config: LightingConfig,
    /// Color configuration
    pub color_config: ColorConfig,
    /// Camera state (for reset)
    pub camera: Camera,
    /// Show UI panel
    pub show_panel: bool,
    /// Show debug info
    pub show_debug: bool,
    /// Auto-rotate camera
    pub auto_rotate: bool,
    /// Rotation speed
    pub rotation_speed: f32,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            fractal_params: FractalParams::default(),
            ray_march_config: RayMarchConfig::default(),
            lighting_config: LightingConfig::default(),
            color_config: ColorConfig::default(),
            camera: Camera::default(),
            show_panel: true,
            show_debug: false,
            auto_rotate: false,
            rotation_speed: 0.5,
        }
    }
}

impl UiState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset all parameters to defaults for current fractal type
    pub fn reset_to_defaults(&mut self) {
        self.fractal_params = FractalParams::for_type(self.fractal_params.fractal_type);
        self.ray_march_config = RayMarchConfig::default();
        self.lighting_config = LightingConfig::default();
        self.color_config = ColorConfig::default();
    }

    /// Change fractal type and load appropriate defaults
    pub fn set_fractal_type(&mut self, fractal_type: FractalType) {
        self.fractal_params = FractalParams::for_type(fractal_type);
    }
}
