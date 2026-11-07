//! Schema v18 and the read-only connection: the trigram index is brought
//! up to date lazily, searches never miss what an item says now, and the
//! UI's reads see every commit.

use super::*;

fn ids(v: &[Item]) -> Vec<LocalId> {
    v.iter().map(|i| i.local_id.clone()).collect()
}

fn fts_ok(s: &Store) {
    s.conn()
        .execute(
            "INSERT INTO items_fts(items_fts, rank) VALUES ('integrity-check', 1)",
            [],
        )
        .unwrap();
}

fn stale(s: &Store) -> i64 {
    s.conn()
        .query_row("SELECT COUNT(*) FROM items_fts_stale", [], |r| r.get(0))
        .unwrap()
}

fn on_disk() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("blygger.db")).unwrap();
    (dir, s)
}

#[test]
fn search_sees_edits_before_the_index_does() {
    for (_dir, s) in [(None, Store::open_in_memory().unwrap()), {
        let (d, s) = on_disk();
        (Some(d), s)
    }] {
        let a = s
            .create_draft(Kind::Fragment, "a harbour at dawn", 0)
            .unwrap();
        let b = s
            .create_draft(Kind::Fragment, "kettle and lantern", 0)
            .unwrap();
        s.save(&a, "a meadow at dusk", 0).unwrap();
        assert_eq!(stale(&s), 1);
        // The old words are gone, the new ones found, before and after the
        // index catches up.
        for _ in 0..2 {
            assert!(s.search("harbour").is_empty());
            assert_eq!(ids(&s.search("MEADOW")), vec![a.clone()]);
            assert_eq!(ids(&s.search("lantern")), vec![b.clone()]);
            assert_eq!(ids(&s.search("at d")), vec![a.clone()]);
            s.flush_fts().unwrap();
            assert_eq!(stale(&s), 0);
            fts_ok(&s);
        }
        // Edited twice before a flush: the index drops what it holds.
        s.save(&a, "first", 0).unwrap();
        s.save(&a, "second thoughts", 0).unwrap();
        assert_eq!(ids(&s.search("thoughts")), vec![a.clone()]);
        assert!(s.search("first").is_empty());
        s.flush_fts().unwrap();
        fts_ok(&s);
        assert_eq!(ids(&s.search("thoughts")), vec![a.clone()]);
        // Deleted while unindexed: the index stays whole.
        s.save(&b, "kettle on", 0).unwrap();
        s.delete_item(&b).unwrap();
        assert_eq!(stale(&s), 0);
        fts_ok(&s);
        assert!(s.search("kettle").is_empty());
    }
}

#[test]
fn writing_the_same_text_marks_nothing_for_the_index() {
    let s = Store::open_in_memory().unwrap();
    let a = s.create_draft(Kind::Fragment, "same words", 0).unwrap();
    s.conn()
        .execute(
            "UPDATE items SET content_md = content_md WHERE local_id = ?1",
            [&a.0],
        )
        .unwrap();
    assert_eq!(stale(&s), 0);
}

#[test]
fn the_picker_finds_an_edited_post_by_its_new_words() {
    let s = Store::open_in_memory().unwrap();
    let a = s.create_draft(Kind::Fragment, "copper signal", 0).unwrap();
    s.after_publish(&a, 1, "https://blyg.example.com/f/abc", "copper signal")
        .unwrap();
    s.conn()
        .execute(
            "UPDATE items SET server_id = 'abc' WHERE local_id = ?1",
            [&a.0],
        )
        .unwrap();
    s.save(&a, "orchard window", 0).unwrap();
    let q = crate::pick::PickQuery {
        text: "orchard".into(),
        ..Default::default()
    };
    let found = s.pick_search(&q).unwrap();
    assert_eq!(found.len(), 1, "{found:?}");
}

#[test]
fn reopening_indexes_what_the_last_session_left() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("blygger.db");
    let a = {
        let s = Store::open(&path).unwrap();
        let a = s.create_draft(Kind::Fragment, "river", 0).unwrap();
        s.save(&a, "river stones", 0).unwrap();
        assert_eq!(stale(&s), 1);
        a
    };
    let s = Store::open(&path).unwrap();
    assert_eq!(stale(&s), 0);
    fts_ok(&s);
    assert_eq!(ids(&s.search("stones")), vec![a]);
}

