//! Key and click modifiers handled outside the keymap.
//!
//! The keymap's tests (`windows_overrides_are_real_and_clash_with_nothing`,
//! `no_windows_key_is_an_altgr_character`) see only what is bound there. A
//! handler that reads `modifiers` by hand, in an `on_key_down`, a
//! `capture_key_down` or a mouse closure, escapes them, and two mistakes
//! came of it:
//!
//! - the Publish sheet took Alt+1 to Alt+9 itself, and on Windows the
//!   keymap binds Alt+1 and Alt+2 globally and won (`auto_crosspost_key`
//!   before the chip-keys-win merge);
//! - handlers tested `modifiers.platform` alone. `platform` is Cmd on a Mac
//!   and the Windows key on Windows, so such a chord is dead on Windows
//!   (About's close, the Reader's select-all, before the same merge).
//!
//! Two checks over the crate's non-test sources:
//!
//! - `no_chord_reads_the_platform_key_alone`: a condition that reads
//!   `modifiers.platform` also reads `modifiers.control` or `secondary()`,
//!   or negates the platform key (a guard).
//! - `modifier_handling_outside_the_keymap_is_on_a_reviewed_list`: every
//!   function that reads a modifier is a line of
//!   `fixtures/key-handlers-allow.txt` (`<file> <fn> | <class> | <reason>`),
//!   and each `chord` line's keys are checked against the keymap.
//!
//! The scan works on text, with these limits. A function is the innermost
//! `fn name` whose braces enclose the mention (closures belong to their
//! enclosing function; two functions of one name in one file share a line;
//! functions a macro writes are missed; a `#[cfg(test)]` item that is not a
//! `mod { }` tail is scanned). A modifier is read through a receiver named
//! `m`, `mods`, `modifiers` or `modifiers()`. A condition is the text
//! between `;`, `{` and `}`, so a match arm that follows another arm
//! without a block can borrow that arm's `control`.

use std::path::Path;

use crate::keymap;
use crate::platform_text::Platform;



/// The handler before the chip-keys-win merge: Alt+1 to Alt+9 ticked the
/// Publish sheet's chips (`extensions/auto_crosspost.rs`, from
/// `capture_key_down` in `sheet/publish.rs`).
const ALT_DIGITS: &str = r#"
impl MainView {
    pub(crate) fn auto_crosspost_key(
        &mut self,
        id: &LocalId,
        ks: &Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        let m = &ks.modifiers;
        if !m.alt || m.control || m.platform || m.shift || m.function {
            return false;
        }
        let Some(n) = ks.key.parse::<usize>().ok() else {
            return false;
        };
        true
    }
}
"#;

/// About's close before the same merge.
const OLD_ABOUT: &str = r#"
fn about(cx: &mut Context<Self>) {
    div().on_key_down(cx.listener(|_, ev: &KeyDownEvent, window, cx| {
        let k = &ev.keystroke;
        if k.key == "escape" || (k.modifiers.platform && k.key == "w") {
            cx.stop_propagation();
            window.remove_window();
        }
    }))
}
"#;

/// The Reader's select-all before the same merge.
const OLD_PICK_ALL: &str = r#"
fn on_key(&mut self, k: &Keystroke) {
    if self.reading.view == View::Reading
        && self.reading.mode == stream_vm::ReadMode::Reader
        && k.modifiers.platform
        && !(k.modifiers.control || k.modifiers.alt || k.modifiers.shift)
        && k.key == "a"
    {
        self.pick_all(cx);
    }
}
"#;

