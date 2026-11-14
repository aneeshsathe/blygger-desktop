//! --- syndication --- Copies elsewhere on real SQLite: a local edit is
//! instant and queued (one op per post, the last wins), a pull never
//! overwrites a waiting edit, the blyg's answer settles it, and reading
//! rows keep what their blyg served.

use std::collections::HashMap;

use serde_json::json;

use super::super::{OpKind, Store};
use crate::api::wire::WireItem;
use crate::model::*;
use crate::syndication::{SyndicationEntry, SyndicationOp, op_key};

fn store() -> Store {
    Store::open_in_memory().unwrap()
}

fn e(url: &str, v: u32) -> SyndicationEntry {
    SyndicationEntry::new(url, v)
}

fn published(id: &str, syndication: Option<serde_json::Value>) -> WireItem {
    WireItem {
        id: id.into(),
        kind: "fragment".into(),
        status: "public".into(),
        version: 2,
        created: "2030-01-01T00:00:00Z".into(),
        updated: "2030-01-01T00:00:00Z".into(),
        content_md: "Tides.".into(),
        syndication,
        ..Default::default()
    }
}

fn ops(s: &Store) -> Vec<(String, SyndicationOp)> {
    s.ops()
        .unwrap()
        .into_iter()
        .filter(|o| o.kind == OpKind::Syndication)
        .map(|o| (o.local_id.0, serde_json::from_str(&o.payload).unwrap()))
        .collect()
}

#[test]
fn an_edit_is_local_first_and_one_op_waits_per_post() {
    let s = store();
    s.merge_all(&[published("S1", None)]).unwrap();
    let id = s.local_id_for_server("S1").unwrap();
    assert_eq!(s.syndication(&id), vec![]);

    let kept = s
        .set_syndication(
            &id,
            &[
                e("https://social.example.net/@me/1", 2),
                e("http://plain.example.net/1", 2),
            ],
        )
        .unwrap();
    assert_eq!(
        kept,
        [e("https://social.example.net/@me/1", 2)],
        "https only"
    );
    assert_eq!(s.syndication(&id), kept);
    s.set_syndication(
        &id,
        &[
            e("https://social.example.net/@me/1", 2),
            e("https://notes.example.org/n/9", 1),
        ],
    )
    .unwrap();
    let waiting = ops(&s);
    assert_eq!(waiting.len(), 1, "the last edit replaces the waiting one");
    assert_eq!(waiting[0].0, op_key(&id).0);
    assert_eq!(waiting[0].1.server_id, "S1");
    assert_eq!(waiting[0].1.entries.len(), 2);
    // Not an item edit: the post isn't pending a text push, and nothing
    // about its version changed.
    let it = s.item(&id).unwrap();
    assert!(!it.pending_sync && !it.dirty);
    assert_eq!(it.version, 2);
}

#[test]
fn a_draft_has_no_copies() {
    let s = store();
    let id = s.create_draft(Kind::Fragment, "x", 0).unwrap();
    assert!(
        s.set_syndication(&id, &[e("https://a.example/1", 1)])
            .is_err()
    );
    assert!(ops(&s).is_empty());
}

