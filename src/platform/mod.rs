//! Platform abstraction layer.
//!
//! AutoCopy needs four OS-level capabilities that don't exist in `std`:
//!
//!   1. Global mouse and keyboard monitoring (even when this app isn't focused)
//!   2. Synthesizing the "copy" keyboard shortcut (⌘C) system-wide
//!   3. Asking the focused app whether any text is actually selected
//!   4. Requesting the OS permissions the above require (Accessibility and
//!      Input Monitoring on macOS)
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
/// messages, X11 records, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    /// The user finished a gesture that plausibly ended a text selection —
    /// a drag past a small threshold, or a double/triple-click. Deliberately
    /// a "maybe": the platform layer filters on gesture shape alone, and the
    /// authoritative "is text actually selected?" check
    /// (`Platform::has_text_selection`) runs later, right before the copy
    /// fires.
    PotentialSelection,

    /// The user pressed the platform's select-all shortcut (⌘A on macOS) as
    /// a bare chord — no extra modifiers, and not an autorepeat. Treated as
    /// "select all *and copy*": AutoCopy's whole premise is that the ⌘C
    /// after ⌘A is redundant, so this copies on the same short delay as a
    /// mouse selection. The known cost — ⌘A followed by a paste/typing
    /// *replaces* the clipboard with the selection first — is a deliberate
    /// trade for predictability; see the README's design notes.
    SelectAllPressed,
}

/// Everything a platform backend must provide. Implement this once per OS
/// and the rest of the application works unmodified.
pub trait Platform {
    /// True if this process already holds every permission it needs to
    /// monitor global input and synthesize key events (Accessibility *and*
    /// Input Monitoring on macOS; likely always `true` on Windows; a
    /// udev/input-group check on Linux).
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

    /// Best-effort answer to "does the focused UI element have text
    /// selected right now?".
    ///
    /// - `Some(true)`: the app reports selected text — copying will work.
    /// - `Some(false)`: the app affirmatively reports *no* selection.
    ///   Sending a copy shortcut now would do nothing except play the
    ///   system alert sound (apps beep when their Copy command has nothing
    ///   to act on), so the caller should skip it.
    /// - `None`: the platform can't tell (the app doesn't expose selection
    ///   state). The caller should copy anyway — wrongly skipping would
    ///   silently break AutoCopy in every such app, while wrongly copying
    ///   at worst beeps.
    fn has_text_selection(&self) -> Option<bool>;
}
