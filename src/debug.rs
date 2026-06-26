//! Opt-in debug logging.
//!
//! When the program is launched with `--debug`/`-d`, every `debug!` invocation
//! is timestamped and appended to `/tmp/oebb-debug.log`. Without the flag the
//! macro compiles to a cheap no-op call that returns early.

use chrono::Local;
use std::io::Write;
use std::sync::LazyLock;

/// A lazily-initialised logger that writes to a temp file when enabled.
pub struct DebugLogger {
    /// Whether `--debug`/`-d` was passed on the command line.
    pub enabled: bool,
    file: Option<std::sync::Mutex<std::fs::File>>,
}

impl DebugLogger {
    fn new(enabled: bool) -> Self {
        let file = if enabled {
            std::fs::File::create("/tmp/oebb-debug.log")
                .ok()
                .map(std::sync::Mutex::new)
        } else {
            None
        };
        Self { enabled, file }
    }

    /// Append a timestamped line to the log file. No-op when disabled.
    pub fn log(&self, msg: String) {
        if !self.enabled {
            return;
        }
        if let Some(ref file) = self.file {
            if let Ok(mut f) = file.lock() {
                let timestamp = Local::now().format("%H:%M:%S%.3f");
                let _ = writeln!(f, "[{}] {}", timestamp, msg);
                let _ = f.flush();
            }
        }
    }
}

/// The process-wide logger, initialised from the command line on first use.
pub static DEBUG: LazyLock<DebugLogger> = LazyLock::new(|| {
    let enabled = std::env::args().any(|arg| arg == "--debug" || arg == "-d");
    DebugLogger::new(enabled)
});

/// Format and log a message through [`DEBUG`]. Usable from any module.
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        $crate::debug::DEBUG.log(format!($($arg)*))
    };
}
