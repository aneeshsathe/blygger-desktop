//! --- trails --- on a real (in-memory) SQLite store.

use rusqlite::params;

use super::Store;
use crate::backend::CoreError;
use crate::model::OWN_SUBSCRIPTION;
use crate::trails::TrailStop;

fn store() -> Store {
    Store::open_in_memory().unwrap()
}

fn stop(sub: &str, id: &str, why: &str) -> TrailStop {
    TrailStop {
        subscription_id: sub.into(),
        remote_id: id.into(),
        origin: "https://blyg.example.org/".into(),
        why: why.into(),
        title: Some(format!("Title of {id}")),
        withdrawn: false,
    }
}

fn held(s: &Store, sub: &str, id: &str, state: &str) {
    s.conn()
        .execute(
            "INSERT OR REPLACE INTO reading \
             (subscription_id, remote_id, origin, observed_at, state, version, json) \
             VALUES (?1, ?2, 'https://blyg.example.org/', '2026-11-01T00:00:00Z', ?3, 1, '{}')",
            params![sub, id, state],
        )
        .unwrap();
}

fn ids(s: &Store, trail: &str) -> Vec<String> {
    s.trail_stops(trail)
        .into_iter()
        .map(|x| x.remote_id)
        .collect()
}

#[test]
fn trails_keep_their_stops_in_order() {
    let s = store();
    assert!(s.trails().is_empty());
    let t = s.create_trail("  weeding ").unwrap();
    assert_eq!(t.name, "weeding");
    for (i, id) in ["a", "b", "c"].iter().enumerate() {
        held(&s, "s1", id, "current");
        assert_eq!(s.add_trail_stop(&t.id, &stop("s1", id, "")).unwrap(), i);
    }
    assert_eq!(ids(&s, &t.id), ["a", "b", "c"]);
    assert_eq!(s.trails()[0].stops, 3);

    // Adding a stop again keeps its place; a why replaces the old one.
    assert_eq!(
        s.add_trail_stop(&t.id, &stop("s1", "a", "first")).unwrap(),
        0
    );
    assert_eq!(s.trail_stops(&t.id)[0].why, "first");
    assert_eq!(s.add_trail_stop(&t.id, &stop("s1", "a", "  ")).unwrap(), 0);
    assert_eq!(s.trail_stops(&t.id)[0].why, "first");

    s.move_trail_stop(&t.id, 2, 0).unwrap();
    assert_eq!(ids(&s, &t.id), ["c", "a", "b"]);
    s.move_trail_stop(&t.id, 0, 99).unwrap();
    assert_eq!(ids(&s, &t.id), ["a", "b", "c"]);
    s.set_trail_why(&t.id, 1, " the turn ").unwrap();
    assert_eq!(s.trail_stops(&t.id)[1].why, "the turn");
    s.remove_trail_stop(&t.id, 0).unwrap();
    assert_eq!(ids(&s, &t.id), ["b", "c"]);
    assert!(matches!(
        s.remove_trail_stop(&t.id, 5),
        Err(CoreError::NotFound)
    ));
}

#[test]
fn trails_are_named_ordered_renamed_and_deleted() {
    let s = store();
    assert!(matches!(
        s.create_trail(" "),
        Err(CoreError::Rejected { status: 400, .. })
    ));
    let a = s.create_trail("tides").unwrap();
    let b = s.create_trail("gardens").unwrap();
    s.rename_trail(&a.id, "tide pools").unwrap();
    let names: Vec<_> = s.trails().into_iter().map(|t| t.name).collect();
    assert_eq!(names, ["tide pools", "gardens"]);
    assert!(matches!(
        s.add_trail_stop("nope", &stop("s1", "x", "")),
        Err(CoreError::NotFound)
    ));
    s.add_trail_stop(&b.id, &stop("s1", "x", "")).unwrap();
    s.delete_trail(&b.id).unwrap();
    assert_eq!(s.trails().len(), 1);
    let left: i64 = s
        .conn()
        .query_row("SELECT COUNT(*) FROM trail_stops", [], |r| r.get(0))
        .unwrap();
    assert_eq!(left, 0);
}

#[test]
fn a_withdrawn_post_becomes_a_tombstone_stop_without_its_text() {
    let s = store();
    let t = s.create_trail("weeding").unwrap();
    held(&s, "s1", "theirs", "current");
    s.add_trail_stop(&t.id, &stop("s1", "theirs", "why kept"))
        .unwrap();
    // One of yours, published, then withdrawn.
    s.conn()
        .execute(
            "INSERT INTO items (local_id, server_id, kind, status, created, updated) \
             VALUES ('L1', 'mine', 'thread', 'public', '2026-01-01', '2026-01-01')",
            [],
        )
        .unwrap();
    s.add_trail_stop(&t.id, &stop(OWN_SUBSCRIPTION, "mine", ""))
        .unwrap();
    assert!(s.trail_stops(&t.id).iter().all(|x| !x.withdrawn));

    held(&s, "s1", "theirs", "tombstone");
    s.conn()
        .execute(
            "UPDATE items SET status = 'withdrawn' WHERE local_id = 'L1'",
            [],
        )
        .unwrap();
    let stops = s.trail_stops(&t.id);
    assert!(stops.iter().all(|x| x.withdrawn && x.title.is_none()));
    // The user's own words stay; the post's are gone from the store too.
    assert_eq!(stops[0].why, "why kept");
    let titles: i64 = s
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM trail_stops WHERE title IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(titles, 0);
    // And a withdrawn post can't be added.
    let t2 = s.create_trail("other").unwrap();
    assert!(s.add_trail_stop(&t2.id, &stop("s1", "theirs", "")).is_err());
}

#[test]
fn the_migration_lands_on_the_latest_schema() {
    let s = store();
    assert_eq!(s.schema_version().unwrap(), super::schema::latest());
}
