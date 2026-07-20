//! Best-effort "is any text actually selected right now?" via the
//! Accessibility API.
//!
//! The event tap can only see the *shape* of a gesture; it can't know
//! whether the drag it just watched selected three paragraphs or moved a
//! window. The authoritative source is the frontmost app itself, which
//! (when it cooperates) publishes its focused element's selection through
//! the Accessibility tree: `AXFocusedUIElement` → `AXSelectedText`. These
//! are the same `AX*` APIs already gated behind the Accessibility
//! permission the app holds (see `permissions.rs`).
//!
//! Three outcomes, and the distinction matters — see
//! `Platform::has_text_selection` for how each is acted on:
//! `Some(true)` (selection confirmed), `Some(false)` (app affirmatively
//! says no selection), `None` (app doesn't expose selection state; plenty
//! don't implement this part of the AX protocol, so treating "unknown" as
//! "no" would silently break AutoCopy in all of them).
//!
//! Privacy note: the selected string is fetched only to test that it is
//! non-empty and is dropped immediately — never stored, logged, or sent
//! anywhere. (The ⌘C that follows puts the same text on the clipboard
//! anyway; this check reads nothing the copy itself wouldn't.)

use std::ffi::c_void;

use core_foundation::base::{CFGetTypeID, CFRelease, CFTypeRef, TCFType};
use core_foundation::string::{CFString, CFStringRef};

/// `AXUIElementRef` is a `CFTypeRef` under the hood; we never look inside
/// it, only pass it back to the AX API and release it.
type AXUIElementRef = CFTypeRef;

const AX_SUCCESS: i32 = 0; // kAXErrorSuccess

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
}

pub fn has_text_selection() -> Option<bool> {
    unsafe {
        let system_wide = AXUIElementCreateSystemWide();
        if system_wide.is_null() {
            return None;
        }
        let focused = copy_attribute(system_wide, "AXFocusedUIElement");
        CFRelease(system_wide);
        let focused = focused?;

        let selected = copy_attribute(focused, "AXSelectedText");
        CFRelease(focused);
        // Attribute unsupported (or errored) — the app doesn't report
        // selection state. That's "unknown", not "empty".
        let selected = selected?;

        if CFGetTypeID(selected) != CFString::type_id() {
            CFRelease(selected);
            return None;
        }
        // wrap_under_create_rule takes over the +1 retain from the copy, so
        // the string is released when `text` drops at the end of this scope.
        let text = CFString::wrap_under_create_rule(selected as CFStringRef);
        Some(text.char_len() > 0)
    }
}

/// Thin wrapper over `AXUIElementCopyAttributeValue`. On success the
/// returned value is owned by the caller (+1 retain, per the Copy rule) and
/// must be `CFRelease`d.
unsafe fn copy_attribute(element: AXUIElementRef, name: &str) -> Option<CFTypeRef> {
    let attribute = CFString::new(name);
    let mut value: CFTypeRef = std::ptr::null::<c_void>();
    let err = AXUIElementCopyAttributeValue(element, attribute.as_concrete_TypeRef(), &mut value);
    if err == AX_SUCCESS && !value.is_null() {
        Some(value)
    } else {
        None
    }
}
