//! A reference already written is edited, not picked (studio 0.66, #157;
//! `palette.ts` `written`): the `[[` / `![[` picker stays shut inside
//! brackets a `]]` closes later on the line (before any new `[[`), and
//! once the caret is past a label's `|`. Clicking or moving into a
//! finished `[[id | label]]` types nothing, so it never opens the picker
//! here; these are the typed cases the same rule covers.

use super::{PickKind, link_trigger, query};

/// `old -> new` typed one `[` at `cursor` (`new` holds it just before).
fn typed(new: &str, cursor: usize) -> Option<usize> {
    let mut old = new.to_string();
    old.remove(cursor - 1);
    link_trigger(&old, new, cursor)
}

#[test]
fn brackets_a_later_close_finishes_stay_shut() {
    // Retyping the second `[` of a written link.
    assert_eq!(typed("see [[abc]] here", 6), None);
    assert_eq!(typed("see [[abc | some label]]", 6), None);
    assert_eq!(typed("[[]]", 2), None);
}

#[test]
fn brackets_being_typed_still_open() {
    // A finished reference later on the line, after a new `[[`, closes nothing.
    assert_eq!(typed("see [[ and also [[abc]]", 6), Some(4));
    // A `]]` on the next line closes nothing on this one.
    assert_eq!(typed("see [[\nnext]]", 6), Some(4));
    assert_eq!(typed("see [[", 6), Some(4));
}

#[test]
fn the_picker_closes_past_a_labels_pipe() {
    let t = "see [[abc | still typing";
    assert_eq!(query(t, 4, PickKind::Link, 9), Some("abc"));
    assert_eq!(query(t, 4, PickKind::Link, 10), Some("abc "));
    assert_eq!(query(t, 4, PickKind::Link, 11), None, "past the `|`");
    assert_eq!(query(t, 4, PickKind::Link, t.len()), None);
    assert_eq!(query("![[q | a no", 0, PickKind::Quote, 11), None);
}
