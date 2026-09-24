//! Native messaging protocol, identical to `native/host.py`: each message is a 32-bit
//! native-endian length followed by UTF-8 JSON.
//!
//! Requests:  {"cmd":"ping"} | {"cmd":"state"} | {"cmd":"set_above","above":bool}
//!            | {"cmd":"set_opacity","opacity":number}
//! Responses: {"ok":true,"window":{...}} | {"ok":false,"error":"...","code":"..."}

use std::io::{self, Read, Write};

use serde_json::{json, Value};

pub const VERSION: u32 = 3;
pub const MIN_OPACITY: f64 = 0.2;
/// Chrome caps messages to the host at 4 GB; anything near that is garbage.
const MAX_MESSAGE_BYTES: u32 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct WindowInfo {
    pub id: u64,
    pub title: String,
    /// Process executable, e.g. `chrome.exe` (named after the GNOME field).
    pub wm_class: String,
    pub above: bool,
    pub opacity: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HostError {
    pub code: &'static str,
    pub message: String,
}

impl HostError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Acts on the focused top-level window (the browser window the user is in).
pub trait Backend {
    fn focused(&mut self) -> Result<Option<WindowInfo>, HostError>;
    fn set_above(&mut self, above: bool) -> Result<Option<WindowInfo>, HostError>;
    fn set_opacity(&mut self, opacity: f64) -> Result<Option<WindowInfo>, HostError>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Ping,
    State,
    SetAbove(bool),
    SetOpacity(f64),
}

pub fn parse(request: &Value) -> Result<Request, HostError> {
    let bad = |msg: &str| HostError::new("bad_request", msg);
    let obj = request
        .as_object()
        .ok_or_else(|| bad("request must be a JSON object"))?;
    match obj.get("cmd").and_then(Value::as_str) {
        Some("ping") => Ok(Request::Ping),
        Some("state") => Ok(Request::State),
        Some("set_above") => Ok(Request::SetAbove(
            obj.get("above").and_then(Value::as_bool).unwrap_or(false),
        )),
        Some("set_opacity") => {
            let opacity = obj
                .get("opacity")
                .and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse().ok()))
                .filter(|v| v.is_finite())
                .ok_or_else(|| bad("opacity must be a number"))?;
            Ok(Request::SetOpacity(opacity.clamp(MIN_OPACITY, 1.0)))
        }
        other => Err(bad(&format!("unknown command: {other:?}"))),
    }
}

fn window_json(w: &WindowInfo) -> Value {
    json!({
        "id": w.id,
        "title": w.title,
        "wm_class": w.wm_class,
        "above": w.above,
        // Rounded like the GNOME helper reports it (a byte of alpha is ~0.004).
        "opacity": (w.opacity * 100.0).round() / 100.0,
    })
}

/// Handles one request and returns the response (never fails).
pub fn handle(request: &Value, backend: &mut dyn Backend) -> Value {
    let result = match parse(request) {
        Ok(Request::Ping) => return json!({ "ok": true, "version": VERSION }),
        Ok(Request::State) => backend.focused(),
        Ok(Request::SetAbove(above)) => backend.set_above(above),
        Ok(Request::SetOpacity(opacity)) => backend.set_opacity(opacity),
        Err(e) => Err(e),
    };
    match result {
        Ok(Some(window)) => json!({ "ok": true, "window": window_json(&window) }),
        Ok(None) => json!({ "ok": false, "error": "No focused window.", "code": "no_window" }),
        Err(e) => json!({ "ok": false, "error": e.message, "code": e.code }),
    }
}

