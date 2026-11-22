//! --- newer contract --- The shelf on a blyg whose contract is newer than
//! Burrow knows: read and shown here as the union, never written to the
//! blyg, and changes made here wait, queued, for a Burrow that knows the
//! contract. On real SQLite; the API points at a closed port, so any write
//! would fail the test.

use crate::api::auth::Credential;
use crate::api::{Api, Prune};
use crate::shelf::{BASE_META, Entry, Remote, SERVED_META};
use crate::store::{OpKind, Store};
use crate::sync::{Engine, SyncOptions};

/// A blyg that offers the shelf, with A shelved here and the change queued.
fn engine() -> Engine {
    let store = Store::open_in_memory().unwrap();
    store.set_meta(SERVED_META, "1").unwrap();
    store
        .set_entry_on_shelf(&Entry::new("s1", "A"), true)
        .unwrap();
    store.queue_shelf().unwrap();
    let api = Api::new("http://127.0.0.1:9", Credential::Token("t".into()));
    Engine::new(store, api, SyncOptions::default())
}

#[test]
fn a_shelf_change_waits_while_the_contract_is_newer() {
    let e = engine();
    e.store.set_lists_prune(Prune::Withheld);
    assert!(e.next_op(None, true).unwrap().is_none(), "held");
    assert!(e.earliest_due().is_none(), "the worker doesn't wake for it");
    assert!(e.store.shelf_waiting(), "still queued");
    // A Burrow that knows the contract sends it.
    e.store.set_lists_prune(Prune::Allowed);
    assert_eq!(
        e.next_op(None, true).unwrap().map(|o| o.kind),
        Some(OpKind::Shelf)
    );
}

#[test]
fn a_newer_contracts_shelf_shows_the_union_and_sends_nothing() {
    let e = engine();
    e.store.set_lists_prune(Prune::Withheld);
    let server = Remote {
        entries: vec![Entry::new("s1", "B")],
        revision: 3,
    };
    assert!(e.show_shelf_union(&server).unwrap());
    let shelf = e.store.shelf_wire();
    assert!(shelf.contains(&Entry::new("s1", "A")), "{shelf:?}");
    assert!(shelf.contains(&Entry::new("s1", "B")), "{shelf:?}");
    assert_eq!(e.store.meta(BASE_META), None, "nothing agreed with the blyg");
    assert!(e.store.shelf_waiting(), "A's change still waits to be sent");
}
