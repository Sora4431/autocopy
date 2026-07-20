//! AutoCopy: automatically copies selected text to the clipboard.
//!
//! No clipboard history, no cloud sync, no AI, no telemetry. See README.md
//! for what this does and why.

mod app;
mod config;
mod platform;
mod tray;

fn main() {
    app::run();
}
