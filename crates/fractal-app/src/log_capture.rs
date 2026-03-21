//! In-app log capture for the log window.
//!
//! Implements `log::Log` to capture log messages into a shared ring buffer
//! while also forwarding to stderr (native) or browser console (WASM).
//! Only captures INFO, WARN, and ERROR levels (debug/trace are ignored).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Maximum number of log entries to keep in memory.
const MAX_ENTRIES: usize = 1000;

/// A captured log entry.
#[derive(Clone)]
pub struct LogEntry {
    pub level: log::Level,
    pub target: String,
    pub message: String,
}

impl LogEntry {
    /// Formatted display string: `[LEVEL] target - message`
    pub fn formatted(&self) -> String {
        format!("[{}] {} - {}", self.level, self.target, self.message)
    }
}

/// Shared log entry buffer. Clone the Arc to share between logger and UI.
pub type LogBuffer = Arc<Mutex<VecDeque<LogEntry>>>;

/// Custom logger that captures entries to a ring buffer and forwards to console.
struct LogCapture {
    buffer: LogBuffer,
    level: log::LevelFilter,
}

impl log::Log for LogCapture {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let entry = LogEntry {
            level: record.level(),
            target: record.target().to_string(),
            message: record.args().to_string(),
        };

        // Store in ring buffer
        if let Ok(mut buf) = self.buffer.lock() {
            if buf.len() >= MAX_ENTRIES {
                buf.pop_front();
            }
            buf.push_back(entry);
        }

        // Forward to platform console
        #[cfg(target_arch = "wasm32")]
        {
            let msg = format!("[{}] {} - {}", record.level(), record.target(), record.args());
            match record.level() {
                log::Level::Error => web_sys::console::error_1(&msg.into()),
                log::Level::Warn => web_sys::console::warn_1(&msg.into()),
                _ => web_sys::console::log_1(&msg.into()),
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            eprintln!("[{}] {} - {}", record.level(), record.target(), record.args());
        }
    }

    fn flush(&self) {}
}

/// Initialize the log capture system. Returns the shared buffer for UI access.
///
/// Only captures INFO, WARN, and ERROR levels. Debug and trace are ignored.
/// Must be called exactly once, before any log macros are used.
pub fn init(level: log::LevelFilter) -> LogBuffer {
    let buffer: LogBuffer = Arc::new(Mutex::new(VecDeque::with_capacity(MAX_ENTRIES)));

    let logger = LogCapture {
        buffer: buffer.clone(),
        level,
    };

    log::set_boxed_logger(Box::new(logger)).expect("Logger already initialized");
    log::set_max_level(level);

    buffer
}
