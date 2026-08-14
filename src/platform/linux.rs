//! Linux backend — not yet implemented.
//!
//! Global input monitoring on Linux is split across two very different
//! stacks, unlike macOS/Windows:
//!
//!   - X11: the `XRecord` extension (what most global-hotkey tools use),
//!     via `x11rb` or raw `libX11` bindings — it covers both the mouse
//!     gestures and the Ctrl+A chord the select-all trigger needs.
//!   - Wayland: no cross-compositor equivalent exists by design — Wayland
//!     deliberately sandboxes input from other clients. The practical path
//!     is the `wlr-virtual-pointer`/`virtual-keyboard` protocols on
//!     compositors that support them, or falling back to raw `evdev` device
//!     access (`/dev/input/event*`), which requires the user to be in the
//!     `input` group. That group membership is the closest Linux analogue
//!     of macOS's Accessibility permission, and a natural fit for
//!     `has_permission`/`request_permission`.
//!   - Synthesizing Ctrl+C: `XTestFakeKeyEvent` on X11, or a virtual
//!     `uinput` device (`/dev/uinput`) as a compositor-agnostic fallback.
//!   - AT-SPI2 (the `org.a11y.atspi` D-Bus interfaces, `Text` in
//!     particular) to ask whether the focused element has a text selection
//!     — the equivalent of macOS's `AXSelectedText` check behind
//!     `has_text_selection`.
//!   - Tray icon: already handled for us, since `tray.rs` uses the
//!     `tray-icon` crate, which supports Linux via
//!     `libappindicator`/`StatusNotifierItem`.
//!
//! Because of the X11/Wayland split, this backend will likely need to detect
//! the session type at startup (`$XDG_SESSION_TYPE`) and choose an
//! implementation accordingly — but that detail stays entirely inside this
//! module; the `Platform` trait doesn't change.

use std::sync::mpsc::Sender;

use crate::platform::{InputEvent, Platform};

pub struct LinuxPlatform;

impl LinuxPlatform {
    pub fn new() -> Self {
        LinuxPlatform
    }
}

impl Platform for LinuxPlatform {
    fn has_permission(&self) -> bool {
        todo!("Linux backend not yet implemented — see module docs")
    }

    fn request_permission(&self) {
        todo!("Linux backend not yet implemented — see module docs")
    }

    fn start_monitoring(&self, _tx: Sender<InputEvent>) {
        todo!("Linux backend not yet implemented — see module docs")
    }

    fn send_copy_shortcut(&self) {
        todo!("Linux backend not yet implemented — see module docs")
    }

    fn has_text_selection(&self) -> Option<bool> {
        todo!("Linux backend not yet implemented — see module docs")
    }
}
