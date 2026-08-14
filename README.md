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
- Auto-copies after ⌘A (select all) — unless you immediately keep typing,
  paste, or click, in which case it correctly does nothing (see below)
- Skips plain clicks, Control-clicks, and gestures that didn't actually
  select anything — no beeps, no clipboard churn
- One on/off checkbox in the tray menu, plus Quit
- Configurable reaction delays (50ms after a mouse selection, a 300ms
  quiet window after ⌘A)
- Zero network access, zero telemetry, zero data collection

## Non-goals

- Clipboard history or multiple clipboards
- Auto-paste, or any other action besides copying
- OCR, AI, or content transformation
- Cloud sync of any kind
- Keyboard shortcuts to trigger actions manually
- Any settings beyond the three in [`Config`](src/config.rs)

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
taps (see below). The mouse tap watches mouse button presses and releases —
nothing else. The keyboard tap sees key-down events and reduces each one,
inside the tap callback, to a single bit: "that was exactly ⌘A" or "some
other key went down" (the cancel signal for a pending select-all copy).
*Which* other key is never forwarded, stored, or logged. After a trigger,
AutoCopy also asks the focused app (through the same Accessibility API, via
`AXSelectedText`) whether any text is actually selected; the answer is
checked for non-emptiness and immediately discarded. Nothing is ever sent
over the network (the app has no network code at all).

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
            simulate.rs   synthesizing ⌘C (CGEventPost), tagged so our own tap ignores it
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
 any mouse button down ─▶  InputEvent::OtherActivity  ──────────▶  cancel armed ⌘A copy
 left mouse down    ───▶  ...and remember where the click started
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

 keyboard tap (CGEventTap)
 ───────────────────
 key down           ───▶  synthesized by AutoCopy itself? → ignore (tagged)
                          autorepeat?                     → ignore
                          exactly ⌘A (no ⇧⌃⌥)?
                            yes → InputEvent::SelectAllPressed
                            no  → InputEvent::OtherActivity
                                    │
                                    ▼
                          SelectAllPressed arms a copy (if cfg.enabled):
                                    │
                          sleep(select_all_delay_ms)   ◀── the quiet window
                                    │
                          did *any* other input arrive meanwhile? → do nothing
                          focused app says "no text selected"?    → do nothing
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

- **Why doesn't ⌘A copy immediately?** Because ⌘A doesn't always mean
  "copy next". Its other everyday uses — ⌘A then ⌘V or typing to *replace*
  everything, ⌘A then an arrow key to jump to the start or end — must never
  auto-copy: a ⌘C fired into the middle of select-all-then-paste silently
  overwrites the very clipboard content you were about to paste, which is
  the worst thing a clipboard utility can do. The two failure modes aren't
  symmetric — a wrongly *skipped* copy costs one manual ⌘C, a wrongly
  *fired* copy is invisible data loss — so the rule errs hard toward
  skipping: after ⌘A, AutoCopy waits `select_all_delay_ms` (default 300ms),
  and **any** input during that window — another key, a mouse press,
  anything — cancels the pending copy, no exceptions. Pressing ⌘C yourself
  in the window also just works: it cancels the pending automatic one and
  your own copy proceeds, so nothing fires twice. A second ⌘A re-arms the
  window rather than double-copying, and AutoCopy tags its own synthesized
  events so they can never be mistaken for user activity.

## Known limitations

- **Apps that don't report selection state via Accessibility** fall back to
  gesture-only detection: there, a drag that didn't actually select text
  can still fire ⌘C — an audible beep, or (e.g. when dragging files in
  Finder) an unintended file-copy landing on the clipboard.
- **Shift+click to extend a selection doesn't auto-copy.** A plain click's
  shape is indistinguishable from list multi-select gestures, and a wrong
  copy is worse than an occasional manual ⌘C, so single clicks never
  qualify regardless of modifiers.
- **Anything you press right after ⌘A cancels the auto-copy — including
  ⌘Tab.** That's the deliberate any-input-cancels rule above, and it means
  a fast ⌘A → ⌘Tab → ⌘V can find the clipboard unchanged (nothing was
  copied). The recovery is one manual ⌘C; the alternative — trying to guess
  which follow-up keys are "safe" — is how clipboards get clobbered.
- **Conversely, if you pause past the quiet window and *then* type or
  paste over the selection,** the auto-copy has already fired and replaced
  whatever was on the clipboard. Lengthen `select_all_delay_ms` in the
  config if you often select-all-and-replace at a slow pace.
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
