use super::*;
use std::time::Instant;

fn store() -> Store {
    let s = Store::open_in_memory().unwrap();
    // A blyg whose contract a sync has checked: its lists prune.
    s.set_lists_prune(crate::api::Prune::Allowed);
    s
}

#[test]
fn migrations_set_user_version() {
    let s = store();
    assert_eq!(s.schema_version().unwrap(), schema::latest());
}

#[test]
fn reopen_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("blygger.db");
    let id = Store::open(&path)
        .unwrap()
        .create_draft(Kind::Fragment, "persist me", 0)
        .unwrap();
    let s = Store::open(&path).unwrap();
    assert_eq!(s.item(&id).unwrap().content_md, "persist me");
    assert_eq!(s.pending_count(), 1);
    let mode: String = s
        .conn()
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode.to_lowercase(), "wal");
}

#[test]
fn search_is_case_insensitive_substring() {
    let s = store();
    let a = s
        .create_draft(Kind::Fragment, "Walking by the lake at Montreux", 0)
        .unwrap();
    let b = s
        .create_draft(Kind::Thread, "# Notes\nthe MONTH of rain", 0)
        .unwrap();
    let _c = s.create_draft(Kind::Fragment, "nothing here", 0).unwrap();

    let ids = |q: &str| {
        s.search(q)
            .into_iter()
            .map(|i| i.local_id)
            .collect::<Vec<_>>()
    };
    assert_eq!(ids("mont").len(), 2);
    assert_eq!(ids("montreux"), vec![a.clone()]);
    assert_eq!(ids("MONTR"), vec![a.clone()]);
    assert_eq!(ids("ontre"), vec![a.clone()], "mid-word substring");
    assert_eq!(ids("month of"), vec![b.clone()], "phrase with a space");
    assert!(ids("zzz").is_empty());
    // short (non-trigram) queries
    assert_eq!(ids("mo").len(), 2);
    assert_eq!(ids("Wa"), vec![a.clone()]);
    // quotes don't break the MATCH syntax
    assert!(ids("\"quoted\"").is_empty());
    // empty = everything, newest-updated first
    assert_eq!(s.search("  ").len(), 3);
    // edits are reindexed (a tick later: `updated` has millisecond precision,
    // and a tie with _c's creation would put _c first)
    crate::wait::wall_clock(std::time::Duration::from_millis(3));
    s.save(&b, "renamed entirely", 0).unwrap();
    assert_eq!(ids("mont"), vec![a.clone()]);
    assert_eq!(ids("entire"), vec![b.clone()]);
    // order matches items(): b was just edited so it's first
    assert_eq!(s.items()[0].local_id, b);
    s.delete_item(&a).unwrap();
    assert!(ids("montreux").is_empty());
}

#[test]
fn search_unicode_case_folding() {
    let s = store();
    let a = s
        .create_draft(Kind::Fragment, "Ärger in Zürich", 0)
        .unwrap();
    assert_eq!(s.search("zürich").len(), 1);
    assert_eq!(s.search("äRGER")[0].local_id, a);
}

fn ids_of(items: Vec<Item>) -> Vec<LocalId> {
    items.into_iter().map(|i| i.local_id).collect()
}

/// Typing a search a key at a time (the short keys read the snapshot, so
/// the longer ones resolve their trigram hits against it: `fts_in`):
/// every prefix typed, and every backspace, finds exactly what a store
/// that never searched before finds (reading the rows: `fts_rows`), in
/// the same order.
#[test]
fn search_typed_a_key_at_a_time_finds_what_a_fresh_search_does() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("blygger.db");
    let s = Store::open(&path).unwrap();
    for md in [
        "Garden notes, spring",
        "the GARDEN in winter: notes",
        "gardening",
        "Ärger im Garten",
        "a garden \"quoted\" note",
        "nothing to see",
        "",
    ] {
        s.create_draft(Kind::Fragment, md, 0).unwrap();
    }
    for typed in ["garden notes", "Ärger", "\"quoted\" n", "gart"] {
        let mut q = String::new();
        let mut steps: Vec<String> = Vec::new();
        for c in typed.chars() {
            q.push(c);
            steps.push(q.clone());
        }
        // ...then backspace back to one character.
        while q.chars().count() > 1 {
            q.pop();
            steps.push(q.clone());
        }
        for q in steps {
            let fresh = Store::open(&path).unwrap();
            assert_eq!(ids_of(s.search(&q)), ids_of(fresh.search(&q)), "{q:?}");
        }
    }
}

