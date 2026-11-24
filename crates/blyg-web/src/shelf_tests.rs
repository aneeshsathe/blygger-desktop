//! --- shelf sync --- Driven as `www/live.js` `syncShelf` drives it, with
//! the blyg's answers written out by hand (shapes from blygger-studio
//! 0.61.0, contract 5.10.0). The whole round trip runs against a real
//! local studio in `scripts/browser-web-live-check.mjs`.

use serde_json::{Value, json};

use crate::Burrow;

/// 2026-11-06T12:00:00Z
const NOW: f64 = 1_793_966_400_000.0;
const BASE: &str = "https://blyg.example.com";
const MINE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaa";

fn v(s: String) -> Value {
    serde_json::from_str(&s).unwrap()
}

fn caps(ops: &[&str]) -> String {
    json!({
        "contractVersion": "5.10.0",
        "endpoints": { "publicBase": BASE, "studio": "/studio", "api": "/api", "issuer": BASE, "mcp": "/mcp", "media": "/media" },
        "operations": ops,
        "extensions": [],
    })
    .to_string()
}

fn row(sub: &str, rid: &str) -> Value {
    json!({
        "subscriptionId": sub, "remoteId": rid, "subscriptionTitle": format!("Sub {sub}"),
        "origin": format!("https://{sub}.example.com"), "kind": "fragment", "version": 1,
        "created": "2026-11-04T09:00:00Z", "updated": "2026-11-04T09:00:00Z",
        "observedAt": "2026-11-04T09:05:00Z", "contentMd": format!("Sample note {rid}."),
        "contentHtml": format!("<p>Sample note {rid}.</p>"), "readVersion": null, "hoppers": []
    })
}

/// A live Burrow with one post of yours on the blyg and two reading rows.
fn live(ops: &[&str]) -> Burrow {
    let mut b = Burrow::new_live(NOW, None, None, BASE);
    let items = json!([{
        "id": MINE, "revision": 3, "kind": "fragment", "status": "public",
        "created": "2026-11-01T10:00:00Z", "updated": "2026-11-05T10:00:00Z",
        "version": 1, "contentMd": "A note about tides", "dirty": false, "stubOf": null,
        "forkedFrom": null, "provenance": [], "responses": "default", "highlight": "default"
    }]);
    b.live_items(&items.to_string());
    b.live_reading(&json!([row("a", "a1"), row("b", "b1")]).to_string(), "[]");
    b.wr_loaded("capabilities", &caps(ops));
    b
}

fn served() -> Burrow {
    live(&["getShelf", "putShelf"])
}

fn shelf(entries: &[(&str, &str)], revision: u64) -> String {
    let e: Vec<Value> = entries
        .iter()
        .map(|(s, r)| json!({ "sub": s, "remoteId": r }))
        .collect();
    json!({ "entries": e, "revision": revision }).to_string()
}

