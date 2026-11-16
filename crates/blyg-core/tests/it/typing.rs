//! `Backend::save_typing`: a keystroke's save, written by the live
//! backend's own thread. Every read through the backend sees it at once,
//! and it lands in the store in order. A real `LiveBackend` on a temp dir
//! with the sync worker off (port 9 is never dialled).

use std::time::{Duration, Instant};

use blyg_core::store::Store;
use blyg_core::*;

fn backend(dir: &std::path::Path) -> LiveBackend {
    LiveBackend::open_with(
        dir,
        "http://127.0.0.1:9",
        "unused-test-credential",
        SyncOptions {
            start_worker: false,
            ..SyncOptions::default()
        },
    )
    .unwrap()
}

/// What the store itself holds, read through a second connection.
fn stored(dir: &std::path::Path, id: &LocalId) -> Item {
    Store::open(&dir.join(live::DB_FILE))
        .unwrap()
        .item(id)
        .unwrap()
}

fn eventually(mut f: impl FnMut() -> bool) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(10), "never happened");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn typed_text_is_read_back_at_once_and_lands_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let b = backend(dir.path());
    let id = b.create_draft(Kind::Fragment, "").unwrap();
    let mut text = String::new();
    for c in "the quiet harbour".chars() {
        text.push(c);
        b.save_typing(&id, &text).unwrap();
        let it = b.item(&id).unwrap();
        assert_eq!(it.content_md, text);
        assert!(it.dirty && it.pending_sync);
    }
    eventually(|| stored(dir.path(), &id).content_md == text);
    // One create, still waiting: the keystrokes coalesced into it.
    assert_eq!(b.sync_status(), SyncStatus::Saving);
}

#[test]
fn other_calls_see_typed_text_first() {
    let dir = tempfile::tempdir().unwrap();
    let b = backend(dir.path());
    let id = b.create_draft(Kind::Fragment, "kettle").unwrap();
    b.save_typing(&id, "kettle on the boil").unwrap();
    // items() and search() write what's pending before they read.
    assert_eq!(
        b.search("boil")
            .into_iter()
            .map(|i| i.local_id)
            .collect::<Vec<_>>(),
        vec![id.clone()]
    );
    assert_eq!(b.items()[0].content_md, "kettle on the boil");
    // A plain save after a typed one is the one that stays.
    b.save_typing(&id, "typed").unwrap();
    b.save(&id, "saved").unwrap();
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(stored(dir.path(), &id).content_md, "saved");
    assert_eq!(b.item(&id).unwrap().content_md, "saved");
}

#[test]
fn a_scratch_note_typed_into_stays_local() {
    let dir = tempfile::tempdir().unwrap();
    let b = backend(dir.path());
    let id = b.create_scratch(Kind::Fragment, "note").unwrap();
    b.save_typing(&id, "note to self").unwrap();
    let it = b.item(&id).unwrap();
    assert_eq!(it.content_md, "note to self");
    assert!(!it.pending_sync);
    eventually(|| stored(dir.path(), &id).content_md == "note to self");
    assert_eq!(b.sync_status(), SyncStatus::Synced);
}

#[test]
fn closing_the_backend_writes_what_was_typed() {
    let dir = tempfile::tempdir().unwrap();
    let id = {
        let b = backend(dir.path());
        let id = b.create_draft(Kind::Thread, "a").unwrap();
        b.save_typing(&id, "ab").unwrap();
        b.save_typing(&id, "abc").unwrap();
        id
    };
    assert_eq!(stored(dir.path(), &id).content_md, "abc");
}
