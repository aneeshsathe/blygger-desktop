//! --- web-reading --- The Reader's side in a browser, driven as
//! `www/web-reading.js` drives it, with the blyg's answers written out by
//! hand (shapes from contract 5.7.0).

use serde_json::{Value, json};

use crate::Burrow;

/// 2026-11-06T12:00:00Z
const NOW: f64 = 1_793_966_400_000.0;
const BASE: &str = "https://blyg.example.com";

fn v(s: String) -> Value {
    serde_json::from_str(&s).unwrap()
}

fn row(sub: &str, rid: &str, kind: &str, md: &str) -> Value {
    json!({
        "subscriptionId": sub, "remoteId": rid, "subscriptionTitle": format!("Sub {sub}"),
        "origin": format!("https://{sub}.example.com"), "kind": kind, "version": 1,
        "created": "2026-11-04T09:00:00Z", "updated": "2026-11-04T09:00:00Z",
        "observedAt": "2026-11-04T09:05:00Z", "contentMd": md,
        "contentHtml": format!("<p>{md}</p>"), "readVersion": null, "hoppers": []
    })
}

fn sub(id: &str, status: &str) -> Value {
    json!({
        "id": id, "kind": "blyg", "origin": format!("https://{id}.example.com/"),
        "feedUrl": format!("https://{id}.example.com/feed.xml"), "title": format!("Sub {id}"),
        "status": status, "lastPollAt": "2026-11-06T11:00:00Z", "failCount": if status == "degraded" { 4 } else { 0 },
        "lastIndexSyncAt": null, "inBlogroll": false,
        "flags": if status == "degraded" { json!([{"type": "fetch-failed", "at": "2026-11-06T11:00:00Z", "detail": "502 from the feed"}]) } else { json!([]) },
        "created": "2026-11-01T00:00:00Z", "titleFollowsSource": true, "hopperFeedUrl": null
    })
}

/// A live Burrow reading two subscriptions: `a` (two fragments, one
/// linking to your blyg) and `b` (a thread), and one hopper.
fn live() -> Burrow {
    let mut b = Burrow::new_live(NOW, None, None, BASE);
    let rows = json!([
        row("a", "a1", "fragment", "Tides again."),
        row(
            "a",
            "a2",
            "fragment",
            "See https://blyg.example.com/f/x for more."
        ),
        row("b", "b1", "thread", "Long thoughts\n\nOn harbours."),
    ]);
    let hoppers = json!([{ "id": "h1", "name": "Keep", "slug": null, "public": false }]);
    b.live_reading(&rows.to_string(), &hoppers.to_string());
    b
}

