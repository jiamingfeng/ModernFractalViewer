//! Session save/load management
//!
//! Provides platform-aware storage for fractal exploration sessions.
//! - Native (Windows/macOS/Linux): filesystem via `dirs::data_dir()`
//! - WASM: `localStorage` via `web_sys`
//! - Android: filesystem via `AndroidApp::internal_data_path()`

use fractal_core::SavedSession;
use std::fmt;

/// Errors that can occur during session I/O
#[derive(Debug)]
pub enum SessionError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Storage(String),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionError::Io(e) => write!(f, "I/O error: {e}"),
            SessionError::Json(e) => write!(f, "JSON error: {e}"),
            SessionError::Storage(e) => write!(f, "Storage error: {e}"),
        }
    }
}

impl From<std::io::Error> for SessionError {
    fn from(e: std::io::Error) -> Self {
        SessionError::Io(e)
    }
}

impl From<serde_json::Error> for SessionError {
    fn from(e: serde_json::Error) -> Self {
        SessionError::Json(e)
    }
}

pub type Result<T> = std::result::Result<T, SessionError>;

// ============================================================================
// Storage backends
// ============================================================================

/// Platform-agnostic storage trait for session data.
/// Values are JSON strings identified by string IDs.
trait StorageBackend {
    fn save(&self, id: &str, data: &str) -> Result<()>;
    fn load(&self, id: &str) -> Result<String>;
    fn delete(&self, id: &str) -> Result<()>;
    /// Returns a list of save IDs (sorted newest-first by convention).
    fn list(&self) -> Result<Vec<String>>;
}

// ---------------------------------------------------------------------------
// Native + Android: Filesystem backend
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
struct FileSystemStorage {
    saves_dir: std::path::PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl FileSystemStorage {
    fn new(saves_dir: std::path::PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&saves_dir)?;
        Ok(Self { saves_dir })
    }

    fn file_path(&self, id: &str) -> std::path::PathBuf {
        self.saves_dir.join(format!("{id}.json"))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl StorageBackend for FileSystemStorage {
    fn save(&self, id: &str, data: &str) -> Result<()> {
        std::fs::write(self.file_path(id), data)?;
        Ok(())
    }

    fn load(&self, id: &str) -> Result<String> {
        let data = std::fs::read_to_string(self.file_path(id))?;
        Ok(data)
    }

    fn delete(&self, id: &str) -> Result<()> {
        let path = self.file_path(id);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    fn list(&self) -> Result<Vec<String>> {
        let mut saves = Vec::new();
        if self.saves_dir.exists() {
            for entry in std::fs::read_dir(&self.saves_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        saves.push(stem.to_string());
                    }
                }
            }
        }
        // Sort reverse-alphabetically (timestamp IDs sort newest-first)
        saves.sort_unstable_by(|a, b| b.cmp(a));
        Ok(saves)
    }
}

// ---------------------------------------------------------------------------
// WASM: localStorage backend
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
struct LocalStorageBackend;

#[cfg(target_arch = "wasm32")]
impl LocalStorageBackend {
    const KEY_PREFIX: &'static str = "fractal_save_";
    const INDEX_KEY: &'static str = "fractal_save_index";

    fn storage() -> Result<web_sys::Storage> {
        web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .ok_or_else(|| SessionError::Storage("localStorage not available".into()))
    }

    fn prefixed_key(id: &str) -> String {
        format!("{}{}", Self::KEY_PREFIX, id)
    }

    /// Load the save index (list of IDs) from localStorage.
    fn load_index(storage: &web_sys::Storage) -> Vec<String> {
        storage
            .get_item(Self::INDEX_KEY)
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
            .unwrap_or_default()
    }

    /// Persist the save index to localStorage.
    fn save_index(storage: &web_sys::Storage, index: &[String]) -> Result<()> {
        let json = serde_json::to_string(index)?;
        storage
            .set_item(Self::INDEX_KEY, &json)
            .map_err(|_| SessionError::Storage("failed to write index".into()))
    }
}

#[cfg(target_arch = "wasm32")]
impl StorageBackend for LocalStorageBackend {
    fn save(&self, id: &str, data: &str) -> Result<()> {
        let storage = Self::storage()?;
        storage
            .set_item(&Self::prefixed_key(id), data)
            .map_err(|_| SessionError::Storage("localStorage write failed".into()))?;
        // Add to index if not already present
        let mut index = Self::load_index(&storage);
        if !index.contains(&id.to_string()) {
            index.insert(0, id.to_string());
            Self::save_index(&storage, &index)?;
        }
        Ok(())
    }

    fn load(&self, id: &str) -> Result<String> {
        let storage = Self::storage()?;
        storage
            .get_item(&Self::prefixed_key(id))
            .map_err(|_| SessionError::Storage("localStorage read failed".into()))?
            .ok_or_else(|| SessionError::Storage(format!("save '{id}' not found")))
    }

