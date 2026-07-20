//! Menu bar icon and dropdown menu.
//!
//! This is the one piece of UI in the entire app: an icon, an "Enabled"
//! checkbox mirroring `Config::enabled`, and Quit. Built on `tray-icon`,
//! which already abstracts over NSStatusItem (macOS) / Shell_NotifyIcon
//! (Windows) / StatusNotifierItem (Linux) — unlike input monitoring, there's
//! no reason to hide this behind our own `Platform` trait, since the crate
//! is already cross-platform.

use std::sync::{Arc, Mutex};

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::config::Config;

pub fn build(config: Arc<Mutex<Config>>) -> TrayIcon {
    let snapshot = config.lock().unwrap().clone();

    let enabled = CheckMenuItem::new("Enabled", true, snapshot.enabled, None);
    let quit = MenuItem::new("Quit AutoCopy", true, None);

    let menu = Menu::new();
    menu.append_items(&[&enabled, &PredefinedMenuItem::separator(), &quit])
        .expect("failed to build tray menu");

    let quit_id = quit.id().clone();
    let enabled_id = enabled.id().clone();

    // `tray-icon`/`muda`'s menu items wrap platform-native handles (`Rc`
    // internally) and are therefore not `Send`/`Sync`, but
    // `set_event_handler` requires its closure to be both — so the closure
    // below intentionally captures only `MenuId`s (plain `String` wrappers)
    // and `config`, never a `CheckMenuItem` itself. The native checkbox
    // already flips its own visual state on click independent of this
    // handler; here we just mirror that into `Config` and persist it.
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == quit_id {
            std::process::exit(0);
        } else if event.id == enabled_id {
            let mut cfg = config.lock().unwrap();
            cfg.enabled = !cfg.enabled;
            cfg.save();
        }
    }));

    TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("AutoCopy")
        .with_icon(app_icon())
        .with_icon_as_template(true)
        .build()
        .expect("failed to create tray icon")
}

/// A small filled dot, generated at compile time from math rather than
/// bundled as an image asset — one less file, and one less dependency
/// (no PNG/image-decoding crate needed for a single-color glyph). Marked as
/// a "template" image via `with_icon_as_template`, so macOS recolors it
/// automatically for light/dark menu bars.
fn app_icon() -> Icon {
    const SIZE: u32 = 22;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    let center = SIZE as f32 / 2.0;
    let radius = SIZE as f32 / 2.0 - 2.0;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            if (dx * dx + dy * dy).sqrt() <= radius {
                let idx = ((y * SIZE + x) * 4) as usize;
                rgba[idx + 3] = 255; // alpha only — template images are monochrome
            }
        }
    }
    Icon::from_rgba(rgba, SIZE, SIZE).expect("failed to build tray icon bitmap")
}