/// A search against the snapshot sees every commit: an edit or delete
/// here, a write from another connection (the sync worker's), on a file
/// store and an in-memory one. (`items()` before each search makes the
/// snapshot current, so the search resolves against it.)
#[test]
fn search_against_the_snapshot_sees_edits_and_syncs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("blygger.db");
    let s = Store::open(&path).unwrap();
    let a = s.create_draft(Kind::Fragment, "garden a", 0).unwrap();
    let b = s.create_draft(Kind::Fragment, "lake b", 0).unwrap();
    s.items();
    assert_eq!(ids_of(s.search("gar")), vec![a.clone()]);
    // An edit makes b match.
    s.save(&b, "garden b", 0).unwrap();
    s.items();
    let hits = ids_of(s.search("gard"));
    assert!(hits.contains(&b), "{hits:?}");
    // Another connection adds c and removes a.
    let other = Store::open(&path).unwrap();
    let c = other.create_draft(Kind::Fragment, "garden c", 0).unwrap();
    s.items();
    let hits = ids_of(s.search("garde"));
    assert!(hits.contains(&c), "{hits:?}");
    other.delete_item(&a).unwrap();
    s.items();
    let hits = ids_of(s.search("garden"));
    assert!(!hits.contains(&a) && hits.len() == 2, "{hits:?}");
    // And an edit that makes a hit stop matching.
    s.save(&b, "gone", 0).unwrap();
    s.items();
    assert_eq!(ids_of(s.search("garden")), vec![c.clone()]);

    let m = store();
    let x = m.create_draft(Kind::Fragment, "garden x", 0).unwrap();
    let y = m.create_draft(Kind::Fragment, "lake y", 0).unwrap();
    assert_eq!(ids_of(m.search("ga")), vec![x.clone()]);
    m.save(&y, "garden y", 0).unwrap();
    m.items();
    assert!(ids_of(m.search("gar")).contains(&y));
    m.delete_item(&x).unwrap();
    m.items();
    assert_eq!(ids_of(m.search("gard")), vec![y]);
}

/// A snapshot taken before a write, and a trigram query that already
/// sees it (a hit the snapshot lacks, one it still holds, an edit the
/// index hasn't caught up with): the search drops nothing and keeps
/// nothing gone, and finds what a fresh store finds.
#[test]
fn a_search_against_a_snapshot_older_than_the_store_finds_what_a_fresh_store_does() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("blygger.db");
    let s = Store::open(&path).unwrap();
    let a = s.create_draft(Kind::Fragment, "garden a", 0).unwrap();
    let b = s.create_draft(Kind::Fragment, "lake b", 0).unwrap();
    let other = Store::open(&path).unwrap();
    type Write<'a> = Box<dyn Fn(&Store) + 'a>;
    let writes: Vec<(&str, Write)> = vec![
        (
            "added",
            Box::new(|o| {
                o.create_draft(Kind::Fragment, "garden c", 0).unwrap();
            }),
        ),
        ("edited", Box::new(|o| o.save(&b, "garden b", 0).unwrap())),
        ("deleted", Box::new(|o| o.delete_item(&a).unwrap())),
    ];
    for (what, write) in writes {
        let snap = s.snapshot().unwrap();
        write(&other);
        let fresh = Store::open(&path).unwrap();
        assert_eq!(
            ids_of(s.fts_in("garden", &snap).unwrap()),
            ids_of(fresh.search("garden")),
            "{what}"
        );
    }

    // In memory: the only connection's own write.
    let m = store();
    let x = m.create_draft(Kind::Fragment, "garden x", 0).unwrap();
    let y = m.create_draft(Kind::Fragment, "lake y", 0).unwrap();
    let snap = m.snapshot().unwrap();
    m.save(&y, "garden y", 0).unwrap();
    m.delete_item(&x).unwrap();
    assert_eq!(ids_of(m.fts_in("garden", &snap).unwrap()), vec![y]);
}

