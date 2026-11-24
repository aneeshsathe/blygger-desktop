//! A studio on a contract major newer than this Burrow knows (studio 0.67
//! moved to 6 and released Burrow stopped syncing until a hotfix): Burrow
//! keeps syncing what the blyg's capabilities offer, takes your writes,
//! says so once, and removes nothing held here on the word of that
//! studio's lists (`api::Contract::Newer`, `sync/newer.rs`).
//!
//! Against the in-process mock: the contract number is all that differs,
//! and what's under test is what Burrow does with a list that lacks a row.

use std::time::Duration;

use crate::common::*;
use blyg_core::*;
use serde_json::{Value, json};

fn row(rid: &str, day: u32) -> Value {
    json!({
        "subscription_id": "S1", "remote_id": rid, "subscription_title": "Them",
        "origin": "https://them.example/", "kind": "fragment", "state": "current",
        "version": 1, "created": null, "updated": null,
        "observed_at": format!("2030-01-{day:02}T00:00:00Z"),
        "content_md": "invented text", "content_html": "<p>invented text</p>",
        "author": null, "page": null, "thumb": null, "hoppers": []
    })
}

/// A blyg on `contract` holding one of your items, a subscription with two
/// reading rows, a hopper and a contact.
fn env_at(contract: &str) -> Env {
    let env = Env::new();
    {
        let mut st = env.mock.state();
        st.contract_version = Some(contract.into());
        st.unchecked.push("/api/contacts".into());
        st.operations.push("listContacts".into());
        st.add_item("fragment", "made in the studio");
        st.subs = vec![json!({
            "id": "S1", "kind": "blyg", "origin": "https://them.example/",
            "feed_url": "https://them.example/feed.xml", "title": "Them",
            "status": "active", "in_blogroll": false
        })];
        st.reading = Some(vec![row("R1", 2), row("R2", 1)]);
        st.hoppers = Some(vec![
            json!({ "id": "h1", "name": "Later", "slug": "later", "public": false, "count": 0 }),
        ]);
        st.contacts = Some(vec![
            json!({ "handle": "lin", "name": "Lin Example", "origin": "https://lin.example.org" }),
        ]);
    }
    env
}

/// Every pull reads every list whole (the safety net is always due), so a
/// list that lacks a row is a whole walk's word that it's gone.
fn open(env: &Env) -> LiveBackend {
    env.open(SyncOptions {
        start_worker: false,
        full_pull_every: Duration::ZERO,
        ..fast()
    })
}

/// What's held here: items, subscriptions, reading rows, hoppers, contacts.
fn held(b: &LiveBackend) -> (usize, usize, usize, usize, usize) {
    (
        b.items().len(),
        b.subscriptions().len(),
        b.reading().len(),
        b.hoppers().len(),
        b.contacts().len(),
    )
}

/// The blyg drops everything it held.
fn drop_everything(env: &Env) {
    let mut st = env.mock.state();
    st.items.clear();
    st.subs.clear();
    st.reading = Some(vec![]);
    st.hoppers = Some(vec![]);
    st.contacts = Some(vec![]);
}

#[test]
fn a_newer_contract_syncs_and_removes_nothing_held_here() {
    let env = env_at("7.0.0");
    let b = open(&env);
    b.sync_now().expect("a newer contract still syncs");
    b.refresh_contacts().unwrap();
    assert_eq!(held(&b), (1, 1, 2, 1, 1), "everything came in");

    drop_everything(&env);
    // Still syncing: what's new comes in.
    env.mock.state().add_item("thread", "# New there\nbody");
    b.sync_now().unwrap();
    b.refresh_contacts().unwrap();
    assert_eq!(
        held(&b),
        (2, 1, 2, 1, 1),
        "nothing held is removed on a newer studio's word; the new item came in"
    );
    assert!(!b.server_outdated());
}

#[test]
fn a_version_that_isnt_a_number_is_a_newer_contract() {
    let env = env_at("next");
    let b = open(&env);
    b.sync_now().expect("an unknown contract still syncs");
    assert_eq!(b.items().len(), 1);
    drop_everything(&env);
    b.sync_now().unwrap();
    assert_eq!(b.items().len(), 1, "kept");
    assert_eq!(b.reading().len(), 2, "kept");
}

#[test]
fn a_newer_contract_still_takes_your_writes() {
    let env = env_at("7.0.0");
    let b = open(&env);
    let id = b.create_draft(Kind::Fragment, "written here").unwrap();
    b.publish(&id, None).expect("publish goes through");
    let sid = b.item(&id).unwrap().server_id.expect("on the blyg").0;
    let st = env.mock.state();
    assert_eq!(st.items[&sid].content_md, "written here");
    assert_eq!(st.items[&sid].status, "public");
}

#[test]
fn a_newer_contract_is_told_once() {
    let env = env_at("7.0.0");
    let b = open(&env);
    b.sync_now().unwrap();
    b.sync_now().unwrap();
    let told: Vec<CoreEvent> = env
        .events()
        .into_iter()
        .filter(|e| matches!(e, CoreEvent::ServerNewer(_)))
        .collect();
    assert_eq!(told, [CoreEvent::ServerNewer("7.0.0".into())]);
}

