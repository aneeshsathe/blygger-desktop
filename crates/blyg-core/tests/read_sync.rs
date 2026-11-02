//! Read-state sync through the user's blyg (contract 5.0.0 read state):
//! merge on pull, and queued/coalesced `read` ops.

mod common;

use std::time::Duration;

use blyg_core::*;
use common::*;
use serde_json::{Value, json};

fn item(sub: &str, rid: &str, page: Option<&str>, version: u32, obs: &str) -> Value {
    json!({
        "subscription_id": sub, "remote_id": rid, "subscription_title": "Them",
        "origin": "https://them.example/", "kind": "fragment", "state": "current",
        "version": version, "created": null, "updated": null, "observed_at": obs,
        "content_md": "text", "content_html": "<p>text</p>",
        "author": null, "page": page, "thumb": null, "hoppers": []
    })
}

/// Two posts from one blyg (S1: a at v2, b at v1).
fn env_with() -> Env {
    let env = Env::new();
    {
        let mut st = env.mock.state();
        st.subs = vec![
            json!({ "id": "S1", "kind": "blyg", "origin": "https://them.example/",
            "feed_url": "x", "title": "Them", "status": "active", "in_blogroll": false }),
        ];
        st.reading = Some(vec![
            item("S1", "a", Some("f/a"), 2, "2030-01-02T00:00:00Z"),
            item("S1", "b", Some("f/b"), 1, "2030-01-01T00:00:00Z"),
        ]);
    }
    env
}

fn read_version(b: &LiveBackend, rid: &str) -> Option<u32> {
    b.reading()
        .into_iter()
        .find(|r| r.remote_id == rid)
        .and_then(|r| r.read_version)
}

fn read_requests(env: &Env) -> Vec<String> {
    env.mock
        .state()
        .log
        .iter()
        .filter(|l| l.starts_with("PUT /api/reading/") || l.starts_with("POST /api/reading/read"))
        .cloned()
        .collect()
}

fn server_read(env: &Env, sub: &str, rid: &str) -> Option<u32> {
    env.mock
        .state()
        .reads
        .get(&(sub.to_string(), rid.to_string()))
        .copied()
}

#[test]
fn pull_merges_the_max_and_never_lowers_local_state() {
    let env = env_with();
    env.mock.state().reads.insert(("S1".into(), "b".into()), 1);
    let b = env.manual();
    b.sync_now().unwrap();
    assert_eq!(read_version(&b, "b"), Some(1), "read on another Mac");
    assert_eq!(read_version(&b, "a"), None);
    assert!(env.events().contains(&CoreEvent::ReadingChanged));

    // Read `a` here at v2; the server (stale) says v1 afterwards.
    b.mark_read("S1", "a").unwrap();
    env.mock.state().reads.insert(("S1".into(), "a".into()), 1);
    b.pull_now().unwrap();
    assert_eq!(
        read_version(&b, "a"),
        Some(2),
        "a lower server value never wins"
    );

    // Contract 5 servers clear read state (`readStateClear`): null is
    // "marked unread elsewhere", so `b` goes unread here too, while `a`,
    // whose read is still queued, keeps it.
    env.mock.state().reads.clear();
    b.pull_now().unwrap();
    assert_eq!(read_version(&b, "a"), Some(2));
    assert_eq!(read_version(&b, "b"), None);
}

#[test]
fn junk_read_version_is_ignored_leniently() {
    let env = env_with();
    {
        let mut st = env.mock.state();
        let mut r = st.reading.take().unwrap();
        r[0]["read_version"] = json!("two");
        r[1]["read_version"] = json!(-3);
        st.reading = Some(r);
        // Deliberately off-contract (both reading lists serve the rows).
        st.unchecked.push("/api/reading".into());
    }
    let b = env.manual();
    b.sync_now().unwrap();
    assert_eq!(b.reading().len(), 2, "odd fields don't drop the rows");
    assert_eq!(read_version(&b, "a"), None);
    assert_eq!(read_version(&b, "b"), None);
}

#[test]
fn mark_read_queues_and_flushes_a_put() {
    let env = env_with();
    let b = env.manual();
    b.sync_now().unwrap();
    assert!(
        read_requests(&env).len() <= 1,
        "at most the (empty) initial upload"
    );
    env.mock.state().log.clear();

    b.mark_read("S1", "a").unwrap();
    assert_eq!(read_version(&b, "a"), Some(2), "instant and local");
    assert!(read_requests(&env).is_empty(), "nothing sent inline");
    b.sync_now().unwrap();
    assert_eq!(read_requests(&env), vec!["PUT /api/reading/S1/a/read"]);
    assert_eq!(server_read(&env, "S1", "a"), Some(2));

    // Nothing left to send.
    env.mock.state().log.clear();
    b.sync_now().unwrap();
    assert!(read_requests(&env).is_empty());
}

