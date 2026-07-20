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

/// The tray icon: an apple silhouette, baked into the binary via
/// `include_bytes!` rather than read from disk at runtime — one less way
/// for the build to break depending on the current directory, and it keeps
/// the whole app a single self-contained executable. `assets/icon.png` is a
/// square RGBA PNG whose color channels are irrelevant; only alpha carries
/// the shape, since `with_icon_as_template` (see `build` above) tells macOS
/// to recolor it for light/dark menu bars and menu highlights itself.
fn app_icon() -> Icon {
    let (rgba, width, height) = decode_icon_png(include_bytes!("../assets/icon.png"));
    Icon::from_rgba(rgba, width, height).expect("failed to build tray icon bitmap")
}

fn decode_icon_png(bytes: &[u8]) -> (Vec<u8>, u32, u32) {
    let mut decoder = png::Decoder::new(bytes);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .expect("embedded tray icon PNG is invalid");

    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .expect("failed to decode embedded tray icon PNG");
    let rgba = to_rgba8(&buf, info.color_type, info.bit_depth);
    (rgba, info.width, info.height)
}

/// `assets/icon.png` is authored as 8-bit RGBA, but this normalizes any
/// other encoding (e.g. an RGB export with no alpha channel) so a future
/// icon swap doesn't silently corrupt the tray icon just because someone
/// exported it slightly differently.
fn to_rgba8(buf: &[u8], color_type: png::ColorType, bit_depth: png::BitDepth) -> Vec<u8> {
    assert_eq!(bit_depth, png::BitDepth::Eight, "expected an 8-bit PNG");
    match color_type {
        png::ColorType::Rgba => buf.to_vec(),
        png::ColorType::Rgb => buf
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&v| [v, v, v, 255]).collect(),
        png::ColorType::Indexed => panic!("indexed PNGs aren't supported — re-export as RGBA"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_icon_decodes() {
        let (rgba, width, height) = decode_icon_png(include_bytes!("../assets/icon.png"));
        assert_eq!(rgba.len(), (width * height * 4) as usize);
        assert!(width > 0 && height > 0);

        let opaque_pixels = rgba.chunks_exact(4).filter(|p| p[3] > 128).count();
        assert!(opaque_pixels > 0, "icon should not be fully transparent");
    }
}
