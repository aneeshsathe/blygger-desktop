//! Unicode on the wire (protocol 0.4 §3, §5, §5.3; decision #103).
//!
//! - **What a publisher may send.** `content_md` MUST NOT hold a C0 control
//!   other than tab, line feed and carriage return, or an unpaired surrogate,
//!   and SHOULD NOT hold U+FFFE or U+FFFF: none can appear in XML, so one of
//!   them breaks the whole `feed.xml` for every strict subscriber. Burrow
//!   cleans them out quietly as text is saved ([`for_save`]). A Rust `str` is
//!   UTF-8 and can't hold an unpaired surrogate at all, so that half of the
//!   rule holds by construction.
//! - **NFC on save** is a SHOULD on publishers, so a quoted selection and a
//!   search match what the eye sees. Readers MUST NOT normalize an import (it
//!   would then hold bytes the hash doesn't cover): nothing here is applied
//!   to remote text.
//! - **Caps count code points**, and a cut never splits a code point (a MUST,
//!   which `char` boundaries give for free) and, for anything a person reads,
//!   not a grapheme cluster either (a SHOULD; [`clip`]).

use std::borrow::Cow;

use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfc_quick};
use unicode_segmentation::UnicodeSegmentation;

/// A character the protocol keeps out of published text: a C0 control other
/// than tab, LF and CR (MUST NOT), or the noncharacters U+FFFE and U+FFFF
/// (SHOULD NOT).
pub fn is_forbidden(c: char) -> bool {
    (c < ' ' && !matches!(c, '\t' | '\n' | '\r')) || matches!(c, '\u{FFFE}' | '\u{FFFF}')
}

/// `s` without the characters [`is_forbidden`] names. Borrowed when there
/// were none, which is nearly always.
pub fn strip_forbidden(s: &str) -> Cow<'_, str> {
    if s.chars().any(is_forbidden) {
        Cow::Owned(s.chars().filter(|&c| !is_forbidden(c)).collect())
    } else {
        Cow::Borrowed(s)
    }
}

/// `s` in Unicode Normalization Form C. Borrowed when it already is (the
/// quick check answers that for almost all text without allocating).
pub fn nfc(s: &str) -> Cow<'_, str> {
    match is_nfc_quick(s.chars()) {
        IsNormalized::Yes => Cow::Borrowed(s),
        _ => {
            let n: String = s.nfc().collect();
            if n == s {
                Cow::Borrowed(s)
            } else {
                Cow::Owned(n)
            }
        }
    }
}

/// Text Burrow is about to keep as the author's own (a post's working copy,
/// a publish note, a hopper's name): forbidden characters stripped, then
/// NFC. Idempotent.
pub fn for_save(s: &str) -> Cow<'_, str> {
    match strip_forbidden(s) {
        Cow::Borrowed(b) => nfc(b),
        Cow::Owned(o) => Cow::Owned(nfc(&o).into_owned()),
    }
}

/// `s` holds nothing [`for_save`] would change.
pub fn is_clean(s: &str) -> bool {
    matches!(for_save(s), Cow::Borrowed(_))
}

/// Length as a cap stated in characters counts it: code points (§5.3).
pub fn char_len(s: &str) -> usize {
    s.chars().count()
}

