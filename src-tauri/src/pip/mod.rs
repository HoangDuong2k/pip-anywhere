//! The PiP window runs in a child process (`pip-anywhere pip --config <json>`), one per
//! floating window. The control app talks to it over stdin/stdout, one JSON message per line.
//!
//! Separate processes keep each capture isolated and let the PiP use X11 (via XWayland)
//! on GNOME Wayland, where "always on top" is otherwise not available to applications.

pub mod render;
mod window;

use serde::{Deserialize, Serialize};

use crate::profiles::Profile;

pub const SUBCOMMAND: &str = "pip";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Everything a PiP child needs, passed on the command line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipConfig {
    pub id: String,
    pub title: String,
    pub profile: Profile,
    pub restore_token: Option<String>,
    pub geometry: Option<Geometry>,
    pub opacity: f32,
}

/// Messages from the child to the control app (stdout).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChildMessage {
    /// Capture started; `restore_token` lets the next session skip the picker.
    Started { restore_token: Option<String> },
    /// The window closed. `by_user` is false when the control app asked it to close.
    Closed { reason: String, by_user: bool, geometry: Option<Geometry>, opacity: f32 },
}

/// Commands from the control app to the child (stdin), one per line: `close`, `opacity <0.2-1.0>`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    Close,
    SetOpacity(f32),
}

pub const MIN_OPACITY: f32 = 0.2;

impl Command {
    pub fn to_line(self) -> String {
        match self {
            Command::Close => "close".into(),
            Command::SetOpacity(v) => format!("opacity {v}"),
        }
    }

    pub fn parse(line: &str) -> Option<Self> {
        match line.trim().split_once(' ') {
            None if line.trim() == "close" => Some(Command::Close),
            Some(("opacity", v)) => v
                .trim()
                .parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .map(|v| Command::SetOpacity(v.clamp(MIN_OPACITY, 1.0))),
            _ => None,
        }
    }
}

pub fn send(msg: &ChildMessage) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    let _ = serde_json::to_writer(&mut out, msg);
    let _ = out.write_all(b"\n");
    let _ = out.flush();
}

/// Entry point of the child process. `args` are the arguments after the subcommand.
pub fn run_child(args: &[String]) -> i32 {
    let config = match args {
        [flag, json] if flag == "--config" => serde_json::from_str::<PipConfig>(json),
        _ => {
            eprintln!("usage: pip-anywhere {SUBCOMMAND} --config <json>");
            return 2;
        }
    };
    match config {
        Ok(config) => match window::run(config) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("pip window failed: {e}");
                1
            }
        },
        Err(e) => {
            eprintln!("invalid --config: {e}");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_round_trip() {
        for cmd in [Command::Close, Command::SetOpacity(0.5)] {
            assert_eq!(Command::parse(&cmd.to_line()), Some(cmd));
        }
    }

    #[test]
    fn opacity_is_clamped_and_garbage_rejected() {
        assert_eq!(Command::parse("opacity 0"), Some(Command::SetOpacity(MIN_OPACITY)));
        assert_eq!(Command::parse("opacity 3"), Some(Command::SetOpacity(1.0)));
        assert_eq!(Command::parse("opacity NaN"), None);
        assert_eq!(Command::parse("opacity"), None);
        assert_eq!(Command::parse("hello"), None);
    }
}
