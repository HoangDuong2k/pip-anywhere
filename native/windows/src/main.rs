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

/// `YYYY-MM-DDTHH:MM:SSZ` (UTC) for a Unix timestamp, without a date library.
fn iso_utc(secs: u64) -> String {
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Civil-from-days (Howard Hinnant), valid for all dates after 1970.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn log_tail(lines: usize) -> Vec<String> {
    let Some(text) = log_path().and_then(|p| std::fs::read_to_string(p).ok()) else {
        return Vec::new();
    };
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..]
        .iter()
        .map(|s| s.to_string())
        .collect()
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
        // The diagnostics report is large and already returned to the extension.
        let summary = serde_json::json!({ "ok": response["ok"] });
        let response = if request["cmd"] == "diagnostics" {
            &summary
        } else {
            response
        };
        writeln!(
            f,
            "{} {}ms {request} -> {response}",
            iso_utc(secs),
            started.elapsed().as_millis()
        )
    })();
}

fn main() {
    let mut backend = backend();
    let (mut stdin, mut stdout) = (io::stdin().lock(), io::stdout().lock());
    while let Ok(Some(request)) = protocol::read_message(&mut stdin) {
        let started = Instant::now();
        let mut response = protocol::handle(&request, &mut backend);
        if request["cmd"] == "diagnostics" {
            response["diagnostics"]["host"]["log_tail"] = log_tail(40).into();
        }
        if protocol::write_message(&mut stdout, &response).is_err() {
            break;
        }
        log(&request, &response, started);
    }
}

#[cfg(test)]
mod tests {
    use super::iso_utc;

    #[test]
    fn formats_utc_timestamps() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_utc(951_782_400), "2000-02-29T00:00:00Z"); // leap day
        assert_eq!(iso_utc(1_790_924_544), "2026-10-02T07:02:24Z");
    }
}
