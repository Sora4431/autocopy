# AutoCopy

AutoCopy is a tiny macOS menu bar utility with exactly one job: when you
select text anywhere on your Mac — by mouse, or with ⌘A — it automatically
copies it to the clipboard, the equivalent of pressing ⌘C for you.

It is **not** a clipboard manager. There's no history, no cloud sync, no AI,
no analytics, no telemetry of any kind. It watches for a selection and sends
one keystroke. That's the whole product.

## Features

- Lives in the menu bar only — no Dock icon, no windows
- Auto-copies when you finish a text selection (drag, double- or triple-click)
- Auto-copies after ⌘A (select all) — with AutoCopy running, ⌘A simply
  *means* "select all and copy"
- Skips plain clicks, Control-clicks, and gestures that didn't actually
  select anything — no beeps, no clipboard churn
- One on/off checkbox in the tray menu, plus Quit
- Configurable reaction delay (default 50ms)
- Zero network access, zero telemetry, zero data collection

## Non-goals

- Clipboard history or multiple clipboards
- Auto-paste, or any other action besides copying
- OCR, AI, or content transformation
- Cloud sync of any kind
- Keyboard shortcuts to trigger actions manually
- Any settings beyond the two in [`Config`](src/config.rs)

## Why does it need Accessibility and Input Monitoring permissions?

Detecting "the user just selected text, anywhere, in any app" and then
simulating a keypress are privileged operations on macOS. They go through
the same API surface — Quartz Event Services — that keyboard remapping and
window-management tools use, and Apple gates it behind two permissions in
System Settings → Privacy & Security:

- **Accessibility** covers the mouse event tap, the "is text actually
  selected?" query, and posting the synthesized ⌘C. There's no narrower
  permission available; observing global mouse input and observing *only
  selection-related* input aren't different capabilities as far as macOS is
  concerned.
- **Input Monitoring** (macOS 10.15+) additionally covers the keyboard
  event tap that notices ⌘A. If you grant only Accessibility, AutoCopy
  still runs — mouse-selection copying works, and only the ⌘A trigger stays
  off (it says so on stderr).

AutoCopy asks for both once, on first launch, via the standard system
prompts. If you skip one, add AutoCopy manually in the corresponding
settings pane, then quit and relaunch.

**What AutoCopy does with that access:** it opens two *listen-only* event
taps (see below). The mouse tap watches left mouse button presses and
releases — nothing else. The keyboard tap sees key-down events and checks
each one, inside the tap callback, for exactly one thing: was it a bare ⌘A?
Every other keystroke is dropped on the spot — never forwarded, stored, or
logged. After a trigger, AutoCopy also asks the focused app (through the
same Accessibility API, via `AXSelectedText`) whether any text is actually
selected; the answer is checked for non-emptiness and immediately
discarded. Nothing is ever sent over the network (the app has no network
code at all).

## Architecture

```
assets/
    icon.png          tray icon artwork (RGBA PNG, alpha = shape)
src/
    main.rs         entry point
    app.rs           wires config + platform + tray together (no OS-specific code)
    config.rs        on-disk settings (JSON), no OS-specific code
    tray.rs           menu bar UI, via the cross-platform `tray-icon` crate
    platform/
        mod.rs        the `Platform` trait + `InputEvent` enum
        macos/         macOS implementation (only backend in the MVP)
            mod.rs      ties the pieces together, owns the Cocoa run loop
            event_tap.rs  global mouse + keyboard monitoring, gesture/chord filters (CGEventTap ×2)
            selection.rs  "is text actually selected?" check (AXSelectedText)
            simulate.rs   synthesizing ⌘C (CGEventPost)
            permissions.rs  Accessibility + Input Monitoring permission check/request
        windows.rs      stub — documents what a real backend would use
        linux.rs        stub — documents what a real backend would use
```

