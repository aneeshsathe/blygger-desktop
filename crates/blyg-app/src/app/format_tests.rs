//! --- studio#168 --- The editor edge cases the studio's formatting oracles
//! found (blygger-studio PR #168, "Add browser formatting oracles and fix
//! editor edge cases"), for the format commands Burrow has: bold, italic
//! and formulas. The keystroke-level twins are in `editor_edges_tests.rs`.

use super::{Emphasis, emphasis, formula_edit};

fn run(text: &str, from: usize, to: usize, kind: Emphasis) -> (String, String) {
    let e = emphasis(text, from, to, kind);
    assert!(
        e.from <= e.to && e.to <= text.len() && e.start <= e.end,
        "a valid edit: {e:?}"
    );
    let out = e.apply(text);
    let sel = out[e.start..e.end].to_string();
    (out, sel)
}

/// "Applying Bold or Italic across lines containing only asterisks: the
/// command preserves those literal asterisks instead of treating them as
/// removable formatting."
#[test]
fn a_line_of_only_asterisks_is_not_formatting() {
    let t = "**\nword";
    assert_eq!(
        run(t, 0, t.len(), Emphasis::Bold).0,
        "******\n**word**",
        "the `**` line is text: every line is wrapped, nothing taken away"
    );
    let t = "*\nword";
    assert_eq!(run(t, 0, t.len(), Emphasis::Italic).0, "***\n*word*");
    // Real formatting on every line still comes off.
    let t = "**one**\n**two**";
    assert_eq!(run(t, 0, t.len(), Emphasis::Bold).0, "one\ntwo");
}

/// "Applying Bold or Italic to a selection containing only spaces or tabs:
/// the command leaves the whitespace outside the empty formatting markers
/// and keeps the edit range valid."
#[test]
fn a_selection_of_only_spaces_keeps_them_outside_the_markers() {
    let t = "a   b";
    let (out, sel) = run(t, 1, 4, Emphasis::Bold);
    assert_eq!(out, "a   ****b");
    assert_eq!(sel, "", "the caret waits between the markers");
    let t = "a\t \tb";
    assert_eq!(run(t, 1, 4, Emphasis::Italic).0, "a\t \t**b");
}

/// "Applying Formula to an existing display formula such as `$$x$$`: the
/// command preserves the existing formula without adding another set of
/// dollar signs. This also covers multiline display formulas."
#[test]
fn a_formula_over_a_displayed_formula_changes_nothing() {
    let t = "$$x$$";
    let e = formula_edit(t, 0, t.len());
    assert_eq!(e.apply(t), t);
    assert_eq!(&t[e.start..e.end], "x");
    let t = "Area:\n$$\n\\frac{a}{b}\n$$\nand on.";
    let from = t.find("$$").unwrap();
    let to = t.rfind("$$").unwrap() + 2;
    let e = formula_edit(t, from, to);
    assert_eq!(e.apply(t), t, "multi-line: no second pair");
    assert_eq!(e.apply(t)[e.start..e.end].trim(), "\\frac{a}{b}");
    // Spaces picked around it stay where they are.
    let t = "say $$x$$ now";
    let e = formula_edit(t, 3, 10);
    assert_eq!(e.apply(t), t);
    // An inline formula is still a word in dollars, not a displayed one.
    let t = "$x$";
    assert_eq!(formula_edit(t, 0, 3).apply(t), "$$x$$");
}

/// "A save or editor action failed without a usable error message: the
/// editor now displays a failure message."
#[test]
fn a_failure_without_a_message_still_says_what_failed() {
    use blyg_core::CoreError;
    let words = |e: CoreError| super::super::writing::save_failed(&e);
    assert_eq!(words(CoreError::Offline), "Couldn't save: offline");
    let blank = words(CoreError::Other("  ".into()));
    assert_eq!(blank, "Couldn't save (no reason given)");
    let blank = words(CoreError::Rejected {
        status: 500,
        message: String::new(),
        details: vec![],
    });
    assert_eq!(blank, "Couldn't save (no reason given)");
}
