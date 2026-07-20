//! Platform abstraction layer.
//!
//! AutoCopy needs three OS-level capabilities that don't exist in `std`:
//!
//!   1. Global mouse monitoring (even when this app isn't focused)
//!   2. Synthesizing the "copy" keyboard shortcut (⌘C) system-wide
//!   3. Requesting the OS permission that (1) and (2) require (Accessibility on macOS)
//!
//! Every OS exposes these very differently (CGEventTap vs. SetWindowsHookEx
//! vs. XRecord/evdev), so all OS-specific code lives behind the `Platform`
//! trait and its own per-OS module. `app.rs` only ever talks to `Platform` —
//! it has no idea which OS it's running on, and adding a new OS means
//! implementing this trait once, not touching the application layer.

use std::sync::mpsc::Sender;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::MacPlatform as CurrentPlatform;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::WindowsPlatform as CurrentPlatform;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::LinuxPlatform as CurrentPlatform;

/// A high-level input event, already translated from whatever raw OS event
/// produced it. `app.rs` reacts only to these — never to raw CGEvents, Win32
/// messages, X11 records, etc. There's only one variant today, but this
/// stays an enum (rather than a bare callback) so a future trigger doesn't
/// require reshaping the channel between `Platform` and `app.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    /// The left mouse button was released — see the doc comment on
    /// `Config::enabled` for why this alone is a sufficient signal.
    MouseUp,
}

/// Everything a platform backend must provide. Implement this once per OS
/// and the rest of the application works unmodified.
pub trait Platform {
    /// True if this process already holds the permission it needs to
    /// monitor global input and synthesize key events (Accessibility on
    /// macOS; likely always `true` on Windows; a udev/input-group check on
    /// Linux).
    fn has_permission(&self) -> bool;

    /// Prompts the user to grant that permission (e.g. opens System
    /// Settings on macOS). Does not block waiting for the grant.
    fn request_permission(&self);

    /// Starts listening for global input events and forwards them to `tx`.
    /// Must be called on the main thread. Runs for the lifetime of the
    /// process — on macOS this pumps the Cocoa run loop and never returns.
    fn start_monitoring(&self, tx: Sender<InputEvent>);

    /// Synthesizes the OS "copy" shortcut (⌘C on macOS) as if the user
    /// pressed it themselves. Safe to call from any thread.
    fn send_copy_shortcut(&self);
}
