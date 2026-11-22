//! Picking a contact in natural grammar (studio 0.66, #158;
//! `ui/handle-trigger.ts` `handleInsert` / `punctuationAfterPick`): a pick
//! writes `@handle ` with a space, and a possessive, hyphen or closing
//! punctuation typed straight after it takes the space's place. The
//! vectors are the studio's own (`test-ui/handle-trigger.test.ts`).

use super::{Candidate, insert, punctuation_after_pick};

fn contact(handle: Option<&str>) -> Candidate {
    Candidate {
        url: "https://lin.example.org/".into(),
        host: "lin.example.org".into(),
        name: Some("Lin Okafor".into()),
        title: None,
        rank: 0,
        why: "in your contacts",
        contact: true,
        handle: handle.map(str::to_string),
    }
}

#[test]
fn a_handle_pick_writes_a_space_after_it() {
    let (t, caret) = insert("Thanks @li", 7, 10, &contact(Some("lin")));
    assert_eq!(t, "Thanks @lin ");
    assert_eq!(caret, t.len());
    // Text already after the caret stays where it was.
    let (t, caret) = insert("Thanks @li, again", 7, 10, &contact(Some("lin")));
    assert_eq!(t, "Thanks @lin , again");
    assert_eq!(&t[..caret], "Thanks @lin ");
    // A plain link (a blyg that doesn't rewrite handles) takes no space.
    let (t, _) = insert("Thanks @li", 7, 10, &contact(None));
    assert_eq!(t, "Thanks [Lin Okafor](https://lin.example.org/)");
}

#[test]
fn punctuation_typed_straight_after_a_pick_takes_the_space() {
    // After a pick the text reads `@kyle ` with the caret after the space;
    // `text` is before the typed character lands.
    let picked = "In @kyle ";
    let at = picked.len();
    assert_eq!(
        punctuation_after_pick(picked, at, "'"),
        Some(("In @kyle'".to_string(), 9))
    );
    assert_eq!(
        punctuation_after_pick(picked, at, "\u{2019}"),
        Some(("In @kyle\u{2019}".to_string(), 11))
    );
    for typed in [
        "-", ",", ".", ":", ";", "!", "?", ")", "]", "}", "\"", "\u{201d}", "\u{2026}", "\u{2013}",
        "\u{2014}", "/", "_",
    ] {
        assert_eq!(
            punctuation_after_pick(picked, at, typed).map(|(t, _)| t),
            Some(format!("In @kyle{typed}")),
            "{typed:?}"
        );
    }
    // Text after the caret is kept.
    assert_eq!(
        punctuation_after_pick("In @kyle  case", 9, "'"),
        Some(("In @kyle' case".to_string(), 9))
    );
}

#[test]
fn letters_spaces_and_a_moved_caret_are_left_alone() {
    let picked = "In @kyle ";
    assert_eq!(punctuation_after_pick(picked, picked.len(), "a"), None);
    assert_eq!(punctuation_after_pick(picked, picked.len(), " "), None);
    assert_eq!(punctuation_after_pick(picked, 3, "'"), None);
    assert_eq!(punctuation_after_pick("In @kyle", 8, "'"), None);
    // Not a handle: an email, a path, a lone `@`, a leading hyphen.
    assert_eq!(punctuation_after_pick("me@kyle ", 8, "'"), None);
    assert_eq!(punctuation_after_pick("x/@kyle ", 8, "'"), None);
    assert_eq!(punctuation_after_pick("@ ", 2, "'"), None);
    assert_eq!(punctuation_after_pick("@-kyle ", 7, "'"), None);
    assert_eq!(punctuation_after_pick("(@kyle ", 7, ")"), Some(("(@kyle)".into(), 7)));
    assert_eq!(punctuation_after_pick("@kyle ", 0, "'"), None);
}
