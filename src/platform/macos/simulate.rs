//! Synthesizing the ⌘C shortcut via `CGEventPost`.
//!
//! Posting a synthetic key event onto the HID event stream is
//! indistinguishable, to the receiving application, from the user physically
//! pressing the key — this is exactly the mechanism system-wide hotkey and
//! automation tools use. `CGEventPost` is safe to call from any thread; it
//! does not require the caller to own the main run loop.
//!
//! Indistinguishable to *other* applications, that is — AutoCopy itself must
//! be able to tell its own key events apart from the user's, because they
//! come right back through the keyboard event tap in `event_tap.rs`, where
//! an untagged ⌘C would register as "user activity" and cancel a pending
//! select-all copy. Hence every event posted here carries
//! [`SYNTHESIZED_EVENT_TAG`] in its user-data field, which the tap checks
//! first and discards.

use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode, EventField};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

/// Virtual keycode for the physical "C" key on ANSI (US QWERTY-family)
/// layouts. A different physical key may sit there on other layouts; see
/// the README's "Known limitations" section.
const KEYCODE_C: CGKeyCode = 0x08;

/// Marker written into `EventField::EVENT_SOURCE_USER_DATA` of every key
/// event AutoCopy posts, so its own event tap can recognize and ignore
/// them. The value is arbitrary but stable — "ACPY" in ASCII. (This is the
/// established pattern for taps that both observe and synthesize input;
/// the field is preserved end-to-end through the HID event stream.)
pub const SYNTHESIZED_EVENT_TAG: i64 = 0x4143_5059;

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
        event.set_integer_value_field(EventField::EVENT_SOURCE_USER_DATA, SYNTHESIZED_EVENT_TAG);
        event.post(CGEventTapLocation::HID);
    }
}