/// The longest prefix of `s` that is at most `n` code points. Always on a
/// `char` boundary, so never half a surrogate pair's code point.
pub fn prefix_chars(s: &str, n: usize) -> &str {
    match s.char_indices().nth(n) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// The longest prefix of `s` that is at most `n` code points and ends on an
/// extended grapheme cluster boundary (UAX #29): a cut that doesn't leave
/// half an emoji family or a letter without its accent. Empty when the first
/// cluster alone is longer than `n`.
pub fn prefix_graphemes(s: &str, n: usize) -> &str {
    let mut end = 0;
    let mut count = 0;
    for (i, g) in s.grapheme_indices(true) {
        count += g.chars().count();
        if count > n {
            break;
        }
        end = i + g.len();
    }
    &s[..end]
}

/// `s` cut for display to at most `n` code points, the last one an ellipsis
/// when anything went, and never inside a grapheme cluster. Trailing space
/// before the ellipsis is dropped.
pub fn clip(s: &str, n: usize) -> Cow<'_, str> {
    if n == 0 {
        return Cow::Borrowed("");
    }
    if char_len(s) <= n {
        return Cow::Borrowed(s);
    }
    let head = prefix_graphemes(s, n - 1).trim_end();
    Cow::Owned(format!("{head}…"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forbidden_characters_are_the_c0_set_and_two_noncharacters() {
        for c in [
            '\0', '\u{7}', '\u{b}', '\u{c}', '\u{1b}', '\u{1f}', '\u{FFFE}', '\u{FFFF}',
        ] {
            assert!(is_forbidden(c), "{:?}", c);
        }
        for c in [
            '\t', '\n', '\r', ' ', 'a', '\u{7f}', '\u{85}', '\u{FFFD}', '😀',
        ] {
            assert!(!is_forbidden(c), "{:?}", c);
        }
    }

    #[test]
    fn stripping_keeps_tab_lf_cr_and_borrows_clean_text() {
        assert_eq!(strip_forbidden("a\0b\u{1b}[0mc\u{FFFF}"), "ab[0mc");
        assert_eq!(strip_forbidden("x\ty\r\nz"), "x\ty\r\nz");
        assert!(matches!(strip_forbidden("plain"), Cow::Borrowed(_)));
    }

    #[test]
    fn nfc_composes_and_borrows_text_already_composed() {
        let decomposed = "Cafe\u{301} na\u{ef}ve";
        assert_eq!(nfc(decomposed), "Caf\u{e9} na\u{ef}ve");
        assert!(matches!(nfc("Caf\u{e9}"), Cow::Borrowed(_)));
        assert!(matches!(nfc("ascii only"), Cow::Borrowed(_)));
        // Hangul jamo compose too.
        assert_eq!(nfc("\u{1112}\u{1161}\u{11ab}"), "\u{d55c}");
    }

    #[test]
    fn for_save_strips_then_composes_and_is_idempotent() {
        let raw = "e\u{0}\u{301}\u{7}";
        let once = for_save(raw).into_owned();
        assert_eq!(once, "\u{e9}");
        assert_eq!(for_save(&once), once);
        assert!(is_clean(&once));
        assert!(!is_clean(raw));
    }

    #[test]
    fn caps_count_code_points() {
        assert_eq!(char_len("a😀b"), 3); // UTF-16 says 4, bytes say 6
        assert_eq!(char_len("e\u{301}"), 2);
    }

    #[test]
    fn prefixes_cut_on_char_boundaries() {
        assert_eq!(prefix_chars("a😀b", 2), "a😀");
        assert_eq!(prefix_chars("a😀b", 9), "a😀b");
        assert_eq!(prefix_chars("😀", 0), "");
    }

    #[test]
    fn grapheme_prefixes_keep_clusters_whole() {
        let family = "👨\u{200d}👩\u{200d}👧"; // 5 code points, one cluster
        let s = format!("ab{family}c");
        assert_eq!(prefix_graphemes(&s, 3), "ab");
        assert_eq!(prefix_graphemes(&s, 7), format!("ab{family}"));
        assert_eq!(prefix_graphemes("e\u{301}x", 1), "");
        assert_eq!(prefix_graphemes("e\u{301}x", 2), "e\u{301}");
    }

    #[test]
    fn clip_ellipsizes_within_the_cap() {
        assert_eq!(clip("short", 10), "short");
        assert_eq!(clip("hello world", 7), "hello…");
        let flag = "🇳🇿"; // two regional indicators, one cluster
        let s = format!("ab{flag}{flag}");
        let c = clip(&s, 4);
        assert_eq!(c, "ab…");
        assert!(char_len(&c) <= 4);
        assert_eq!(clip("anything", 0), "");
    }
}