fn keys(b: &Burrow) -> Vec<String> {
    v(b.reading_view(NOW, "", false))["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["key"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_source_and_a_lens_filter_the_stream() {
    let mut b = live();
    assert_eq!(keys(&b).len(), 3);
    b.wr_set_source("sub:a");
    assert_eq!(keys(&b), ["a|a1", "a|a2"]);
    assert_eq!(b.wr_source_label(), "Sub a");
    b.wr_set_source("smart:all");
    b.wr_set_lens("thread");
    assert_eq!(keys(&b), ["b|b1"]);
    assert_eq!(b.wr_source_label(), "All · threads");
    b.wr_set_source("hopper:h1");
    let view = v(b.reading_view(NOW, "", false));
    assert_eq!(view["total"], 0);
    assert!(
        view["empty"]
            .as_str()
            .unwrap()
            .starts_with("No threads here.")
    );
    b.wr_clear_source();
    assert_eq!(keys(&b).len(), 3);
    // The unread count stays the whole list's (the tab's badge).
    b.wr_set_source("sub:b");
    assert_eq!(v(b.reading_view(NOW, "", false))["unread"], 3);
}

#[test]
fn the_sources_pane_lists_feeds_mentions_hoppers_and_subscriptions() {
    let b = live();
    let s = v(b.wr_sources(NOW));
    let rows = s["rows"].as_array().unwrap();
    let labels: Vec<&str> = rows.iter().map(|r| r["label"].as_str().unwrap()).collect();
    assert_eq!(
        labels,
        [
            "Smart feeds",
            "All unread",
            "Today",
            "Thumbed",
            "All",
            "Background",
            "Mentions",
            "Hoppers",
            "Keep",
            "Subscriptions",
            "Sub a",
            "Sub b"
        ]
    );
    // Before /api/subscriptions is read, the reading list names them.
    assert_eq!(s["subsLoaded"], false);
    let unread = rows.iter().find(|r| r["key"] == "smart:unread").unwrap();
    assert_eq!(unread["count"], 3);
    // A post linking to your blyg is in Mentions: a dot, never a count.
    let m = rows.iter().find(|r| r["key"] == "mentions").unwrap();
    assert_eq!(m["dot"], true);
    assert!(m["count"].is_null());
    let mut b = b;
    b.wr_set_source("mentions");
    assert_eq!(keys(&b), ["a|a2"]);
    // The blyg's own lens wins once read.
    b.wr_loaded(
        "lens",
        &json!([{ "key": "x", "imported": { "subscriptionId": "b", "remoteId": "b1" } }])
            .to_string(),
    );
    assert_eq!(keys(&b), ["b|b1"]);
}

#[test]
fn withdrawn_posts_are_only_in_background() {
    let mut b = Burrow::new_live(NOW, None, None, BASE);
    let mut gone = row("a", "a9", "fragment", "Withdrawn.");
    gone["withdrawn"] = json!(true);
    gone["thumb"] = json!(1);
    b.live_reading(
        &json!([row("a", "a1", "fragment", "Here."), gone]).to_string(),
        "[]",
    );
    assert_eq!(keys(&b), ["a|a1"]);
    b.wr_set_source("smart:background");
    assert_eq!(keys(&b), ["a|a9"]);
}

#[test]
fn subscriptions_are_read_with_how_their_polling_goes() {
    let mut b = live();
    assert_eq!(v(b.wr_list("subs"))["path"], "/api/subscriptions");
    let r = v(b.wr_loaded(
        "subs",
        &json!([sub("a", "active"), sub("b", "degraded"), sub("c", "paused")]).to_string(),
    ));
    assert_eq!(r["n"], 3);
    assert!(b.wr_has("subs"));
    let view = v(b.wr_subs_view(NOW));
    let rows = view["rows"].as_array().unwrap();
    assert_eq!(rows[0]["state"], "checked 1h");
    assert_eq!(rows[0]["held"], 2);
    assert_eq!(
        rows[1]["state"],
        "Having trouble: 4 failed checks · 502 from the feed"
    );
    assert_eq!(rows[1]["trouble"], true);
    assert_eq!(rows[2]["state"], "Paused");
    let s = v(b.wr_sources(NOW));
    let c = s["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == "sub:c")
        .unwrap()
        .clone();
    assert_eq!(c["note"], "Paused");
}

#[test]
fn subscribing_previews_then_confirms_for_the_whole_blyg() {
    let mut b = live();
    b.wr_set_contract("5.7.0");
    let r = v(b.wr_do("preview", r#"{"url":"https://new.example.com/"}"#, NOW));
    assert_eq!(r["req"]["method"], "POST");
    assert_eq!(r["req"]["path"], "/api/subscriptions");
    assert_eq!(
        r["req"]["body"],
        json!({ "url": "https://new.example.com/", "wholeBlyg": true })
    );
    let p = v(b.wr_done(
        "preview",
        r#"{"url":"https://new.example.com/"}"#,
        200,
        &json!({ "needsConfirm": true, "kind": "blyg", "origin": "https://new.example.com/", "feedUrl": "https://new.example.com/feed.xml", "title": "New Notes" }).to_string(),
        NOW,
    ));
    assert_eq!(p["preview"]["title"], "New Notes");
    assert!(p["preview"]["mismatch"].is_null());
    let r = v(b.wr_do(
        "subscribe",
        r#"{"url":"https://new.example.com/","title":"New Notes"}"#,
        NOW,
    ));
    assert_eq!(
        r["req"]["body"],
        json!({ "url": "https://new.example.com/", "wholeBlyg": true, "confirm": true, "title": "New Notes" })
    );
    let mut created = sub("new", "active");
    created["title"] = json!("New Notes");
    let d = v(b.wr_done("subscribe", "{}", 201, &created.to_string(), NOW));
    assert!(
        d["toast"]
            .as_str()
            .unwrap()
            .starts_with("Subscribed to New Notes")
    );
    assert_eq!(d["reload"], json!(["reading"]));
    // A 5.0 studio refuses the key: it isn't sent.
    b.wr_set_contract("5.0.3");
    let r = v(b.wr_do("preview", r#"{"url":"https://x.example.com/"}"#, NOW));
    assert!(r["req"]["body"].get("wholeBlyg").is_none());
    // Nothing to send without an address.
    assert!(v(b.wr_do("preview", r#"{"url":"  "}"#, NOW))["error"].is_string());
    assert!(v(b.wr_do("preview", r#"{"url":"not a url"}"#, NOW))["error"].is_string());
}

#[test]
fn managing_a_subscription_sends_what_changed() {
    let mut b = live();
    b.wr_loaded(
        "subs",
        &json!([sub("a", "active"), sub("b", "active")]).to_string(),
    );
    let pause = v(b.wr_do("pause", r#"{"id":"a","on":true}"#, NOW));
    assert_eq!(pause["req"]["method"], "PATCH");
    assert_eq!(pause["req"]["path"], "/api/subscriptions/a");
    assert_eq!(pause["req"]["body"], json!({ "paused": true }));
    let rename = v(b.wr_do("rename", r#"{"id":"a","title":"Tides"}"#, NOW));
    assert_eq!(rename["req"]["body"], json!({ "title": "Tides" }));
    // An empty name hands it back to the source.
    let back = v(b.wr_do("rename", r#"{"id":"a","title":""}"#, NOW));
    assert_eq!(back["req"]["body"], json!({ "title": null }));
    let blog = v(b.wr_do("blogroll", r#"{"id":"a","on":true}"#, NOW));
    assert_eq!(blog["req"]["body"], json!({ "inBlogroll": true }));
    let mut renamed = sub("a", "active");
    renamed["title"] = json!("Tides");
    renamed["titleFollowsSource"] = json!(false);
    b.wr_done(
        "rename",
        r#"{"id":"a","title":"Tides"}"#,
        200,
        &renamed.to_string(),
        NOW,
    );
    let rows = v(b.reading_view(NOW, "", false));
    assert!(
        rows["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["who"] == "Tides"),
        "the rows take the new name"
    );
    assert_eq!(
        v(b.wr_do("resync", r#"{"id":"a"}"#, NOW))["req"]["path"],
        "/api/subscriptions/a/resync"
    );
    assert_eq!(
        v(b.wr_do("poll", "{}", NOW))["req"]["path"],
        "/api/subscriptions/poll"
    );
    // Unsubscribing drops its posts, and a stream showing it shows All.
    b.wr_set_source("sub:a");
    let un = v(b.wr_do("unsubscribe", r#"{"id":"a"}"#, NOW));
    assert_eq!(un["req"]["method"], "DELETE");
    b.wr_done("unsubscribe", r#"{"id":"a"}"#, 200, r#"{"ok":true}"#, NOW);
    assert_eq!(b.wr_source(), "smart:all");
    assert_eq!(keys(&b), ["b|b1"]);
    assert!(b.reading_dirty());
}

#[test]
fn offline_a_change_needs_the_blyg() {
    let mut b = live();
    b.set_offline(true);
    assert!(v(b.wr_do("poll", "{}", NOW))["error"].is_string());
}

#[test]
fn a_thumb_shows_at_once_and_goes_back_if_refused() {
    let mut b = live();
    let r = v(b.wr_do("thumb", r#"{"key":"a|a1","thumb":1}"#, NOW));
    assert_eq!(r["req"]["method"], "PUT");
    assert_eq!(r["req"]["path"], "/api/signals/a/a1");
    assert_eq!(r["req"]["body"], json!({ "thumb": 1 }));
    assert_eq!(v(b.wr_post_extra("a|a1"))["thumb"], 1);
    b.wr_set_source("smart:thumbed");
    assert_eq!(keys(&b), ["a|a1"]);
    let args = json!({ "key": "a|a1", "thumb": 1, "prev": r["prev"] }).to_string();
    let e = v(b.wr_done("thumb", &args, 500, r#"{"code":"x","message":"nope"}"#, NOW));
    assert!(e["error"].is_string());
    assert!(v(b.wr_post_extra("a|a1"))["thumb"].is_null());
    // Clearing one is a DELETE.
    b.wr_do("thumb", r#"{"key":"a|a1","thumb":-1}"#, NOW);
    let r = v(b.wr_do("thumb", r#"{"key":"a|a1","thumb":0}"#, NOW));
    assert_eq!(r["req"]["method"], "DELETE");
}

#[test]
fn hoppers_are_made_edited_and_deleted() {
    let mut b = live();
    assert!(v(b.wr_do("hopper_new", r#"{"name":"  "}"#, NOW))["error"].is_string());
    let r = v(b.wr_do("hopper_new", r#"{"name":"Harbours"}"#, NOW));
    assert_eq!(r["req"]["body"], json!({ "name": "Harbours" }));
    b.wr_done(
        "hopper_new",
        "{}",
        201,
        r#"{"id":"h2","name":"Harbours","slug":null,"public":false,"slugFrozen":false,"description":null}"#,
        NOW,
    );
    let e = v(b.wr_do(
        "hopper_edit",
        r#"{"id":"h2","public":true,"description":"Boats"}"#,
        NOW,
    ));
    assert_eq!(e["req"]["path"], "/api/hoppers/h2");
    assert_eq!(
        e["req"]["body"],
        json!({ "description": "Boats", "public": true })
    );
    b.wr_done(
        "hopper_edit",
        "{}",
        200,
        r#"{"id":"h2","name":"Harbours","slug":"harbours","public":true,"slugFrozen":true,"description":"Boats"}"#,
        NOW,
    );
    let hs = v(b.wr_hoppers_view());
    assert_eq!(hs[1]["url"], "https://blyg.example.com/h/harbours/");
    assert_eq!(hs[1]["description"], "Boats");
    let long = "x".repeat(281);
    assert!(
        v(b.wr_do(
            "hopper_edit",
            &json!({ "id": "h2", "description": long }).to_string(),
            NOW
        ))["error"]
            .is_string()
    );
    // Deleting one takes it off its posts, and drops changes still waiting.
    b.toggle_hopper("a|a1", "h1");
    b.wr_set_source("hopper:h1");
    assert_eq!(keys(&b), ["a|a1"]);
    b.wr_done(
        "hopper_delete",
        r#"{"id":"h1"}"#,
        200,
        r#"{"ok":true}"#,
        NOW,
    );
    assert_eq!(b.wr_source(), "smart:all");
    assert_eq!(b.outbox_next("[]"), "null");
    assert_eq!(v(b.wr_hoppers_view()).as_array().unwrap().len(), 1);
}

#[test]
fn mentions_are_grouped_under_your_posts_and_hidden_one_by_one() {
    let mut b = Burrow::new(NOW, None);
    b.wr_demo_lists(NOW);
    let m = v(b.wr_mentions_view(NOW));
    let groups = m["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0]["rows"][0]["relation"], "replied");
    assert_eq!(groups[0]["rows"][0]["new"], true);
    assert_eq!(m["accept"], true);
    // Seen once, no longer new.
    let again = v(b.wr_mentions_view(NOW));
    assert_eq!(again["groups"][0]["rows"][0]["new"], false);
    let id = groups[0]["rows"][0]["id"].as_str().unwrap().to_string();
    let t = v(b.wr_do(
        "mention_hide",
        &json!({ "id": id, "on": true }).to_string(),
        NOW,
    ));
    assert_eq!(t["toast"], "Hidden");
    let after = v(b.wr_mentions_view(NOW));
    assert_eq!(after["groups"][0]["rows"][0]["hidden"], true);
}

#[test]
fn live_mentions_hide_and_the_accept_toggle_are_sent() {
    let mut b = live();
    b.wr_loaded(
        "mentions",
        &json!([{ "id": "m1", "targetItemId": "zz", "status": "pending", "relation": "stub",
                  "source": "https://rue.example.com/t/1", "sourceOrigin": "https://rue.example.com/",
                  "firstSeen": "2026-11-06T10:00:00Z", "hidden": false }])
        .to_string(),
    );
    b.wr_loaded(
        "outbound",
        &json!([{ "id": "o1", "itemId": "bbbb", "version": 1, "target": "https://ada.example.com/t/9",
                  "status": "failed", "attempts": 3, "lastError": "timeout", "created": "2026-11-05T10:00:00Z" }])
        .to_string(),
    );
    b.wr_loaded("settings", r#"{"acceptMentions":false}"#);
    let m = v(b.wr_mentions_view(NOW));
    assert_eq!(m["groups"][0]["rows"][0]["status"], "not checked yet");
    assert_eq!(m["outbound"][0]["status"], "failed after 3 tries");
    assert_eq!(m["outbound"][0]["error"], "timeout");
    assert_eq!(m["accept"], false);
    let h = v(b.wr_do("mention_hide", r#"{"id":"m1","on":true}"#, NOW));
    assert_eq!(h["req"]["path"], "/api/mentions/m1");
    assert_eq!(h["req"]["body"], json!({ "hidden": true }));
    // Refused: it shows again.
    b.wr_done("mention_hide", r#"{"id":"m1","on":true}"#, 403, "{}", NOW);
    assert_eq!(
        v(b.wr_mentions_view(NOW))["groups"][0]["rows"][0]["hidden"],
        false
    );
    let a = v(b.wr_do("accept_mentions", r#"{"on":true}"#, NOW));
    assert_eq!(a["req"]["method"], "PATCH");
    assert_eq!(a["req"]["path"], "/api/settings");
    assert_eq!(a["req"]["body"], json!({ "acceptMentions": true }));
    b.wr_done(
        "accept_mentions",
        r#"{"on":true}"#,
        200,
        r#"{"acceptMentions":true}"#,
        NOW,
    );
    assert_eq!(v(b.wr_mentions_view(NOW))["accept"], true);
}

#[test]
fn opml_marks_what_you_follow_already_and_exports() {
    let mut b = live();
    b.wr_loaded("subs", &json!([sub("a", "active")]).to_string());
    let opml = r#"<?xml version="1.0"?><opml version="2.0"><body>
      <outline text="Sub a" xmlUrl="https://a.example.com/feed.xml"/>
      <outline text="Other" xmlUrl="https://other.example.net/rss"/>
      <outline text="Other again" xmlUrl="https://other.example.net/rss"/>
    </body></opml>"#;
    let p = v(b.wr_opml(opml));
    assert_eq!(p["feeds"][0]["followed"], true);
    assert_eq!(p["feeds"][1]["followed"], false);
    assert_eq!(p["duplicates"], 1);
    assert!(v(b.wr_opml("hello"))["error"].is_string());
    assert!(
        b.wr_opml_export()
            .contains("https://a.example.com/feed.xml")
    );
}

#[test]
fn peeking_reads_a_post_you_dont_follow_and_logs_it() {
    let mut b = live();
    // An address of a post held here opens it.
    let held = v(b.wr_do("peek_url", r#"{"url":"https://a.example.com/f/a1/"}"#, NOW));
    assert_eq!(held["open"], "a|a1");
    let r = v(b.wr_do(
        "peek_url",
        r#"{"url":"https://far.example.org/t/X1/"}"#,
        NOW,
    ));
    assert_eq!(r["req"]["path"], "/api/peek/links");
    let next = v(b.wr_done(
        "peek_url",
        "{}",
        200,
        r#"{"links":[{"url":"https://far.example.org/t/X1/","target":{"origin":"https://far.example.org/","id":"X1"}}]}"#,
        NOW,
    ));
    assert_eq!(next["next"]["action"], "peek");
    let args = next["next"]["args"].to_string();
    let r = v(b.wr_do("peek", &args, NOW));
    assert_eq!(
        r["req"]["path"],
        "/api/peek?origin=https%3A%2F%2Ffar.example.org%2F&id=X1"
    );
    let p = v(b.wr_done(
        "peek",
        &args,
        200,
        r#"{"origin":"https://far.example.org/","id":"X1","kind":"thread","version":2,
            "contentHtml":"<h1>Far away</h1><p>Hello <script>x()</script>there.</p>",
            "page":null,"url":"https://far.example.org/t/X1/","stubOf":null,"forkedFrom":null,"subscriptionId":null}"#,
        NOW,
    ));
    assert_eq!(p["peek"]["title"], "Far away");
    assert!(!p["peek"]["html"].as_str().unwrap().contains("script"));
    assert_eq!(p["peek"]["followed"], false);
    assert_eq!(p["fire"]["path"], "/api/browse");
    assert_eq!(p["fire"]["body"]["via"], "start");
    assert_eq!(p["fire"]["body"]["remoteId"], "X1");
    // A page that isn't a blyg post says so.
    let none = v(b.wr_done(
        "peek_url",
        "{}",
        200,
        r#"{"links":[{"url":"x","target":null}]}"#,
        NOW,
    ));
    assert!(none["error"].is_string());
    b.wr_loaded(
        "history",
        r#"{"items":[{"id":1,"at":"2026-11-06T11:00:00Z","trail":"t","parentId":null,"origin":"https://far.example.org/","remoteId":"X1","version":2,"via":"start","title":"Far away"}],"page":{"limit":100,"nextCursor":null}}"#,
    );
    let h = v(b.wr_history_view(NOW));
    assert_eq!(h["rows"][0]["title"], "Far away");
    assert_eq!(h["rows"][0]["host"], "far.example.org");
}

#[test]
fn the_demo_changes_its_sample_data_and_sends_nothing() {
    let mut b = Burrow::new(NOW, None);
    assert!(v(b.wr_list("subs")).is_null());
    let before = v(b.wr_subs_view(NOW))["rows"].as_array().unwrap().len();
    let p = v(b.wr_do("preview", r#"{"url":"https://new.example.com/"}"#, NOW));
    assert_eq!(p["preview"]["kind"], "blyg");
    let s = v(b.wr_do("subscribe", r#"{"url":"https://new.example.com/"}"#, NOW));
    assert!(s["toast"].as_str().unwrap().starts_with("Subscribed"));
    assert_eq!(
        v(b.wr_subs_view(NOW))["rows"].as_array().unwrap().len(),
        before + 1
    );
    let h = v(b.wr_do("hopper_new", r#"{"name":"Demo"}"#, NOW));
    assert!(h["req"].is_null());
    assert!(
        v(b.wr_hoppers_view())
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["name"] == "Demo")
    );
}