/// What deleting, withdrawing, a sync dropping a post and an edit leave in
/// the trigram tables never sends a search to the slow path (reading
/// each matching row, about 80 VM steps a hit): once the snapshot is
/// current, a search still costs a few steps a hit. A delete takes the
/// row out of `items_fts` and `items_fts_stale` in the same statement
/// (`items_ad`); a withdrawal and an edit keep the row (an edit notes it
/// in `items_fts_stale`, and the snapshot holds it).
#[test]
fn a_search_after_deletes_withdrawals_syncs_and_edits_stays_on_the_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("blygger.db")).unwrap();
    s.set_lists_prune(crate::api::Prune::Allowed);
    let wire = |i: usize, status: &str, md: String| WireItem {
        id: format!("S{i}"),
        kind: "fragment".into(),
        status: status.into(),
        version: 1,
        created: "2030-01-01T00:00:00Z".into(),
        updated: format!("2030-01-01T00:{:02}:{:02}Z", i / 60, i % 60),
        content_md: md,
        ..Default::default()
    };
    let n = 200;
    let all: Vec<WireItem> = (0..n)
        .map(|i| wire(i, "public", format!("a garden post, number {i}")))
        .collect();
    s.merge_all(&all).unwrap();
    let draft = s.create_draft(Kind::Fragment, "a garden draft", 0).unwrap();
    let sid = |i: usize| s.local_id_for_server(&format!("S{i}")).unwrap();

    // Deleted here: a draft, and a synced post.
    s.delete_item(&draft).unwrap();
    s.delete_item(&sid(4)).unwrap();
    // Edited (noted in items_fts_stale), then deleted while still noted.
    s.save(&sid(5), "a garden, edited", 0).unwrap();
    s.save(&sid(6), "a garden, edited then deleted", 0).unwrap();
    s.delete_item(&sid(6)).unwrap();
    // A pull: S2 withdrawn, S3 gone from the server (dropped here).
    let pulled: Vec<WireItem> = all
        .iter()
        .filter(|w| w.id != "S3" && w.id != "S4" && w.id != "S6")
        .map(|w| match w.id.as_str() {
            "S2" => wire(2, "withdrawn", String::new()),
            _ => w.clone(),
        })
        .collect();
    s.merge_all(&pulled).unwrap();
    assert!(s.local_id_for_server("S3").is_none(), "the pull dropped S3");

    s.items();
    let work = s.count_work();
    let found = s.search("garden");
    let steps = work.take();
    let hits = found.len() as u64;
    assert_eq!(hits, n as u64 - 4, "S2 withdrawn, S3, S4 and S6 gone");
    assert_eq!(ids_of(found), ids_of(s.fts_rows("garden").unwrap()));
    let limit = hits * 20 + 500;
    assert!(
        steps <= limit,
        "{steps} VM steps for {hits} hits (limit {limit}): the search read every row"
    );
}

/// Resolving the trigram hits against the snapshot gives exactly what
/// reading the matching rows gives, in the same order: posts edited since
/// they were indexed, and posts whose dates tie.
#[test]
fn trigram_hits_resolved_against_the_snapshot_match_reading_the_rows() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("blygger.db")).unwrap();
    let mut ids = Vec::new();
    for i in 0..30 {
        let md = match i % 3 {
            0 => format!("Garden {i}"),
            1 => format!("the GARDEN path {i}"),
            _ => format!("lake {i}"),
        };
        ids.push(s.create_draft(Kind::Fragment, &md, 0).unwrap());
    }
    // Edited, not yet re-indexed: one starts matching, one stops.
    s.save(&ids[2], "a garden now", 0).unwrap();
    s.save(&ids[0], "no longer", 0).unwrap();
    // Dates that tie: the order falls back to what comes after them.
    s.conn()
        .execute("UPDATE items SET updated = '2026-01-01T00:00:00.000Z'", [])
        .unwrap();
    for q in ["garden", "GARDEN", "den p", "arden 1", "lake"] {
        let snap = s.snapshot().unwrap();
        assert_eq!(
            ids_of(s.fts_in(q, &snap).unwrap()),
            ids_of(s.fts_rows(q).unwrap()),
            "{q:?}"
        );
    }
}

