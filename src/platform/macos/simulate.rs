//! Synthesizing keyboard shortcuts via `CGEventPost`.
//!
//! Posting a synthetic key event onto the HID event stream is
//! indistinguishable, to the receiving application, from the user physically
//! pressing the key — this is exactly the mechanism system-wide hotkey and
//! automation tools use. `CGEventPost` is safe to call from any thread; it
//! does not require the caller to own the main run loop.

use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

/// Virtual keycode for the physical "C" key (ANSI layout). See the layout
/// note in `event_tap.rs`.
const KEYCODE_C: CGKeyCode = 0x08;
/// Virtual keycode for the physical "V" key (ANSI layout).
const KEYCODE_V: CGKeyCode = 0x09;

pub fn send_copy() {
    send_command_key(KEYCODE_C);
}

pub fn send_paste() {
    send_command_key(KEYCODE_V);
}

fn send_command_key(keycode: CGKeyCode) {
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
        eprintln!("AutoCopy: failed to create an event source; skipping synthesized shortcut");
        return;
    };

    // A real key press is a down event followed by an up event; sending only
    // one leaves the modifier/key in a "stuck" state as far as some apps'
    // event tracking is concerned.
    for key_down in [true, false] {
        let Ok(event) = CGEvent::new_keyboard_event(source.clone(), keycode, key_down) else {
            continue;
        };
        event.set_flags(CGEventFlags::CGEventFlagCommand);
        event.post(CGEventTapLocation::HID);
    }
}