fn put_entries(step: &Value) -> Vec<(String, String)> {
    step["put"]["body"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["sub"].as_str().unwrap().to_string(),
                e["remoteId"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn own_id(b: &Burrow) -> crate::LocalId {
    b.store
        .raw()
        .iter()
        .find(|i| i.server_id.as_ref().is_some_and(|s| s.0 == MINE))
        .unwrap()
        .local_id
        .clone()
}

#[test]
fn it_syncs_only_where_the_blyg_offers_it() {
    assert!(served().shelf_served());
    assert_eq!(served().shelf_state(), "waiting", "until the first sync");
    let old = live(&["getItem"]);
    assert!(!old.shelf_served());
    assert_eq!(old.shelf_state(), "local");
    let demo = Burrow::new_live(NOW, None, None, "");
    assert_eq!(demo.shelf_state(), "local");
}

#[test]
fn a_reading_post_goes_on_and_off_the_shelf() {
    let mut b = served();
    assert!(!b.reading_shelved("a|a1"));
    let o = v(b.toggle_reading_shelf("a|a1"));
    assert_eq!(o["message"], "Saved");
    assert!(b.reading_shelved("a|a1"));
    // The Shelf lens lists it.
    assert_eq!(b.shelf_order(b.rd.rows.iter().collect()).len(), 1);
    let o = v(b.toggle_reading_shelf("a|a1"));
    assert_eq!(o["message"], "Removed from Saved");
    assert!(!b.reading_shelved("a|a1"));
}

#[test]
fn the_first_sync_unions_the_lists_the_blygs_order_first() {
    let mut b = served();
    let mine = own_id(&b);
    b.store.set_on_shelf(&mine, true).unwrap();
    b.toggle_reading_shelf("a|a1");
    assert_eq!(b.shelf_state(), "waiting");
    let step = v(b.shelf_answer(200, &shelf(&[("b", "b1")], 4)));
    assert_eq!(step["put"]["method"], "PUT");
    assert_eq!(step["put"]["path"], "/api/shelf");
    assert_eq!(step["put"]["body"]["expectedRevision"], 4);
    assert_eq!(
        put_entries(&step),
        [("b", "b1"), ("~own", MINE), ("a", "a1")].map(|(s, r)| (s.to_string(), r.to_string()))
    );
    // Stored: agreed, nothing waits.
    let done = v(b.shelf_stored(
        200,
        &shelf(&[("b", "b1"), ("~own", MINE), ("a", "a1")], 5),
        true,
    ));
    assert!(done.get("put").is_none());
    assert_eq!(b.shelf_state(), "synced");
    assert!(b.reading_shelved("b|b1"));
    assert!(b.store.is_shelved(&mine));
}

#[test]
fn a_shelf_changed_elsewhere_is_laid_over_this_one() {
    let mut b = served();
    let step = v(b.shelf_answer(200, &shelf(&[("~own", MINE), ("b", "b1")], 2)));
    assert!(step.get("put").is_none(), "nothing to send: {step}");
    assert_eq!(step["changed"], true);
    assert!(b.store.is_shelved(&own_id(&b)));
    assert_eq!(b.shelf_state(), "synced");
    // Taken off elsewhere: it leaves here too.
    let step = v(b.shelf_answer(200, &shelf(&[("b", "b1")], 3)));
    assert!(step.get("put").is_none());
    assert!(!b.store.is_shelved(&own_id(&b)));
}

#[test]
fn a_conflict_merges_onto_the_blygs_list_and_sends_once_more() {
    let mut b = served();
    v(b.shelf_answer(200, &shelf(&[("b", "b1")], 2)));
    b.toggle_reading_shelf("a|a1");
    let step = v(b.shelf_answer(200, &shelf(&[("b", "b1")], 2)));
    assert_eq!(step["put"]["body"]["expectedRevision"], 2);
    // Meanwhile another computer put your post first.
    let conflict = json!({
        "code": "revision-conflict",
        "message": "shelf revision conflict; reload and try again",
        "details": { "current": { "entries": [
            { "sub": "~own", "remoteId": MINE }, { "sub": "b", "remoteId": "b1" }
        ], "revision": 3 } }
    });
    let again = v(b.shelf_stored(409, &conflict.to_string(), true));
    assert_eq!(again["put"]["body"]["expectedRevision"], 3);
    assert_eq!(
        put_entries(&again),
        [("~own", MINE), ("b", "b1"), ("a", "a1")].map(|(s, r)| (s.to_string(), r.to_string()))
    );
    // A second conflict waits for the next refresh: nothing is lost.
    let later = v(b.shelf_stored(409, &conflict.to_string(), false));
    assert!(later.get("put").is_none());
    assert!(b.reading_shelved("a|a1"));
    assert_eq!(b.shelf_state(), "waiting");
}

#[test]
fn a_refused_list_waits_for_a_change_and_a_404_keeps_it_here() {
    let mut b = served();
    v(b.shelf_answer(200, &shelf(&[], 0)));
    b.toggle_reading_shelf("a|a1");
    let bad = json!({ "code": "invalid-input", "message": "too many" });
    let r = v(b.shelf_stored(400, &bad.to_string(), true));
    assert!(r["error"].is_object());
    assert_eq!(b.shelf_state(), "synced", "not sent again as it is");
    b.toggle_reading_shelf("b|b1");
    assert_eq!(b.shelf_state(), "waiting");

    let mut gone = served();
    let r = v(gone.shelf_answer(404, r#"{"code":"not-found","message":"no route"}"#));
    assert!(r.get("put").is_none() && r.get("error").is_none());
    assert!(!gone.shelf_served());
    assert_eq!(gone.shelf_state(), "local");
}

#[test]
fn the_shelf_survives_a_reload() {
    let mut b = served();
    b.toggle_reading_shelf("a|a1");
    v(b.shelf_answer(200, &shelf(&[], 1)));
    let again = Burrow::new_live(NOW, Some(b.to_json()), None, BASE);
    assert!(again.reading_shelved("a|a1"));
    assert_eq!(again.store.live.shelf_base, b.store.live.shelf_base);
}
