//! --- shelf --- your own posts kept to hand in the Write list.

use super::Store;
use crate::api::Prune;
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

#[test]
fn the_shelf_holds_at_most_fifty() {
    let s = store();
    for i in 0..super::shelf::SHELF_MAX {
        let id = post(&s, &format!("post {i}"));
        s.set_on_shelf(&id, true).unwrap();
    }
    let one_more = post(&s, "one too many");
    assert!(matches!(
        s.set_on_shelf(&one_more, true),
        Err(CoreError::Rejected { status: 400, .. })
    ));
    assert_eq!(s.shelf().len(), super::shelf::SHELF_MAX);
}

// --- shelf sync --- (schema v22) anyone's posts, the wire names, and the
// blyg's list laid over this computer's.

use crate::shelf::Entry;

/// Give a post the server id `sid`, as a create's answer would.
fn sent(s: &Store, id: &LocalId, sid: &str) {
    s.conn()
        .execute(
            "UPDATE items SET server_id = ?2 WHERE local_id = ?1",
            rusqlite::params![id.0, sid],
        )
        .unwrap();
}

fn theirs(id: &str) -> Entry {
    Entry::new("sub-tide", id)
}

/// The blyg's list takes nothing off unless the decision allows it: on a
/// newer contract (or before any check) what it lacks stays, after it.
#[test]
fn a_withheld_shelf_list_takes_nothing_off() {
    let s = store();
    s.set_entry_on_shelf(&theirs("r1"), true).unwrap();
    s.set_entry_on_shelf(&theirs("r2"), true).unwrap();
    s.apply_shelf(&[theirs("r3"), theirs("r2")], Prune::Withheld)
        .unwrap();
    assert_eq!(s.shelf_wire(), [theirs("r3"), theirs("r2"), theirs("r1")]);
    s.apply_shelf(&[theirs("r3")], Prune::Allowed).unwrap();
    assert_eq!(s.shelf_wire(), [theirs("r3")]);
}

#[test]
fn others_posts_share_the_shelf_and_its_cap() {
    let s = store();
    let a = post(&s, "now");
    sent(&s, &a, "it-now");
    s.set_on_shelf(&a, true).unwrap();
    s.set_entry_on_shelf(&theirs("r1"), true).unwrap();
    s.set_entry_on_shelf(&theirs("r1"), true).unwrap();
    assert_eq!(s.shelf_wire(), [Entry::own("it-now"), theirs("r1")]);
    // Your own by its wire name is the same row as by its local id.
    s.set_entry_on_shelf(&Entry::own("it-now"), false).unwrap();
    assert!(s.shelf().is_empty());
    assert_eq!(s.shelf_wire(), [theirs("r1")]);
    s.set_entry_on_shelf(&Entry::own("it-now"), true).unwrap();
    assert_eq!(s.shelf(), std::slice::from_ref(&a));
    // Moving your post keeps someone else's where it is.
    let b = post(&s, "log");
    s.set_on_shelf(&b, true).unwrap();
    s.move_on_shelf(&b, 0).unwrap();
    assert_eq!(s.shelf(), [b.clone(), a.clone()]);
    assert_eq!(s.shelf_rows().len(), 3);
    s.set_entry_on_shelf(&theirs("r1"), false).unwrap();
    assert_eq!(s.shelf_rows().len(), 2);

    for i in 0..(super::shelf::SHELF_MAX - 2) {
        s.set_entry_on_shelf(&theirs(&format!("x{i}")), true)
            .unwrap();
    }
    assert!(matches!(
        s.set_entry_on_shelf(&theirs("one-more"), true),
        Err(CoreError::Rejected { status: 400, .. })
    ));
}

#[test]
fn the_wire_list_leaves_out_what_the_blyg_cant_name() {
    let s = store();
    let (a, b, c) = (post(&s, "sent"), post(&s, "not yet"), post(&s, "gone"));
    sent(&s, &a, "it-a");
    sent(&s, &c, "it-c");
    for id in [&a, &b, &c] {
        s.set_on_shelf(id, true).unwrap();
    }
    s.set_status_version(&c, Status::Withdrawn, 1).unwrap();
    assert_eq!(s.shelf_wire(), [Entry::own("it-a")]);
    // Once the draft reaches the blyg, it goes too.
    sent(&s, &b, "it-b");
    assert_eq!(s.shelf_wire(), [Entry::own("it-a"), Entry::own("it-b")]);
}

#[test]
fn the_blygs_list_is_laid_over_with_local_posts_kept_in_place() {
    let s = store();
    let (a, note, b) = (post(&s, "a"), post(&s, "scratch"), post(&s, "b"));
    sent(&s, &a, "it-a");
    sent(&s, &b, "it-b");
    for id in [&a, &note, &b] {
        s.set_on_shelf(id, true).unwrap();
    }
    // Elsewhere: b moved first, someone's post added, a taken off.
    let changed = s
        .apply_shelf(&[Entry::own("it-b"), theirs("r1")], Prune::Allowed)
        .unwrap();
    assert!(changed);
    assert_eq!(s.shelf_wire(), [Entry::own("it-b"), theirs("r1")]);
    // The scratch note (no server id) stays, at the top where it followed
    // nothing the blyg kept.
    assert_eq!(s.shelf(), [note.clone(), b.clone()]);
    // The same list again changes nothing.
    assert!(
        !s.apply_shelf(&[Entry::own("it-b"), theirs("r1")], Prune::Allowed)
            .unwrap()
    );

    // A post of yours this computer doesn't hold yet is kept by name, and
    // counts once it arrives.
    s.apply_shelf(
        &[Entry::own("it-later"), Entry::own("it-b")],
        Prune::Allowed,
    )
    .unwrap();
    assert_eq!(s.shelf_wire(), [Entry::own("it-later"), Entry::own("it-b")]);
    assert_eq!(s.shelf(), [note.clone(), b.clone()]);
    let later = post(&s, "later");
    sent(&s, &later, "it-later");
    assert_eq!(s.shelf(), [note.clone(), later.clone(), b.clone()]);
    // Taking it off by its local id finds it by name.
    s.set_on_shelf(&later, false).unwrap();
    assert_eq!(s.shelf_wire(), [Entry::own("it-b")]);
}

#[test]
fn a_shelf_from_before_v22_keeps_its_posts() {
    let mut c = rusqlite::Connection::open_in_memory().unwrap();
    super::schema::migrate_to(&mut c, 21).unwrap();
    c.execute_batch(
        "INSERT INTO items (local_id, kind, status, created, updated) \
             VALUES ('l1', 'thread', 'draft', 't', 't'), ('l2', 'thread', 'draft', 't', 't');
         INSERT INTO shelf (local_id, position) VALUES ('l2', 0), ('l1', 1);",
    )
    .unwrap();
    super::schema::migrate(&mut c).unwrap();
    let rows: Vec<(String, i64)> = c
        .prepare("SELECT local_id, position FROM shelf ORDER BY position")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(rows, [("l2".to_string(), 0), ("l1".to_string(), 1)]);
}
