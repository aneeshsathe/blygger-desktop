//! --- hoppers --- The hopper cache and membership ops on real SQLite:
//! a local add is instant and queued, the last action wins, a fresh read
//! keeps what's still waiting, a delete lets go of the posts, and your own
//! posts (`~own`) are members like any other.

use std::collections::HashMap;

use super::{FetchedHopper, OpKind, Store, hopper_key};
use crate::model::*;

fn store() -> Store {
    Store::open_in_memory().unwrap()
}

fn ri(sub: &str, rid: &str) -> ReadingItem {
    ReadingItem {
        subscription_id: sub.into(),
        remote_id: rid.into(),
        subscription_title: sub.into(),
        origin: "https://a.example/".into(),
        kind: Kind::Fragment,
        state: "current".into(),
        version: 1,
        created: None,
        updated: None,
        observed_at: "2026-10-01T00:00:00Z".into(),
        content_md: format!("{rid} text"),
        content_html: String::new(),
        author: None,
        page: None,
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
    }
}

fn hopper(id: &str, name: &str, count: u32) -> Hopper {
    Hopper {
        id: id.into(),
        name: name.into(),
        slug: Some(name.to_lowercase()),
        public: false,
        count,
        description: None,
        slug_frozen: false,
    }
}

fn fetched(h: Hopper, members: &[(&str, &str)]) -> FetchedHopper {
    FetchedHopper {
        hopper: h,
        members: members
            .iter()
            .map(|(s, r)| (s.to_string(), r.to_string(), "2026-10-01T00:00:00Z".into()))
            .collect(),
        items: vec![],
    }
}

/// s1/A and s1/B held; one hopper H1 holding A.
fn seeded() -> Store {
    let s = store();
    let kinds: HashMap<String, SubscriptionKind> =
        [("s1".to_string(), SubscriptionKind::Blyg)].into();
    s.merge_reading(&[ri("s1", "A"), ri("s1", "B")], true, &kinds)
        .unwrap();
    s.replace_hoppers(&[fetched(hopper("H1", "Slow", 1), &[("s1", "A")])])
        .unwrap();
    s
}

fn row_hoppers(s: &Store, rid: &str) -> Vec<String> {
    s.reading_row("s1", rid).unwrap().0.hoppers
}

fn hopper_ops(s: &Store) -> Vec<super::HopperOp> {
    s.ops()
        .unwrap()
        .into_iter()
        .filter(|o| o.kind == OpKind::Hopper)
        .map(|o| serde_json::from_str(&o.payload).unwrap())
        .collect()
}

#[test]
fn a_read_fills_the_list_and_the_rows() {
    let s = seeded();
    assert_eq!(s.hoppers(), vec![hopper("H1", "Slow", 1)]);
    assert_eq!(row_hoppers(&s, "A"), ["H1"]);
    assert!(row_hoppers(&s, "B").is_empty());
    assert_eq!(
        s.hopper_members().get(&("s1".into(), "A".into())),
        Some(&vec!["H1".to_string()])
    );
}

#[test]
fn an_add_is_local_and_queued_and_the_last_action_wins() {
    let s = seeded();
    assert!(s.set_hopper_member("H1", "s1", "B", true).unwrap());
    assert_eq!(row_hoppers(&s, "B"), ["H1"]);
    assert_eq!(s.hoppers()[0].count, 2, "the count follows");
    assert_eq!(
        hopper_ops(&s),
        [super::HopperOp {
            hopper: "H1".into(),
            sub: "s1".into(),
            remote_id: "B".into(),
            on: true
        }]
    );
    // Already in: nothing more is queued.
    assert!(!s.set_hopper_member("H1", "s1", "B", true).unwrap());
    // Out again: one op waits, the latest.
    assert!(s.set_hopper_member("H1", "s1", "B", false).unwrap());
    let ops = hopper_ops(&s);
    assert_eq!(ops.len(), 1);
    assert!(!ops[0].on);
    assert!(row_hoppers(&s, "B").is_empty());
    assert_eq!(s.hoppers()[0].count, 1);
    // An unknown hopper changes nothing.
    assert!(!s.set_hopper_member("H9", "s1", "B", true).unwrap());
}

#[test]
fn a_fresh_read_keeps_what_still_waits() {
    let s = seeded();
    s.set_hopper_member("H1", "s1", "B", true).unwrap();
    s.set_hopper_member("H1", "s1", "A", false).unwrap();
    // The server hasn't heard yet: it still says A, not B.
    s.replace_hoppers(&[fetched(hopper("H1", "Slow", 1), &[("s1", "A")])])
        .unwrap();
    assert_eq!(row_hoppers(&s, "B"), ["H1"]);
    assert!(row_hoppers(&s, "A").is_empty());
    // A reading pull that writes the server's memberships into the rows
    // is set right again.
    let kinds: HashMap<String, SubscriptionKind> =
        [("s1".to_string(), SubscriptionKind::Blyg)].into();
    let mut a = ri("s1", "A");
    a.hoppers = vec!["H1".into()];
    s.merge_reading(&[a, ri("s1", "B")], true, &kinds).unwrap();
    assert!(s.sync_reading_hoppers().unwrap());
    assert!(row_hoppers(&s, "A").is_empty());
    assert_eq!(row_hoppers(&s, "B"), ["H1"]);
}

