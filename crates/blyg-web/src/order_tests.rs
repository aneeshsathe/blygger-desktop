//! The one list order (`blyg_core::order`) in a browser, and your own posts
//! among the Reading rows (`blyg_core::own`), driven as `www/live.js` drives
//! them, with the blyg's answers written out by hand.

use serde_json::{Value, json};

use crate::Burrow;

/// 2026-11-06T12:00:00Z
const NOW: f64 = 1_793_966_400_000.0;
const BASE: &str = "https://blyg.example.com";

fn v(s: String) -> Value {
    serde_json::from_str(&s).unwrap()
}

fn wire(id: &str, md: &str, status: &str, version: u32, updated: &str) -> Value {
    json!({
        "id": id, "revision": 3, "kind": "fragment", "status": status,
        "created": "2026-11-01T10:00:00Z", "updated": updated,
        "version": version, "contentMd": md, "dirty": false, "stubOf": null,
        "forkedFrom": null, "provenance": [], "responses": "default", "highlight": "default"
    })
}

/// A row as `/api/reading/imported` sends it: a whole backfill shares one
/// `observedAt` second.
fn row(sub: &str, rid: &str, updated: &str) -> Value {
    json!({
        "subscriptionId": sub, "remoteId": rid, "subscriptionTitle": "Sample notes",
        "origin": "https://b.blyg.example.com", "kind": "fragment", "version": 1,
        "created": updated, "updated": updated,
        "observedAt": "2026-11-05T18:30:43Z", "contentMd": format!("Sample note {rid}"),
        "contentHtml": "", "readVersion": null, "hoppers": []
    })
}

fn keys(b: &Burrow) -> Vec<String> {
    v(b.reading_view(NOW, "", false))["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["key"].as_str().unwrap().to_string())
        .collect()
}

fn at(day: u32, hour: u32) -> String {
    format!("2026-11-{day:02}T{hour:02}:00:00Z")
}

#[test]
fn reading_rows_come_out_newest_first_whatever_order_the_blyg_sends() {
    let mut b = Burrow::new_live(NOW, None, None, BASE);
    // The blyg's order for a backfill: one `observedAt`, then its ids.
    let rows = json!([
        row("s1", "01K9", &at(2, 9)),
        row("s1", "01K7", &at(4, 9)),
        row("s1", "01K3", &at(3, 9)),
        row("s1", "01K1", &at(5, 9)),
    ]);
    b.live_reading(&rows.to_string(), "[]");
    assert_eq!(keys(&b), ["s1|01K1", "s1|01K7", "s1|01K3", "s1|01K9"]);
    // Read again in another order: the same list.
    let mut rev: Vec<Value> = rows.as_array().unwrap().clone();
    rev.reverse();
    b.live_reading(&Value::Array(rev).to_string(), "[]");
    assert_eq!(keys(&b), ["s1|01K1", "s1|01K7", "s1|01K3", "s1|01K9"]);
    // A republish dates the post anew: it moves to the top.
    let rows = json!([
        row("s1", "01K9", &at(6, 9)),
        row("s1", "01K7", &at(4, 9)),
        row("s1", "01K3", &at(3, 9)),
        row("s1", "01K1", &at(5, 9)),
    ]);
    b.live_reading(&rows.to_string(), "[]");
    assert_eq!(keys(&b)[0], "s1|01K9");
}

#[test]
fn your_published_posts_are_among_the_reading_rows() {
    let mut b = Burrow::new_live(NOW, None, None, BASE);
    // The list's own order, chronological (the Feed defaults to
    // pace-layered, as native: `studio69_tests.rs`).
    b.w_set_settings(r#"{"readerOrder":"newest"}"#);
    let items = json!([
        wire(
            "01OWNPUBLISHED0000000000AA",
            "A published note",
            "public",
            1,
            &at(4, 12)
        ),
        wire(
            "01OWNDRAFT00000000000000AA",
            "A draft",
            "draft",
            0,
            &at(5, 12)
        ),
        wire(
            "01OWNWITHDRAWN000000000AAA",
            "Gone",
            "withdrawn",
            1,
            &at(5, 13)
        ),
    ]);
    b.live_items(&items.to_string());
    let rows = json!([row("s1", "01K1", &at(5, 9)), row("s1", "01K2", &at(3, 9))]);
    b.live_reading(&rows.to_string(), "[]");
    let own = "~own|01OWNPUBLISHED0000000000AA";
    // By date among the others; the draft and the withdrawn post aren't.
    assert_eq!(keys(&b), ["s1|01K1", own, "s1|01K2"]);
    // Never unread, and reading one sends nothing.
    let view = v(b.reading_view(NOW, "", false));
    assert_eq!(view["unread"], 2);
    b.set_read(own, false);
    b.open_reading(own, NOW);
    assert_eq!(
        v(b.outbox_next("[]")),
        Value::Null,
        "nothing sent for yours"
    );
    assert_eq!(v(b.reading_view(NOW, "", false))["unread"], 2);
    // No thumbs.
    let r = v(b.wr_do("thumb", &json!({ "key": own, "thumb": 1 }).to_string(), NOW));
    assert!(r["error"].is_string(), "{r}");
    // Read again (items or reading): one row of yours, not two.
    b.live_reading(&rows.to_string(), "[]");
    b.live_items(&items.to_string());
    assert_eq!(keys(&b).iter().filter(|k| *k == own).count(), 1);
    // Withdrawn elsewhere: it leaves the list on the next read.
    let items = json!([wire(
        "01OWNPUBLISHED0000000000AA",
        "A published note",
        "withdrawn",
        1,
        &at(5, 14)
    )]);
    b.live_items(&items.to_string());
    assert_eq!(keys(&b), ["s1|01K1", "s1|01K2"]);
}

#[test]
fn your_posts_dated_the_same_second_go_by_id_descending() {
    let mut b = Burrow::new_live(NOW, None, None, BASE);
    // The list's own order, chronological (the Feed defaults to
    // pace-layered, as native: `studio69_tests.rs`).
    b.w_set_settings(r#"{"readerOrder":"newest"}"#);
    let t = "2026-11-05T18:30:43Z";
    let items = json!([
        wire("01A03000000000000000000000", "A03", "public", 2, t),
        wire("01A13000000000000000000000", "A13", "public", 1, t),
        wire("01A05000000000000000000000", "A05", "withdrawn", 2, t),
        // Milliseconds and an offset: an instant, not text.
        wire(
            "01A01000000000000000000000",
            "A01",
            "public",
            1,
            "2026-11-05T18:30:43.120+00:00"
        ),
    ]);
    b.live_items(&items.to_string());
    let ids: Vec<String> = b
        .store
        .items()
        .iter()
        .map(|i| i.server_id.as_ref().unwrap().0.clone())
        .collect();
    assert_eq!(
        ids,
        [
            "01A01000000000000000000000",
            "01A13000000000000000000000",
            "01A05000000000000000000000",
            "01A03000000000000000000000"
        ]
    );
    // The Reading rows of yours in the same order.
    b.live_reading("[]", "[]");
    assert_eq!(
        keys(&b),
        [
            "~own|01A01000000000000000000000",
            "~own|01A13000000000000000000000",
            "~own|01A03000000000000000000000"
        ]
    );
}
