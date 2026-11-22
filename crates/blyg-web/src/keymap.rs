//! The three helpers `crates/blyg-app/src/vm.rs` calls as `crate::keymap::…`
//! (the real ones are in `crates/blyg-app/src/keymap.rs`, beside the GPUI
//! key table). There they respell macOS keys for Windows; here they respell
//! the few strings that name the Mac, since this copy of Burrow lives in a
//! browser tab. Keys keep their ⌘ glyphs: the page accepts ⌃ for ⌘ too.

/// Respell a static hint for the browser.
pub fn hint(text: &'static str) -> &'static str {
    match text {
        "saved on this Mac" => "saved in this browser",
        "scratch · only on this Mac" => "scratch · only in this browser",
        other => other,
    }
}

/// `hint` for text built at runtime.
pub fn hint_owned(text: String) -> String {
    text.replace("on this Mac", "in this browser")
}

/// Whether an action has no key here (Windows' `WINDOWS_KEYLESS`). Every
/// action keeps its key in the browser.
pub fn keyless_here(_action: &str) -> bool {
    false
}
