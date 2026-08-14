//! Synthesizing the ⌘C shortcut via `CGEventPost`.
//!
//! Posting a synthetic key event onto the HID event stream is
//! indistinguishable, to the receiving application, from the user physically
//! pressing the key — this is exactly the mechanism system-wide hotkey and
//! automation tools use. `CGEventPost` is safe to call from any thread; it
//! does not require the caller to own the main run loop.
//!
//! These events also echo back through our own keyboard tap in
//! `event_tap.rs`, but that's harmless by construction: the only chord the
//! tap reacts to is ⌘A, and the only key ever synthesized here is C — the
//! echo can't re-trigger anything.

use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

/// Virtual keycode for the physical "C" key on ANSI (US QWERTY-family)
/// layouts. A different physical key may sit there on other layouts; see
/// the README's "Known limitations" section.
const KEYCODE_C: CGKeyCode = 0x08;

pub fn send_copy() {
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
        eprintln!("AutoCopy: failed to create an event source; skipping synthesized shortcut");
        return;
    };

    // A real key press is a down event followed by an up event; sending only
    // one leaves the modifier/key in a "stuck" state as far as some apps'
    // event tracking is concerned.
    for key_down in [true, false] {
        let Ok(event) = CGEvent::new_keyboard_event(source.clone(), KEYCODE_C, key_down) else {
            continue;
        };
        event.set_flags(CGEventFlags::CGEventFlagCommand);
        event.post(CGEventTapLocation::HID);
    }
}
