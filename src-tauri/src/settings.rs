//! Persistent state: remembered sources (restore token, geometry, opacity) in `state.json`.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::pip::Geometry;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSource {
    pub id: String,
    pub label: String,
    pub profile_id: String,
    /// Portal restore token; lets the PiP reopen the same window without the picker.
    pub restore_token: Option<String>,
    pub geometry: Option<Geometry>,
    pub opacity: f32,
    /// True when the PiP was still open when the app quit.
    #[serde(default)]
    pub reopen_on_start: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub sources: Vec<SavedSource>,
}

impl Store {
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_else(|e| {
                log::warn!("ignoring corrupt {}: {e}", path.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, path)
    }

    pub fn get(&self, id: &str) -> Option<&SavedSource> {
        self.sources.iter().find(|s| s.id == id)
    }

    pub fn upsert(&mut self, source: SavedSource) {
        match self.sources.iter_mut().find(|s| s.id == source.id) {
            Some(existing) => *existing = source,
            None => self.sources.push(source),
        }
    }

    pub fn remove(&mut self, id: &str) {
        self.sources.retain(|s| s.id != id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(id: &str) -> SavedSource {
        SavedSource {
            id: id.into(),
            label: "Terminal #1".into(),
            profile_id: "terminal".into(),
            restore_token: Some("tok".into()),
            geometry: Some(Geometry { x: 10, y: 20, width: 300, height: 200 }),
            opacity: 0.9,
            reopen_on_start: true,
        }
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = std::env::temp_dir().join(format!("pip-anywhere-test-{}", uuid::Uuid::new_v4()));
        let path = dir.join("state.json");
        let mut store = Store::default();
        store.upsert(source("a"));
        store.upsert(source("b"));
        store.save(&path).unwrap();

        let loaded = Store::load(&path);
        assert_eq!(loaded.sources.len(), 2);
        assert_eq!(loaded.get("a").unwrap().geometry, source("a").geometry);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn upsert_replaces_and_remove_deletes() {
        let mut store = Store::default();
        store.upsert(source("a"));
        store.upsert(SavedSource { label: "renamed".into(), ..source("a") });
        assert_eq!(store.sources.len(), 1);
        assert_eq!(store.get("a").unwrap().label, "renamed");
        store.remove("a");
        assert!(store.sources.is_empty());
    }

    #[test]
    fn missing_or_corrupt_file_gives_empty_store() {
        assert!(Store::load(Path::new("/nonexistent/state.json")).sources.is_empty());
    }
}
