//! Config file manager for UI control ranges.
//!
//! Loads and saves `UiControlRanges` as TOML to the user's data directory
//! (native) or localStorage (WASM).

use fractal_ui::UiControlRanges;

const CONFIG_FILENAME: &str = "control_ranges.toml";

/// Load control ranges from the data directory.
/// Falls back to defaults if the file doesn't exist or fails to parse.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_control_ranges(data_dir: &std::path::Path) -> UiControlRanges {
    let path = data_dir.join(CONFIG_FILENAME);
    match std::fs::read_to_string(&path) {
        Ok(contents) => match toml::from_str::<UiControlRanges>(&contents) {
            Ok(ranges) => {
                log::info!("Loaded control ranges from {}", path.display());
                ranges
            }
            Err(e) => {
                log::warn!("Failed to parse {}: {e}. Using defaults.", path.display());
                UiControlRanges::default()
            }
        },
        Err(_) => {
            log::info!("No control ranges file found, using defaults.");
            UiControlRanges::default()
        }
    }
}

/// Save control ranges to the data directory as pretty-printed TOML.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_control_ranges(
    data_dir: &std::path::Path,
    ranges: &UiControlRanges,
) -> Result<(), String> {
    let path = data_dir.join(CONFIG_FILENAME);
    let toml_str =
        toml::to_string_pretty(ranges).map_err(|e| format!("TOML serialize error: {e}"))?;

    let contents = format!(
        "# Modern Fractal Viewer — UI Control Ranges\n\
         # This file is auto-generated. Edit values to customize slider ranges.\n\
         # Delete this file to reset to defaults.\n\n\
         {toml_str}"
    );

    std::fs::create_dir_all(data_dir)
        .map_err(|e| format!("Failed to create config dir: {e}"))?;
    std::fs::write(&path, contents)
        .map_err(|e| format!("Failed to write {}: {e}", path.display()))?;

    log::info!("Saved control ranges to {}", path.display());
    Ok(())
}

/// Load control ranges from WASM localStorage.
#[cfg(target_arch = "wasm32")]
pub fn load_control_ranges_wasm() -> UiControlRanges {
    let storage = web_sys::window().and_then(|w| w.local_storage().ok().flatten());
    if let Some(storage) = storage {
        if let Ok(Some(toml_str)) = storage.get_item("fractal_control_ranges") {
            if let Ok(ranges) = toml::from_str::<UiControlRanges>(&toml_str) {
                log::info!("Loaded control ranges from localStorage");
                return ranges;
            }
        }
    }
    UiControlRanges::default()
}

/// Save control ranges to WASM localStorage.
#[cfg(target_arch = "wasm32")]
pub fn save_control_ranges_wasm(ranges: &UiControlRanges) -> Result<(), String> {
    let toml_str =
        toml::to_string_pretty(ranges).map_err(|e| format!("TOML serialize error: {e}"))?;
    let storage = web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .ok_or("localStorage not available")?;
    storage
        .set_item("fractal_control_ranges", &toml_str)
        .map_err(|_| "Failed to write to localStorage".to_string())?;
    log::info!("Saved control ranges to localStorage");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_from_nonexistent_returns_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let ranges = load_control_ranges(dir.path());
        assert_eq!(ranges, UiControlRanges::default());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut ranges = UiControlRanges::default();
        ranges.camera.fov.min = 10.0;
        ranges.camera.fov.max = 170.0;

        save_control_ranges(dir.path(), &ranges).unwrap();
        let loaded = load_control_ranges(dir.path());

        assert_eq!(loaded.camera.fov.min, 10.0);
        assert_eq!(loaded.camera.fov.max, 170.0);
        // Other fields should still be defaults
        assert_eq!(loaded.fractal, ranges.fractal);
    }

    #[test]
    fn embedded_default_toml_parses_correctly() {
        let parsed: UiControlRanges =
            toml::from_str(UiControlRanges::DEFAULT_TOML).expect("Default TOML should parse");
        assert_eq!(parsed, UiControlRanges::default());
    }
}
