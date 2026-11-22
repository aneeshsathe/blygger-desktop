//! A studio on a contract newer than this Burrow knows
//! (`Store::set_lists_prune(Prune::Withheld)`): its lists add and update
//! what's held here, and remove nothing. Hoppers and read state have theirs
//! in `hoppers_tests.rs` and `read_state_tests.rs`.

use std::collections::{HashMap, HashSet};

use super::Store;
use crate::api::Prune;
use crate::api::wire::WireItem;
use crate::model::*;

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
    assert!(s.local_id_for_server("S2").is_none(), "a known contract prunes");
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
