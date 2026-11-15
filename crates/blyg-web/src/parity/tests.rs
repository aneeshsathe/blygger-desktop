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

// ------------------------------------------------------------ post filters

fn wire(id: &str, rev: u64, md: &str, status: &str, version: u32, dirty: bool) -> Value {
    json!({
        "id": id, "revision": rev, "kind": "thread", "status": status,
        "created": "2026-11-01T10:00:00Z", "updated": "2026-11-05T10:00:00Z",
        "version": version, "contentMd": md, "dirty": dirty, "stubOf": null,
        "forkedFrom": null, "provenance": [], "responses": "default", "highlight": "default"
    })
}

/// Your posts: a draft, a public post, a public one with changes, a
/// withdrawn one.
fn own(ops: &[&str]) -> Burrow {
    let mut b = live(ops);
    let items = json!([
        wire(
            "d000000000000000000000000d",
            1,
            "Sample draft about tides",
            "draft",
            0,
            false
        ),
        wire(
            "p000000000000000000000000p",
            4,
            "Sample public note on harbours",
            "public",
            2,
            false
        ),
        wire(
            "c000000000000000000000000c",
            6,
            "Sample changed thread on gulls",
            "public",
            1,
            true
        ),
        wire(
            "w000000000000000000000000w",
            9,
            "Sample withdrawn note",
            "withdrawn",
            3,
            false
        ),
    ]);
    b.live_items(&items.to_string());
    b
}

fn titles(b: &Burrow) -> Vec<String> {
    v(b.view(NOW))["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["title_html"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn filters_count_as_the_studios_summary_does() {
    let b = own(&[]);
    let f = v(b.post_filters());
    assert_eq!(f["on"], "all");
    let n: Vec<(String, u64)> = f["chips"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["key"].as_str().unwrap().into(), c["n"].as_u64().unwrap()))
        .collect();
    assert_eq!(
        n,
        [
            ("all".to_string(), 4),
            ("drafts".into(), 1),
            ("changes".into(), 1),
            ("public".into(), 2),
            ("withdrawn".into(), 1),
        ]
    );
}

#[test]
fn a_filter_narrows_the_list_and_a_search_ignores_it() {
    let mut b = own(&[]);
    assert_eq!(b.set_post_filter("withdrawn", false), "Withdrawn");
    assert_eq!(titles(&b), ["Sample withdrawn note"]);
    b.set_post_filter("changes", false);
    assert_eq!(titles(&b), ["Sample changed thread on gulls"]);
    b.set_post_filter("drafts", false);
    assert_eq!(titles(&b), ["Sample draft about tides"]);
    // A search looks through every post, so ⏎ finds the public one.
    b.set_query("harbours");
    assert_eq!(titles(&b), ["Sample public note on <mark>harbours</mark>"]);
    b.set_query("");
    assert_eq!(titles(&b), ["Sample draft about tides"]);
    // Unknown is All.
    b.set_post_filter("nonsense", false);
    assert_eq!(titles(&b).len(), 4);
}

#[test]
fn the_next_filter_comes_round_again() {
    let mut b = own(&[]);
    let labels: Vec<String> = (0..5).map(|_| b.next_post_filter(false)).collect();
    assert_eq!(labels, ["Drafts", "Changes", "Public", "Withdrawn", "All"]);
}

#[test]
fn the_open_post_stays_open_when_the_filter_hides_it() {
    let mut b = own(&[]);
    b.set_query("gulls");
    b.enter(NOW);
    let open = v(b.view(NOW))["current"]["id"].clone();
    assert!(open.is_string());
    b.set_post_filter("withdrawn", true);
    assert_eq!(v(b.view(NOW))["current"]["id"], open);
    // Without keep, the selection moves into the filtered list.
    b.set_query("");
    b.set_post_filter("drafts", false);
    assert_ne!(v(b.view(NOW))["current"]["id"], open);
    assert_eq!(titles(&b), ["Sample draft about tides"]);
}

// ------------------------------------------------------------ updates

fn report(id: &str, stale: u32, blocking: u32, dirty: bool) -> Value {
    json!({
        "id": id, "version": 2, "dirty": dirty, "stale": stale, "blocking": blocking,
        "behind": stale * 2,
        "quotes": [{ "id": "q1", "origin": "https://b.example.com", "baked": 1, "held": 3,
                     "live": 3, "partial": false, "status": "refreshable" }],
    })
}

