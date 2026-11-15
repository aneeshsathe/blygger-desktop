//! --- web-parity --- Driven as `www/parity.js` drives it, with the blyg's
//! answers written out by hand (shapes from blygger-studio 0.60.0,
//! contract 5.9.0). The features run against a real local studio in
//! `crates/blyg-web/parity-check.mjs`.

use serde_json::{Value, json};

use crate::Burrow;

/// 2026-11-06T12:00:00Z
const NOW: f64 = 1_793_966_400_000.0;
const BASE: &str = "https://blyg.example.com";

fn v(s: String) -> Value {
    serde_json::from_str(&s).unwrap()
}

fn row(sub: &str, rid: &str, kind: &str, version: u32, read: Option<u32>) -> Value {
    json!({
        "subscriptionId": sub, "remoteId": rid, "subscriptionTitle": format!("Sub {sub}"),
        "origin": format!("https://{sub}.example.com"), "kind": kind, "version": version,
        "created": "2026-11-04T09:00:00Z", "updated": "2026-11-04T09:00:00Z",
        "observedAt": "2026-11-04T09:05:00Z", "contentMd": format!("Sample note {rid}."),
        "contentHtml": format!("<p>Sample note {rid}.</p>"), "readVersion": read, "hoppers": []
    })
}

fn caps(ops: &[&str]) -> String {
    json!({
        "contractVersion": "5.9.0",
        "endpoints": { "publicBase": BASE, "studio": "/studio", "api": "/api", "issuer": BASE, "mcp": "/mcp", "media": "/media" },
        "operations": ops,
        "extensions": [],
    })
    .to_string()
}

/// A live Burrow reading `a` (two fragments: one unread, one read at an
/// older version) and `b` (a thread, read).
fn live(ops: &[&str]) -> Burrow {
    let mut b = Burrow::new_live(NOW, None, None, BASE);
    let rows = json!([
        row("a", "a1", "fragment", 1, None),
        row("a", "a2", "fragment", 3, Some(2)),
        row("b", "b1", "thread", 1, Some(1)),
    ]);
    b.live_reading(&rows.to_string(), "[]");
    b.wr_loaded("capabilities", &caps(ops));
    b
}

fn unread(b: &Burrow) -> u64 {
    v(b.reading_view(NOW, "", false))["unread"]
        .as_u64()
        .unwrap()
}

// ------------------------------------------------------------ mark all read

#[test]
fn capabilities_gate_on_operation_names_not_the_contract() {
    let b = live(&["markRead"]);
    assert!(b.wr_ops_known());
    assert!(b.wr_can("markRead"));
    assert!(!b.wr_can("markReadBatch"));
    let ask = v(b.mark_all_ask(NOW));
    assert_eq!(ask["can"], false, "5.9.0 alone doesn't make it so");
    assert_eq!(ask["known"], true);
    // The demo has no server to ask.
    let demo = Burrow::new(NOW, None);
    assert!(!demo.wr_can("markReadBatch"));
    assert_eq!(v(demo.mark_all_ask(NOW))["can"], true);
}

#[test]
fn mark_all_read_marks_the_source_and_batches_the_marks() {
    let mut b = live(&["markReadBatch"]);
    let ask = v(b.mark_all_ask(NOW));
    // a1 (unread) and a2 (read at v2, now v3); b1 is read.
    assert_eq!(ask["n"], 2);
    assert_eq!(ask["label"], "All");
    let out = v(b.mark_all_read(NOW));
    assert_eq!(out["n"], 2);
    let batches = out["batches"].as_array().unwrap();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0]["req"]["method"], "POST");
    assert_eq!(batches[0]["req"]["path"], "/api/reading/read");
    let mut items = batches[0]["req"]["body"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.to_string())
        .collect::<Vec<_>>();
    items.sort();
    assert_eq!(
        items,
        [
            json!({ "sub": "a", "remoteId": "a1", "version": 1 }).to_string(),
            json!({ "sub": "a", "remoteId": "a2", "version": 3 }).to_string(),
        ]
    );
    assert_eq!(unread(&b), 0);
    assert!(b.reading_dirty());
    // Nothing went to the outbox: the batch is the request.
    assert_eq!(v(b.outbox_next("[]")), Value::Null);
    // Nothing left to mark.
    assert_eq!(v(b.mark_all_ask(NOW))["n"], 0);
}

#[test]
fn mark_all_read_keeps_to_the_chosen_source() {
    let mut b = live(&["markReadBatch"]);
    b.wr_set_source("sub:b");
    assert_eq!(v(b.mark_all_ask(NOW))["n"], 0);
    b.wr_set_source("sub:a");
    b.wr_set_lens("thread");
    assert_eq!(v(b.mark_all_ask(NOW))["n"], 0);
    b.wr_set_lens("fragment");
    assert_eq!(v(b.mark_all_ask(NOW))["label"], "Sub a · fragments");
    assert_eq!(v(b.mark_all_read(NOW))["n"], 2);
}

#[test]
fn a_failed_batch_puts_its_rows_back() {
    let mut b = live(&["markReadBatch"]);
    let out = v(b.mark_all_read(NOW));
    assert_eq!(unread(&b), 0);
    let prior = out["batches"][0]["prior"].to_string();
    b.mark_all_rollback(&prior);
    // a1 unread again; a2 back to read at v2 (shown first, edited since read).
    let view = v(b.reading_view(NOW, "", false));
    assert_eq!(view["unread"], 1);
    assert_eq!(v(b.mark_all_ask(NOW))["n"], 2);
}

#[test]
fn mark_all_read_drops_queued_marks_for_its_rows() {
    let mut b = live(&["markReadBatch"]);
    // Marked unread here, not sent yet: the batch supersedes it.
    b.set_read("b|b1", false);
    assert_eq!(v(b.outbox_next("[]"))["op"], "read");
    v(b.mark_all_read(NOW));
    assert_eq!(v(b.outbox_next("[]")), Value::Null);
}

#[test]
fn offline_or_without_batches_nothing_changes() {
    let mut b = live(&[]);
    assert!(v(b.mark_all_read(NOW))["error"].is_string());
    assert_eq!(unread(&b), 1);
    let mut b = live(&["markReadBatch"]);
    b.set_offline(true);
    assert_eq!(v(b.mark_all_ask(NOW))["offline"], true);
    assert!(v(b.mark_all_read(NOW))["error"].is_string());
    assert_eq!(unread(&b), 1);
}

#[test]
fn batches_hold_500_marks_at_most() {
    use crate::parity::mark_all::{mark_of, read_batches};
    let marks: Vec<_> = (0..1201)
        .filter_map(|i| mark_of(&format!("s|r{i}"), 1))
        .collect();
    let reqs = read_batches(&marks);
    assert_eq!(reqs.len(), 3);
    assert_eq!(
        reqs[2].body.as_ref().unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        201
    );
    assert!(mark_of("no-bar", 1).is_none());
}

#[test]
fn the_demo_marks_all_read_here_only() {
    let mut b = Burrow::new(NOW, None);
    let before = v(b.mark_all_ask(NOW))["n"].as_u64().unwrap();
    assert!(before > 0, "the sample reading has unread posts");
    let out = v(b.mark_all_read(NOW));
    assert_eq!(out["n"].as_u64().unwrap(), before);
    assert_eq!(out["batches"], json!([]));
    assert_eq!(v(b.mark_all_ask(NOW))["n"], 0);
}
