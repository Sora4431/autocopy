//! Wires the platform backend, config, and tray menu together.
//!
//! Nothing in this file is OS-specific — swap `platform::CurrentPlatform` for
//! a Windows or Linux backend and this module doesn't change. This is also
//! where the select-all arm/cancel state machine lives (rather than in the
//! platform layer), precisely because it has no OS in it: it's pure timing
//! logic over `InputEvent`s, shared by every backend and unit-testable with
//! a mock platform.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::config::Config;
use crate::platform::{CurrentPlatform, InputEvent, Platform};
use crate::tray;

pub fn run() -> ! {
    let config = Arc::new(Mutex::new(Config::load()));
    let platform = Arc::new(CurrentPlatform::new());

    if !platform.has_permission() {
        eprintln!(
            "AutoCopy needs the Accessibility permission (to detect text \
             selection and send the copy shortcut) and, on macOS 10.15+, \
             the Input Monitoring permission (to notice the select-all \
             shortcut). Opening System Settings — grant both, then quit \
             and relaunch AutoCopy."
        );
        platform.request_permission();
    }

    let (tx, rx) = mpsc::channel::<InputEvent>();

    {
        let platform = Arc::clone(&platform);
        let config = Arc::clone(&config);
        thread::spawn(move || react_to_events(rx, platform, config));
    }

    // Must be built on the main thread, before the platform's run loop
    // starts below (macOS's Cocoa APIs assert this).
    let _tray = tray::build(Arc::clone(&config));

    // Blocks for the lifetime of the process.
    platform.start_monitoring(tx);
    unreachable!("Platform::start_monitoring must never return");
}

