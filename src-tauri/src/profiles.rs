//! App profiles: per-application presets (crop, frame rate, size, opacity).
//!
//! Built-in profiles live in `/profiles/*.json` and are compiled into the binary.

use serde::{Deserialize, Serialize};

const BUILTIN: &[&str] = &[
    include_str!("../../profiles/default.json"),
    include_str!("../../profiles/chrome.json"),
    include_str!("../../profiles/terminal.json"),
    include_str!("../../profiles/files.json"),
    include_str!("../../profiles/vscode.json"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub fps: u32,
    pub opacity: f32,
    pub default_size: Size,
    #[serde(default)]
    pub crop: Crop,
    /// App ids / process names used for automatic matching (Windows, macOS).
    #[serde(default)]
    pub match_apps: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

/// Region of the source window to show, as insets in source pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Crop {
    #[serde(default)]
    pub top: u32,
    #[serde(default)]
    pub right: u32,
    #[serde(default)]
    pub bottom: u32,
    #[serde(default)]
    pub left: u32,
    /// After insets, keep only this many pixels at the bottom (e.g. the latest terminal output).
    #[serde(default)]
    pub keep_bottom: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Crop {
    /// Resolves the crop against a source of `width` x `height`.
    /// Falls back to the full source when the insets would leave nothing visible,
    /// e.g. while the source window is tiny.
    pub fn apply(&self, width: u32, height: u32) -> Rect {
        let full = Rect { x: 0, y: 0, width, height };
        let w = width.saturating_sub(self.left + self.right);
        let mut h = height.saturating_sub(self.top + self.bottom);
        if w == 0 || h == 0 {
            return full;
        }
        let mut y = self.top;
        if let Some(keep) = self.keep_bottom {
            if keep > 0 && keep < h {
                y += h - keep;
                h = keep;
            }
        }
        Rect { x: self.left, y, width: w, height: h }
    }
}

pub fn builtin() -> Vec<Profile> {
    BUILTIN
        .iter()
        .map(|json| serde_json::from_str(json).expect("built-in profile must be valid JSON"))
        .collect()
}

pub fn find(id: &str) -> Profile {
    let all = builtin();
    all.iter().find(|p| p.id == id).cloned().unwrap_or_else(|| all[0].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_profiles_parse_and_have_unique_ids() {
        let all = builtin();
        assert_eq!(all[0].id, "default");
        let mut ids: Vec<_> = all.iter().map(|p| p.id.clone()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), all.len());
        assert!(all.iter().all(|p| p.fps > 0 && p.opacity > 0.0 && p.opacity <= 1.0));
    }

    #[test]
    fn unknown_profile_falls_back_to_default() {
        assert_eq!(find("nope").id, "default");
        assert_eq!(find("terminal").id, "terminal");
    }

    #[test]
    fn crop_insets() {
        let c = Crop { top: 10, right: 5, bottom: 20, left: 15, keep_bottom: None };
        assert_eq!(c.apply(100, 100), Rect { x: 15, y: 10, width: 80, height: 70 });
    }

    #[test]
    fn crop_keep_bottom() {
        let c = Crop { top: 40, keep_bottom: Some(100), ..Default::default() };
        assert_eq!(c.apply(800, 600), Rect { x: 0, y: 500, width: 800, height: 100 });
        // Smaller than keep_bottom: keep everything below the inset.
        assert_eq!(c.apply(800, 120), Rect { x: 0, y: 40, width: 800, height: 80 });
    }

    #[test]
    fn crop_larger_than_source_shows_full_source() {
        let c = Crop { top: 88, ..Default::default() };
        assert_eq!(c.apply(300, 50), Rect { x: 0, y: 0, width: 300, height: 50 });
    }
}
