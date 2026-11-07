//! --- loose ends --- the omnibar's operators over a real store.

use std::collections::HashMap;

use super::Store;
use crate::loose::{self, Summary};
use crate::model::*;

const OWN: &str = "https://blyg.example.com";

fn published(s: &Store, sid: &str, text: &str) -> LocalId {
    let id = s.create_draft(Kind::Thread, text, 0).unwrap();
    s.conn()
        .execute(
            "UPDATE items SET server_id = ?2 WHERE local_id = ?1",
            [&id.0, sid],
        )
        .unwrap();
    s.after_publish(&id, 1, &format!("{OWN}/t/{sid}"), text)
        .unwrap();
    id
}

fn held(rid: &str, origin: &str, quotes: Option<(&str, &str)>) -> ReadingItem {
    ReadingItem {
        subscription_id: "s1".into(),
        remote_id: rid.into(),
        subscription_title: "Lin".into(),
        origin: origin.into(),
        kind: Kind::Thread,
        state: "current".into(),
        version: 1,
        created: None,
        updated: None,
        observed_at: "2026-10-01T00:00:00Z".into(),
        content_md: "A held post".into(),
        content_html: String::new(),
        author: None,
        page: None,
        thumb: None,
        hoppers: vec![],
        pinned_version_retained: None,
        read_version: None,
        stub_of: None,
        forked_from: None,
        transclusions: quotes
            .map(|(id, origin)| {
                vec![TransclusionRef {
                    id: id.into(),
                    version: Some(1),
                    origin: Some(origin.into()),
                    selector: None,
                    cited: None,
                }]
            })
            .unwrap_or_default(),
        lineage_known: false,
        source_url: None,
        generated: None,
    }
}

fn run(s: &Store, q: &str) -> Vec<String> {
    let mut v: Vec<String> = loose::search(
        q,
        |t| s.search(t),
        || s.items(),
        || s.loose_facts(Some(OWN)),
    )
    .into_iter()
    .map(|i| i.title())
    .collect();
    v.sort();
    v
}

const A: &str = "01j9qk3aaaaaaaaaaaaaaaaaaa";
const B: &str = "01j9qk3bbbbbbbbbbbbbbbbbbb";
const C: &str = "01j9qk3ccccccccccccccccccc";
const D: &str = "01j9qk3ddddddddddddddddddd";
const LIN: &str = "01jxnaaaaaaaaaaaaaaaaaaaaa";

fn sample() -> Store {
    let s = Store::open_in_memory().unwrap();
    published(&s, A, "Tide pools");
    published(&s, B, "Gardens, not streams");
    published(&s, C, &format!("Rock pool economics\n\n![[{LIN}]]\n"));
    published(&s, D, &format!("Shore notes\n\nsee [[{B}]]"));
    s.create_draft(Kind::Fragment, "A half thought [TK]expand this[/TK]", 0)
        .unwrap();
    let old = s.create_draft(Kind::Fragment, "An old draft", 0).unwrap();
    s.conn()
        .execute(
            "UPDATE items SET updated = '2026-01-01T00:00:00.000Z' WHERE local_id = ?1",
            [&old.0],
        )
        .unwrap();
    let kinds = HashMap::from([("s1".to_string(), SubscriptionKind::Blyg)]);
    s.merge_reading(
        &[held(LIN, "https://lin.example.org", Some((A, OWN)))],
        true,
        &kinds,
    )
    .unwrap();
    s
}

#[test]
fn orphans_are_published_posts_nothing_points_at() {
    let s = sample();
    // A is quoted by a held post, B is linked from D: C and D are loose.
    assert_eq!(run(&s, "is:orphan"), ["Rock pool economics", "Shore notes"]);
    // The remaining words still search.
    assert_eq!(run(&s, "is:orphan shore"), ["Shore notes"]);
}

#[test]
fn drafts_tk_stale_kind_and_version() {
    let s = sample();
    assert_eq!(run(&s, "is:draft"), ["A half thought", "An old draft"]);
    assert_eq!(run(&s, "has:tk"), ["A half thought"]);
    assert_eq!(run(&s, "is:stale"), ["An old draft"]);
    assert_eq!(
        run(&s, "kind:fragment v:0"),
        ["A half thought", "An old draft"]
    );
    assert_eq!(run(&s, "kind:thread").len(), 4);
    assert_eq!(run(&s, "v:1 tide"), ["Tide pools"]);
}

#[test]
fn links_and_quotes_by_id_prefix_and_host() {
    let s = sample();
    assert_eq!(run(&s, "links:01J9QK3B"), ["Shore notes"]);
    assert_eq!(run(&s, "quotes:01JXN"), ["Rock pool economics"]);
    assert_eq!(run(&s, "quotes:lin.example.org"), ["Rock pool economics"]);
    assert!(run(&s, "quotes:elsewhere.example.net").is_empty());
    assert!(run(&s, "is:leaf").contains(&"Tide pools".to_string()));
    assert!(!run(&s, "is:leaf").contains(&"Rock pool economics".to_string()));
}

#[test]
fn not_an_operator_searches_as_text() {
    let s = sample();
    s.create_draft(Kind::Fragment, "status is:blue today", 0)
        .unwrap();
    assert_eq!(run(&s, "is:blue"), ["status is:blue today"]);
}

#[test]
fn the_summary_is_in_words() {
    let s = sample();
    let sum = Summary::of(&s.items(), &s.loose_facts(Some(OWN)));
    assert_eq!(
        sum.words(),
        "2 orphans · 1 ungenerated TK · 1 draft older than a month"
    );
    assert_eq!(sum.parts()[0].1, "is:orphan");
    assert_eq!(Summary::default().words(), "");
}