/// The reaction loop, and the select-all arm/cancel state machine.
///
/// The `generation` counter is the whole mechanism: every incoming event
/// bumps it, and arming a select-all copy captures the post-bump value.
/// When the armed timer wakes up it fires only if the counter is *still*
/// that value — i.e. the quiet window passed with no other input. Any
/// keypress or mouse press in between bumped the counter, and the timer
/// silently stands down.
///
/// Why err toward cancelling: the two failure modes are not symmetric. A
/// wrongly *skipped* copy costs the user one manual ⌘C. A wrongly *fired*
/// copy after ⌘A overwrites the clipboard right before a paste — invisible
/// data loss, the worst thing a clipboard utility can do. So the rule is
/// blunt: any input during the quiet window cancels, no exceptions.
///
/// A plain atomic (no lock) is sound here because events are processed
/// strictly in arrival order on this one thread; timer threads only ever
/// read and compare.
fn react_to_events<P>(rx: Receiver<InputEvent>, platform: Arc<P>, config: Arc<Mutex<Config>>)
where
    P: Platform + Send + Sync + 'static,
{
    let generation = Arc::new(AtomicU64::new(0));

    for event in rx {
        match event {
            // Not select-all-related in itself — its arrival just means the
            // user did something else, which is exactly what disarms a
            // pending select-all copy.
            InputEvent::OtherActivity => {
                generation.fetch_add(1, Ordering::SeqCst);
            }

            InputEvent::PotentialSelection => {
                // A new mouse selection supersedes any armed select-all
                // copy (the mouse-down that started it already cancelled,
                // but re-bumping here keeps that true even if a backend
                // doesn't report mouse-downs).
                generation.fetch_add(1, Ordering::SeqCst);

                let cfg = config.lock().unwrap().clone();
                if !cfg.enabled {
                    continue;
                }

                // Runs on its own short-lived thread so the delay doesn't
                // block the receiver loop — important for double/triple-
                // clicks, which produce several events in quick succession
                // and should each be handled independently (the last one
                // naturally "wins" the clipboard, since it reflects the
                // final selection).
                let platform = Arc::clone(&platform);
                let delay = Duration::from_millis(cfg.action_delay_ms);
                thread::spawn(move || {
                    thread::sleep(delay);
                    // `Some(false)` is the app affirmatively saying "nothing
                    // is selected" — copying then would only trigger the
                    // system alert sound. `None` (app doesn't expose
                    // selection state) copies anyway; see
                    // `Platform::has_text_selection` for the rationale.
                    if platform.has_text_selection() == Some(false) {
                        return;
                    }
                    platform.send_copy_shortcut();
                });
            }

            InputEvent::SelectAllPressed => {
                // Arm. This event's own bump is the value the timer must
                // still observe after the quiet window; a second ⌘A simply
                // re-arms (the older timer sees a newer value and yields).
                let armed = generation.fetch_add(1, Ordering::SeqCst) + 1;

                let cfg = config.lock().unwrap().clone();
                if !cfg.enabled {
                    continue;
                }

                let platform = Arc::clone(&platform);
                let generation = Arc::clone(&generation);
                let delay = Duration::from_millis(cfg.select_all_delay_ms);
                thread::spawn(move || {
                    thread::sleep(delay);
                    if generation.load(Ordering::SeqCst) != armed {
                        return;
                    }
                    // Same three-state check as the mouse path. After a
                    // real select-all there is essentially always a
                    // selection; this catches ⌘A chords the frontmost app
                    // ignored (nothing to select → nothing to copy).
                    if platform.has_text_selection() == Some(false) {
                        return;
                    }
                    // The Accessibility round-trip above can take tens of
                    // milliseconds in a slow app — long enough for a ⌘V to
                    // sneak in. Re-check so that window stays closed.
                    if generation.load(Ordering::SeqCst) != armed {
                        return;
                    }
                    platform.send_copy_shortcut();
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::mpsc::Sender;

    struct MockPlatform {
        copies: AtomicUsize,
        selection: Option<bool>,
    }

    impl Platform for MockPlatform {
        fn has_permission(&self) -> bool {
            true
        }
        fn request_permission(&self) {}
        fn start_monitoring(&self, _tx: Sender<InputEvent>) {}
        fn send_copy_shortcut(&self) {
            self.copies.fetch_add(1, Ordering::SeqCst);
        }
        fn has_text_selection(&self) -> Option<bool> {
            self.selection
        }
    }

    /// Quiet window used by the tests. Big enough that the "cancel arrives
    /// in time" tests survive a scheduler hiccup on a busy CI runner; small
    /// enough to keep the suite fast.
    const QUIET_MS: u64 = 200;
    /// How long tests wait before asserting "the copy fired" / "it never
    /// will": comfortably past the quiet window plus thread-spawn slack.
    const SETTLE: Duration = Duration::from_millis(700);
    /// Gap between "arm" and the event that should beat the quiet window.
    const SOON: Duration = Duration::from_millis(50);

    fn test_config(enabled: bool) -> Config {
        Config {
            enabled,
            action_delay_ms: 0,
            select_all_delay_ms: QUIET_MS,
        }
    }

    fn harness(cfg: Config, selection: Option<bool>) -> (Sender<InputEvent>, Arc<MockPlatform>) {
        let platform = Arc::new(MockPlatform {
            copies: AtomicUsize::new(0),
            selection,
        });
        let config = Arc::new(Mutex::new(cfg));
        let (tx, rx) = mpsc::channel();
        {
            let platform = Arc::clone(&platform);
            thread::spawn(move || react_to_events(rx, platform, config));
        }
        (tx, platform)
    }

    fn copies(platform: &MockPlatform) -> usize {
        platform.copies.load(Ordering::SeqCst)
    }

    #[test]
    fn select_all_copies_after_quiet_window() {
        let (tx, platform) = harness(test_config(true), Some(true));
        tx.send(InputEvent::SelectAllPressed).unwrap();
        thread::sleep(SETTLE);
        assert_eq!(copies(&platform), 1);
    }

    #[test]
    fn any_activity_in_the_window_cancels_the_copy() {
        let (tx, platform) = harness(test_config(true), Some(true));
        tx.send(InputEvent::SelectAllPressed).unwrap();
        thread::sleep(SOON);
        // The ⌘V / typing / arrow-key / mouse-press case: whatever it was,
        // it reaches this loop as `OtherActivity` and must disarm.
        tx.send(InputEvent::OtherActivity).unwrap();
        thread::sleep(SETTLE);
        assert_eq!(copies(&platform), 0);
    }

    #[test]
    fn repeated_select_all_rearms_instead_of_double_copying() {
        let (tx, platform) = harness(test_config(true), Some(true));
        tx.send(InputEvent::SelectAllPressed).unwrap();
        thread::sleep(SOON);
        tx.send(InputEvent::SelectAllPressed).unwrap();
        thread::sleep(SETTLE);
        assert_eq!(copies(&platform), 1);
    }

    #[test]
    fn mouse_selection_supersedes_an_armed_select_all() {
        let (tx, platform) = harness(test_config(true), Some(true));
        tx.send(InputEvent::SelectAllPressed).unwrap();
        thread::sleep(SOON);
        // The drag ends; the mouse path copies (action_delay_ms = 0) and
        // the armed select-all timer must stand down — exactly one copy.
        tx.send(InputEvent::PotentialSelection).unwrap();
        thread::sleep(SETTLE);
        assert_eq!(copies(&platform), 1);
    }

    #[test]
    fn disabled_config_copies_nothing() {
        let (tx, platform) = harness(test_config(false), Some(true));
        tx.send(InputEvent::SelectAllPressed).unwrap();
        tx.send(InputEvent::PotentialSelection).unwrap();
        thread::sleep(SETTLE);
        assert_eq!(copies(&platform), 0);
    }

    #[test]
    fn affirmatively_empty_selection_skips_the_copy() {
        let (tx, platform) = harness(test_config(true), Some(false));
        tx.send(InputEvent::SelectAllPressed).unwrap();
        thread::sleep(SETTLE);
        assert_eq!(copies(&platform), 0);
    }

    #[test]
    fn unknown_selection_state_copies_anyway() {
        let (tx, platform) = harness(test_config(true), None);
        tx.send(InputEvent::SelectAllPressed).unwrap();
        thread::sleep(SETTLE);
        assert_eq!(copies(&platform), 1);
    }
}