#[test]
fn a_pull_takes_the_served_list_unless_an_edit_waits() {
    let s = store();
    let served = json!([{"url": "https://social.example.net/@me/1", "version": 2}]);
    s.merge_all(&[published("S1", Some(served.clone()))])
        .unwrap();
    let id = s.local_id_for_server("S1").unwrap();
    assert!(
        !s.merge_wire_syndication(&[published("S1", None)]).unwrap(),
        "absent is unknown: kept"
    );
    assert!(
        s.merge_wire_syndication(&[published("S1", Some(served.clone()))])
            .unwrap()
    );
    assert_eq!(
        s.syndication(&id),
        [e("https://social.example.net/@me/1", 2)]
    );
    assert!(
        !s.merge_wire_syndication(&[published("S1", Some(served.clone()))])
            .unwrap(),
        "the same list changes nothing"
    );

    // An edit waits: a pull leaves it alone.
    s.set_syndication(&id, &[]).unwrap();
    assert!(
        !s.merge_wire_syndication(&[published("S1", Some(served))])
            .unwrap()
    );
    assert_eq!(s.syndication(&id), vec![]);

    // The blyg answers: the op goes and its list is kept.
    let seq = s
        .ops()
        .unwrap()
        .into_iter()
        .find(|o| o.kind == OpKind::Syndication)
        .unwrap()
        .seq;
    s.syndication_sent(seq, &id, &[]).unwrap();
    assert!(ops(&s).is_empty());
    assert_eq!(s.syndication(&id), vec![]);

    // Withdrawn: an endcap carries none.
    s.set_syndication(&id, &[e("https://a.example/1", 1)])
        .unwrap();
    let seq = s.ops().unwrap().last().unwrap().seq;
    s.syndication_sent(seq, &id, &[e("https://a.example/1", 1)])
        .unwrap();
    let mut gone = published("S1", None);
    gone.status = "withdrawn".into();
    assert!(s.merge_wire_syndication(&[gone]).unwrap());
    assert_eq!(s.syndication(&id), vec![]);
}

#[test]
fn an_answer_doesnt_undo_a_newer_edit() {
    let s = store();
    s.merge_all(&[published("S1", None)]).unwrap();
    let id = s.local_id_for_server("S1").unwrap();
    s.set_syndication(&id, &[e("https://a.example/1", 1)])
        .unwrap();
    let first = s.ops().unwrap().last().unwrap().seq;
    s.set_in_flight(first, true).unwrap();
    s.set_syndication(&id, &[e("https://b.example/1", 2)])
        .unwrap();
    assert_eq!(ops(&s).len(), 2, "the one on the wire stays until answered");
    s.syndication_sent(first, &id, &[e("https://a.example/1", 1)])
        .unwrap();
    assert_eq!(s.syndication(&id), [e("https://b.example/1", 2)]);
    assert_eq!(ops(&s).len(), 1);
}

fn ri(sub: &str, rid: &str) -> ReadingItem {
    serde_json::from_value(json!({
        "subscription_id": sub,
        "remote_id": rid,
        "subscription_title": sub,
        "origin": "https://a.example/",
        "kind": "fragment",
        "state": "current",
        "version": 3,
        "created": null,
        "updated": null,
        "observed_at": "2030-01-01T00:00:00Z",
        "content_md": "Their post.",
        "content_html": "",
        "author": null,
        "page": null,
        "thumb": null,
        "hoppers": []
    }))
    .unwrap()
}

#[test]
fn reading_rows_keep_what_their_blyg_served() {
    let s = store();
    let kinds: HashMap<String, SubscriptionKind> = HashMap::new();
    s.merge_reading(&[ri("sub1", "r1"), ri("sub1", "r2")], true, &kinds)
        .unwrap();
    let list = vec![e("https://social.example.net/@them/7", 3)];
    assert!(
        s.set_reading_syndication(&[
            ("sub1".into(), "r1".into(), list.clone()),
            ("sub1".into(), "r2".into(), vec![]),
        ])
        .unwrap()
    );
    assert_eq!(s.reading_syndication("sub1", "r1"), list);
    assert_eq!(s.reading_syndication("sub1", "r2"), vec![]);
    assert_eq!(s.reading_syndication("sub1", "nope"), vec![]);
    // A re-merge of the same rows keeps the list; the same list again
    // changes nothing.
    s.merge_reading(&[ri("sub1", "r1"), ri("sub1", "r2")], true, &kinds)
        .unwrap();
    assert_eq!(s.reading_syndication("sub1", "r1"), list);
    assert!(
        !s.set_reading_syndication(&[("sub1".into(), "r1".into(), list.clone())])
            .unwrap()
    );
    // The blyg drops it: gone here too.
    assert!(
        s.set_reading_syndication(&[("sub1".into(), "r1".into(), vec![])])
            .unwrap()
    );
    assert_eq!(s.reading_syndication("sub1", "r1"), vec![]);
}
