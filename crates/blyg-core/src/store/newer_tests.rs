//! A studio on a contract newer than this Burrow knows
//! (`Store::set_lists_prune(Prune::Withheld)`): its lists add and update
//! what's held here, and remove nothing. Hoppers and read state have theirs
//! in `hoppers_tests.rs` and `read_state_tests.rs`.

use std::collections::{HashMap, HashSet};

use super::Store;
use crate::api::Prune;
use crate::api::wire::WireItem;
use crate::model::*;

fn contact(id: &str, handle: &str) -> crate::contacts::Contact {
    crate::contacts::Contact {
        handle: handle.into(),
        name: None,
        origin: "https://lin.example.org".into(),
        id: id.into(),
        ids: vec![],
    }
}

/// The contact book has one writer for a served list, and it takes the
/// decision: on a newer contract what the book lacks stays here. A contact
/// deleted here goes whatever the contract.
#[test]
fn a_served_contact_book_removes_only_when_allowed() {
    let s = Store::open_in_memory().unwrap();
    let handles = |s: &Store| -> Vec<String> { s.contacts().into_iter().map(|c| c.handle).collect() };
    s.take_served_contacts(vec![contact("c1", "lin"), contact("c2", "ade")], Prune::Allowed)
        .unwrap();
    s.take_served_contacts(vec![contact("c1", "lin")], Prune::Withheld)
        .unwrap();
    assert_eq!(handles(&s), ["lin", "ade"]);
    s.take_served_contacts(vec![contact("c1", "lin")], Prune::Allowed)
        .unwrap();
    assert_eq!(handles(&s), ["lin"]);
    s.drop_contact("c1").unwrap();
    assert!(s.contacts().is_empty());
}

fn wire(id: &str, md: &str) -> WireItem {
    WireItem {
        id: id.into(),
        kind: "fragment".into(),
        status: "draft".into(),
        version: 0,
        created: "2030-01-01T00:00:00Z".into(),
        updated: "2030-01-01T00:00:00Z".into(),
        content_md: md.into(),
        ..Default::default()
    }
}

fn ri(rid: &str, thumb: Option<i8>) -> ReadingItem {
    ReadingItem {
        subscription_id: "s1".into(),
        remote_id: rid.into(),
        subscription_title: "s1".into(),
        origin: "https://a.example/".into(),
        kind: Kind::Fragment,
        state: "current".into(),
        version: 1,
        created: None,
        updated: None,
        observed_at: "2030-01-01T00:00:00Z".into(),
        content_md: format!("{rid} text"),
        content_html: String::new(),
        author: None,
        page: None,
        thumb,
        hoppers: vec![],
        pinned_version_retained: None,
        read_version: None,
        stub_of: None,
        forked_from: None,
        transclusions: vec![],
        lineage_known: false,
        source_url: None,
        generated: None,
        background: None,
    }
}

fn kinds() -> HashMap<SubId, SubscriptionKind> {
    [("s1".into(), SubscriptionKind::Blyg)].into()
}

#[test]
fn your_items_stay_when_a_newer_contracts_list_lacks_them() {
    let s = Store::open_in_memory().unwrap();
    s.merge_all(&[wire("S1", "one"), wire("S2", "two")])
        .unwrap();
    s.set_lists_prune(Prune::Withheld);
    s.merge_all(&[wire("S1", "one, edited there")]).unwrap();
    assert!(s.local_id_for_server("S2").is_some(), "S2 kept");
    let s1 = s.local_id_for_server("S1").unwrap();
    assert_eq!(s.item(&s1).unwrap().content_md, "one, edited there");
    s.set_lists_prune(Prune::Allowed);
    s.merge_all(&[wire("S1", "one, edited there")]).unwrap();
    assert!(
        s.local_id_for_server("S2").is_none(),
        "a known contract prunes"
    );
}

#[test]
fn reading_rows_stay_when_a_newer_contracts_walk_lacks_them() {
    let s = Store::open_in_memory().unwrap();
    s.merge_reading(&[ri("A", None), ri("B", None)], true, &kinds())
        .unwrap();
    s.set_lists_prune(Prune::Withheld);
    assert!(!s.prune_reading_except(&HashSet::new()).unwrap());
    assert!(!s.merge_reading(&[], true, &kinds()).unwrap());
    assert_eq!(s.reading().len(), 2);
}

#[test]
fn thumbs_stay_when_a_newer_contracts_signals_lack_them() {
    let s = Store::open_in_memory().unwrap();
    s.merge_reading(&[ri("A", Some(1)), ri("B", None)], true, &kinds())
        .unwrap();
    s.set_lists_prune(Prune::Withheld);
    let list = vec![("s1".to_string(), "B".to_string(), -1i8)];
    assert!(s.replace_thumbs(&list).unwrap(), "B's thumb lands");
    let thumbs: HashMap<String, Option<i8>> = s
        .reading()
        .into_iter()
        .map(|r| (r.remote_id.to_string(), r.thumb))
        .collect();
    assert_eq!(thumbs["A"], Some(1), "A's kept");
    assert_eq!(thumbs["B"], Some(-1));
}

