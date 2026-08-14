//! Best-effort startup check: are the two chords AutoCopy is about to rely
//! on — ⌘A (watched) and ⌘C (synthesized) — already claimed by something
//! else on this machine?
//!
//! There are two places such a claim is actually recorded on disk, and both
//! are readable through `CFPreferences`:
//!
//! - **System keyboard shortcuts** (`com.apple.symbolichotkeys`): the
//!   bindings under System Settings → Keyboard → Keyboard Shortcuts. None
//!   of them uses a bare ⌘-letter chord out of the box, but they are
//!   user-editable, and a system-wide binding on ⌘A or ⌘C would fire on
//!   every selection AutoCopy makes or watches.
//! - **Custom App Shortcuts** (`NSUserKeyEquivalents`, global domain plus
//!   one domain per app): System Settings → Keyboard → App Shortcuts lets
//!   the user reassign a menu item to any chord. If someone has given a
//!   menu item the shortcut ⌘C, AutoCopy's synthesized ⌘C triggers *that
//!   menu item* instead of Copy in the affected app; a reassigned ⌘A
//!   means select-all isn't select-all there.
//!
//! Anything found is reported as a warning on stderr — AutoCopy still
//! starts, because a conflict in one app is no reason to lose auto-copy
//! everywhere else, and the user may well know about it already.
//!
//! **What this deliberately cannot see:** global hotkeys that other
//! processes register at runtime (Keyboard Maestro macros, launcher apps,
//! …). macOS has no public API to enumerate those, so no startup check can
//! be complete — this one covers everything that's actually written down.
//!
//! The scan is a few hundred `cfprefsd` lookups (one per preference
//! domain), runs once at startup before monitoring begins, and takes on
//! the order of tens of milliseconds.

use std::ffi::c_void;

use core_foundation::array::CFArray;
use core_foundation::base::{CFGetTypeID, CFType, CFTypeRef, TCFType};
use core_foundation::boolean::{CFBoolean, CFBooleanRef};
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::{CFNumber, CFNumberRef};
use core_foundation::string::{CFString, CFStringRef};

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFPreferencesAnyApplication: CFStringRef;
    fn CFPreferencesCopyAppValue(key: CFStringRef, application_id: CFStringRef) -> CFTypeRef;
}

/// Virtual keycodes matching `event_tap::KEYCODE_A` / `simulate::KEYCODE_C`.
const KEYCODE_A: i64 = 0x00;
const KEYCODE_C: i64 = 0x08;

/// Device-independent modifier masks (`NX_*MASK`) as used by the
/// `AppleSymbolicHotKeys` parameter list.
const MASK_SHIFT: i64 = 1 << 17;
const MASK_CONTROL: i64 = 1 << 18;
const MASK_OPTION: i64 = 1 << 19;
const MASK_COMMAND: i64 = 1 << 20;

pub fn warn_about_conflicts() {
    for finding in collect_findings() {
        eprintln!("AutoCopy: heads-up: {finding}");
    }
}

fn collect_findings() -> Vec<String> {
    let mut findings = Vec::new();
    check_system_shortcuts(&mut findings);
    check_app_shortcut_overrides(&mut findings);
    findings
}