    fn delete(&self, id: &str) -> Result<()> {
        let storage = Self::storage()?;
        storage
            .remove_item(&Self::prefixed_key(id))
            .map_err(|_| SessionError::Storage("localStorage delete failed".into()))?;
        // Remove from index
        let mut index = Self::load_index(&storage);
        index.retain(|s| s != id);
        Self::save_index(&storage, &index)?;
        Ok(())
    }

    fn list(&self) -> Result<Vec<String>> {
        let storage = Self::storage()?;
        Ok(Self::load_index(&storage))
    }
}

// ============================================================================
// SessionManager — public API
// ============================================================================

/// Manages save/load operations for fractal sessions.
pub struct SessionManager {
    backend: Box<dyn StorageBackend>,
}

impl SessionManager {
    /// Create a new session manager with the platform-appropriate storage backend.
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    pub fn new() -> Result<Self> {
        let saves_dir = dirs::data_dir()
            .ok_or_else(|| SessionError::Storage("could not determine data directory".into()))?
            .join("ModernFractalViewer")
            .join("saves");
        log::info!("Session saves directory: {}", saves_dir.display());
        let backend = FileSystemStorage::new(saves_dir)?;
        Ok(Self {
            backend: Box::new(backend),
        })
    }

    /// Create a new session manager for Android.
    #[cfg(target_os = "android")]
    pub fn new_android(data_path: std::path::PathBuf) -> Result<Self> {
        let saves_dir = data_path.join("saves");
        log::info!("Session saves directory (Android): {}", saves_dir.display());
        let backend = FileSystemStorage::new(saves_dir)?;
        Ok(Self {
            backend: Box::new(backend),
        })
    }

    /// Create a new session manager for WASM.
    #[cfg(target_arch = "wasm32")]
    pub fn new() -> Result<Self> {
        Ok(Self {
            backend: Box::new(LocalStorageBackend),
        })
    }

    /// Generate a unique save ID from the current timestamp.
    pub fn generate_id() -> String {
        // Use SystemTime to avoid adding chrono dependency.
        // Format: YYYYMMDD_HHMMSS (UTC)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Convert epoch seconds to date/time components
        let secs = now;
        let days = secs / 86400;
        let time_of_day = secs % 86400;
        let hours = time_of_day / 3600;
        let minutes = (time_of_day % 3600) / 60;
        let seconds = time_of_day % 60;

        // Simple days-to-date conversion (accounts for leap years)
        let (year, month, day) = days_to_ymd(days);

        format!(
            "{year:04}{month:02}{day:02}_{hours:02}{minutes:02}{seconds:02}"
        )
    }

    /// Generate an ISO 8601 timestamp string.
    pub fn timestamp_iso8601() -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let secs = now;
        let days = secs / 86400;
        let time_of_day = secs % 86400;
        let hours = time_of_day / 3600;
        let minutes = (time_of_day % 3600) / 60;
        let seconds = time_of_day % 60;
        let (year, month, day) = days_to_ymd(days);

        format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
    }

    /// Save a session.
    pub fn save(&self, session: &SavedSession) -> Result<String> {
        let id = Self::generate_id();
        let json = serde_json::to_string_pretty(session)?;
        self.backend.save(&id, &json)?;
        log::info!("Saved session '{}' as {}", session.name, id);
        Ok(id)
    }

    /// Load a session by ID.
    pub fn load(&self, id: &str) -> Result<SavedSession> {
        let json = self.backend.load(id)?;
        // Parse through serde_json::Value first for version-gated migration
        let raw: serde_json::Value = serde_json::from_str(&json)?;
        match raw.get("version").and_then(|v| v.as_str()) {
            Some("1") | None => {
                // Version 1 or legacy (no version field): direct deserialize
                let session: SavedSession = serde_json::from_value(raw)?;
                log::info!("Loaded session '{}' from {}", session.name, id);
                Ok(session)
            }
            Some(v) => Err(SessionError::Storage(format!(
                "unsupported save version: {v}"
            ))),
        }
    }

    /// Delete a session by ID.
    pub fn delete(&self, id: &str) -> Result<()> {
        self.backend.delete(id)?;
        log::info!("Deleted session {}", id);
        Ok(())
    }

    /// List all saved session IDs (newest first).
    pub fn list_saves(&self) -> Result<Vec<String>> {
        self.backend.list()
    }

    /// List all saved sessions with metadata (newest first).
    /// Sessions that fail to parse are silently skipped.
    pub fn list_sessions(&self) -> Result<Vec<(String, SavedSession)>> {
        let ids = self.backend.list()?;
        let mut sessions = Vec::with_capacity(ids.len());
        for id in ids {
            if let Ok(session) = self.load(&id) {
                sessions.push((id, session));
            }
        }
        Ok(sessions)
    }
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Adapted from Howard Hinnant's civil_from_days algorithm
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
