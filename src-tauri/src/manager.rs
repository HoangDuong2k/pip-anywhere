//! Spawns and tracks PiP child processes and keeps the saved-source store in sync.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{ChildStdin, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager as _};

use crate::pip::{self, ChildMessage, PipConfig};
use crate::profiles::{self, Profile};
use crate::settings::{SavedSource, Store};

pub struct Manager {
    app: AppHandle,
    path: PathBuf,
    inner: Mutex<Inner>,
}

struct Inner {
    store: Store,
    running: HashMap<String, Running>,
}

struct Running {
    source: SavedSource,
    stdin: ChildStdin,
    started: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceView {
    pub id: String,
    pub label: String,
    pub profile_id: String,
    pub running: bool,
    /// False while the window picker is still open.
    pub started: bool,
    /// True when the portal gave a restore token (reopen skips the picker).
    pub remembered: bool,
    pub opacity: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct StateView {
    pub profiles: Vec<Profile>,
    pub running: Vec<SourceView>,
    pub saved: Vec<SourceView>,
}

impl Manager {
    pub fn new(app: AppHandle, path: PathBuf) -> Self {
        let store = Store::load(&path);
        Self { app, path, inner: Mutex::new(Inner { store, running: HashMap::new() }) }
    }

    pub fn state(&self) -> StateView {
        let inner = self.inner.lock().unwrap();
        let view = |s: &SavedSource, running: Option<&Running>| SourceView {
            id: s.id.clone(),
            label: s.label.clone(),
            profile_id: s.profile_id.clone(),
            running: running.is_some(),
            started: running.is_some_and(|r| r.started),
            remembered: s.restore_token.is_some(),
            opacity: s.opacity,
        };
        let mut running: Vec<_> = inner.running.values().map(|r| view(&r.source, Some(r))).collect();
        running.sort_by(|a, b| a.label.cmp(&b.label));
        let saved = inner.store.sources.iter().map(|s| view(s, inner.running.get(&s.id))).collect();
        StateView { profiles: profiles::builtin(), running, saved }
    }

    /// Opens the window picker and floats the chosen window with `profile_id`.
    pub fn pop_out(&self, profile_id: &str) -> Result<String, String> {
        let profile = profiles::find(profile_id);
        let label = {
            let inner = self.inner.lock().unwrap();
            let taken = |label: &str| {
                inner.store.sources.iter().any(|s| s.label == label)
                    || inner.running.values().any(|r| r.source.label == label)
            };
            (1..).map(|n| format!("{} #{n}", profile.name)).find(|l| !taken(l)).unwrap()
        };
        self.spawn(SavedSource {
            id: uuid::Uuid::new_v4().to_string(),
            label,
            profile_id: profile.id.clone(),
            restore_token: None,
            geometry: None,
            opacity: profile.opacity,
            reopen_on_start: false,
        })
    }

    pub fn reopen(&self, id: &str) -> Result<String, String> {
        let source = {
            let inner = self.inner.lock().unwrap();
            if inner.running.contains_key(id) {
                return Err("this PiP is already open".into());
            }
            inner.store.get(id).cloned().ok_or("unknown source")?
        };
        self.spawn(source)
    }

    pub fn close(&self, id: &str) {
        if let Some(r) = self.inner.lock().unwrap().running.get_mut(id) {
            let _ = writeln!(r.stdin, "{}", pip::Command::Close.to_line());
        }
    }

    pub fn set_opacity(&self, id: &str, opacity: f32) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap();
        let r = inner.running.get_mut(id).ok_or("this PiP is not open")?;
        let opacity = opacity.clamp(pip::MIN_OPACITY, 1.0);
        r.source.opacity = opacity;
        writeln!(r.stdin, "{}", pip::Command::SetOpacity(opacity).to_line()).map_err(|e| e.to_string())
    }

    pub fn forget(&self, id: &str) {
        self.close(id);
        let mut inner = self.inner.lock().unwrap();
        inner.store.remove(id);
        // A running PiP must not be re-saved when it reports back.
        inner.running.remove(id);
        self.save(&inner.store);
        drop(inner);
        self.changed();
    }

    pub fn rename(&self, id: &str, label: &str) -> Result<(), String> {
        let label = label.trim();
        if label.is_empty() {
            return Err("name must not be empty".into());
        }
        let mut inner = self.inner.lock().unwrap();
        if let Some(r) = inner.running.get_mut(id) {
            r.source.label = label.to_string();
        }
        if let Some(s) = inner.store.sources.iter_mut().find(|s| s.id == id) {
            s.label = label.to_string();
        }
        self.save(&inner.store);
        drop(inner);
        self.changed();
        Ok(())
    }

    /// Reopens the PiPs that were still open when the app last quit.
    pub fn restore_on_start(&self) {
        let ids: Vec<String> = {
            let inner = self.inner.lock().unwrap();
            inner.store.sources.iter().filter(|s| s.reopen_on_start).map(|s| s.id.clone()).collect()
        };
        for id in ids {
            if let Err(e) = self.reopen(&id) {
                log::warn!("could not restore {id}: {e}");
            }
        }
    }

    /// Asks every PiP to close and waits (bounded) for them to report their geometry.
    pub fn close_all_and_wait(&self, timeout: Duration) {
        let ids: Vec<String> = self.inner.lock().unwrap().running.keys().cloned().collect();
        for id in &ids {
            self.close(id);
        }
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline && !self.inner.lock().unwrap().running.is_empty() {
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn spawn(&self, source: SavedSource) -> Result<String, String> {
        let config = PipConfig {
            id: source.id.clone(),
            title: format!("PiP · {}", source.label),
            profile: profiles::find(&source.profile_id),
            restore_token: source.restore_token.clone(),
            geometry: source.geometry,
            opacity: source.opacity,
        };
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut cmd = Command::new(exe);
        cmd.arg(pip::SUBCOMMAND)
            .arg("--config")
            .arg(serde_json::to_string(&config).map_err(|e| e.to_string())?)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        // GNOME/Mutter ignores "always on top" for Wayland clients, so run the PiP through XWayland.
        #[cfg(target_os = "linux")]
        if std::env::var_os("DISPLAY").is_some() {
            cmd.env_remove("WAYLAND_DISPLAY");
        }

        let mut child = cmd.spawn().map_err(|e| format!("failed to start PiP process: {e}"))?;
        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = child.stdout.take().expect("stdout is piped");

        let id = source.id.clone();
        self.inner.lock().unwrap().running.insert(id.clone(), Running { source, stdin, started: false });

        let app = self.app.clone();
        let child_id = id.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                match serde_json::from_str::<ChildMessage>(&line) {
                    Ok(msg) => app.state::<Manager>().on_message(&child_id, msg),
                    Err(_) => log::debug!("pip {child_id}: {line}"),
                }
            }
            let _ = child.wait();
            app.state::<Manager>().on_exit(&child_id);
        });

        self.changed();
        Ok(id)
    }