#[test]
fn updates_list_stalest_first_with_the_desktops_reasons() {
    let mut b = own(&["listStaleThreads", "refreshItem", "getItemFreshness"]);
    let body = json!({ "items": [
        report("p000000000000000000000000p", 1, 0, false),
        report("c000000000000000000000000c", 2, 0, false),
        report("z000000000000000000000000z", 3, 0, false),
    ]});
    assert_eq!(v(b.upd_loaded(200, &body.to_string()))["n"], 3);
    let view = v(b.upd_view());
    assert_eq!(view["canRefresh"], true);
    let rows = view["rows"].as_array().unwrap();
    // Stalest first: z (not held here), c (changed here), p.
    assert_eq!(rows[0]["sid"], "z000000000000000000000000z");
    assert_eq!(rows[0]["why"], "This thread isn't in this browser yet");
    assert_eq!(
        rows[1]["why"],
        "It has unpublished edits: publish or discard them first"
    );
    assert_eq!(rows[2]["why"], Value::Null);
    assert_eq!(rows[2]["line"], "2 versions behind · 1 stale quote");
    assert_eq!(rows[2]["title"], "Sample public note on harbours");
}

#[test]
fn quotes_show_what_changed() {
    let mut b = own(&["listStaleThreads", "getItemFreshness", "refreshItem"]);
    let sid = "p000000000000000000000000p";
    b.upd_loaded(
        200,
        &json!({ "items": [report(sid, 1, 0, false)] }).to_string(),
    );
    assert_eq!(b.upd_probe_path(sid), format!("/api/items/{sid}/freshness"));
    b.upd_probed(sid, 200, &report(sid, 1, 0, false).to_string());
    let row = &v(b.upd_view())["rows"][0];
    assert_eq!(row["quotes"][0]["label"], "b.example.com");
    assert_eq!(row["quotes"][0]["words"], "v1 → v3 available");
    assert_eq!(row["quotes"][0]["stale"], true);
    // A blocking quote found on asking: refused.
    b.upd_probed(sid, 200, &report(sid, 1, 1, false).to_string());
    let r = v(b.upd_refresh_request(sid, ""));
    assert!(r["error"].as_str().unwrap().contains("edit the thread"));
}

#[test]
fn refresh_sends_the_revision_and_note_then_leaves_the_list() {
    let mut b = own(&["listStaleThreads", "refreshItem"]);
    let sid = "p000000000000000000000000p";
    b.upd_loaded(
        200,
        &json!({ "items": [report(sid, 1, 0, false)] }).to_string(),
    );
    let r = v(b.upd_refresh_request(sid, "  "));
    assert_eq!(r["req"]["method"], "POST");
    assert_eq!(r["req"]["path"], format!("/api/items/{sid}/refresh"));
    assert_eq!(r["req"]["body"]["expectedRevision"], 4);
    assert_eq!(r["req"]["body"]["note"], "refreshed quoted snapshots");
    let r = v(b.upd_refresh_request(sid, " new quotes "));
    assert_eq!(r["req"]["body"]["note"], "new quotes");
    let answer =
        json!({ "ok": true, "revision": 5, "version": 3, "refreshed": ["q1"], "resynced": 1 });
    let out = v(b.upd_refreshed(sid, 200, &answer.to_string()));
    assert_eq!(
        out["toast"],
        "v3 · 1 quote refreshed · 1 blyg fetched again"
    );
    assert_eq!(v(b.upd_view())["rows"], json!([]));
}

#[test]
fn a_refused_refresh_stays_listed_with_why() {
    let mut b = own(&["listStaleThreads", "refreshItem"]);
    let sid = "p000000000000000000000000p";
    b.upd_loaded(
        200,
        &json!({ "items": [report(sid, 1, 0, false)] }).to_string(),
    );
    let err = json!({ "code": "invalid-input", "message": "A quote no longer resolves" });
    let out = v(b.upd_refreshed(sid, 400, &err.to_string()));
    assert!(out["error"].is_string());
    let row = &v(b.upd_view())["rows"][0];
    assert_eq!(row["failed"], out["error"]);
}

#[test]
fn updates_are_gated_on_the_operations() {
    let mut b = own(&["listStaleThreads"]);
    let sid = "p000000000000000000000000p";
    b.upd_loaded(
        200,
        &json!({ "items": [report(sid, 1, 0, false)] }).to_string(),
    );
    assert_eq!(v(b.upd_view())["canRefresh"], false);
    assert!(v(b.upd_refresh_request(sid, ""))["error"].is_string());
    let b = own(&[]);
    assert_eq!(v(b.upd_view())["can"], false);
    // 401: the page asks to sign in.
    let mut b = own(&["listStaleThreads"]);
    assert_eq!(v(b.upd_loaded(401, "{}"))["code"], "unauthorized");
}