#[test]
fn a_known_contract_still_removes_what_the_blyg_dropped() {
    for contract in ["5.7.0", "6.0.0"] {
        let env = env_at(contract);
        let b = open(&env);
        b.sync_now().unwrap();
        b.refresh_contacts().unwrap();
        assert_eq!(held(&b), (1, 1, 2, 1, 1), "{contract}");
        drop_everything(&env);
        b.sync_now().unwrap();
        b.refresh_contacts().unwrap();
        let (items, subs, reading, hoppers, _) = held(&b);
        assert_eq!(
            (items, subs, reading, hoppers),
            (0, 0, 0, 0),
            "{contract}: a whole list is the truth"
        );
        assert!(
            !env.events()
                .iter()
                .any(|e| matches!(e, CoreEvent::ServerNewer(_))),
            "{contract}: nothing to tell"
        );
    }
}

#[test]
fn connecting_to_a_newer_contract_works() {
    let env = env_at("7.0.0");
    assert_eq!(api::verify_connection(&env.mock.url, TOKEN), Ok(1));
}

/// A second subscription, S2, on the blyg.
fn add_s2(env: &Env) {
    env.mock.state().subs.push(json!({
        "id": "S2", "kind": "blyg", "origin": "https://other.example/",
        "feed_url": "https://other.example/feed.xml", "title": "Other",
        "status": "active", "in_blogroll": false
    }));
}

fn sub_ids(b: &LiveBackend) -> Vec<String> {
    let mut ids: Vec<String> = b
        .subscriptions()
        .into_iter()
        .map(|s| s.id.to_string())
        .collect();
    ids.sort();
    ids
}

/// Every path that replaces the subscription list (a refresh, a rename, a
/// subscribe, as well as a pull) keeps what a newer studio's list lacks.
#[test]
fn a_newer_contracts_subscription_list_removes_nothing_on_any_path() {
    let env = env_at("7.0.0");
    add_s2(&env);
    // `wholeBlyg` is sent (and refused by the mock's 5.0.0 schema).
    env.mock.state().unchecked.push("/api/subscriptions".into());
    let b = open(&env);
    b.sync_now().unwrap();
    assert_eq!(sub_ids(&b), ["S1", "S2"]);
    env.mock.state().subs.retain(|s| s["id"] != "S2");

    b.refresh_subscriptions().unwrap();
    assert_eq!(sub_ids(&b), ["S1", "S2"], "a refresh kept S2");
    b.rename_subscription(&"S1".into(), Some("Them, renamed"))
        .unwrap();
    assert!(sub_ids(&b).contains(&"S2".to_string()), "a rename kept S2");
    let new = b.subscribe("https://new.example/", None).unwrap();
    let ids = sub_ids(&b);
    assert!(
        ids.contains(&"S2".to_string()),
        "a subscribe kept S2: {ids:?}"
    );
    assert!(ids.contains(&new.id.to_string()), "and added {}", new.id);
}

/// The same refresh on a known contract takes the blyg's word.
#[test]
fn a_known_contracts_refresh_removes_a_dropped_subscription() {
    let env = env_at("6.0.0");
    add_s2(&env);
    let b = open(&env);
    b.sync_now().unwrap();
    env.mock.state().subs.retain(|s| s["id"] != "S2");
    b.refresh_subscriptions().unwrap();
    assert_eq!(sub_ids(&b), ["S1"]);
}

/// A studio upgraded to a newer contract while Burrow runs: it's seen
/// before anything is removed on its lists' word.
#[test]
fn a_studio_upgraded_mid_session_removes_nothing() {
    let env = env_at("6.0.0");
    let b = open(&env);
    b.sync_now().unwrap();
    b.refresh_contacts().unwrap();
    assert_eq!(held(&b), (1, 1, 2, 1, 1));
    env.mock.state().contract_version = Some("7.0.0".into());
    drop_everything(&env);
    b.sync_now().unwrap();
    let (items, subs, reading, hoppers, _) = held(&b);
    assert_eq!((items, subs, reading, hoppers), (1, 1, 2, 1), "kept");
    assert!(
        env.events()
            .iter()
            .any(|e| matches!(e, CoreEvent::ServerNewer(_))),
        "and told"
    );

    // Between safety nets (the change counters say what moved): each list
    // that would drop something reads the contract first, your items,
    // hoppers and thumbs too.
    let env = env_at("6.0.0");
    env.mock
        .state()
        .signals
        .insert(("S1".into(), "R1".into()), 1);
    let b = env.open(SyncOptions {
        start_worker: false,
        ..fast()
    });
    b.sync_now().unwrap();
    let thumb = |b: &LiveBackend| {
        b.reading()
            .into_iter()
            .find(|r| r.remote_id == "R1")
            .and_then(|r| r.thumb)
    };
    assert_eq!(thumb(&b), Some(1));
    env.mock.state().contract_version = Some("7.0.0".into());
    drop_everything(&env);
    env.mock.state().signals.clear();
    b.sync_now().unwrap();
    let (items, subs, reading, hoppers, _) = held(&b);
    assert_eq!(
        (items, subs, reading, hoppers),
        (1, 1, 2, 1),
        "kept between safety nets"
    );
    assert_eq!(thumb(&b), Some(1), "the thumb kept");
}

/// Unsubscribing is your removal: its rows leave here at once, whatever
/// the contract (on a newer one, no list would take them).
#[test]
fn unsubscribing_on_a_newer_contract_takes_its_rows_here() {
    let env = env_at("7.0.0");
    let b = open(&env);
    b.sync_now().unwrap();
    assert_eq!(b.reading().len(), 2);
    b.unsubscribe(&"S1".into()).unwrap();
    assert!(b.subscriptions().is_empty());
    assert!(
        b.reading().iter().all(|r| r.subscription_id != "S1"),
        "S1's rows went with it"
    );
}