#[test]
fn reads_on_the_second_connection_see_every_commit() {
    let (_dir, s) = on_disk();
    assert!(s.read.is_some());
    let a = s.create_draft(Kind::Fragment, "one", 0).unwrap();
    assert_eq!(s.items().len(), 1);
    assert_eq!(s.public_origin(), None);
    s.save(&a, "two", 0).unwrap();
    assert_eq!(s.item(&a).unwrap().content_md, "two");
    assert_eq!(s.items()[0].content_md, "two");
    s.after_publish(&a, 1, "https://blyg.example.com/blyg/f/abc", "two")
        .unwrap();
    assert_eq!(
        s.public_origin().as_deref(),
        Some("https://blyg.example.com/blyg/")
    );
    let b = s.create_draft(Kind::Fragment, "three", 0).unwrap();
    assert_eq!(s.items().len(), 2);
    assert_eq!(s.local_id_for_server("abc"), None);
    s.delete_item(&b).unwrap();
    assert_eq!(s.items().len(), 1);
}

#[test]
fn earliest_due_skips_conflicted_items() {
    let s = Store::open_in_memory().unwrap();
    let a = s.create_draft(Kind::Fragment, "a", 1_000).unwrap();
    let b = s.create_draft(Kind::Fragment, "b", 60_000).unwrap();
    let due = |id: &LocalId| {
        s.ops()
            .unwrap()
            .into_iter()
            .find(|o| &o.local_id == id)
            .unwrap()
            .not_before
    };
    assert_eq!(s.earliest_due(), Some(due(&a)));
    s.conn()
        .execute("UPDATE items SET conflict = 1 WHERE local_id = ?1", [&a.0])
        .unwrap();
    assert_eq!(s.earliest_due(), Some(due(&b)));
    s.conn()
        .execute("UPDATE items SET conflict = 1 WHERE local_id = ?1", [&b.0])
        .unwrap();
    assert_eq!(s.earliest_due(), None);
    // An op whose item is gone still counts (as the LEFT JOIN did).
    s.conn()
        .execute(
            "INSERT INTO outbox (local_id, op, not_before) VALUES ('gone', 'read', 5)",
            [],
        )
        .unwrap();
    assert_eq!(s.earliest_due(), Some(5));
}

#[test]
fn saved_locally_is_what_a_save_stores() {
    let s = Store::open_in_memory().unwrap();
    let a = s.create_draft(Kind::Fragment, "draft", 0).unwrap();
    let n = s.create_scratch(Kind::Fragment, "note").unwrap();
    for id in [&a, &n] {
        let before = s.item(id).unwrap();
        let guess = saved_locally(&before, "new words");
        s.save(id, "new words", 0).unwrap();
        let after = s.item(id).unwrap();
        assert_eq!(guess.content_md, after.content_md);
        assert_eq!(guess.dirty, after.dirty);
        assert_eq!(guess.pending_sync, after.pending_sync);
        assert_eq!(guess.status, after.status);
        assert!(guess.updated >= before.updated);
        let same = saved_locally(&after, "new words");
        assert_eq!(same.updated, after.updated);
    }
}

#[test]
fn reading_with_id_is_reading_filtered() {
    let s = Store::open_in_memory().unwrap();
    s.replace_subscriptions(&crate::synth::subscriptions())
        .unwrap();
    let mut rows = crate::synth::reading(600, 4);
    // A duplicate of a post through another subscription, under another id.
    let mut dup = rows[10].clone();
    dup.subscription_id = "S16".into();
    dup.remote_id = "rss-guid".into();
    dup.version += 1;
    rows.push(dup);
    s.merge_reading(&rows, true, &crate::synth::kinds())
        .unwrap();
    let all = s.reading();
    for id in ["r000010", "rss-guid", "r000003", "r000129", "nope"] {
        let want: Vec<_> = all.iter().filter(|r| r.remote_id == id).cloned().collect();
        assert_eq!(s.reading_with_id(id), want, "{id}");
    }
}
