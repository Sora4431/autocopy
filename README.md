# AutoCopy

AutoCopy is a tiny macOS menu bar utility with exactly one job: when you
finish selecting text anywhere on your Mac, it automatically copies it to
the clipboard — the equivalent of pressing ⌘C for you.

It is **not** a clipboard manager. There's no history, no cloud sync, no AI,
no analytics, no telemetry of any kind. It watches for a selection to end
and sends one keystroke. That's the whole product.

## Features

- Lives in the menu bar only — no Dock icon, no windows
- Auto-copies when you finish a drag/click text selection (mouse-up)
- Optionally auto-copies after ⌘A (Select All)
- Optionally auto-pastes at the cursor when you ⌥-click (Option+Click)
- Each trigger is an independent on/off checkbox in the tray menu
- Configurable reaction delay (default 50ms)
- Zero network access, zero telemetry, zero data collection

## Non-goals

- Clipboard history or multiple clipboards
- OCR, AI, or content transformation
- Cloud sync of any kind
- Keyboard shortcuts to trigger actions manually
- Any settings beyond the four in [`Config`](src/config.rs)

## Why does it need Accessibility permission?

Detecting "the user just finished selecting text, anywhere, in any app" and
then simulating a keypress are both privileged operations on macOS. They go
through the same API surface — Quartz Event Services — that keyboard
remapping and window-management tools use, and Apple gates all of it behind
the **Accessibility** permission (System Settings → Privacy & Security →
Accessibility). There's no narrower permission available; observing global
input and observing *only selection-related* input aren't different
capabilities as far as macOS is concerned.

AutoCopy asks for this once, on first launch, via the standard system
prompt. If you skip it, add AutoCopy manually in that settings pane, then
quit and relaunch.

**What AutoCopy does with that access:** it opens a *listen-only* event tap
(see below) that reacts to two things — left mouse button releases, and
⌘A key-down events — and does nothing else with the input stream. It never
reads or stores keystrokes, never logs anything, and never sends anything
over the network (the app has no network code at all).

## Architecture

```
src/
    main.rs         entry point
    app.rs           wires config + platform + tray together (no OS-specific code)
    config.rs        on-disk settings (JSON), no OS-specific code
    tray.rs           menu bar UI, via the cross-platform `tray-icon` crate
    platform/
        mod.rs        the `Platform` trait + `InputEvent` enum
        macos/         macOS implementation (only backend in the MVP)
            mod.rs      ties the pieces together, owns the Cocoa run loop
            event_tap.rs  global input monitoring (CGEventTap)
            simulate.rs   synthesizing ⌘C/⌘V (CGEventPost)
            permissions.rs  Accessibility permission check/request
        windows.rs      stub — documents what a real backend would use
        linux.rs        stub — documents what a real backend would use
```

The application layer (`app.rs`, `config.rs`, `tray.rs`) never imports
anything OS-specific. It only knows about the `Platform` trait:

```rust
pub trait Platform {
    fn has_permission(&self) -> bool;
    fn request_permission(&self);
    fn start_monitoring(&self, tx: Sender<InputEvent>);
    fn send_copy_shortcut(&self);
    fn send_paste_shortcut(&self);
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
 CGEventTap (macOS)              event_tap.rs             app.rs                platform
 ───────────────────             ─────────────            ──────                ────────
 left mouse button up   ───▶   InputEvent::MouseUp   ───▶  if copy_on_selection  ───▶ sleep(delay_ms)
                                                                                       send_copy_shortcut()
 plain ⌘A key-down      ───▶  InputEvent::SelectAll  ───▶  if copy_on_select_all ───▶ sleep(delay_ms)
                                                                                       send_copy_shortcut()
 ⌥ held + mouse up      ───▶ InputEvent::ModifierClick──▶ if paste_on_modifier   ───▶ sleep(delay_ms)
                                                              _click                  send_paste_shortcut()
```

Two design decisions worth calling out:

- **Why react to every mouse-up, not just "the end of a drag"?** A mouse-up
  is a mouse-up whether it ends a drag, a double-click (word select), or a
  triple-click (paragraph select). Sending ⌘C when nothing happens to be
  selected is a harmless no-op, so there's no need to measure drag distance
  or otherwise classify the click — and for double/triple-clicks, each
  mouse-up fires its own (independent, non-blocking) copy, so the *last* one
  naturally reflects the final selection.

- **Why a delay before acting?** The frontmost app needs a moment after the
  input event to actually update its internal selection/cursor state before
  a synthesized ⌘C or ⌘V would do the right thing. 50ms is imperceptible to
  a human but enough of a buffer in practice; it's configurable via
  `Config::action_delay_ms` if a particular app needs more.

## Known limitations

- **Keyboard layout:** the ⌘A detection matches the physical key at the
  ANSI "A" position (virtual keycode `0x00`). On non-QWERTY-family layouts
  (Dvorak, AZERTY, non-Latin layouts) this may not correspond to "select
  all" in every app. Handling every layout correctly requires translating
  through the current input source (`UCKeyTranslate`), which was left out
  of the MVP for simplicity.
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

- **Windows:** `SetWindowsHookExW(WH_MOUSE_LL/WH_KEYBOARD_LL)` for
  monitoring, `SendInput` to synthesize Ctrl+C/Ctrl+V. No Accessibility-style
  permission gate exists, so `has_permission` can simply return `true`. See
  [`src/platform/windows.rs`](src/platform/windows.rs).
- **Linux:** split by display server — `XRecord` on X11, `evdev`/
  `wlr-virtual-pointer` on Wayland (which has no cross-compositor input
  monitoring API by design). Synthesizing keys via `XTestFakeKeyEvent` or a
  virtual `uinput` device. See [`src/platform/linux.rs`](src/platform/linux.rs).

## License

MIT — see [LICENSE](LICENSE).