#[test]
fn a_hopper_gone_from_the_server_takes_its_ops_along() {
    let s = seeded();
    s.set_hopper_member("H1", "s1", "B", true).unwrap();
    s.replace_hoppers(&[]).unwrap();
    assert!(s.hoppers().is_empty());
    assert!(hopper_ops(&s).is_empty());
    assert!(row_hoppers(&s, "A").is_empty());
    assert!(row_hoppers(&s, "B").is_empty());
}

#[test]
fn deleting_a_hopper_lets_go_of_the_posts_only() {
    let s = seeded();
    s.replace_hoppers(&[
        fetched(hopper("H1", "Slow", 1), &[("s1", "A")]),
        fetched(hopper("H2", "Gardens", 1), &[("s1", "A")]),
    ])
    .unwrap();
    s.set_hopper_member("H1", "s1", "B", true).unwrap();
    assert!(s.remove_hopper("H1").unwrap());
    assert_eq!(s.hoppers(), vec![hopper("H2", "Gardens", 1)]);
    assert_eq!(row_hoppers(&s, "A"), ["H2"]);
    assert!(row_hoppers(&s, "B").is_empty());
    assert!(hopper_ops(&s).is_empty(), "its waiting op went too");
    assert_eq!(s.reading().len(), 2, "the posts stay");
}

#[test]
fn made_renamed_and_made_public_in_place() {
    let s = seeded();
    let made = s.put_hopper(&hopper("H2", "New", 0)).unwrap();
    assert_eq!(made.count, 0);
    assert_eq!(
        s.hoppers()
            .iter()
            .map(|h| h.id.as_str())
            .collect::<Vec<_>>(),
        ["H1", "H2"],
        "a new one goes at the end"
    );
    // The server's answer to a PATCH carries no count: the held one stays.
    let mut renamed = hopper("H1", "Slower", 0);
    renamed.public = true;
    renamed.slug_frozen = true;
    let got = s.put_hopper(&renamed).unwrap();
    assert_eq!(got.count, 1);
    let h1 = &s.hoppers()[0];
    assert_eq!((h1.name.as_str(), h1.public, h1.count), ("Slower", true, 1));
}

#[test]
fn your_own_posts_are_members_too() {
    let s = seeded();
    assert!(
        s.set_hopper_member("H1", OWN_SUBSCRIPTION, "OWNPOST", true)
            .unwrap()
    );
    assert_eq!(
        s.hopper_members()
            .get(&(OWN_SUBSCRIPTION.into(), "OWNPOST".into())),
        Some(&vec!["H1".to_string()])
    );
    let key = hopper_key("H1", OWN_SUBSCRIPTION, "OWNPOST");
    assert!(s.ops().unwrap().iter().any(|o| o.local_id == key));
    // The server's read lists it; it stays a member.
    s.replace_hoppers(&[fetched(
        hopper("H1", "Slow", 2),
        &[("s1", "A"), (OWN_SUBSCRIPTION, "OWNPOST")],
    )])
    .unwrap();
    assert_eq!(s.hoppers()[0].count, 2, "already there: not counted twice");
}

#[test]
fn a_post_only_the_hopper_holds_is_kept_for_it() {
    let s = seeded();
    let mut far = ri("s9", "Z");
    far.content_md = "held by the hopper".into();
    s.replace_hoppers(&[FetchedHopper {
        hopper: hopper("H1", "Slow", 2),
        members: vec![
            ("s1".into(), "A".into(), String::new()),
            ("s9".into(), "Z".into(), String::new()),
        ],
        items: vec![ri("s1", "A"), far],
    }])
    .unwrap();
    let unheld = s.hopper_unheld();
    assert_eq!(unheld.len(), 1, "A is held as a reading row");
    assert_eq!(unheld[0].remote_id, "Z");
    assert_eq!(unheld[0].hoppers, ["H1"]);
}

#[test]
fn before_the_first_read_the_rows_keep_the_pulled_memberships() {
    let s = store();
    let kinds: HashMap<String, SubscriptionKind> =
        [("s1".to_string(), SubscriptionKind::Blyg)].into();
    let mut a = ri("s1", "A");
    a.hoppers = vec!["H1".into()];
    s.merge_reading(&[a], true, &kinds).unwrap();
    assert!(!s.sync_reading_hoppers().unwrap());
    assert_eq!(row_hoppers(&s, "A"), ["H1"]);
}

#[test]
fn a_hopper_not_read_here_yet_keeps_the_rows_word() {
    // Made on another Mac since the last full read: the reading pull says
    // A is in it, and nothing here says otherwise.
    let s = seeded();
    let kinds: HashMap<String, SubscriptionKind> =
        [("s1".to_string(), SubscriptionKind::Blyg)].into();
    let mut a = ri("s1", "A");
    a.hoppers = vec!["H1".into(), "H7".into()];
    s.merge_reading(&[a, ri("s1", "B")], true, &kinds).unwrap();
    s.sync_reading_hoppers().unwrap();
    assert_eq!(row_hoppers(&s, "A"), ["H1", "H7"]);
}
