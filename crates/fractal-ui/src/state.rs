//! UI state management

use fractal_core::{Camera, FractalParams, FractalType};
use fractal_core::sdf::{ColorConfig, LightingConfig, RayMarchConfig};

/// Display data for a saved session slot (populated by App, consumed by UI).
#[derive(Clone)]
pub struct SessionSlotDisplay {
    /// Save ID (timestamp-based filename stem)
    pub id: String,
    /// User-provided name
    pub name: String,
    /// ISO 8601 timestamp
    pub timestamp: String,
    /// Human-readable fractal type
    pub fractal_type_name: String,
    /// Thumbnail texture (lazily loaded from base64 PNG)
    pub thumbnail: Option<egui::TextureHandle>,
}

impl std::fmt::Debug for SessionSlotDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionSlotDisplay")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("timestamp", &self.timestamp)
            .field("fractal_type_name", &self.fractal_type_name)
            .field("thumbnail", &self.thumbnail.as_ref().map(|_| "..."))
            .finish()
    }
}

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
    /// Enable vsync
    pub vsync: bool,

    // -- Session save/load state (transient, not saved) --
    /// Request to save to a new session slot
    pub pending_save: bool,
    /// Request to overwrite an existing session (save ID)
    pub pending_overwrite: Option<String>,
    /// Request to load a session (save ID)
    pub pending_load: Option<String>,
    /// Request to delete a session (save ID)
    pub pending_delete: Option<String>,
    /// Available saved session slots (populated by App)
    pub session_slots: Vec<SessionSlotDisplay>,
    /// Whether the session slot list needs refreshing
    pub sessions_dirty: bool,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            fractal_params: FractalParams::default(),
            ray_march_config: RayMarchConfig::default(),
            lighting_config: LightingConfig::default(),
            color_config: ColorConfig::default(),
            camera: Camera::default(),
            show_panel: !cfg!(target_os = "android"),
            show_debug: false,
            auto_rotate: false,
            rotation_speed: 0.5,
            vsync: true,
            pending_save: false,
            pending_overwrite: None,
            pending_load: None,
            pending_delete: None,
            session_slots: Vec::new(),
            sessions_dirty: true,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reset_preserves_fractal_type() {
        let mut state = UiState::default();
        state.set_fractal_type(FractalType::Julia3D);
        assert_eq!(state.fractal_params.fractal_type, FractalType::Julia3D);
        state.reset_to_defaults();
        // Fractal type should be preserved
        assert_eq!(state.fractal_params.fractal_type, FractalType::Julia3D);
    }

    #[test]
    fn test_set_fractal_type() {
        let mut state = UiState::default();
        state.set_fractal_type(FractalType::Menger);
        let expected = FractalParams::for_type(FractalType::Menger);
        assert_eq!(state.fractal_params.fractal_type, FractalType::Menger);
        assert_eq!(state.fractal_params.iterations, expected.iterations);
        assert_eq!(state.fractal_params.scale, expected.scale);
    }

}
