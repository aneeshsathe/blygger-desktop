//! The helpers the shared view-models call as `crate::keymap::…` (the real
//! ones are in `crates/blyg-app/src/keymap.rs`, beside the GPUI key table).
//! There they respell macOS keys for Windows; here they respell the few
//! strings that name the Mac, since this copy of Burrow lives in a browser
//! tab. Keys keep their ⌘ glyphs: the page accepts ⌃ for ⌘ too.
//!
//! Windows' spelling ([`crate::platform_text::Platform::Windows`]) is only
//! ever asked for by the shared view-models' own tests: a plain glyph
//! respelling (`⌘` → `Ctrl`, `⏎` → `Enter`), enough for what they pin.

use crate::platform_text::Platform;

/// The actions with no key on Windows (blyg-app's `WINDOWS_KEYLESS`).
const WINDOWS_KEYLESS: &[&str] = &["DiscardChanges"];

/// The browser's words for the Mac's.
fn browser_words(text: &str) -> Option<&'static str> {
    match text {
        "saved on this Mac" => Some("saved in this browser"),
        "scratch · only on this Mac" => Some("scratch · only in this browser"),
        _ => None,
    }
}

/// Respell a static hint for the page.
pub fn hint(text: &'static str) -> &'static str {
    if let Some(w) = browser_words(text) {
        return w;
    }
    #[cfg(test)]
    if Platform::spelling() == Platform::Windows {
        return Box::leak(windows_hint(text).into_boxed_str());
    }
    text
}

/// `hint` for text built at runtime.
pub fn hint_owned(text: String) -> String {
    let text = text.replace("on this Mac", "in this browser");
    if Platform::spelling() == Platform::Windows {
        windows_hint(&text)
    } else {
        text
    }
}

/// Whether `action` has no key when spelled as `p`. Every action keeps
/// its key in the browser, whose spelling is the Mac's.
pub fn keyless_on(action: &str, p: Platform) -> bool {
    p == Platform::Windows && WINDOWS_KEYLESS.contains(&action)
}

/// Each run of macOS modifier glyphs and the key after it, as Windows
/// writes a key: `⌘⏎` → `Ctrl+Enter`, `⇧⌘K` → `Ctrl+Shift+K`.
fn windows_hint(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let mut mods: Vec<&str> = Vec::new();
        let mut c = Some(c);
        while let Some(m) = c.and_then(|c| match c {
            '⌃' | '⌘' => Some("Ctrl"),
            '⌥' => Some("Alt"),
            '⇧' => Some("Shift"),
            _ => None,
        }) {
            if !mods.contains(&m) {
                mods.push(m);
            }
            c = chars.next();
        }
        if mods.is_empty() {
            out.extend(c);
            continue;
        }
        // Windows writes Ctrl first, then Alt, then Shift.
        mods.sort_by_key(|m| ["Ctrl", "Alt", "Shift"].iter().position(|x| x == m));
        out.push_str(&mods.join("+"));
        let key = match c {
            Some('⏎') => Some("Enter".to_string()),
            Some('⌫') => Some("Backspace".to_string()),
            Some(k) if !k.is_whitespace() => Some(k.to_uppercase().collect()),
            _ => None,
        };
        match key {
            Some(k) => {
                out.push('+');
                out.push_str(&k);
            }
            None => out.extend(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_browser_keeps_the_macs_keys_and_says_browser() {
        assert_eq!(hint("saved on this Mac"), "saved in this browser");
        assert_eq!(hint("⌥⌘1"), "⌥⌘1");
        assert_eq!(
            hint_owned("kept on this Mac".into()),
            "kept in this browser"
        );
        assert!(!keyless_on("DiscardChanges", Platform::spelling()));
    }

    #[test]
    fn windows_spelling_is_for_the_shared_tests() {
        assert_eq!(windows_hint("⌘⏎ publishes"), "Ctrl+Enter publishes");
        assert_eq!(windows_hint("⇧⌘K posts"), "Ctrl+Shift+K posts");
        assert_eq!(windows_hint("no keys"), "no keys");
        assert!(keyless_on("DiscardChanges", Platform::Windows));
    }
}
