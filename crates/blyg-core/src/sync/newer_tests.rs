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
    assert_eq!(
        e.store.meta(BASE_META),
        None,
        "nothing agreed with the blyg"
    );
    assert!(e.store.shelf_waiting(), "A's change still waits to be sent");
}

/// A contact a newer studio's book lacks is kept by its id when it has one
/// (a renamed handle is the same contact, not a second one), else by its
/// handle.
#[test]
fn a_contact_kept_on_a_newer_contract_is_matched_by_id_then_handle() {
    use crate::contacts::Contact;
    use crate::sync::newer::{keep_held, same_contact};
    let c = |id: &str, handle: &str| Contact {
        handle: handle.into(),
        name: None,
        origin: "https://lin.example.org".into(),
        id: id.into(),
        ids: vec![],
    };
    let held = vec![c("c1", "lin"), c("", "ade"), c("c3", "kit")];
    let fresh = vec![c("c1", "lin2"), c("c9", "ade")];
    let kept = keep_held(&held, fresh, same_contact);
    let handles: Vec<&str> = kept.iter().map(|c| c.handle.as_str()).collect();
    assert_eq!(handles, ["lin2", "ade", "kit"]);
}

/// A removal made here sticks: what the blyg last agreed (the base) and
/// this side took off stays off, though the blyg still lists it. What the
/// blyg dropped and this side holds stays (no removal on its word).
#[test]
fn a_shelf_removal_here_sticks_on_a_newer_contract() {
    let e = engine();
    e.store.set_lists_prune(Prune::Withheld);
    let (a, b, c) = (
        Entry::new("s1", "A"),
        Entry::new("s1", "B"),
        Entry::new("s1", "C"),
    );
    // Agreed A and C; C taken off here since.
    let base = serde_json::to_string(&[a.clone(), c.clone()]).unwrap();
    e.store.set_meta(BASE_META, &base).unwrap();
    let server = Remote {
        entries: vec![c.clone(), b.clone()],
        revision: 4,
    };
    e.show_shelf_union(&server).unwrap();
    let shelf = e.store.shelf_wire();
    assert!(!shelf.contains(&c), "C stays off: {shelf:?}");
    assert!(shelf.contains(&a), "A, dropped there, stays: {shelf:?}");
    assert!(shelf.contains(&b), "B, added there, shows: {shelf:?}");
    assert_eq!(e.store.meta(BASE_META), Some(base), "the base is untouched");
}