#[test]
fn save_coalesces_into_one_op() {
    let s = store();
    let id = s.create_draft(Kind::Fragment, "", 800).unwrap();
    for i in 0..50 {
        s.save(&id, &"x".repeat(i + 1), 800).unwrap();
    }
    let ops = s.ops().unwrap();
    assert_eq!(ops.len(), 1, "create absorbs saves");
    assert_eq!(ops[0].kind, OpKind::Create);
    // once the create has landed, 50 more saves are one save
    s.op_created(ops[0].seq, &id, "SID1", "x", Kind::Fragment, 1)
        .unwrap();
    for i in 0..50 {
        s.save(&id, &"y".repeat(i + 1), 800).unwrap();
    }
    let ops = s.ops().unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].kind, OpKind::Save);
    // a save while that op is in flight queues a new one
    s.set_in_flight(ops[0].seq, true).unwrap();
    s.save(&id, "z", 800).unwrap();
    assert_eq!(s.ops().unwrap().len(), 2);
}

#[test]
fn a_kind_change_lands_in_place() {
    let s = store();
    let id = s.create_draft(Kind::Fragment, "a", 0).unwrap();
    let seq = s.ops().unwrap()[0].seq;
    s.op_created(seq, &id, "S1", "a", Kind::Fragment, 1)
        .unwrap();
    s.set_kind(&id, Kind::Thread, 0).unwrap();
    let ops = s.ops().unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].kind, OpKind::Recreate);
    s.op_kind_saved(ops[0].seq, &id, "a", Kind::Thread, 2)
        .unwrap();
    assert!(s.ops().unwrap().is_empty());
    let it = s.item(&id).unwrap();
    assert_eq!(it.server_id.unwrap().0, "S1", "same server draft");
    assert_eq!(it.kind, Kind::Thread);
    assert!(!it.pending_sync);
}

#[test]
fn retirement_ops_are_not_local_edits() {
    let s = store();
    let id = s.create_draft(Kind::Fragment, "a", 0).unwrap();
    let seq = s.ops().unwrap()[0].seq;
    s.op_created(seq, &id, "NEW", "a", Kind::Fragment, 1)
        .unwrap();
    // A retirement queued by a build from before studio 0.9, when a kind
    // change made a new draft and deleted the old one.
    s.conn()
        .execute(
            "INSERT INTO outbox (local_id, op, payload, not_before) VALUES (?1, 'delete_remote', 'OLD', 0)",
            [&id.0],
        )
        .unwrap();
    // only the delete of OLD is queued: not an unpushed edit
    let ops = s.ops().unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].kind, OpKind::DeleteRemote);
    assert_eq!(ops[0].payload, "OLD");
    assert!(!s.item(&id).unwrap().pending_sync);
    // a save must queue its own op, not coalesce into the delete
    s.save(&id, "b", 0).unwrap();
    let kinds: Vec<_> = s.ops().unwrap().into_iter().map(|o| o.kind).collect();
    assert_eq!(kinds, vec![OpKind::DeleteRemote, OpKind::Save]);
    assert!(s.item(&id).unwrap().pending_sync);
    // deleting the item locally keeps the retirement op
    s.delete_item(&id).unwrap();
    assert_eq!(s.ops().unwrap().len(), 1);
}