/// System Settings → Keyboard → Keyboard Shortcuts, stored as
/// `com.apple.symbolichotkeys` → `AppleSymbolicHotKeys`: a dictionary of
/// hotkey-id → `{enabled, value: {parameters: [character, keycode,
/// modifier-mask]}}`.
fn check_system_shortcuts(findings: &mut Vec<String>) {
    let Some(root) = copy_preference("AppleSymbolicHotKeys", Some("com.apple.symbolichotkeys"))
    else {
        return;
    };
    let Some(hotkeys) = as_dictionary(root.as_CFTypeRef()) else {
        return;
    };

    let (ids, entries) = hotkeys.get_keys_and_values();
    for (&id, &entry) in ids.iter().zip(entries.iter()) {
        let Some(entry) = as_dictionary(entry) else {
            continue;
        };
        if find(&entry, "enabled").and_then(as_i64) != Some(1) {
            continue;
        }
        let Some(value) = find(&entry, "value").and_then(as_dictionary) else {
            continue;
        };
        let Some(params) = find(&value, "parameters").and_then(as_array) else {
            continue;
        };
        let keycode = params.get(1).map(|v| *v).and_then(as_i64);
        let mods = params.get(2).map(|v| *v).and_then(as_i64);
        let (Some(keycode), Some(mods)) = (keycode, mods) else {
            continue;
        };
        if !is_bare_command(mods) {
            continue;
        }
        let chord = match keycode {
            KEYCODE_A => "the select-all chord (Cmd+A)",
            KEYCODE_C => "the copy chord (Cmd+C)",
            _ => continue,
        };
        let id = as_string(id).unwrap_or_else(|| "?".into());
        findings.push(format!(
            "a system-wide keyboard shortcut (symbolic hotkey {id}, System \
             Settings -> Keyboard -> Keyboard Shortcuts) is bound to \
             {chord}. AutoCopy watches Cmd+A and synthesizes Cmd+C, so \
             every auto-copy would also trigger that system action."
        ));
    }
}

/// System Settings → Keyboard → App Shortcuts, stored as
/// `NSUserKeyEquivalents` (menu title → key equivalent) in the global
/// domain and/or per-app preference domains.
fn check_app_shortcut_overrides(findings: &mut Vec<String>) {
    check_user_key_equivalents(None, findings);
    for domain in preference_domains() {
        check_user_key_equivalents(Some(&domain), findings);
    }
}

fn check_user_key_equivalents(domain: Option<&str>, findings: &mut Vec<String>) {
    let Some(root) = copy_preference("NSUserKeyEquivalents", domain) else {
        return;
    };
    let Some(overrides) = as_dictionary(root.as_CFTypeRef()) else {
        return;
    };

    let (titles, keys) = overrides.get_keys_and_values();
    for (&title, &key) in titles.iter().zip(keys.iter()) {
        let Some(key) = as_string(key) else {
            continue;
        };
        let Some(chord) = watched_chord_name(&key) else {
            continue;
        };
        let title = as_string(title).unwrap_or_else(|| "?".into());
        let scope = domain.unwrap_or("every app (global)");
        findings.push(format!(
            "{chord} is reassigned to the menu item \"{title}\" for {scope} \
             (System Settings -> Keyboard -> App Shortcuts). Where that \
             override applies, the chord no longer does what AutoCopy \
             assumes — auto-copy may misfire or trigger that menu item."
        ));
    }
}

/// `NSUserKeyEquivalents` encodes shortcuts as modifier sigils followed by
/// the key: `@` Command, `$` Shift, `~` Option, `^` Control — and an
/// uppercase letter implies Shift by itself. Only the two *bare* Command
/// chords AutoCopy relies on are of interest; anything with more modifiers
/// is a different chord and can't collide.
fn watched_chord_name(key_equivalent: &str) -> Option<&'static str> {
    match key_equivalent {
        "@a" => Some("the select-all chord (Cmd+A)"),
        "@c" => Some("the copy chord (Cmd+C)"),
        _ => None,
    }
}

fn is_bare_command(mods: i64) -> bool {
    mods & MASK_COMMAND != 0 && mods & (MASK_SHIFT | MASK_CONTROL | MASK_OPTION) == 0
}

/// Looks up a string key in an untyped dictionary, returning the borrowed
/// (Get-rule) value pointer — valid only while `dict` is.
fn find(dict: &CFDictionary, key: &str) -> Option<*const c_void> {
    let key = CFString::new(key);
    dict.find(key.as_concrete_TypeRef() as *const c_void)
        .map(|value| *value)
}

/// Per-app preference domains = the plist file names in
/// `~/Library/Preferences`. The dot-prefixed `.GlobalPreferences` is the
/// on-disk name of the global domain, which is queried separately via
/// `kCFPreferencesAnyApplication`, so hidden files are skipped.
fn preference_domains() -> Vec<String> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(home.join("Library/Preferences")) else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let domain = name.strip_suffix(".plist")?;
            (!domain.starts_with('.')).then(|| domain.to_string())
        })
        .collect()
}

