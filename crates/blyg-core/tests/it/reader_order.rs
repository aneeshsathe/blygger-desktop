//! `LiveBackend::save_reader_order`'s gate: the Feed's order goes to the
//! blyg (`PATCH /api/settings {readerOrder}`) only once the blyg's settings
//! were seen to have the key (`reader.order.served` is "1"); before that,
//! or on an older studio, it's kept in the store's meta alone. A real
//! `LiveBackend` on a temp dir with the sync worker off; port 9 is the
//! discard port, so a PATCH that's sent fails, and one that isn't can't.

use blyg_core::order::{READER_ORDER_META, READER_ORDER_SERVED_META, ReaderOrder};
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

#[test]
fn before_the_blyg_is_seen_to_keep_it_the_order_stays_local() {
    let dir = tempfile::tempdir().unwrap();
    let b = backend(dir.path());
    assert_eq!(b.reader_order(), ReaderOrder::PaceLayered, "the default");
    // Never read from the blyg: kept here, nothing sent.
    assert_eq!(
        b.save_reader_order(ReaderOrder::Newest).unwrap(),
        ReaderOrder::Newest
    );
    assert_eq!(b.reader_order(), ReaderOrder::Newest);
    // A blyg that answered without the key (older than studio 0.68): the same.
    let store = Store::open(&dir.path().join("blygger.db")).unwrap();
    store.set_meta(READER_ORDER_SERVED_META, "0").unwrap();
    assert_eq!(
        b.save_reader_order(ReaderOrder::PaceLayered).unwrap(),
        ReaderOrder::PaceLayered
    );
    assert_eq!(
        store.meta(READER_ORDER_META).as_deref(),
        Some("pace-layered")
    );
}

#[test]
fn once_the_blyg_keeps_it_the_order_is_sent_and_kept_only_on_success() {
    let dir = tempfile::tempdir().unwrap();
    let b = backend(dir.path());
    let store = Store::open(&dir.path().join("blygger.db")).unwrap();
    store.set_meta(READER_ORDER_META, "pace-layered").unwrap();
    store.set_meta(READER_ORDER_SERVED_META, "1").unwrap();
    // Sent (to the discard port, so it fails), and not kept locally.
    assert!(b.save_reader_order(ReaderOrder::Newest).is_err());
    assert_eq!(b.reader_order(), ReaderOrder::PaceLayered);
}
