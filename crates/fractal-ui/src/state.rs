//! UI state management

use fractal_core::{Camera, FractalParams, FractalType};
use fractal_core::benchmark_types::BenchmarkResult;
use fractal_core::mesh::ExportConfig;
use fractal_core::sdf::{ColorConfig, LightingConfig, RayMarchConfig};

use crate::app_settings::AppSettings;

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
    /// Confirmation dialog state for overwrite (session id, session name)
    pub confirming_overwrite: Option<(String, String)>,
    /// Confirmation dialog state for delete (session id, session name)
    pub confirming_delete: Option<(String, String)>,
    /// Version info string for display (e.g. "v0.1.1 (abc1234)")
    pub version_info: String,
    /// UI control ranges (min/max/speed/decimals for all sliders)
    pub settings: AppSettings,
    /// Whether control ranges have been modified and need saving
    pub settings_dirty: bool,
    /// Request to open the config file in the OS default editor
    pub open_config_requested: bool,
    /// Show the in-app log window
    pub show_logs: bool,
    /// Whether light control mode is active (L key held)
    pub light_control_active: bool,
    /// Log filter: free-text search (case-insensitive)
    pub log_filter_text: String,
    /// Log filter: show INFO level
    pub log_show_info: bool,
    /// Log filter: show WARN level
    pub log_show_warn: bool,
    /// Log filter: show ERROR level
    pub log_show_error: bool,

    // -- Benchmark state (transient, not saved) --
    /// Whether a benchmark is currently running
    pub benchmark_running: bool,
    /// Completed benchmark results
    pub benchmark_results: Vec<BenchmarkResult>,
    /// Rolling buffer of frame times for live graph (ms)
    pub benchmark_frame_times: Vec<f64>,
    /// Name of the currently running scenario
    pub benchmark_current_scenario: String,
    /// Overall progress (0.0 to 1.0)
    pub benchmark_progress: f32,
    /// Show the benchmark panel
    pub show_benchmark: bool,
    /// Signal to app to start a benchmark run
    pub pending_benchmark: bool,
    /// Signal to app to stop a benchmark run
    pub benchmark_stop_requested: bool,

    // -- Export state (transient, not saved) --
    /// Export configuration (resolution, bounds, etc.)
    pub export_config: ExportConfig,
    /// Request to start an export
    pub pending_export: bool,
    /// Export progress (0.0 to 1.0), None if not exporting
    pub export_progress: Option<f32>,
    /// Export status message (e.g., "Export complete: path")
    pub export_status: Option<String>,
    /// Whether an export is currently in progress
    pub export_in_progress: bool,
    /// Custom filename for Android export (base name + extension, no path). Transient.
    pub export_filename: String,
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
            confirming_overwrite: None,
            confirming_delete: None,
            version_info: String::new(),
            settings: AppSettings::default(),
            settings_dirty: false,
            open_config_requested: false,
            show_logs: false,
            log_filter_text: String::new(),
            log_show_info: true,
            log_show_warn: true,
            log_show_error: true,
            light_control_active: false,
            benchmark_running: false,
            benchmark_results: Vec::new(),
            benchmark_frame_times: Vec::new(),
            benchmark_current_scenario: String::new(),
            benchmark_progress: 0.0,
            show_benchmark: false,
            pending_benchmark: false,
            benchmark_stop_requested: false,
            export_config: ExportConfig::default(),
            pending_export: false,
            export_progress: None,
            export_status: None,
            export_in_progress: false,
            export_filename: String::new(),
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
