//! Menu bar icon and dropdown menu.
//!
//! This is the one piece of UI in the entire app: an icon, three checkboxes
//! (one per trigger, mirroring `Config`), and Quit. Built on `tray-icon`,
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

    let copy_on_selection = CheckMenuItem::new(
        "Copy on text selection",
        true,
        snapshot.copy_on_selection,
        None,
    );
    let copy_on_select_all = CheckMenuItem::new(
        "Copy on \u{2318}A (Select All)",
        true,
        snapshot.copy_on_select_all,
        None,
    );
    let paste_on_modifier_click = CheckMenuItem::new(
        "Paste on \u{2325}-click (Option+Click)",
        true,
        snapshot.paste_on_modifier_click,
        None,
    );
    let quit = MenuItem::new("Quit AutoCopy", true, None);

    let menu = Menu::new();
    menu.append_items(&[
        &copy_on_selection,
        &copy_on_select_all,
        &paste_on_modifier_click,
        &PredefinedMenuItem::separator(),
        &quit,
    ])
    .expect("failed to build tray menu");

    let quit_id = quit.id().clone();
    let copy_on_selection_id = copy_on_selection.id().clone();
    let copy_on_select_all_id = copy_on_select_all.id().clone();
    let paste_on_modifier_click_id = paste_on_modifier_click.id().clone();

    // `tray-icon`/`muda`'s menu items wrap platform-native handles (`Rc`
    // internally) and are therefore not `Send`/`Sync`, but
    // `set_event_handler` requires its closure to be both — so the closure
    // below intentionally captures only `MenuId`s (plain `String` wrappers)
    // and `config`, never a `CheckMenuItem` itself. The native checkbox
    // already flips its own visual state on click independent of this
    // handler; here we just mirror that into `Config` and persist it. Each
    // arm is deliberately explicit rather than table-driven — there are only
    // three items, and this keeps the id-to-field mapping obvious.
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == quit_id {
            std::process::exit(0);
        }

        let mut cfg = config.lock().unwrap();
        if event.id == copy_on_selection_id {
            cfg.copy_on_selection = !cfg.copy_on_selection;
        } else if event.id == copy_on_select_all_id {
            cfg.copy_on_select_all = !cfg.copy_on_select_all;
        } else if event.id == paste_on_modifier_click_id {
            cfg.paste_on_modifier_click = !cfg.paste_on_modifier_click;
        } else {
            return;
        }
        cfg.save();
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
