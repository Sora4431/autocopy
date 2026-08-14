//! Wires the platform backend, config, and tray menu together.
//!
//! Nothing in this file is OS-specific — swap `platform::CurrentPlatform` for
//! a Windows or Linux backend and this module doesn't change. The reaction
//! logic lives here (rather than in the platform layer) because it has no OS
//! in it: it's pure timing logic over `InputEvent`s, shared by every backend
//! and unit-testable with a mock platform.

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

fn react_to_events<P>(rx: Receiver<InputEvent>, platform: Arc<P>, config: Arc<Mutex<Config>>)
where
    P: Platform + Send + Sync + 'static,
{
    for event in rx {
        // Both triggers mean the same thing — "the user just selected
        // something, so copy it" — and get the same treatment. ⌘A copies
        // immediately (well, after the same settle delay) on purpose:
        // predictable always-copies behavior beats a heuristic that
        // second-guesses the user. See the README for the trade-off this
        // accepts.
        let (InputEvent::PotentialSelection | InputEvent::SelectAllPressed) = event;

        let cfg = config.lock().unwrap().clone();
        if !cfg.enabled {
            continue;
        }

        // Runs on its own short-lived thread so the delay doesn't block the
        // receiver loop — important for double/triple-clicks, which produce
        // several events in quick succession and should each be handled
        // independently (the last one naturally "wins" the clipboard, since
        // it reflects the final selection).
        let platform = Arc::clone(&platform);
        let delay = Duration::from_millis(cfg.action_delay_ms);
        thread::spawn(move || {
            thread::sleep(delay);
            // `Some(false)` is the app affirmatively saying "nothing is
            // selected" — copying then would only trigger the system alert
            // sound. That also covers ⌘A chords the frontmost app ignored
            // (nothing got selected → nothing to copy). `None` (app doesn't
            // expose selection state) copies anyway; see
            // `Platform::has_text_selection` for the rationale.
            if platform.has_text_selection() == Some(false) {
                return;
            }
            platform.send_copy_shortcut();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
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

    /// How long tests wait before asserting "the copy fired" / "it never
    /// will" — generous enough to survive a scheduler hiccup on a busy CI
    /// runner (the reaction delay itself is 0 in tests).
    const SETTLE: Duration = Duration::from_millis(400);

    fn test_config(enabled: bool) -> Config {
        Config {
            enabled,
            action_delay_ms: 0,
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
    fn select_all_copies() {
        let (tx, platform) = harness(test_config(true), Some(true));
        tx.send(InputEvent::SelectAllPressed).unwrap();
        thread::sleep(SETTLE);
        assert_eq!(copies(&platform), 1);
    }

    #[test]
    fn mouse_selection_copies() {
        let (tx, platform) = harness(test_config(true), Some(true));
        tx.send(InputEvent::PotentialSelection).unwrap();
        thread::sleep(SETTLE);
        assert_eq!(copies(&platform), 1);
    }

    #[test]
    fn triggers_are_independent() {
        // A ⌘A right after a mouse selection (or vice versa) is two real
        // selections and two copies — the later one wins the clipboard.
        let (tx, platform) = harness(test_config(true), Some(true));
        tx.send(InputEvent::PotentialSelection).unwrap();
        tx.send(InputEvent::SelectAllPressed).unwrap();
        thread::sleep(SETTLE);
        assert_eq!(copies(&platform), 2);
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
