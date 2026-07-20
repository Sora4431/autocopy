# AutoCopy

AutoCopy is a tiny macOS menu bar utility with exactly one job: when you
finish selecting text anywhere on your Mac, it automatically copies it to
the clipboard — the equivalent of pressing ⌘C for you.

It is **not** a clipboard manager. There's no history, no cloud sync, no AI,
no analytics, no telemetry of any kind. It watches for a selection to end
and sends one keystroke. That's the whole product.

## Features

- Lives in the menu bar only — no Dock icon, no windows
- Auto-copies when you finish a text selection (drag, double- or triple-click)
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

## Why does it need Accessibility permission?

Detecting "the user just finished selecting text, anywhere, in any app" and
then simulating a keypress are both privileged operations on macOS. They go
through the same API surface — Quartz Event Services — that keyboard
remapping and window-management tools use, and Apple gates all of it behind
the **Accessibility** permission (System Settings → Privacy & Security →
Accessibility). There's no narrower permission available; observing global
mouse input and observing *only selection-related* input aren't different
capabilities as far as macOS is concerned.

AutoCopy asks for this once, on first launch, via the standard system
prompt. If you skip it, add AutoCopy manually in that settings pane, then
quit and relaunch.

**What AutoCopy does with that access:** it opens a *listen-only* event tap
(see below) that watches exactly one thing — left mouse button presses and
releases — and does nothing else with the input stream. After a
selection-shaped gesture, it also asks the focused app (through the same
Accessibility API, via `AXSelectedText`) whether any text is actually
selected; the answer is checked for non-emptiness and immediately
discarded. It never reads or stores keystrokes, never logs anything, and
never sends anything over the network (the app has no network code at all).

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
            event_tap.rs  global mouse monitoring + gesture filter (CGEventTap)
            selection.rs  "is text actually selected?" check (AXSelectedText)
            simulate.rs   synthesizing ⌘C (CGEventPost)
            permissions.rs  Accessibility permission check/request
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
 CGEventTap (macOS)        event_tap.rs                              app.rs / platform
 ───────────────────       ─────────────                             ─────────────────
 left mouse down    ───▶  remember where the click started
 left mouse up      ───▶  gesture filter:
                            Control held?              → ignore (right-click stand-in)
                            double/triple click?       → selection gesture
                            single click, moved ≥ 4pt? → drag selection
                            otherwise                  → ignore (plain click)
                                    │
                                    ▼
                          InputEvent::PotentialSelection
                                    │  if cfg.enabled
                                    ▼
                          sleep(action_delay_ms)
                                    │
                          focused app says "no text selected"? → do nothing
                                    ▼
                          send ⌘C (send_copy_shortcut)
```

Three design decisions worth calling out:

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

## Known limitations

- **Apps that don't report selection state via Accessibility** fall back to
  gesture-only detection: there, a drag that didn't actually select text
  can still fire ⌘C — an audible beep, or (e.g. when dragging files in
  Finder) an unintended file-copy landing on the clipboard.
- **Shift+click to extend a selection doesn't auto-copy.** A plain click's
  shape is indistinguishable from list multi-select gestures, and a wrong
  copy is worse than an occasional manual ⌘C, so single clicks never
  qualify regardless of modifiers.
- **Restart after granting permission:** AutoCopy doesn't poll for the
  Accessibility permission being granted while running; if you launch it
  before granting access, quit and relaunch after granting it.

## Building

Requires a recent stable Rust toolchain (`rustup` recommended) and Xcode
Command Line Tools (for the macOS system frameworks AutoCopy links against).

```sh
git clone https://github.com/sora4431/autocopy
cd autocopy
cargo build --release
./target/release/autocopy
```

On first launch, macOS will prompt for Accessibility permission — grant it,
then quit (from the tray menu) and relaunch.

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
