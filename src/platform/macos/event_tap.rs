//! Global input monitoring via Quartz Event Taps.
//!
//! `CGEventTap` (Quartz Event Services) is macOS's only public API for
//! observing input events system-wide, regardless of which application has
//! focus. It's the same primitive behind Karabiner, Rectangle, and most
//! other input-remapping/window-management tools — which is also why it
//! requires user-granted permissions (see `permissions.rs`).
//!
//! We create every tap in `ListenOnly` mode: we only ever observe events,
//! never swallow or rewrite them. This is a deliberate safety choice — an
//! *active* tap that hangs (e.g. a panic inside the callback) can freeze all
//! keyboard and mouse input system-wide until the process is killed from
//! another machine. A listen-only tap can never do that; worst case, we
//! simply miss an event.
//!
//! There are **two** taps, not one, because macOS gates them differently:
//! mouse taps need only the Accessibility permission, but since macOS 10.15
//! a listen-only tap that includes *keyboard* events additionally requires
//! the separate Input Monitoring permission. Keeping them separate means a
//! missing Input Monitoring grant degrades gracefully — ⌘A detection goes
//! dark (with a clear message on stderr), while mouse-selection copying
//! keeps working exactly as before.
//!
//! What each tap watches, and why:
//!
//! - **Mouse tap.** Not every left-click means "a selection just ended".
//!   Clicking a sidebar item, pressing a button, dragging a window — all of
//!   those end in a `LeftMouseUp` too, and blindly firing ⌘C after them is
//!   not harmless: when an app's Copy command has nothing to act on, it
//!   answers a synthesized ⌘C with the system alert sound. So this module
//!   implements the first, purely mechanical layer of filtering (gesture
//!   shape: did the pointer travel, was it a multi-click?); the second
//!   layer — "is there actually text selected?" — lives in `selection.rs`
//!   and runs right before the copy fires.
//!
//! - **Keyboard tap.** Watches key-downs for exactly one chord — bare ⌘A,
//!   not an autorepeat — and emits an event only when it sees it; every
//!   other keystroke is dropped on the floor inside the tap callback,
//!   never forwarded, stored, or logged. (Our own synthesized ⌘C also
//!   passes through here, but it can't trigger anything: the only chord
//!   this tap reacts to is on a different key.)

use std::cell::Cell;
use std::sync::mpsc::Sender;

use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType, EventField,
};

use crate::platform::InputEvent;

/// Minimum distance (in screen points) the pointer must travel between
/// mouse-down and mouse-up for a single click to count as a drag-selection.
/// Below this it's treated as a plain click and ignored. Small enough that
/// any real drag-select clears it, large enough that the incidental wobble
/// of a normal (especially trackpad) click doesn't.
const DRAG_THRESHOLD: f64 = 4.0;

/// Virtual keycode for the physical "A" key on ANSI (US QWERTY-family)
/// layouts — same caveat as `simulate::KEYCODE_C`: a different physical key
/// may sit there on other layouts (e.g. AZERTY); see the README's "Known
/// limitations" section.
const KEYCODE_A: i64 = 0x00;

/// Starts both event taps and registers them on the current (main) thread's
/// run loop. Does not block — the caller is expected to pump the run loop
/// itself afterwards (see `MacPlatform::start_monitoring`).
pub fn start(tx: Sender<InputEvent>) {
    start_mouse_tap(tx.clone());
    start_keyboard_tap(tx);
}

fn start_mouse_tap(tx: Sender<InputEvent>) {
    // Where the current click started; written on mouse-down, read on the
    // matching mouse-up. The callback only ever runs on this thread's run
    // loop, so a plain `Cell` (no locking) is enough.
    let press_origin = Cell::new((0.0_f64, 0.0_f64));

    let tap = CGEventTap::new(
        CGEventTapLocation::HID,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::ListenOnly,
        vec![CGEventType::LeftMouseDown, CGEventType::LeftMouseUp],
        move |_proxy, event_type, event: &CGEvent| {
            match event_type {
                CGEventType::LeftMouseDown => {
                    let p = event.location();
                    press_origin.set((p.x, p.y));
                }
                CGEventType::LeftMouseUp => {
                    if is_selection_shaped(event, press_origin.get()) {
                        let _ = tx.send(InputEvent::PotentialSelection);
                    }
                }
                _ => {}
            }
            None
        },
    );

    match tap {
        Ok(tap) => install(tap),
        Err(_) => eprintln!(
            "AutoCopy: failed to create the mouse event tap. This almost \
             always means Accessibility permission hasn't been granted \
             yet. Grant it in System Settings -> Privacy & Security -> \
             Accessibility, then quit and relaunch AutoCopy."
        ),
    }
}