/// Reads one message; `Ok(None)` at end of input.
pub fn read_message(r: &mut impl Read) -> io::Result<Option<Value>> {
    let mut header = [0u8; 4];
    match r.read_exact(&mut header) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_ne_bytes(header);
    if len > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message too large",
        ));
    }
    let mut body = vec![0u8; len as usize];
    r.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn write_message(w: &mut impl Write, message: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(message)?;
    w.write_all(&(body.len() as u32).to_ne_bytes())?;
    w.write_all(&body)?;
    w.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Fake {
        calls: Vec<String>,
        window: Option<WindowInfo>,
    }

    impl Fake {
        fn with_window() -> Self {
            let window = WindowInfo {
                id: 42,
                title: "Tab – Google Chrome".into(),
                wm_class: "chrome.exe".into(),
                above: false,
                opacity: 1.0,
            };
            Self {
                calls: vec![],
                window: Some(window),
            }
        }
    }

    impl Backend for Fake {
        fn focused(&mut self) -> Result<Option<WindowInfo>, HostError> {
            self.calls.push("focused".into());
            Ok(self.window.clone())
        }
        fn set_above(&mut self, above: bool) -> Result<Option<WindowInfo>, HostError> {
            self.calls.push(format!("above {above}"));
            if let Some(w) = &mut self.window {
                w.above = above;
            }
            Ok(self.window.clone())
        }
        fn set_opacity(&mut self, opacity: f64) -> Result<Option<WindowInfo>, HostError> {
            self.calls.push(format!("opacity {opacity:.3}"));
            if let Some(w) = &mut self.window {
                w.opacity = opacity;
            }
            Ok(self.window.clone())
        }
    }

    #[test]
    fn framing_round_trip() {
        let mut buf = Vec::new();
        write_message(&mut buf, &json!({"cmd": "state", "text": "Tiếng Việt"})).unwrap();
        let mut r = buf.as_slice();
        assert_eq!(
            read_message(&mut r).unwrap(),
            Some(json!({"cmd": "state", "text": "Tiếng Việt"}))
        );
        assert_eq!(read_message(&mut r).unwrap(), None);
    }

    #[test]
    fn rejects_oversized_messages() {
        let buf = (MAX_MESSAGE_BYTES + 1).to_ne_bytes();
        assert!(read_message(&mut buf.as_slice()).is_err());
    }

    #[test]
    fn ping_reports_version() {
        let mut fake = Fake::default();
        assert_eq!(
            handle(&json!({"cmd": "ping"}), &mut fake),
            json!({"ok": true, "version": VERSION})
        );
        assert!(fake.calls.is_empty());
    }

    #[test]
    fn commands_reach_the_backend() {
        let mut fake = Fake::with_window();
        handle(&json!({"cmd": "state"}), &mut fake);
        let r = handle(&json!({"cmd": "set_above", "above": true}), &mut fake);
        assert_eq!(r["window"]["above"], json!(true));
        let r = handle(&json!({"cmd": "set_opacity", "opacity": 0.5}), &mut fake);
        assert_eq!(r["window"]["opacity"], json!(0.5));
        assert_eq!(r["window"]["wm_class"], json!("chrome.exe"));
        assert_eq!(fake.calls, ["focused", "above true", "opacity 0.500"]);
    }

    #[test]
    fn opacity_is_clamped() {
        let mut fake = Fake::with_window();
        handle(&json!({"cmd": "set_opacity", "opacity": 0}), &mut fake);
        handle(&json!({"cmd": "set_opacity", "opacity": 7}), &mut fake);
        assert_eq!(fake.calls, ["opacity 0.200", "opacity 1.000"]);
    }

    #[test]
    fn opacity_is_rounded_like_gnome() {
        let mut fake = Fake::with_window();
        fake.window.as_mut().unwrap().opacity = 128.0 / 255.0;
        assert_eq!(
            handle(&json!({"cmd": "state"}), &mut fake)["window"]["opacity"],
            json!(0.5)
        );
    }

    #[test]
    fn bad_requests_do_not_reach_the_backend() {
        let mut fake = Fake::with_window();
        for req in [
            json!([]),
            json!({"cmd": "nope"}),
            json!({"cmd": "set_opacity", "opacity": "x"}),
            json!({"cmd": "set_opacity"}),
        ] {
            let r = handle(&req, &mut fake);
            assert_eq!(r["ok"], json!(false));
            assert_eq!(r["code"], json!("bad_request"));
        }
        assert!(fake.calls.is_empty());
    }

    #[test]
    fn no_focused_window() {
        let mut fake = Fake::default();
        assert_eq!(
            handle(&json!({"cmd": "state"}), &mut fake),
            json!({"ok": false, "error": "No focused window.", "code": "no_window"})
        );
    }
}