/// A store removes nothing on a list's word until a sync has checked the
/// blyg's contract (`Engine::check_server` allows it).
#[test]
fn a_store_removes_nothing_until_the_contract_is_checked() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("blygger.db")).unwrap();
    assert_eq!(s.lists_prune(), Prune::Withheld);
    assert_eq!(
        Store::open_in_memory().unwrap().lists_prune(),
        Prune::Withheld
    );
    s.merge_reading(&[ri("A", None)], true, &kinds()).unwrap();
    assert!(!s.prune_reading_except(&HashSet::new()).unwrap());
    assert_eq!(s.reading().len(), 1);
    // Not checked isn't newer: nothing says "until Burrow is updated".
    assert!(!s.newer_contract());
    s.set_lists_prune(Prune::Withheld);
    assert!(s.newer_contract(), "a check that found a newer contract");
    s.set_lists_prune(Prune::Allowed);
    assert!(!s.newer_contract());
}

/// A check from an earlier pull is stale: a removal it would allow is held
/// back (`take_held_back`) until the contract is read again, so a studio
/// upgraded since removes nothing. What drops nothing isn't held back.
#[test]
fn a_stale_check_holds_removals_back_until_the_contract_is_read_again() {
    let s = Store::open_in_memory().unwrap();
    s.set_lists_prune(Prune::Allowed);
    s.merge_all(&[wire("S1", "one"), wire("S2", "two")])
        .unwrap();
    s.merge_reading(&[ri("A", Some(1)), ri("B", None)], true, &kinds())
        .unwrap();
    s.contract_stale();

    // Nothing would drop: nothing held back.
    s.merge_all(&[wire("S1", "one"), wire("S2", "two")])
        .unwrap();
    assert!(!s.take_held_back());

    // Your items.
    s.merge_all(&[wire("S1", "one")]).unwrap();
    assert!(s.local_id_for_server("S2").is_some(), "S2 held back");
    assert!(s.take_held_back());
    // Thumbs.
    s.replace_thumbs(&[]).unwrap();
    let a = s.reading().into_iter().find(|r| r.remote_id == "A").unwrap();
    assert_eq!(a.thumb, Some(1), "A's thumb held back");
    assert!(s.take_held_back());
    assert!(!s.take_held_back(), "taken once");

    // Read again (`set_lists_prune`): the same lists remove.
    s.set_lists_prune(Prune::Allowed);
    s.merge_all(&[wire("S1", "one")]).unwrap();
    assert!(s.local_id_for_server("S2").is_none());
    s.replace_thumbs(&[]).unwrap();
    let a = s.reading().into_iter().find(|r| r.remote_id == "A").unwrap();
    assert_eq!(a.thumb, None);
    assert!(!s.take_held_back());
}

/// An RSS post whose guid changed is re-keyed only on a list Burrow can
/// read: on a newer contract's word the old row (its thumb, its place on
/// the shelf and in hoppers) stays, beside the new one.
#[test]
fn a_newer_contracts_new_guid_doesnt_re_key_a_held_row() {
    let page = Some("https://a.example/p".to_string());
    let with_page = |rid: &str, thumb: Option<i8>| ReadingItem {
        page: page.clone(),
        ..ri(rid, thumb)
    };
    let listed = |rid: &str| HashSet::from([(SubId::from("s1"), RemoteId::from(rid))]);
    for (prune, held) in [
        (Prune::Withheld, vec!["A", "B"]),
        (Prune::Allowed, vec!["B"]),
    ] {
        let s = Store::open_in_memory().unwrap();
        s.set_lists_prune(Prune::Allowed);
        s.merge_reading_page(&[with_page("A", Some(1))], &listed("A"), &kinds())
            .unwrap();
        s.set_lists_prune(prune);
        s.merge_reading_page(&[with_page("B", None)], &listed("B"), &kinds())
            .unwrap();
        // Every row held (`reading()` shows duplicates of a page once).
        let mut rows: Vec<(String, Option<i8>)> = s
            .reading_rows_of(&"s1".into())
            .into_iter()
            .map(|r| (r.remote_id.to_string(), r.thumb))
            .collect();
        rows.sort();
        let ids: Vec<&str> = rows.iter().map(|(r, _)| r.as_str()).collect();
        assert_eq!(ids, held, "{prune:?}");
        if prune == Prune::Withheld {
            assert_eq!(rows[0], ("A".into(), Some(1)), "A keeps its thumb");
        }
    }
}
