//! A stock blygger-studio: no `/api/reading/imported` (nor the fork's other
//! extensions). The reading list is assembled from upstream's own routes and
//! must come out the same as the fork's rows.

mod common;

use blyg_core::*;
use common::*;
use serde_json::{Value, json};

const ORIGIN: &str = "https://them.example.com/";

fn seed(env: &Env, stock: bool) {
    let mut st = env.mock.state();
    st.stock = stock;
    st.subs = vec![json!({ "id": "S1", "kind": "blyg", "origin": ORIGIN, "title": "Them" })];
    st.hoppers = Some(vec![
        json!({ "id": "H1", "name": "Later", "slug": null, "public": false }),
    ]);
    st.reading = Some(vec![
        json!({ "subscription_id": "S1", "remote_id": "A", "subscription_title": "Them", "origin": ORIGIN,
            "kind": "fragment", "state": "current", "version": 2, "created": "2030-01-01T00:00:00Z",
            "updated": "2030-01-02T00:00:00Z", "observed_at": "2030-01-03T00:00:00Z",
            "content_md": "First *post*", "content_html": "<p>First <em>post</em></p>",
            "author": { "name": "Them", "url": "https://them.example.com/" }, "page": "f/A/",
            "thumb": 1, "hoppers": ["H1"] }),
        json!({ "subscription_id": "S1", "remote_id": "B", "subscription_title": "Them", "origin": ORIGIN,
            "kind": "thread", "state": "current", "version": 1, "created": null, "updated": null,
            "observed_at": "2030-01-02T00:00:00Z", "content_md": "A thread", "content_html": "<p>A thread</p>",
            "author": null, "page": null, "thumb": null, "hoppers": [],
            "transclusions": [{ "id": "A", "version": 2, "origin": ORIGIN }] }),
        json!({ "subscription_id": "S1", "remote_id": "C", "subscription_title": "Them", "origin": ORIGIN,
            "kind": "fragment", "state": "tombstone", "version": 3, "created": null, "updated": null,
            "observed_at": "2030-01-01T00:00:00Z", "content_md": "kept", "content_html": "<p>kept</p>",
            "author": null, "page": "f/C/", "thumb": -1, "hoppers": [], "pinned_version_retained": 1 }),
    ]);
}

/// Every reading row the store holds, raw (tombstones included), in key order.
fn rows(b: &LiveBackend) -> Vec<Value> {
    let mut v: Vec<Value> = ["A", "B", "C"]
        .iter()
        .filter_map(|rid| b.reading_row("S1", rid))
        .map(|r| {
            let mut v = serde_json::to_value(r).unwrap();
            v.as_object_mut().unwrap().remove("read_version");
            v
        })
        .collect();
    v.sort_by_key(|r| r["remote_id"].as_str().unwrap_or("").to_string());
    v
}

#[test]
fn a_stock_server_gives_the_same_reading_rows_as_the_fork() {
    let fork = Env::new();
    seed(&fork, false);
    let a = fork.manual();
    a.sync_now().unwrap();

    let stock = Env::new();
    seed(&stock, true);
    let b = stock.manual();
    b.sync_now().unwrap();

    let (ra, rb) = (rows(&a), rows(&b));
    assert_eq!(ra.len(), 3, "{ra:?}");
    assert_eq!(ra, rb);
    assert_eq!(a.server_extensions(), Some(true));
    assert_eq!(b.server_extensions(), Some(false));
}

#[test]
fn a_stock_server_is_explained_once_per_database() {
    let env = Env::new();
    seed(&env, true);
    let b = env.manual();
    b.sync_now().unwrap();
    b.sync_now().unwrap();
    let told = |env: &Env| {
        env.events()
            .iter()
            .filter(|e| matches!(e, CoreEvent::ServerLimited))
            .count()
    };
    assert_eq!(told(&env), 1);
    drop(b);
    let b = env.manual(); // relaunch, same database
    b.sync_now().unwrap();
    assert_eq!(told(&env), 1, "not again after a relaunch");

    let fork = Env::new();
    seed(&fork, false);
    fork.manual().sync_now().unwrap();
    assert_eq!(told(&fork), 0, "a server with the extensions says nothing");
}

#[test]
fn only_new_or_changed_posts_are_fetched_one_by_one() {
    let env = Env::new();
    seed(&env, true);
    let b = env.manual();
    b.sync_now().unwrap();
    let imports = |env: &Env| {
        env.mock
            .state()
            .log
            .iter()
            .filter(|l| l.starts_with("GET /api/imports/"))
            .count()
    };
    assert_eq!(imports(&env), 3);
    b.sync_now().unwrap();
    assert_eq!(imports(&env), 3, "nothing changed, nothing fetched");
    {
        let mut st = env.mock.state();
        let r = &mut st.reading.as_mut().unwrap()[1];
        r["observed_at"] = json!("2030-02-01T00:00:00Z");
        r["content_md"] = json!("A thread, edited");
    }
    b.sync_now().unwrap();
    assert_eq!(imports(&env), 4, "only the edited one");
    assert_eq!(
        b.reading_row("S1", "B").unwrap().content_md,
        "A thread, edited"
    );
}
