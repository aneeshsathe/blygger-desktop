//! A walk's page merges and its prune (sync/walk.rs) on real SQLite: a
//! page merged on its own removes nothing, and only the prune after a
//! complete walk takes away the rows that walk didn't read.

use std::collections::{HashMap, HashSet};

use super::Store;
use crate::model::*;

fn row(sub: &str, rid: &str, observed: &str, page: Option<&str>) -> ReadingItem {
    ReadingItem {
        subscription_id: sub.into(),
        remote_id: rid.into(),
        subscription_title: sub.into(),
        origin: "https://a.blyg.example.com/".into(),
        kind: Kind::Fragment,
        state: "current".into(),
        version: 1,
        created: None,
        updated: None,
        observed_at: observed.into(),
        content_md: format!("{rid} text"),
        content_html: String::new(),
        author: None,
        page: page.map(str::to_string),
        thumb: None,
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

fn keys(rows: &[ReadingItem]) -> HashSet<(SubId, RemoteId)> {
    rows.iter()
        .map(|r| (r.subscription_id.clone(), r.remote_id.clone()))
        .collect()
}

fn held(s: &Store) -> Vec<String> {
    let mut v: Vec<String> = s
        .reading()
        .into_iter()
        .map(|r| r.remote_id.into_string())
        .collect();
    v.sort();
    v
}

#[test]
fn a_page_merge_removes_nothing() {
    let s = Store::open_in_memory().unwrap();
    let kinds = HashMap::new();
    // Held from before: an old row, and one at the head the server has
    // since dropped.
    let before = [
        row("s1", "old", "2026-01-01T00:00:00Z", None),
        row("s1", "gone", "2026-11-01T00:00:00Z", None),
    ];
    s.merge_reading(&before, true, &kinds).unwrap();
    // A head page with a new row, at the same instants: no window prune.
    let page = [
        row("s1", "new", "2026-11-02T00:00:00Z", None),
        row("s1", "mid", "2026-11-01T00:00:00Z", None),
    ];
    assert!(s.merge_reading_rows(&page, &keys(&page), &kinds).unwrap());
    assert_eq!(held(&s), ["gone", "mid", "new", "old"]);
    // The same page again changes nothing.
    assert!(!s.merge_reading_rows(&page, &keys(&page), &kinds).unwrap());
}

#[test]
fn the_prune_after_a_complete_walk_takes_what_it_did_not_read() {
    let s = Store::open_in_memory().unwrap();
    let kinds = HashMap::new();
    let all = [
        row("s1", "a", "2026-11-03T00:00:00Z", None),
        row("s1", "b", "2026-11-02T00:00:00Z", None),
        row("s2", "c", "2026-11-01T00:00:00Z", None),
    ];
    s.merge_reading(&all, true, &kinds).unwrap();
    // A walk in two pages; `s2` was removed on the blyg meanwhile.
    let (p1, p2) = ([all[0].clone()], [all[1].clone()]);
    let mut walked = keys(&p1);
    s.merge_reading_rows(&p1, &walked, &kinds).unwrap();
    walked.extend(keys(&p2));
    s.merge_reading_rows(&p2, &walked, &kinds).unwrap();
    assert_eq!(held(&s), ["a", "b", "c"], "nothing goes before the end");
    assert!(s.prune_reading_except(&walked).unwrap());
    assert_eq!(held(&s), ["a", "b"]);
    assert!(!s.prune_reading_except(&walked).unwrap());
}

#[test]
fn a_row_listed_further_on_is_not_taken_for_a_renamed_one() {
    let s = Store::open_in_memory().unwrap();
    let kinds = HashMap::new();
    let page_url = Some("https://a.blyg.example.com/same");
    // An old row on the same page URL, which a later page still lists.
    let old = row("s1", "old", "2026-01-01T00:00:00Z", page_url);
    s.merge_reading(std::slice::from_ref(&old), true, &kinds)
        .unwrap();
    let new = row("s1", "new", "2026-11-01T00:00:00Z", page_url);
    // Read so far: only the new row. The old one is taken for it (an RSS
    // guid that changed on edit)…
    let first = [new.clone()];
    s.merge_reading_rows(&first, &keys(&first), &kinds).unwrap();
    let has = |rid: &str| {
        s.reading_row(&SubId::from("s1"), &RemoteId::from(rid))
            .is_some()
    };
    assert!(has("new") && !has("old"));
    // …and comes back when the walk reaches it, as its own row.
    let mut walked = keys(&first);
    walked.insert(("s1".into(), "old".into()));
    s.merge_reading_rows(std::slice::from_ref(&old), &walked, &kinds)
        .unwrap();
    assert!(has("new") && has("old"));
}
