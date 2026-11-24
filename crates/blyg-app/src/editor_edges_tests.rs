//! --- studio#168 --- The editor edge cases from blygger-studio PR #168,
//! typed into the real editor: each a keystroke through GPUI's dispatch,
//! against a zero-latency FakeBackend. The pure rules are in
//! `app/format_tests.rs`. The cases about commands Burrow doesn't have
//! (code blocks, the hard-break button, list indent, link cleanup,
//! read-only regions) are listed in `changes/unreleased/editor-edges.md`.

use super::*;

fn selection(view: &Entity<MainView>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |v, cx| {
        let s = v.editor.read(cx);
        let r = s.selected_range();
        s.value()[r.start..r.end].to_string()
    })
}

fn write(view: &Entity<MainView>, cx: &mut VisualTestContext, text: &str) {
    open(view, cx, DRAFT);
    keys(cx, "cmd-a");
    cx.simulate_input(text);
    cx.run_until_parked();
    assert_eq!(editor_text(view, cx), text);
}

/// ⌃⌘B over a line that is only asterisks keeps them (they're text).
#[gpui_kit::test]
fn bold_over_a_line_of_asterisks_keeps_them(cx: &mut TestAppContext) {
    let (view, _, cx) = setup(cx);
    write(&view, cx, "**\nword");
    keys(cx, "cmd-a");
    keys(cx, "ctrl-cmd-b");
    assert_eq!(editor_text(&view, cx), "******\n**word**");
}

/// ⌃⌘I over spaces only: the markers go after them, empty, and what's typed
/// next lands between them.
#[gpui_kit::test]
fn italic_over_spaces_puts_empty_markers_after_them(cx: &mut TestAppContext) {
    let (view, _, cx) = setup(cx);
    write(&view, cx, "a   b");
    keys(cx, "left");
    for _ in 0..3 {
        keys(cx, "shift-left");
    }
    assert_eq!(selection(&view, cx), "   ");
    keys(cx, "ctrl-cmd-i");
    assert_eq!(editor_text(&view, cx), "a   **b");
    cx.simulate_input("so");
    cx.run_until_parked();
    assert_eq!(editor_text(&view, cx), "a   *so*b");
}

/// ⌃⌘M over `$$x$$` changes nothing, and deletes nothing: an edit that
/// writes what's there already is no edit (the studio's empty edit erased
/// the character before the caret).
#[gpui_kit::test]
fn a_formula_over_a_displayed_one_is_left_alone(cx: &mut TestAppContext) {
    let (view, _, cx) = setup(cx);
    write(&view, cx, "$$x$$");
    keys(cx, "cmd-a");
    keys(cx, "ctrl-cmd-m");
    assert_eq!(editor_text(&view, cx), "$$x$$");
    assert_eq!(selection(&view, cx), "x");
    write(&view, cx, "$$\nx\n$$");
    keys(cx, "cmd-a");
    keys(cx, "ctrl-cmd-m");
    assert_eq!(editor_text(&view, cx), "$$\nx\n$$");
}

/// A selection whose edge falls inside an emoji or an accented letter made
/// of two characters (the editor steps ⇧← by character, so it can): the
/// edit takes exactly the selection, never more of the cluster.
#[gpui_kit::test]
fn formatting_inside_an_emoji_takes_exactly_the_selection(cx: &mut TestAppContext) {
    let (view, _, cx) = setup(cx);
    let coder = "\u{1F469}\u{200D}\u{1F4BB}"; // woman technologist (a ZWJ sequence)
    let accent = "e\u{0301}"; // e and a combining acute
    for (text, kind, mark) in [
        (format!("a{coder}b{accent}c"), "ctrl-cmd-b", "**"),
        (format!("a{coder}b"), "ctrl-cmd-i", "*"),
    ] {
        write(&view, cx, &text);
        keys(cx, "left");
        keys(cx, "shift-left");
        let range = view.read_with(cx, |v, cx| v.editor.read(cx).selected_range());
        assert!(!range.is_empty(), "a selection");
        let picked = &text[range.clone()];
        keys(cx, kind);
        assert_eq!(
            editor_text(&view, cx),
            format!(
                "{}{mark}{picked}{mark}{}",
                &text[..range.start],
                &text[range.end..]
            ),
            "exactly {picked:?} of {text:?}"
        );
    }
}
