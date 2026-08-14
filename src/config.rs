//! User-facing settings, persisted as JSON.
//!
//! There are exactly two: whether AutoCopy is active, and how long it waits
//! before acting. There is deliberately no other configuration surface — no
//! clipboard history, no per-app rules, nothing that would turn this into a
//! bigger tool than it claims to be. There is also no in-app settings
//! window; `enabled` is exposed directly as a checkbox in the tray menu (see
//! `tray.rs`), and this struct is just what gets written to disk.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Master switch, mirrored by the "Enabled" checkbox in the tray menu.
    /// When on, AutoCopy copies after gestures that look like a finished
    /// text selection (a drag, or a double/triple-click) — see the
    /// `platform` module docs for the two layers of filtering behind that.
    pub enabled: bool,

    /// Delay, in milliseconds, between the triggering event (a
    /// selection-shaped mouse gesture, or ⌘A) and sending the synthesized
    /// shortcut. Gives the frontmost app a moment to finish updating its
    /// selection before we act on it.
    pub action_delay_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            action_delay_ms: 50,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let Some(path) = config_path() else {
            return Config::default();
        };
        std::fs::read_to_string(path)
            .ok()
            .and_then(|contents| serde_json::from_str(&contents).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = config_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
}

/// `~/Library/Application Support/AutoCopy/config.json` on macOS. Uses the
/// `dirs` crate rather than hardcoding that path so the same code works
/// unmodified once Windows/Linux backends exist (`%APPDATA%` / `~/.config`
/// respectively).
fn config_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("AutoCopy").join("config.json"))
}