fn entries(text: &str) -> Vec<Entry> {
    parse_fixture(text).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn the_alt_digit_handler_is_unlisted_and_then_clashes_on_windows() {
    let file = "extensions/auto_crosspost.rs";
    let all = sites(file, ALT_DIGITS);
    assert!(all.iter().all(|s| s.func == "auto_crosspost_key"), "{all:?}");
    assert_eq!(all.len(), 5, "alt, control, platform, shift, function: {all:?}");
    // The guard names control too, so the platform scan lets it be.
    assert!(platform_alone(ALT_DIGITS).is_empty());

    let bad = unlisted(&all, &[]);
    assert_eq!(bad.len(), 1, "{bad:?}");
    assert!(bad[0].contains("auto_crosspost_key") && bad[0].contains("line 11"), "{bad:?}");

    // Listed as the chord it is, the Windows keymap already has alt-1.
    let line = format!("{file} auto_crosspost_key | chord alt-1 | taken by hand");
    let list = entries(&line);
    assert!(unlisted(&all, &list).is_empty());
    let (mac, windows) = tables();
    let clashes = chord_clashes(&list, &mac, &windows);
    assert!(
        clashes.iter().any(|c| c.contains("bound on Windows as alt-1")),
        "{clashes:?}"
    );
    assert!(!clashes.iter().any(|c| c.contains("on the Mac")), "{clashes:?}");
    // A reason that says what it shares excuses it.
    let line = format!("{file} auto_crosspost_key | chord alt-1 | shares the sheet context |");
    assert!(chord_clashes(&entries(&line), &mac, &windows).is_empty());
}

#[test]
fn a_chord_the_mac_table_binds_clashes_too() {
    let list = entries("x.rs f | chord cmd-w | a handler for the closing key");
    let (mac, windows) = tables();
    let clashes = chord_clashes(&list, &mac, &windows);
    assert!(clashes.iter().any(|c| c.contains("on the Mac")), "{clashes:?}");
    assert!(clashes.iter().any(|c| c.contains("on Windows as ctrl-w")), "{clashes:?}");
}

#[test]
fn the_old_about_line_reads_the_platform_key_alone() {
    let bad = platform_alone(OLD_ABOUT);
    assert_eq!(bad.len(), 1, "{bad:?}");
    assert_eq!(bad[0].func, "about");
    assert_eq!(bad[0].line, 6);
    assert_eq!(
        bad[0].condition,
        "if k.key == \"escape\" || (k.modifiers.platform && k.key == \"w\")"
            .replace("\"escape\"", "\"escape\"")
            .replace("\"w\"", "\"w\"")
    );
}

#[test]
fn the_old_select_all_names_control_only_to_reject_it() {
    let bad = platform_alone(OLD_PICK_ALL);
    assert_eq!(bad.len(), 1, "{bad:?}");
    assert_eq!(bad[0].line, 5);
}

#[test]
fn platform_with_control_or_secondary_passes() {
    for ok in [
        "fn f(k: K) { if k.modifiers.platform || k.modifiers.control { go(); } }",
        "fn f(k: K) { if k.modifiers.control || k.modifiers.platform { go(); } }",
        "fn f(k: K) { if k.modifiers.secondary() { go(); } }",
        "fn f(k: K) { if m.platform && m.secondary() { go(); } }",
        "fn f(k: K) { if ev.modifiers().platform || ev.modifiers().control { go(); } }",
    ] {
        assert!(platform_alone(ok).is_empty(), "{ok}");
    }
    for bad in [
        "fn f(k: K) { if m.platform && !m.shift { go(); } }",
        "fn f(k: K) { if m.platform && !m.control { go(); } }",
        "fn f(w: W) { let m = w.modifiers(); (m.platform, m.alt) }",
    ] {
        assert_eq!(platform_alone(bad).len(), 1, "{bad}");
    }
}

#[test]
fn a_guard_over_two_lines_passes() {
    let src = "fn f(k: K) {\n    if !(k.modifiers.platform\n        || k.modifiers.control\n        || k.modifiers.alt)\n    {\n        go();\n    }\n}\n";
    assert!(platform_alone(src).is_empty());
    let src = "fn f(k: K) {\n    if k.modifiers.platform\n        || k.modifiers.control\n        || k.modifiers.alt\n    {\n        return;\n    }\n}\n";
    assert!(platform_alone(src).is_empty());
    let src = "fn f(k: K) { if page && !k.modifiers.platform && !k.modifiers.alt { go(); } }";
    assert!(platform_alone(src).is_empty(), "a negated platform key is a guard");
}

#[test]
fn longer_names_are_not_mentions() {
    let src = "fn f(m: M) { m.platform_text(); m.controller.go(); self.alt.as_deref(); \
               x.shift_by(1); m.platform.name(); other.platform; y.function_name(); \
               m.modified_at(); w.modified(); }";
    assert!(sites("a.rs", src).is_empty(), "{:?}", sites("a.rs", src));
    assert!(platform_alone(src).is_empty());
    // The same words as real reads are mentions.
    let src = "fn f(m: M) { if m.platform || m.shift {} }";
    assert_eq!(sites("a.rs", src).len(), 2);
}

#[test]
fn comments_strings_and_test_tails_are_ignored() {
    let src = r##"
fn live() {
    // k.modifiers.platform is a comment here
    /* and k.modifiers.platform in a block /* nested */ */
    let s = "k.modifiers.platform";
    let r = r#"k.modifiers.platform "quoted""#;
    let c = '{';
    let x: &'static str = "";
}

#[cfg(test)]
mod tests {
    fn t(k: K) { if k.modifiers.platform { go(); } }
}
"##;
    assert!(sites("a.rs", src).is_empty(), "{:?}", sites("a.rs", src));
    assert!(platform_alone(src).is_empty());
    // A `#[cfg(test)] mod x;` declaration is no tail; the code after it counts.
    let src = "#[cfg(test)]\nmod x_tests;\nfn f(m: M) { if m.platform { go(); } }\n";
    assert_eq!(platform_alone(src).len(), 1);
}

#[test]
fn the_function_is_the_innermost_that_holds_the_read() {
    let src = "impl V {\n    fn outer(&self) {\n        fn inner(m: M) -> bool { m.alt }\n        let f = |m: M| m.shift;\n        if m.control {}\n    }\n    fn next(&self) { x.modifiers.function; }\n}\n";
    let got: Vec<(String, usize)> = sites("a.rs", src).into_iter().map(|s| (s.func, s.line)).collect();
    assert_eq!(
        got,
        [
            ("inner".to_string(), 3),
            ("outer".to_string(), 4),
            ("outer".to_string(), 5),
            ("next".to_string(), 7),
        ]
    );
}

#[test]
fn a_stale_fixture_line_fails() {
    let src = "fn f(m: M) { if m.alt {} }";
    let all = sites("a.rs", src);
    let list = entries("a.rs f | guard | bare keys only\na.rs gone | guard | was renamed\n");
    assert!(unlisted(&all, &list).is_empty());
    let old = stale(&all, &list);
    assert_eq!(old.len(), 1, "{old:?}");
    assert!(old[0].contains("gone") && old[0].contains(":2:"), "{old:?}");
}

#[test]
fn the_fixture_format_is_checked() {
    for bad in [
        "a.rs f | guard",
        "a.rs | guard | r",
        "a.rs f | chord | r",
        "a.rs f | sometimes | r",
        "a.rs f | guard | r\na.rs f | mouse | r",
    ] {
        assert!(parse_fixture(bad).is_err(), "{bad}");
    }
    let ok = entries("# c\n\na.rs f | chord cmd-backspace alt-up | why\nb.rs g | mac-only | r");
    assert_eq!(
        ok[0].class,
        Class::Chord(vec!["cmd-backspace".into(), "alt-up".into()])
    );
    assert_eq!(ok[1].class, Class::MacOnly);
}
