//! Config file manager for application settings.
//!
//! Loads and saves `AppSettings` as TOML to the user's data directory
//! (native) or localStorage (WASM).

use fractal_ui::AppSettings;

const CONFIG_FILENAME: &str = "settings.toml";

/// Load settings from the data directory.
/// Falls back to defaults if the file doesn't exist or fails to parse.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_settings(data_dir: &std::path::Path) -> AppSettings {
    let path = data_dir.join(CONFIG_FILENAME);
    match std::fs::read_to_string(&path) {
        Ok(contents) => match toml::from_str::<AppSettings>(&contents) {
            Ok(settings) => {
                log::info!("Loaded settings from {}", path.display());
                settings
            }
            Err(e) => {
                log::warn!("Failed to parse {}: {e}. Using defaults.", path.display());
                AppSettings::default()
            }
        },
        Err(_) => {
            log::info!("No settings file found, using defaults.");
            AppSettings::default()
        }
    }
}

/// Save settings to the data directory as pretty-printed TOML.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_settings(
    data_dir: &std::path::Path,
    settings: &AppSettings,
) -> Result<(), String> {
    let path = data_dir.join(CONFIG_FILENAME);
    let toml_str =
        toml::to_string_pretty(settings).map_err(|e| format!("TOML serialize error: {e}"))?;

    let contents = format!(
        "# Modern Fractal Viewer — Application Settings\n\
         # This file is auto-generated. Edit values to customize.\n\
         # Delete this file to reset to defaults.\n\n\
         {toml_str}"
    );

    std::fs::create_dir_all(data_dir)
        .map_err(|e| format!("Failed to create config dir: {e}"))?;
    std::fs::write(&path, contents)
        .map_err(|e| format!("Failed to write {}: {e}", path.display()))?;

    log::info!("Saved settings to {}", path.display());
    Ok(())
}

/// Load settings from WASM localStorage.
#[cfg(target_arch = "wasm32")]
pub fn load_settings_wasm() -> AppSettings {
    let storage = web_sys::window().and_then(|w| w.local_storage().ok().flatten());
    if let Some(storage) = storage {
        if let Ok(Some(toml_str)) = storage.get_item("fractal_settings") {
            if let Ok(settings) = toml::from_str::<AppSettings>(&toml_str) {
                log::info!("Loaded settings from localStorage");
                return settings;
            }
        }
    }
    AppSettings::default()
}

/// Save settings to WASM localStorage.
#[cfg(target_arch = "wasm32")]
pub fn save_settings_wasm(settings: &AppSettings) -> Result<(), String> {
    let toml_str =
        toml::to_string_pretty(settings).map_err(|e| format!("TOML serialize error: {e}"))?;
    let storage = web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .ok_or("localStorage not available")?;
    storage
        .set_item("fractal_settings", &toml_str)
        .map_err(|_| "Failed to write to localStorage".to_string())?;
    log::info!("Saved settings to localStorage");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_from_nonexistent_returns_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let settings = load_settings(dir.path());
        assert_eq!(settings, AppSettings::default());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = AppSettings::default();
        settings.camera.fov.min = 10.0;
        settings.camera.fov.max = 170.0;
        settings.auto_load_last_session = true;

        save_settings(dir.path(), &settings).unwrap();
        let loaded = load_settings(dir.path());

        assert_eq!(loaded.camera.fov.min, 10.0);
        assert_eq!(loaded.camera.fov.max, 170.0);
        assert!(loaded.auto_load_last_session);
        assert_eq!(loaded.fractal, settings.fractal);
    }

    #[test]
    fn embedded_default_toml_parses_correctly() {
        let parsed: AppSettings =
            toml::from_str(AppSettings::DEFAULT_TOML).expect("Default TOML should parse");
        assert_eq!(parsed, AppSettings::default());
    }
}
