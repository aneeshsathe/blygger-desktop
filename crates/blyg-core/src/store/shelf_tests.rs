//! --- shelf --- your own posts kept to hand in the Write list.

use super::Store;
use crate::backend::CoreError;
use crate::model::*;

fn store() -> Store {
    Store::open_in_memory().unwrap()
}

fn post(s: &Store, text: &str) -> LocalId {
    s.create_draft(Kind::Thread, text, 0).unwrap()
}

#[test]
fn the_shelf_keeps_its_order_and_renumbers() {
    let s = store();
    let (a, b, c) = (post(&s, "Now"), post(&s, "Reading log"), post(&s, "Uses"));
    assert!(s.shelf().is_empty());
    s.set_on_shelf(&b, true).unwrap();
    s.set_on_shelf(&a, true).unwrap();
    s.set_on_shelf(&c, true).unwrap();
    // Shelving twice changes nothing.
    s.set_on_shelf(&a, true).unwrap();
    assert_eq!(s.shelf(), [b.clone(), a.clone(), c.clone()]);

    s.set_on_shelf(&a, false).unwrap();
    assert_eq!(s.shelf(), [b.clone(), c.clone()]);
    // Taking off what isn't there is fine too.
    s.set_on_shelf(&a, false).unwrap();
    // A newly shelved post goes to the end.
    s.set_on_shelf(&a, true).unwrap();
    assert_eq!(s.shelf(), [b, c, a]);
}

#[test]
fn shelved_posts_move_and_clamp() {
    let s = store();
    let ids: Vec<_> = ["one", "two", "three"]
        .iter()
        .map(|t| post(&s, t))
        .collect();
    for id in &ids {
        s.set_on_shelf(id, true).unwrap();
    }
    s.move_on_shelf(&ids[2], 0).unwrap();
    assert_eq!(s.shelf(), [ids[2].clone(), ids[0].clone(), ids[1].clone()]);
    s.move_on_shelf(&ids[2], 99).unwrap();
    assert_eq!(s.shelf(), [ids[0].clone(), ids[1].clone(), ids[2].clone()]);
    s.move_on_shelf(&ids[0], 1).unwrap();
    assert_eq!(s.shelf(), [ids[1].clone(), ids[0].clone(), ids[2].clone()]);
}

#[test]
fn unknown_posts_are_refused() {
    let s = store();
    let ghost = LocalId("nope".into());
    assert!(matches!(
        s.set_on_shelf(&ghost, true),
        Err(CoreError::NotFound)
    ));
    assert!(matches!(
        s.move_on_shelf(&ghost, 0),
        Err(CoreError::NotFound)
    ));
}

#[test]
fn deleted_posts_drop_out_and_withdrawn_ones_hide() {
    let s = store();
    let (a, b, c) = (post(&s, "a"), post(&s, "b"), post(&s, "c"));
    for id in [&a, &b, &c] {
        s.set_on_shelf(id, true).unwrap();
    }
    s.delete_item(&a).unwrap();
    assert_eq!(s.shelf(), [b.clone(), c.clone()]);

    // Withdrawn: hidden, and moves count only what's shown.
    s.set_status_version(&b, Status::Withdrawn, 1).unwrap();
    assert_eq!(s.shelf(), std::slice::from_ref(&c));
    let d = post(&s, "d");
    s.set_on_shelf(&d, true).unwrap();
    s.move_on_shelf(&d, 0).unwrap();
    assert_eq!(s.shelf(), [d, c]);
}

#[test]
fn scratch_notes_stay_shelved_when_promoted() {
    let s = store();
    let n = s.create_scratch(Kind::Thread, "a living note").unwrap();
    s.set_on_shelf(&n, true).unwrap();
    s.set_status_version(&n, Status::Draft, 0).unwrap();
    assert_eq!(s.shelf(), [n]);
}