/// Reads one key from one preference domain (`None` = the global domain).
/// The returned `CFType` owns the value (Copy rule) and releases it on
/// drop; `None` means "not set", which is the common case.
fn copy_preference(key: &str, domain: Option<&str>) -> Option<CFType> {
    let key = CFString::new(key);
    let domain_name = domain.map(CFString::new);
    unsafe {
        let domain_ref = match &domain_name {
            Some(name) => name.as_concrete_TypeRef(),
            None => kCFPreferencesAnyApplication,
        };
        let value = CFPreferencesCopyAppValue(key.as_concrete_TypeRef(), domain_ref);
        if value.is_null() {
            None
        } else {
            Some(CFType::wrap_under_create_rule(value))
        }
    }
}

// The helpers below take borrowed (Get-rule) pointers into a container the
// caller keeps alive, verify the actual runtime type, and hand back either
// a plain Rust value or a retained wrapper — so nothing here can be used
// after its container goes away, and a plist with unexpected shapes (which
// third-party apps do write) degrades to "no finding" instead of UB or a
// panic.

fn as_dictionary(value: *const c_void) -> Option<CFDictionary> {
    if value.is_null() {
        return None;
    }
    unsafe {
        if CFGetTypeID(value) != CFDictionary::<*const c_void, *const c_void>::type_id() {
            return None;
        }
        Some(CFDictionary::wrap_under_get_rule(value as CFDictionaryRef))
    }
}

fn as_array(value: *const c_void) -> Option<CFArray> {
    if value.is_null() {
        return None;
    }
    unsafe {
        if CFGetTypeID(value) != CFArray::<*const c_void>::type_id() {
            return None;
        }
        Some(CFArray::wrap_under_get_rule(value as *const _))
    }
}

fn as_string(value: *const c_void) -> Option<String> {
    if value.is_null() {
        return None;
    }
    unsafe {
        if CFGetTypeID(value) != CFString::type_id() {
            return None;
        }
        Some(CFString::wrap_under_get_rule(value as CFStringRef).to_string())
    }
}

/// Numbers and booleans are interchangeable in these plists (`enabled` is
/// written both ways), so both come back as an integer.
fn as_i64(value: *const c_void) -> Option<i64> {
    if value.is_null() {
        return None;
    }
    unsafe {
        let type_id = CFGetTypeID(value);
        if type_id == CFNumber::type_id() {
            CFNumber::wrap_under_get_rule(value as CFNumberRef).to_i64()
        } else if type_id == CFBoolean::type_id() {
            Some(bool::from(CFBoolean::wrap_under_get_rule(value as CFBooleanRef)) as i64)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_bare_command_chords_are_watched() {
        assert!(watched_chord_name("@a").is_some());
        assert!(watched_chord_name("@c").is_some());
        // Uppercase means Shift is part of the chord (⌘⇧A ≠ ⌘A).
        assert_eq!(watched_chord_name("@A"), None);
        assert_eq!(watched_chord_name("@C"), None);
        // Extra modifier sigils are different chords.
        assert_eq!(watched_chord_name("$@a"), None);
        assert_eq!(watched_chord_name("~@c"), None);
        assert_eq!(watched_chord_name("^@a"), None);
        // Other keys are none of our business.
        assert_eq!(watched_chord_name("@b"), None);
        assert_eq!(watched_chord_name(""), None);
    }

    #[test]
    fn bare_command_mask_check() {
        assert!(is_bare_command(MASK_COMMAND));
        // Symbolic hotkey masks routinely carry extra device bits alongside
        // the device-independent ones; only the four standard modifiers
        // should matter.
        assert!(is_bare_command(MASK_COMMAND | 0x8));
        assert!(!is_bare_command(MASK_COMMAND | MASK_SHIFT));
        assert!(!is_bare_command(MASK_COMMAND | MASK_CONTROL));
        assert!(!is_bare_command(MASK_COMMAND | MASK_OPTION));
        assert!(!is_bare_command(MASK_SHIFT));
        assert!(!is_bare_command(0));
    }
}
