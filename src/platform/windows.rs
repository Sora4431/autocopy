//! Windows backend — not yet implemented.
//!
//! Sketch of what a real implementation would use:
//!
//!   - `SetWindowsHookExW(WH_MOUSE_LL, ...)` for global mouse monitoring —
//!     the Win32 equivalent of a `CGEventTap` — plus `WH_KEYBOARD_LL` to
//!     spot the Ctrl+A select-all chord.
//!   - `SendInput` to synthesize Ctrl+C.
//!   - UI Automation (`IUIAutomation` + the `TextPattern`) to ask whether
//!     the focused element has a text selection — the equivalent of
//!     macOS's `AXSelectedText` check behind `has_text_selection`.
//!   - No Accessibility-style permission gate exists for a process hooking
//!     its own session's input, so `has_permission` can simply return
//!     `true` and `request_permission` can be a no-op. (Some antivirus/EDR
//!     software flags low-level keyboard hooks; that's a code-signing and
//!     distribution concern, not a runtime permission to request.)
//!   - A system tray icon via `Shell_NotifyIconW` — already handled for us
//!     here, since `tray.rs` uses the `tray-icon` crate, which supports
//!     Windows out of the box.
//!
//! Implementing this only requires filling in `Platform` for
//! `WindowsPlatform` below; `app.rs` and `tray.rs` need no changes.

use std::sync::mpsc::Sender;

use crate::platform::{InputEvent, Platform};

pub struct WindowsPlatform;

impl WindowsPlatform {
    pub fn new() -> Self {
        WindowsPlatform
    }
}

impl Platform for WindowsPlatform {
    fn has_permission(&self) -> bool {
        todo!("Windows backend not yet implemented — see module docs")
    }

    fn request_permission(&self) {
        todo!("Windows backend not yet implemented — see module docs")
    }

    fn start_monitoring(&self, _tx: Sender<InputEvent>) {
        todo!("Windows backend not yet implemented — see module docs")
    }

    fn send_copy_shortcut(&self) {
        todo!("Windows backend not yet implemented — see module docs")
    }

    fn has_text_selection(&self) -> Option<bool> {
        todo!("Windows backend not yet implemented — see module docs")
    }
}