// ------------------------------------------------------------ access tokens

fn grant(id: &str, name: &str, manual: bool) -> Value {
    json!({ "id": id, "clientId": format!("client-{id}"), "name": name, "manual": manual,
            "resource": "https://blyg.example.com/api", "scope": ["owner:read"],
            "createdAt": 1_793_966_400_i64, "expiresAt": 1_796_558_400_i64 })
}

#[test]
fn access_lists_the_grants() {
    let mut b = own(&[]);
    assert_eq!(v(b.acc_view())["state"], "loading");
    let body =
        json!({ "items": [grant("g1", "Sample tool", true), grant("g2", "Sample app", false)] });
    assert_eq!(v(b.acc_loaded(200, &body.to_string()))["n"], 2);
    let view = v(b.acc_view());
    assert_eq!(view["state"], "ready");
    assert_eq!(view["rows"][0]["kind"], "manual token");
    assert_eq!(view["rows"][1]["kind"], "OAuth");
    assert_eq!(view["rows"][0]["scopes"], "owner:read");
    assert_eq!(view["rows"][0]["created"], "6 Nov 2026");
    assert_eq!(view["rows"][0]["expires"], "6 Dec 2026");
    assert_eq!(view["scopes"].as_array().unwrap().len(), 5);
    // No client access on this blyg.
    let mut b = own(&[]);
    b.acc_loaded(404, "{}");
    assert_eq!(v(b.acc_view())["state"], "unsupported");
    let mut b = own(&[]);
    assert_eq!(v(b.acc_loaded(401, "{}"))["code"], "unauthorized");
}

#[test]
fn a_token_request_is_what_the_studio_takes() {
    let b = own(&[]);
    let r = v(b.acc_create_request(
        "  Sample tool ",
        r#"["reading:state","owner:read","nonsense","owner:read"]"#,
        "mcp",
    ));
    assert_eq!(r["req"]["path"], "/api/authorizations");
    assert_eq!(
        r["req"]["body"],
        json!({ "name": "Sample tool", "scope": ["owner:read", "reading:state"], "resource": "mcp" })
    );
    assert!(v(b.acc_create_request(" ", r#"["owner:read"]"#, "api"))["error"].is_string());
    assert!(v(b.acc_create_request("x", "[]", "api"))["error"].is_string());
    let long = "x".repeat(101);
    assert!(v(b.acc_create_request(&long, r#"["owner:read"]"#, "api"))["error"].is_string());
}

#[test]
fn a_new_token_is_handed_over_once_and_not_kept() {
    let mut b = own(&[]);
    b.acc_loaded(200, r#"{"items":[]}"#);
    let secret = "sample-not-a-real-token-0123456789";
    let answer = json!({ "authorization": grant("g3", "Sample tool", true), "accessToken": secret,
                         "tokenType": "Bearer", "expiresIn": 2_592_000 });
    let out = v(b.acc_created(200, &answer.to_string()));
    assert_eq!(out["token"], secret);
    assert_eq!(out["days"], 30);
    // Burrow keeps the grant, never the token: not in any view, nor in
    // what the page persists.
    assert_eq!(v(b.acc_view())["rows"][0]["name"], "Sample tool");
    assert!(!b.acc_view().contains(secret));
    assert!(!b.to_json().contains(secret));
    assert!(!b.reading_json().contains(secret));
}

#[test]
fn revoking_one_or_all() {
    let mut b = own(&[]);
    let body =
        json!({ "items": [grant("g1", "Sample tool", true), grant("g2", "Sample app", false)] });
    b.acc_loaded(200, &body.to_string());
    let r = v(b.acc_revoke_request("g1"));
    assert_eq!(r["req"]["method"], "DELETE");
    assert_eq!(r["req"]["path"], "/api/authorizations/g1");
    assert_eq!(
        v(b.acc_revoked("g1", 200, r#"{"ok":true}"#))["toast"],
        "Revoked"
    );
    assert_eq!(v(b.acc_view())["rows"].as_array().unwrap().len(), 1);
    assert_eq!(
        v(b.acc_revoke_request("all"))["req"]["path"],
        "/api/authorizations"
    );
    b.acc_revoked("all", 200, r#"{"ok":true}"#);
    assert_eq!(v(b.acc_view())["rows"], json!([]));
    // Refused: kept.
    let mut b = own(&[]);
    b.acc_loaded(200, &body.to_string());
    assert!(v(b.acc_revoked("g1", 403, r#"{"message":"no"}"#))["error"].is_string());
    assert_eq!(v(b.acc_view())["rows"].as_array().unwrap().len(), 2);
}
