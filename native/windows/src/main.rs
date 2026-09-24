//! PiP Anywhere native messaging host for Windows. Chrome starts it per request from the
//! extension; see `protocol.rs` for the message format (shared with `native/host.py`).

mod protocol;
#[cfg(windows)]
mod win32;

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Instant;

use protocol::Backend;
use serde_json::Value;

const LOG_MAX_BYTES: u64 = 256 * 1024;

#[cfg(windows)]
fn backend() -> impl Backend {
    win32::Win32
}

/// Lets the protocol be built and unit-tested on other systems.
#[cfg(not(windows))]
fn backend() -> impl Backend {
    use protocol::{HostError, WindowInfo};

    struct Unsupported;
    impl Backend for Unsupported {
        fn focused(&mut self) -> Result<Option<WindowInfo>, HostError> {
            Err(HostError::new(
                "unsupported",
                "This host only works on Windows; use native/host.py on Linux.",
            ))
        }
        fn set_above(&mut self, _: bool) -> Result<Option<WindowInfo>, HostError> {
            self.focused()
        }
        fn set_opacity(&mut self, _: f64) -> Result<Option<WindowInfo>, HostError> {
            self.focused()
        }
    }
    Unsupported
}

/// `%LOCALAPPDATA%\PipAnywhere\host.log` (Windows) or `$XDG_STATE_HOME/pip-anywhere/host.log`.
fn log_path() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(|p| PathBuf::from(p).join("PipAnywhere"))
        .or_else(|| {
            std::env::var_os("XDG_STATE_HOME").map(|p| PathBuf::from(p).join("pip-anywhere"))
        })?;
    Some(base.join("host.log"))
}

/// One line per request, for troubleshooting. Never fails the request.
fn log(request: &Value, response: &Value, started: Instant) {
    let Some(path) = log_path() else { return };
    let _ = (|| -> io::Result<()> {
        std::fs::create_dir_all(path.parent().unwrap())?;
        if std::fs::metadata(&path)
            .map(|m| m.len() > LOG_MAX_BYTES)
            .unwrap_or(false)
        {
            std::fs::rename(&path, path.with_extension("log.1"))?;
        }
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        writeln!(
            f,
            "{secs} {}ms {request} -> {response}",
            started.elapsed().as_millis()
        )
    })();
}

fn main() {
    let mut backend = backend();
    let (mut stdin, mut stdout) = (io::stdin().lock(), io::stdout().lock());
    while let Ok(Some(request)) = protocol::read_message(&mut stdin) {
        let started = Instant::now();
        let response = protocol::handle(&request, &mut backend);
        if protocol::write_message(&mut stdout, &response).is_err() {
            break;
        }
        log(&request, &response, started);
    }
}
