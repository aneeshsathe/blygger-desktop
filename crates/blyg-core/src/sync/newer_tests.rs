//! --- newer contract --- The shelf on a blyg whose contract is newer than
//! Burrow knows: read and shown here as the union, never written to the
//! blyg, and changes made here wait, queued, for a Burrow that knows the
//! contract. On real SQLite; the API points at a closed port, so any write
//! would fail the test.

use crate::api::auth::Credential;
use crate::api::{Api, Prune};
use crate::shelf::{BASE_META, Entry, Remote, SERVED_META, SyncState, WRITES_META};
use crate::store::{OpKind, Store};
use crate::sync::{Engine, SyncOptions};

/// A blyg that offers the shelf, with A shelved here and the change queued.
fn engine() -> Engine {
    let store = Store::open_in_memory().unwrap();
    store.set_meta(SERVED_META, "1").unwrap();
    store.set_meta(WRITES_META, "1").unwrap();
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

/// A sign-in that may read Saved (`getShelf`) but not write it
/// (`putShelf`, owner:manage): the change is held here as on a newer
/// contract, and Saved says why rather than "syncing".
#[test]
fn a_read_only_sign_in_holds_a_shelf_change_and_says_so() {
    let e = engine();
    e.store.set_lists_prune(Prune::Allowed);
    assert_eq!(e.shelf_state(), SyncState::Waiting, "a writer sends it");
    e.store.set_meta(WRITES_META, "0").unwrap();
    assert!(e.shelf_served(), "still read");
    assert!(e.next_op(None, true).unwrap().is_none(), "held");
    assert!(e.earliest_due().is_none(), "the worker doesn't wake for it");
    assert!(e.store.shelf_waiting(), "still queued");
    assert_eq!(e.shelf_state(), SyncState::HeldReadOnly);
    e.store.set_meta(WRITES_META, "1").unwrap();
    e.store.set_lists_prune(Prune::Withheld);
    assert_eq!(e.shelf_state(), SyncState::HeldNewer);
}

/// Launched offline with a Saved change waiting: the contract isn't known
/// yet, so the change waits, and Saved says it's syncing, not "until
/// Burrow is updated".
#[test]
fn an_unchecked_contract_holds_a_shelf_change_without_calling_it_newer() {
    let e = engine();
    assert!(!e.store.newer_contract());
    assert!(
        e.next_op(None, true).unwrap().is_none(),
        "held until checked"
    );
    assert_eq!(e.shelf_state(), SyncState::Waiting);
}

/// Read-only on a known contract: the blyg's list is the truth. Another
/// device's removal comes off here (a three-way merge), this side's
/// changes stay here, and nothing is sent.
#[test]
fn a_read_only_saved_takes_another_devices_removal() {
    let e = engine();
    e.store.set_lists_prune(Prune::Allowed);
    e.store.set_meta(WRITES_META, "0").unwrap();
    let (a, b, c) = (
        Entry::new("s1", "A"),
        Entry::new("s1", "B"),
        Entry::new("s1", "C"),
    );
    e.store.set_entry_on_shelf(&b, true).unwrap();
    let base = serde_json::to_string(&[a.clone(), b.clone()]).unwrap();
    e.store.set_meta(BASE_META, &base).unwrap();
    // Another device took B off and added C.
    let server = Remote {
        entries: vec![a.clone(), c.clone()],
        revision: 5,
    };
    e.follow_shelf_read_only(&server).unwrap();
    let shelf = e.store.shelf_wire();
    assert!(!shelf.contains(&b), "B came off: {shelf:?}");
    assert!(shelf.contains(&a) && shelf.contains(&c), "{shelf:?}");
    // This side takes A off: it stays off, on this computer.
    e.store.set_entry_on_shelf(&a, false).unwrap();
    e.follow_shelf_read_only(&server).unwrap();
    let shelf = e.store.shelf_wire();
    assert!(!shelf.contains(&a), "A stays off here: {shelf:?}");
    assert!(shelf.contains(&c));
}

/// A removal made here sticks with no base yet (a fresh install on a newer
/// studio, or a sign-in read-only from the start): what this side took
/// off is kept, not worked out from an agreement that never happened.
#[test]
fn a_saved_removal_with_no_base_stays_off() {
    let a = Entry::new("s1", "A");
    let b = Entry::new("s1", "B");
    let server = Remote {
        entries: vec![a.clone(), b.clone()],
        revision: 2,
    };
    // A newer contract.
    let e = engine();
    e.store.set_lists_prune(Prune::Withheld);
    e.store.set_entry_on_shelf(&a, false).unwrap();
    assert_eq!(e.store.meta(BASE_META), None);
    e.show_shelf_union(&server).unwrap();
    let shelf = e.store.shelf_wire();
    assert!(!shelf.contains(&a), "newer: A stays off: {shelf:?}");
    assert!(shelf.contains(&b));
    // Read-only from the start.
    let e = engine();
    e.store.set_lists_prune(Prune::Allowed);
    e.store.set_meta(WRITES_META, "0").unwrap();
    e.store.set_entry_on_shelf(&a, false).unwrap();
    e.follow_shelf_read_only(&server).unwrap();
    let shelf = e.store.shelf_wire();
    assert!(!shelf.contains(&a), "read-only: A stays off: {shelf:?}");
    assert!(shelf.contains(&b));
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