    fn on_message(&self, id: &str, msg: ChildMessage) {
        let mut guard = self.inner.lock().unwrap();
        let inner = &mut *guard;
        let Some(running) = inner.running.get_mut(id) else { return };
        match msg {
            ChildMessage::Started { restore_token } => {
                running.started = true;
                if restore_token.is_some() {
                    running.source.restore_token = restore_token;
                }
                running.source.reopen_on_start = false;
                let source = running.source.clone();
                inner.store.upsert(source);
            }
            ChildMessage::Closed { reason, by_user, geometry, opacity } => {
                let running = inner.running.remove(id).expect("checked above");
                if running.started {
                    let mut source = running.source;
                    source.geometry = geometry.or(source.geometry);
                    source.opacity = opacity;
                    source.reopen_on_start = !by_user;
                    inner.store.upsert(source);
                } else if reason != "cancelled" {
                    let _ = self.app.emit("pip-error", format!("Could not start PiP: {reason}"));
                }
            }
        }
        self.save(&inner.store);
        drop(guard);
        self.changed();
    }

    fn on_exit(&self, id: &str) {
        // Only reached with an entry left if the child exited without a Closed message (crash).
        if self.inner.lock().unwrap().running.remove(id).is_some() {
            let _ = self.app.emit("pip-error", "A PiP window stopped unexpectedly.");
            self.changed();
        }
    }

    fn save(&self, store: &Store) {
        if let Err(e) = store.save(&self.path) {
            log::error!("failed to save {}: {e}", self.path.display());
        }
    }

    fn changed(&self) {
        let _ = self.app.emit("state-changed", self.state());
        crate::tray::refresh(&self.app);
    }
}
