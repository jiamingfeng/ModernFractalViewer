//! File-polling hot-reload watcher for shaders and config files.
//!
//! Only compiled when the `hot-reload` feature is enabled.
//! Polls file modification times at a configurable interval (default 500ms).

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

/// Events returned by the hot-reloader.
#[derive(Debug, PartialEq, Eq)]
pub enum HotReloadEvent {
    None,
    ShaderChanged,
    ConfigChanged,
}

/// Simple file-polling hot-reload watcher.
pub struct HotReloader {
    /// Shader file paths: (sdf_common.wgsl, raymarcher.wgsl)
    shader_paths: Option<(PathBuf, PathBuf)>,
    config_path: Option<PathBuf>,
    last_shader_common_modified: Option<SystemTime>,
    last_shader_render_modified: Option<SystemTime>,
    last_config_modified: Option<SystemTime>,
    last_check: Instant,
    check_interval: Duration,
    /// Last shader compile error (shown in debug overlay).
    pub shader_error: Option<String>,
}

impl HotReloader {
    pub fn new(shader_paths: Option<(PathBuf, PathBuf)>, config_path: Option<PathBuf>) -> Self {
        let last_shader_common_modified = shader_paths.as_ref().and_then(|p| file_mtime(&p.0));
        let last_shader_render_modified = shader_paths.as_ref().and_then(|p| file_mtime(&p.1));
        let last_config_modified = config_path.as_ref().and_then(|p| file_mtime(p));

        if let Some(ref paths) = shader_paths {
            log::info!("Hot-reload: watching shaders at {}, {}", paths.0.display(), paths.1.display());
        }
        if let Some(ref path) = config_path {
            log::info!("Hot-reload: watching config at {}", path.display());
        }

        Self {
            shader_paths,
            config_path,
            last_shader_common_modified,
            last_shader_render_modified,
            last_config_modified,
            last_check: Instant::now(),
            check_interval: Duration::from_millis(500),
            shader_error: None,
        }
    }

    /// Call once per frame. Returns what changed (if anything).
    /// Only checks file mtimes every `check_interval` to avoid I/O overhead.
    pub fn poll(&mut self) -> HotReloadEvent {
        if self.last_check.elapsed() < self.check_interval {
            return HotReloadEvent::None;
        }
        self.last_check = Instant::now();

        // Check shader files (either sdf_common.wgsl or raymarcher.wgsl)
        if let Some(ref paths) = self.shader_paths {
            let common_mtime = file_mtime(&paths.0);
            let render_mtime = file_mtime(&paths.1);
            let common_changed = common_mtime != self.last_shader_common_modified && common_mtime.is_some();
            let render_changed = render_mtime != self.last_shader_render_modified && render_mtime.is_some();
            if common_changed || render_changed {
                self.last_shader_common_modified = common_mtime;
                self.last_shader_render_modified = render_mtime;
                return HotReloadEvent::ShaderChanged;
            }
        }

        // Check config file
        if let Some(ref path) = self.config_path {
            let current_mtime = file_mtime(path);
            if current_mtime != self.last_config_modified && current_mtime.is_some() {
                self.last_config_modified = current_mtime;
                return HotReloadEvent::ConfigChanged;
            }
        }

        HotReloadEvent::None
    }

    /// Read the concatenated shader source (sdf_common + raymarcher) from disk.
    /// Returns None if either file doesn't exist.
    pub fn read_shader(&self) -> Option<String> {
        let paths = self.shader_paths.as_ref()?;
        let common = std::fs::read_to_string(&paths.0).ok()?;
        let render = std::fs::read_to_string(&paths.1).ok()?;
        Some(format!("{common}\n{render}"))
    }

    /// Read the config source from disk. Returns None if file doesn't exist.
    pub fn read_config(&self) -> Option<String> {
        self.config_path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
    }
}

fn file_mtime(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
}
