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

use std::sync::mpsc::Sender;

use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_graphics::event::{
    CGEvent, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
};

use crate::platform::InputEvent;

/// Starts the event tap and registers it on the current (main) thread's run
/// loop. Does not block — the caller is expected to pump the run loop itself
/// afterwards (see `MacPlatform::start_monitoring`).
pub fn start(tx: Sender<InputEvent>) {
    let tap = CGEventTap::new(
        CGEventTapLocation::HID,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::ListenOnly,
        vec![CGEventType::LeftMouseUp],
        move |_proxy, _event_type, _event: &CGEvent| {
            // The only thing we watch for is a left mouse button release —
            // see `Config::enabled`'s doc comment for why that alone is a
            // sufficient signal for "the user may have just selected text".
            let _ = tx.send(InputEvent::MouseUp);
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