`assets/icon.png` is baked into the binary at compile time (`include_bytes!`
in `tray.rs`), so the built executable stays a single self-contained file —
no separate icon file to lose track of at runtime. It's rendered as a
"template" image, so only its alpha channel matters; macOS recolors it
automatically for light/dark menu bars.

The application layer (`app.rs`, `config.rs`, `tray.rs`) never imports
anything OS-specific. It only knows about the `Platform` trait:

```rust
pub trait Platform {
    fn has_permission(&self) -> bool;
    fn request_permission(&self);
    fn start_monitoring(&self, tx: Sender<InputEvent>);
    fn send_copy_shortcut(&self);
    fn has_text_selection(&self) -> Option<bool>;
}
```

`platform::CurrentPlatform` resolves to `MacPlatform`, `WindowsPlatform`, or
`LinuxPlatform` at compile time via `#[cfg(target_os = ...)]` — there's no
runtime dispatch, and no OS-specific type ever leaks past `platform/mod.rs`.

The tray menu (`tray.rs`) is the one exception to "everything OS-specific is
behind `Platform`" — it's built on the [`tray-icon`](https://docs.rs/tray-icon)
crate, which is already cross-platform (macOS/Windows/Linux), so there's
nothing left for our own trait to abstract there.

## Event flow

```
 mouse tap (CGEventTap)    event_tap.rs                              app.rs
 ───────────────────       ─────────────                             ─────────────────
 left mouse down    ───▶  remember where the click started
 left mouse up      ───▶  gesture filter:
                            Control held?              → ignore (right-click stand-in)
                            double/triple click?       → selection gesture
                            single click, moved ≥ 4pt? → drag selection
                            otherwise                  → ignore (plain click)
                                    │
                                    ▼
                          InputEvent::PotentialSelection ──┐
                                                           │
 keyboard tap (CGEventTap)                                 │
 ───────────────────                                       │
 key down           ───▶  chord filter:                    │
                            autorepeat?           → ignore │
                            exactly ⌘A (no ⇧⌃⌥)?  → InputEvent::SelectAllPressed
                            anything else         → dropped inside the callback
                                                           │
                                              both paths, identically:
                                                           │  if cfg.enabled
                                                           ▼
                                                 sleep(action_delay_ms)
                                                           │
                                       focused app says "no text selected"? → do nothing
                                                           ▼
                                              send ⌘C (send_copy_shortcut)
```

Four design decisions worth calling out:

- **Why not just copy on every mouse-up?** The first prototype did exactly
  that, on the theory that ⌘C with nothing selected is a harmless no-op.
  It isn't: when an app's Copy command has nothing to act on, a synthesized
  ⌘C triggers the system alert sound — so every plain click (a Finder
  sidebar item, a button, focusing a window) beeped. Hence the two-layer
  filter: first the *shape* of the gesture (did the pointer travel, or was
  it a multi-click?), then the *substance* (does the focused app report
  actual selected text?).

- **Why is the selection check only "best effort"?** Not every app
  implements the part of the Accessibility protocol that exposes selected
  text (`AXSelectedText`). The check is therefore three-state: confirmed
  selection → copy; confirmed *empty* → skip; unknown → copy anyway,
  because silently skipping would break AutoCopy entirely in apps that
  simply don't report selection state, while a spurious copy at worst
  beeps.

- **Why a delay before acting?** The frontmost app needs a moment after the
  mouse-up to actually update its internal selection state before a
  synthesized ⌘C would pick up the right thing. 50ms is imperceptible to a
  human but enough of a buffer in practice; it's configurable via
  `Config::action_delay_ms` if a particular app needs more.

