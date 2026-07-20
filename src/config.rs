//! User-facing settings, persisted as JSON.
//!
//! Each field is a toggle for one of AutoCopy's triggers, plus the shared
//! reaction delay. There is deliberately no other configuration surface —
//! no clipboard history, no per-app rules, nothing that would turn this into
//! a bigger tool than it claims to be. There is also no in-app settings
//! window; every toggle here is exposed directly as a checkbox in the tray
//! menu (see `tray.rs`), and this struct is just what gets written to disk.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Copy whenever the left mouse button is released. This alone covers
    /// drag selection, double-click word-select, and triple-click
    /// paragraph-select — a mouse-up is a mouse-up, and sending ⌘C when
    /// nothing happens to be selected is a harmless no-op, so there's no
    /// need to distinguish "was this actually a drag" from the event alone.
    pub copy_on_selection: bool,

    /// Copy after a plain ⌘A (Select All), with no other modifiers held.
    pub copy_on_select_all: bool,

    /// Paste at the click location when ⌥ (Option) is held during a click.
    /// Off by default: unlike the copy triggers, an unwanted paste can
    /// overwrite content the user didn't intend to touch, so this one is
    /// opt-in rather than opt-out.
    pub paste_on_modifier_click: bool,

    /// Delay, in milliseconds, between the triggering event and sending the
    /// synthesized shortcut. Gives the frontmost app a moment to finish
    /// updating its selection or cursor position before we act on it.
    pub action_delay_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            copy_on_selection: true,
            copy_on_select_all: true,
            paste_on_modifier_click: false,
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