fn start_keyboard_tap(tx: Sender<InputEvent>) {
    let tap = CGEventTap::new(
        CGEventTapLocation::HID,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::ListenOnly,
        vec![CGEventType::KeyDown],
        move |_proxy, event_type, event: &CGEvent| {
            match event_type {
                // Only the initial press qualifies — a held-down ⌘A
                // autorepeats, and firing a copy per repeat would hammer
                // the frontmost app with ⌘C for no new information.
                CGEventType::KeyDown if !is_autorepeat(event) && is_select_all_chord(event) => {
                    let _ = tx.send(InputEvent::SelectAllPressed);
                }
                _ => {}
            }
            None
        },
    );

    match tap {
        Ok(tap) => install(tap),
        Err(_) => eprintln!(
            "AutoCopy: failed to create the keyboard event tap, so the \
             select-all (Cmd+A) trigger is disabled for this run. \
             Text-selection copying still works. On macOS 10.15+ this tap \
             needs the Input Monitoring permission: System Settings -> \
             Privacy & Security -> Input Monitoring, then quit and relaunch \
             AutoCopy."
        ),
    }
}

/// Registers a tap on the current thread's run loop and enables it. The tap
/// must outlive this call (its run loop source keeps firing the callback for
/// the life of the process), so it's intentionally leaked rather than
/// dropped — dropping it here would remove the tap immediately after this
/// function returns.
fn install(tap: CGEventTap<'static>) {
    unsafe {
        let loop_source = tap
            .mach_port
            .create_runloop_source(0)
            .expect("failed to create run loop source for event tap");
        CFRunLoop::get_current().add_source(&loop_source, kCFRunLoopCommonModes);
    }
    tap.enable();
    std::mem::forget(tap);
}

/// The gesture-shape filter: does this mouse-up plausibly end a text
/// selection?
fn is_selection_shaped(event: &CGEvent, (origin_x, origin_y): (f64, f64)) -> bool {
    // Control+click is macOS's mouse-only stand-in for a secondary (right)
    // click — physically still a left button release, so the tap sees it as
    // `LeftMouseUp`. It opens a context menu rather than ending a selection,
    // and firing a synthesized ⌘C into that menu's tracking loop gets
    // interpreted as a menu command, dismissing the menu.
    if event.get_flags().contains(CGEventFlags::CGEventFlagControl) {
        return false;
    }

    // Double- and triple-clicks are the word/paragraph selection gestures.
    let click_count = event.get_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE);
    if click_count >= 2 {
        return true;
    }

    // A single click only qualifies if the pointer actually travelled —
    // i.e. a drag-selection. Plain clicks (sidebar items, buttons, focusing
    // a window, ...) stay below the threshold and are ignored, which is
    // what keeps AutoCopy from spamming ⌘C at apps with nothing to copy.
    let p = event.location();
    let (dx, dy) = (p.x - origin_x, p.y - origin_y);
    (dx * dx + dy * dy).sqrt() >= DRAG_THRESHOLD
}

/// True while a key is being held down and the system is generating repeat
/// events.
fn is_autorepeat(event: &CGEvent) -> bool {
    event.get_integer_value_field(EventField::KEYBOARD_EVENT_AUTOREPEAT) != 0
}

/// The select-all chord filter: bare ⌘A and nothing else.
fn is_select_all_chord(event: &CGEvent) -> bool {
    if event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) != KEYCODE_A {
        return false;
    }
    let flags = event.get_flags();
    // ⇧⌘A, ⌃⌘A, and ⌥⌘A are entirely different shortcuts — "deselect all"
    // among them — so any extra modifier disqualifies the chord. Caps Lock
    // and Fn are deliberately not checked: neither changes what ⌘A means.
    flags.contains(CGEventFlags::CGEventFlagCommand)
        && !flags.intersects(
            CGEventFlags::CGEventFlagControl
                | CGEventFlags::CGEventFlagAlternate
                | CGEventFlags::CGEventFlagShift,
        )
}