#[test]
fn the_worker_flushes_a_read_promptly() {
    let env = env_with();
    let b = env.open(SyncOptions {
        pull_interval: Duration::from_secs(3600),
        ..fast()
    });
    assert!(wait_until(Duration::from_secs(5), || b.reading().len() == 2));
    b.mark_read("S1", "b").unwrap();
    assert!(wait_until(Duration::from_secs(5), || server_read(
        &env, "S1", "b"
    ) == Some(1)));
    drop(b);
}

#[test]
fn offline_mark_then_flush_and_restart() {
    let env = env_with();
    {
        let b = env.manual();
        b.sync_now().unwrap();
        env.mock.set_down(true);
        b.mark_read("S1", "a").unwrap();
        assert_eq!(read_version(&b, "a"), Some(2));
        assert!(b.sync_now().is_err(), "offline");
        assert!(matches!(
            b.sync_status(),
            SyncStatus::Offline { pending: 1 }
        ));
    }
    // Restart while still offline: the capability is remembered, so a new
    // read queues too.
    let b = env.manual();
    b.mark_read("S1", "b").unwrap();
    env.mock.set_down(false);
    b.sync_now().unwrap();
    assert_eq!(server_read(&env, "S1", "a"), Some(2));
    assert_eq!(server_read(&env, "S1", "b"), Some(1));
}

#[test]
fn a_transient_failure_retries_later() {
    let env = env_with();
    let b = env.manual();
    b.sync_now().unwrap();
    env.mock.set_down(true);
    b.mark_read("S1", "a").unwrap();
    assert!(b.sync_now().is_err());
    env.mock.set_down(false);
    env.mock.state().log.clear();
    b.sync_now().unwrap();
    assert_eq!(read_requests(&env), vec!["PUT /api/reading/S1/a/read"]);
}

#[test]
fn duplicates_are_marked_and_sent_per_row() {
    let env = Env::new();
    let page = "https://them.example/f/A1";
    {
        let mut st = env.mock.state();
        st.subs = vec![
            json!({ "id": "B", "kind": "blyg", "origin": "https://them.example/", "feed_url": "x",
                    "title": "Them", "status": "active", "in_blogroll": false }),
            json!({ "id": "F", "kind": "rss", "origin": "https://them.example/feed.xml", "feed_url": "x",
                    "title": "Them (feed)", "status": "active", "in_blogroll": false }),
        ];
        // The studio numbers RSS items from 1, like every imported row.
        let mut rss = item("F", page, Some(page), 1, "2030-01-02T00:00:00Z");
        rss["origin"] = json!("https://them.example/feed.xml");
        st.reading = Some(vec![
            rss,
            item("B", "A1", Some("f/A1"), 1, "2030-01-01T00:00:00Z"),
        ]);
    }
    let b = env.manual();
    b.sync_now().unwrap();
    assert_eq!(b.reading().len(), 1, "one entry per post");
    b.mark_read("B", "A1").unwrap();
    b.sync_now().unwrap();
    // Both rows go in one batch (owner write budgets: never a PUT burst).
    assert_eq!(
        read_requests(&env),
        vec!["POST /api/reading/read".to_string()]
    );
    assert_eq!(server_read(&env, "B", "A1"), Some(1));
    assert_eq!(server_read(&env, "F", page), Some(1));

    // A second Mac sees both rows read.
    let env2_dir = tempfile::tempdir().unwrap();
    let b2 = LiveBackend::open_with(
        env2_dir.path(),
        &env.mock.url,
        TOKEN,
        SyncOptions {
            start_worker: false,
            ..fast()
        },
    )
    .unwrap();
    b2.sync_now().unwrap();
    let r = b2.reading();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].read_version, Some(1));
    assert!(!r[0].is_unread());
}

#[test]
fn repeated_reads_coalesce_to_the_highest_version() {
    let env = env_with();
    let b = env.manual();
    b.sync_now().unwrap();
    env.mock.set_down(true);
    b.mark_read("S1", "b").unwrap(); // v1
    // The post is edited (v3) and read again, all while offline.
    env.mock.set_down(false);
    env.mock.state().reading = Some(vec![
        item("S1", "a", Some("f/a"), 2, "2030-01-02T00:00:00Z"),
        item("S1", "b", Some("f/b"), 3, "2030-01-03T00:00:00Z"),
    ]);
    b.pull_now().unwrap();
    env.mock.set_down(true);
    b.mark_read("S1", "b").unwrap(); // v3
    env.mock.set_down(false);
    env.mock.state().log.clear();
    b.sync_now().unwrap();
    assert_eq!(
        read_requests(&env),
        vec!["PUT /api/reading/S1/b/read"],
        "one op per row, not one per mark"
    );
    assert_eq!(server_read(&env, "S1", "b"), Some(3));
}
