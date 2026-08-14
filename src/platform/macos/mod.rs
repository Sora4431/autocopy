//! macOS backend — the only platform implemented in the MVP.
//!
//! macOS is the reference implementation: every `Platform` method maps to a
//! well-defined macOS API (see the submodules for why each one is used).
//! Future Windows/Linux backends implement the same trait against their own
//! native APIs — see `platform/windows.rs` and `platform/linux.rs`.

mod event_tap;
mod permissions;
mod selection;
mod simulate;

use std::sync::mpsc::Sender;

use cocoa::appkit::{NSApp, NSApplication, NSApplicationActivationPolicyAccessory};

use crate::platform::{InputEvent, Platform};

pub struct MacPlatform;

impl MacPlatform {
    pub fn new() -> Self {
        MacPlatform
    }
}

impl Platform for MacPlatform {
    fn has_permission(&self) -> bool {
        // Two separate TCC gates: Accessibility covers the mouse tap, the
        // AX selection check, and posting ⌘C; Input Monitoring (10.15+)
        // covers the keyboard tap that watches for ⌘A. Missing either one
        // degrades part of the product, so "has permission" means both.
        permissions::has_permission() && permissions::has_input_monitoring()
    }

    fn request_permission(&self) {
        // Each call prompts at most once per app; macOS queues the two
        // dialogs if both are outstanding.
        permissions::request_permission();
        permissions::request_input_monitoring();
    }

    fn start_monitoring(&self, tx: Sender<InputEvent>) {
        event_tap::start(tx);

        unsafe {
            let app = NSApp();
            // "Accessory" apps have no Dock icon and no application menu —
            // exactly what a menu-bar-only utility wants. This is the
            // programmatic equivalent of `LSUIElement = true` in an
            // Info.plist, and it works even when running the bare binary
            // outside of an .app bundle (e.g. via `cargo run`).
            app.setActivationPolicy_(NSApplicationActivationPolicyAccessory);

            // `NSApplication.run()` starts the Cocoa main event loop. This is
            // what actually drives menu bar clicks, and it also pumps the
            // CFRunLoop that `event_tap::start` just registered its event
            // tap source on — they share the same main-thread run loop. This
            // call never returns for the life of the process; termination
            // happens via `std::process::exit` from the tray's Quit item.
            app.run();
        }
    }

    fn send_copy_shortcut(&self) {
        simulate::send_copy();
    }

    fn has_text_selection(&self) -> Option<bool> {
        selection::has_text_selection()
    }
}