#[test]
fn bench_search_5000() {
    let s = store();
    let words = [
        "alpine", "lake", "Montreux", "jazz", "river", "sparrow", "ledger", "quartz", "orbit",
        "fennel",
    ];
    {
        let mut c = s.conn();
        let tx = c.transaction().unwrap();
        for i in 0..5000usize {
            let body = format!(
                "Item {i} {} {} {}\n\n{}",
                words[i % 10],
                words[(i * 7) % 10],
                if i % 250 == 0 { "zebracorn" } else { "plain" },
                "lorem ipsum dolor sit amet ".repeat(20)
            );
            tx.execute(
                "INSERT INTO items (local_id, kind, status, content_md, created, updated) VALUES (?1, 'fragment', 'draft', ?2, ?3, ?3)",
                params![format!("L{i}"), body, crate::util::iso_from_ms(1_700_000_000_000 + i as i64)],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    }
    // A search goes through the trigram index: its work is the 20 hits,
    // not the 5,000 items. Measured against one plain scan of them.
    let work = s.count_work();
    s.conn()
        .query_row(
            "SELECT count(*) FROM items WHERE content_md LIKE '%zebracorn%'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap();
    let scan = work.take();
    for q in ["zebracorn", "ebraco"] {
        assert_eq!(s.search(q).len(), 20);
        let steps = work.take();
        eprintln!("search {q:?} over 5000 items: {steps} VM steps (a scan: {scan})");
        assert!(steps < scan / 2, "search {q:?} took {steps} VM steps");
    }

    let n = 50;
    let t = Instant::now();
    let all = s.items();
    eprintln!("items() over 5000 items, cold: {:?}", t.elapsed());
    assert_eq!(all.len(), 5000);
    work.take();
    let t = Instant::now();
    for _ in 0..n {
        assert_eq!(s.items().len(), 5000);
    }
    eprintln!("items() over 5000 items, cached: {:?}", t.elapsed() / n);
    // Cached: no reads but the commit stamp.
    let steps = work.take() / n as u64;
    assert!(steps < 50, "a cached items() took {steps} VM steps");
    // the cache is invalidated by writes
    let id = s.create_draft(Kind::Fragment, "fresh", 0).unwrap();
    assert_eq!(s.items()[0].local_id, id);
    s.save(&id, "fresher", 0).unwrap();
    assert_eq!(s.items()[0].content_md, "fresher");
}

fn generated(model: &str) -> Option<ScopeProvenance> {
    Some(ScopeProvenance {
        model: model.into(),
        sources: vec![],
        pages: vec![],
        at: None,
        ..Default::default()
    })
}

#[test]
fn tracked_provenance_follows_its_scopes() {
    let s = store();
    let text = "[TK]one[=]generated words[/TK]";
    let id = s.create_draft(Kind::Fragment, "x", 0).unwrap();
    s.save_inner(&id, text, Some(&[generated("m")]), 0).unwrap();
    let t = s.provenance(&id);
    assert!(t.dirty && t.any());
    assert_eq!(t.keyed_to.as_deref(), Some(text));
    s.prov_pushed(&id, text, &[generated("m")]).unwrap();
    assert!(!s.provenance(&id).dirty);

    // Typing outside any scope: nothing to re-push.
    let more = format!("{text} and more");
    s.save(&id, &more, 0).unwrap();
    assert!(!s.provenance(&id).dirty);

    // A scope typed in front shifts the generated one to position 1.
    let shifted = format!("[TK]mine[=]by hand[/TK] {more}");
    s.save(&id, &shifted, 0).unwrap();
    let t = s.provenance(&id);
    assert!(t.dirty, "needs the combined push");
    assert_eq!(t.scopes.unwrap(), vec![None, generated("m")]);

    // Mid-typing malformed text keeps the last good keying.
    s.save(&id, &format!("{shifted} [TK]half"), 0).unwrap();
    assert_eq!(
        s.provenance(&id).keyed_to.as_deref(),
        Some(shifted.as_str())
    );

    // A pushed result only sticks if the text didn't move on meanwhile.
    s.prov_pushed(&id, &shifted, &[None, generated("m")])
        .unwrap();
    assert!(s.provenance(&id).dirty);
}

#[test]
fn a_server_edit_forgets_tracked_provenance() {
    let s = store();
    let id = s.create_draft(Kind::Fragment, "x", 0).unwrap();
    let text = "[TK]one[=]generated[/TK]";
    s.save_inner(&id, text, Some(&[generated("m")]), 0).unwrap();
    s.conn().execute("DELETE FROM outbox", []).unwrap();
    s.conn()
        .execute(
            "UPDATE items SET server_id = 'S1' WHERE local_id = ?1",
            [&id.0],
        )
        .unwrap();
    let w = WireItem {
        id: "S1".into(),
        kind: "fragment".into(),
        status: "draft".into(),
        version: 0,
        dirty: true,
        created: String::new(),
        updated: String::new(),
        content_md: "rewritten in the studio".into(),
        ..Default::default()
    };
    s.merge_all(&[w]).unwrap();
    assert_eq!(s.item(&id).unwrap().content_md, "rewritten in the studio");
    assert_eq!(s.provenance(&id), Tracked::default());
}

#[test]
fn public_origin_comes_from_permalinks() {
    let s = store();
    assert_eq!(s.public_origin(), None);
    let id = s.create_draft(Kind::Fragment, "x", 0).unwrap();
    s.after_publish(&id, 1, "https://x.example/blyg/f/abc", "x")
        .unwrap();
    assert_eq!(
        s.public_origin().as_deref(),
        Some("https://x.example/blyg/")
    );
}

// --- reading ---
#[test]
fn a_stub_is_a_draft_thread_with_stub_of() {
    let s = store();
    let of = RemoteRef {
        origin: "https://rue.blyg.example.com/".into(),
        id: "01K2RUE0TRUST0000000000001".into(),
        version: 5,
        cited: None,
    };
    let id = s
        .create_stub(&of, "![[01K2RUE0TRUST0000000000001]]\n\n", 0)
        .unwrap();
    let it = s.item(&id).unwrap();
    assert_eq!(it.kind, Kind::Thread);
    assert_eq!(it.status, Status::Draft);
    assert_eq!(it.stub_of, Some(of));
    let ops = s.ops().unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].kind, OpKind::Create, "pushed like any draft");
}

/// Discard changes (`Backend::restore` of the published version): the
/// server's copy replaces the working copy, and a save still waiting in
/// the outbox goes with it, so the old text can't be pushed back after.
#[test]
fn discard_changes_drops_the_waiting_save() {
    let s = store();
    let id = s.create_draft(Kind::Fragment, "as published", 0).unwrap();
    s.conn().execute("DELETE FROM outbox", []).unwrap();
    s.conn()
        .execute(
            "UPDATE items SET server_id = 'S1' WHERE local_id = ?1",
            [&id.0],
        )
        .unwrap();
    s.after_publish(&id, 1, "https://blyg.example.com/f/S1", "as published")
        .unwrap();
    // An edit, saved locally; its push waits (debounce).
    s.save(&id, "an edit to throw away", 60_000).unwrap();
    let it = s.item(&id).unwrap();
    assert!(it.dirty && it.pending_sync);
    assert_eq!(s.pending_count(), 1);
    let restored = |dirty: bool| WireItem {
        id: "S1".into(),
        kind: "fragment".into(),
        status: "public".into(),
        version: 1,
        dirty,
        created: String::new(),
        updated: String::new(),
        content_md: "as published".into(),
        ..Default::default()
    };
    s.replace_from_server(&id, &restored(false)).unwrap();
    let it = s.item(&id).unwrap();
    assert_eq!(it.content_md, "as published");
    assert!(!it.dirty, "clean again");
    assert!(!it.pending_sync);
    assert_eq!(s.pending_count(), 0, "the waiting save is gone");

    // A version with generated text stays `dirty` on the blyg (upstream's
    // `restoreVersion`): the store keeps the server's word for it.
    s.save(&id, "another edit", 60_000).unwrap();
    s.replace_from_server(&id, &restored(true)).unwrap();
    let it = s.item(&id).unwrap();
    assert_eq!(it.content_md, "as published");
    assert!(it.dirty, "the blyg still says unpublished");
    assert_eq!(s.pending_count(), 0);
}

// --- unicode --- (spec decision #103)

#[test]
fn saving_cleans_controls_and_composes_to_nfc() {
    let s = store();
    // A decomposed é, a NUL, an escape and U+FFFF; tab, LF and CR stay.
    let raw = "Cafe\u{301}\0 \u{1b}[1mbold\u{FFFF}\tend\r\n";
    let want = "Caf\u{e9} [1mbold\tend\r\n";
    let id = s.create_draft(Kind::Fragment, raw, 0).unwrap();
    assert_eq!(s.item(&id).unwrap().content_md, want);
    s.save(&id, &format!("{raw}more\u{7}"), 0).unwrap();
    assert_eq!(s.item(&id).unwrap().content_md, format!("{want}more"));
    // A save that differs only before cleaning changes nothing.
    let before = s.item(&id).unwrap();
    s.save(&id, &format!("{raw}more"), 0).unwrap();
    assert_eq!(s.item(&id).unwrap().updated, before.updated);
    // The optimistic copy matches what the store kept.
    let local = saved_locally(&before, &format!("{raw}again"));
    s.save(&id, &format!("{raw}again"), 0).unwrap();
    assert_eq!(local.content_md, s.item(&id).unwrap().content_md);

    let scratch = s
        .create_scratch(Kind::Fragment, "n\u{0}o\u{308}te")
        .unwrap();
    assert_eq!(s.item(&scratch).unwrap().content_md, "n\u{f6}te");
}

#[test]
fn a_save_checked_against_its_base_compares_the_cleaned_text() {
    let s = store();
    let id = s.create_draft(Kind::Fragment, "e\u{301}", 0).unwrap();
    let base = content_hash("\u{e9}");
    s.save_if_base(&id, "e\u{301}!", &base, 0).unwrap();
    assert_eq!(s.item(&id).unwrap().content_md, "\u{e9}!");
}

// --- studio 0.71 --- an edit typed while the blyg generated

/// After a generate on the blyg, an edit typed during the request waits
/// for the writer's merge, whether or not the blyg's copy could be read
/// back (`answer` is the text the generate answered with, if any). Before
/// the fix the hold was taken only after a successful read-back, so a
/// failed GET let the edit go at once and come back as a conflict.
#[test]
fn an_edit_typed_during_a_generate_waits_for_the_merge() {
    let s = store();
    let id = s
        .create_draft(Kind::Fragment, "Fog. [TK]a line[/TK]", 0)
        .unwrap();
    s.conn().execute("DELETE FROM outbox", []).unwrap();
    s.conn()
        .execute(
            "UPDATE items SET server_id = 'S1' WHERE local_id = ?1",
            [&id.0],
        )
        .unwrap();
    let answer = "Fog. [TK]a line[=]Written.[/TK]";
    let until = now_ms() + 60_000;
    let held = |s: &Store| s.ops().unwrap().iter().all(|o| o.not_before >= until);
    // Nothing typed: nothing to hold.
    assert!(!s.hold_for_merge(&id, Some(answer), until).unwrap());
    // Typed during the request: held, with the answer read or not.
    s.save(&id, "Grey. Fog. [TK]a line[/TK]", 0).unwrap();
    assert!(s.hold_for_merge(&id, Some(answer), until).unwrap());
    assert!(held(&s), "{:?}", s.ops().unwrap());
    s.conn()
        .execute("UPDATE outbox SET not_before = 0", [])
        .unwrap();
    assert!(s.hold_for_merge(&id, None, until).unwrap());
    assert!(held(&s), "held without the answer's text too");
    // The edit already holds the answer: it can go.
    s.conn()
        .execute("UPDATE outbox SET not_before = 0", [])
        .unwrap();
    s.save(&id, answer, 0).unwrap();
    assert!(!s.hold_for_merge(&id, Some(answer), until).unwrap());
}
