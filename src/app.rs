//! Wires the platform backend, config, and tray menu together.
//!
//! Nothing in this file is OS-specific — swap `platform::CurrentPlatform` for
//! a Windows or Linux backend and this module doesn't change.

use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::config::Config;
use crate::platform::{CurrentPlatform, InputEvent, Platform};
use crate::tray;

pub fn run() -> ! {
    let config = Arc::new(Mutex::new(Config::load()));
    let platform = Arc::new(CurrentPlatform::new());

    if !platform.has_permission() {
        eprintln!(
            "AutoCopy needs Accessibility permission to detect text selection. \
             Opening System Settings — grant it, then quit and relaunch AutoCopy."
        );
        platform.request_permission();
    }

    let (tx, rx) = mpsc::channel::<InputEvent>();

    {
        let platform = Arc::clone(&platform);
        let config = Arc::clone(&config);
        thread::spawn(move || react_to_events(rx, platform, config));
    }

    // Must be built on the main thread, before the platform's run loop
    // starts below (macOS's Cocoa APIs assert this).
    let _tray = tray::build(Arc::clone(&config));

    // Blocks for the lifetime of the process.
    platform.start_monitoring(tx);
    unreachable!("Platform::start_monitoring must never return");
}

enum Action {
    Copy,
    Paste,
}

fn react_to_events(
    rx: Receiver<InputEvent>,
    platform: Arc<CurrentPlatform>,
    config: Arc<Mutex<Config>>,
) {
    for event in rx {
        let cfg = config.lock().unwrap().clone();
        let action = match event {
            InputEvent::MouseUp if cfg.copy_on_selection => Action::Copy,
            InputEvent::SelectAll if cfg.copy_on_select_all => Action::Copy,
            InputEvent::ModifierClick if cfg.paste_on_modifier_click => Action::Paste,
            _ => continue,
        };

        // Runs on its own short-lived thread so the delay doesn't block the
        // receiver loop — important for double/triple-clicks, which produce
        // several MouseUp events in quick succession and should each be
        // handled independently (the last one naturally "wins" the
        // clipboard, since it reflects the final selection).
        let platform = Arc::clone(&platform);
        let delay = Duration::from_millis(cfg.action_delay_ms);
        thread::spawn(move || {
            thread::sleep(delay);
            match action {
                Action::Copy => platform.send_copy_shortcut(),
                Action::Paste => platform.send_paste_shortcut(),
            }
        });
    }
}
