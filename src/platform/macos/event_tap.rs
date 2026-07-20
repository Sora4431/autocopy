//! Global input monitoring via a Quartz Event Tap.
//!
//! `CGEventTap` (Quartz Event Services) is macOS's only public API for
//! observing input events system-wide, regardless of which application has
//! focus. It's the same primitive behind Karabiner, Rectangle, and most
//! other input-remapping/window-management tools — which is also why it
//! requires Accessibility permission (see `permissions.rs`).
//!
//! We create the tap in `ListenOnly` mode: we only ever observe events,
//! never swallow or rewrite them. This is a deliberate safety choice — an
//! *active* tap that hangs (e.g. a panic inside the callback) can freeze all
//! keyboard and mouse input system-wide until the process is killed from
//! another machine. A listen-only tap can never do that; worst case, we
//! simply miss an event.
//!
//! Not every left-click means "a selection just ended", though. Clicking a
//! sidebar item, pressing a button, dragging a window — all of those end in
//! a `LeftMouseUp` too, and blindly firing ⌘C after them is not harmless:
//! when an app's Copy command has nothing to act on, it answers a
//! synthesized ⌘C with the system alert sound. So this module implements
//! the first, purely mechanical layer of filtering (gesture shape: did the
//! pointer travel, was it a multi-click?); the second layer — "is there
//! actually text selected?" — lives in `selection.rs` and runs right before
//! the copy fires.

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

/// Starts the event tap and registers it on the current (main) thread's run
/// loop. Does not block — the caller is expected to pump the run loop itself
/// afterwards (see `MacPlatform::start_monitoring`).
pub fn start(tx: Sender<InputEvent>) {
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

    let tap = match tap {
        Ok(tap) => tap,
        Err(_) => {
            eprintln!(
                "AutoCopy: failed to create the input event tap. This almost \
                 always means Accessibility permission hasn't been granted \
                 yet. Grant it in System Settings -> Privacy & Security -> \
                 Accessibility, then quit and relaunch AutoCopy."
            );
            return;
        }
    };

    unsafe {
        let loop_source = tap
            .mach_port
            .create_runloop_source(0)
            .expect("failed to create run loop source for event tap");
        CFRunLoop::get_current().add_source(&loop_source, kCFRunLoopCommonModes);
    }
    tap.enable();

    // The tap must outlive `start` (its run loop source keeps firing the
    // callback for the life of the process), so it's intentionally leaked
    // rather than dropped — dropping it here would remove the tap
    // immediately after this function returns.
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
