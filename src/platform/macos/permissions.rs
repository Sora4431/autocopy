//! Accessibility permission (`AXIsProcessTrusted`).
//!
//! Any API that can observe global input or synthesize events on behalf of
//! the user — `CGEventTap`, `CGEventPost`, and the Accessibility (`AX*`) APIs
//! — requires the process to be an Accessibility-"trusted" application. This
//! is a per-app, user-granted permission (System Settings → Privacy &
//! Security → Accessibility), separate from macOS's other TCC permissions
//! (camera, microphone, …) mostly for historical reasons — it predates TCC's
//! unified consent API, hence the distinct `AXIsProcessTrusted*` functions
//! rather than a `TCC`-style prompt.
//!
//! A listen-only `CGEventTap` (see `event_tap.rs`) can technically observe
//! everything the user types system-wide, including passwords — which is
//! exactly why macOS gates it behind explicit, visible user consent instead
//! of a silent entitlement.

use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::string::CFString;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
}

pub fn has_permission() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// Triggers the system "AutoCopy would like to control this computer using
/// Accessibility features" prompt, which deep-links straight to the
/// Accessibility settings pane. macOS only shows this prompt once per app
/// (identified by binary/bundle path); if the user previously denied it,
/// this call is a silent no-op and they have to add AutoCopy manually.
pub fn request_permission() {
    unsafe {
        let key = CFString::new("AXTrustedCheckOptionPrompt");
        let value = CFBoolean::true_value();
        let options = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);
        AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef());
    }
}