- **Why does ⌘A copy immediately, with no grace period?** An earlier
  design waited a few hundred milliseconds after ⌘A and cancelled the copy
  if any other input arrived, to protect the select-all-then-paste-over
  flow (⌘A, then ⌘V or typing to replace everything) from having its
  clipboard clobbered. It was dropped on purpose: it made the trigger
  unpredictable — whether a fast ⌘A → ⌘Tab → ⌘V found anything on the
  clipboard depended on typing speed — and the failure it guarded against
  is smaller than it looks. If ⌘A auto-copies and you then paste over,
  you paste the selection back onto itself: the text is unchanged (and
  ⌘Z-recoverable in any case); all you've lost is the old clipboard
  content, which a re-copy restores. Someone running AutoCopy knows ⌘A
  now means "select all *and copy*" — a simple rule that always holds
  beats a clever one that sometimes doesn't. Autorepeat is still ignored
  (holding ⌘A fires one copy, not fifteen a second), and extra modifiers
  disqualify the chord, since ⇧⌘A / ⌃⌘A / ⌥⌘A are different shortcuts.

## Known limitations

- **Apps that don't report selection state via Accessibility** fall back to
  gesture-only detection: there, a drag that didn't actually select text
  can still fire ⌘C — an audible beep, or (e.g. when dragging files in
  Finder) an unintended file-copy landing on the clipboard.
- **Shift+click to extend a selection doesn't auto-copy.** A plain click's
  shape is indistinguishable from list multi-select gestures, and a wrong
  copy is worse than an occasional manual ⌘C, so single clicks never
  qualify regardless of modifiers.
- **⌘A always replaces the clipboard, even when you weren't going to
  copy.** Select-all-then-paste-over (⌘A, then ⌘V or typing) puts the
  selection on the clipboard first, so the follow-up ⌘V pastes the text
  back onto itself and whatever you had copied before is gone (a re-copy
  gets it back; the text itself is never lost — ⌘Z covers edits). This is
  the accepted cost of a predictable trigger; see the design note above.
- **⌘A and ⌘C are matched/synthesized by physical key position** (ANSI
  virtual keycodes), which is correct on US-style and JIS layouts but off
  on layouts that move those letters (e.g. AZERTY).
- **Apps that remap ⌘A** to something other than select-all will still arm
  the quiet window; the `AXSelectedText` check usually catches the "nothing
  got selected" case, but apps that don't report selection state fall back
  to copy-anyway, same as the mouse path.
- **Restart after granting permissions:** AutoCopy doesn't poll for the
  Accessibility or Input Monitoring permissions being granted while
  running; if you launch it before granting access, quit and relaunch
  after granting them.

## Building

Requires a recent stable Rust toolchain (`rustup` recommended) and Xcode
Command Line Tools (for the macOS system frameworks AutoCopy links against).

```sh
git clone https://github.com/sora4431/autocopy
cd autocopy
cargo build --release
./target/release/autocopy
```

On first launch, macOS will prompt for the Accessibility and Input
Monitoring permissions — grant both, then quit (from the tray menu) and
relaunch.

### Packaging as a .app

```sh
./scripts/bundle.sh
```

This produces `AutoCopy.app` with `LSUIElement` set (no Dock icon even
before the code's own activation-policy call runs), suitable for dragging
into `/Applications` and adding as a Login Item.

## Future platform support

The `Platform` trait is designed so a new OS backend is the only thing that
needs writing — `app.rs`, `config.rs`, and `tray.rs` (which already supports
Windows/Linux via the `tray-icon` crate) need no changes.

- **Windows:** `SetWindowsHookExW(WH_MOUSE_LL)` for monitoring, `SendInput`
  to synthesize Ctrl+C. No Accessibility-style permission gate exists, so
  `has_permission` can simply return `true`. See
  [`src/platform/windows.rs`](src/platform/windows.rs).
- **Linux:** split by display server — `XRecord` on X11, `evdev`/
  `wlr-virtual-pointer` on Wayland (which has no cross-compositor input
  monitoring API by design). Synthesizing the key via `XTestFakeKeyEvent` or
  a virtual `uinput` device. See
  [`src/platform/linux.rs`](src/platform/linux.rs).

## License

MIT — see [LICENSE](LICENSE).
