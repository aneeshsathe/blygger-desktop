//! Burrow in a browser on today's studio (v0.69.0, contract 6.2.0), driven
//! as the page drives it, with the blyg's answers written out by hand:
//!
//! - the owner API is `{mount}/api`, the mount read from the page's own
//!   address (studio 0.65 dropped the host-root `/api` alias);
//! - `GET /api/changes` is gone (contract 6): every list is read
//!   conditionally (`If-None-Match`, a bodyless 304), and the reading list
//!   by its head until its safety net is due, as native sync does;
//! - contract majors 5 and 6 are spoken;
//! - Reading's tabs are studio 0.68's: Feed, Mentions, Agentic feed, Saved,
//!   Background, Raw, and the Feed follows the blyg's `readerOrder`.

use serde_json::{Value, json};

use crate::Burrow;
use crate::api::{self, Mode};

/// 2026-11-06T12:00:00Z
const NOW: f64 = 1_793_966_400_000.0;
const BASE: &str = "https://blyg.example.com";
const MIN: f64 = 60_000.0;

fn v(s: String) -> Value {
    serde_json::from_str(&s).unwrap()
}

// ---------------------------------------------------------------- the API base

#[test]
fn the_mount_is_whatever_comes_before_burrow_in_the_pages_path() {
    // A blyg at the host root.
    assert_eq!(api::mount_of("/burrow/"), "");
    assert_eq!(api::mount_of("/burrow/index.html"), "");
    // A path-mounted blyg.
    assert_eq!(api::mount_of("/notes/burrow/"), "/notes");
    assert_eq!(api::mount_of("/notes/deep/burrow/index.html"), "/notes/deep");
    // Not served as Burrow: nothing to go on, so no mount.
    assert_eq!(api::mount_of("/"), "");
    assert_eq!(api::mount_of("/notes/"), "");
    // The page asks the same question.
    assert_eq!(Burrow::mount_of("/notes/burrow/"), "/notes");
}

#[test]
fn the_api_is_under_the_mount_never_the_host_root() {
    assert_eq!(api::api_url("/burrow/", "/api/items"), "/api/items");
    assert_eq!(
        api::api_url("/notes/burrow/", "/api/items?limit=100"),
        "/notes/api/items?limit=100"
    );
    // The blyg's own address: origin and mount.
    assert_eq!(api::site_base(BASE, "/burrow/"), BASE);
    assert_eq!(
        api::site_base(BASE, "/notes/burrow/"),
        "https://blyg.example.com/notes"
    );
    // A blyg whose settings name no siteUrl is at that address, mount and
    // all, not at the bare origin.
    let site = api::site_base(BASE, "/notes/burrow/");
    assert_eq!(
        Burrow::public_base("{}", &site),
        "https://blyg.example.com/notes"
    );
}

#[test]
fn the_page_derives_its_mount_as_the_rust_does() {
    let net = include_str!("../www/net.js");
    // net.js runs before the WebAssembly (its early reads), so it has its
    // own copy of `api::mount_of`; live.js checks the two agree.
    assert!(net.contains("const i = p.lastIndexOf('/burrow/');"));
    assert!(net.contains("return i >= 0 ? p.slice(0, i) : '';"));
    assert!(net.contains("export const MOUNT = mountOf(location.pathname);"));
    assert!(net.contains("export const SITE = location.origin + MOUNT;"));
    assert!(net.contains("fetch(MOUNT + path"));
    let live = include_str!("../www/live.js");
    assert!(live.contains("Burrow.mount_of(location.pathname) !== MOUNT"));
    // The blyg's base is SITE (origin and mount), never the bare origin.
    for (name, src) in [
        ("live.js", live),
        ("writing.js", include_str!("../www/writing.js")),
    ] {
        assert!(
            !src.contains(", location.origin)"),
            "{name} must pass SITE, not location.origin"
        );
    }
}

// ---------------------------------------------------------------- the contract

#[test]
fn contracts_5_and_6_are_spoken() {
    for cv in ["5.14.0", "6.0.0", "6.2.0"] {
        let body = json!({ "contractVersion": cv, "operations": [] }).to_string();
        assert_eq!(
            api::detect(200, &body, false),
            Mode::Live {
                contract: cv.into()
            },
            "{cv}"
        );
    }
    let Mode::Unsupported { why } = api::detect(200, r#"{"contractVersion":"4.2.0"}"#, false)
    else {
        panic!("contract 4 is too old");
    };
    assert!(why.contains("5.x or 6.x"), "{why}");
    let Mode::Unsupported { why } = api::detect(200, r#"{"contractVersion":"7.0.0"}"#, false)
    else {
        panic!("contract 7 is newer than Burrow knows");
    };
    assert!(why.contains("newer than Burrow knows"), "{why}");
}

// ---------------------------------------------------------------- conditional reads

fn live() -> Burrow {
    Burrow::new_live(NOW, None, None, BASE)
}

fn reading_row(rid: &str) -> Value {
    json!({
        "subscriptionId": "sub1", "remoteId": rid, "subscriptionTitle": "Sample notes",
        "origin": "https://notes.example.com", "kind": "fragment", "version": 1,
        "created": "2026-11-04T09:00:00Z", "updated": "2026-11-04T09:00:00Z",
        "observedAt": "2026-11-04T09:05:00Z", "contentMd": format!("Note {rid}"),
        "contentHtml": format!("<p>Note {rid}</p>"), "readVersion": null, "hoppers": []
    })
}

#[test]
fn a_304_hands_back_the_body_last_read_never_an_empty_list() {
    let mut b = live();
    let path = "/api/items?limit=100";
    assert_eq!(b.cond_etag(path), None, "nothing kept: asked unconditionally");
    let body = json!({ "items": [{ "id": "a" }], "nextCursor": null }).to_string();
    let a = v(b.cond_answer(path, 200, &body, "\"e1\""));
    assert_eq!((a["status"].as_u64(), a["same"].as_bool()), (Some(200), Some(false)));
    assert_eq!(a["body"], body);
    assert_eq!(b.cond_etag(path).as_deref(), Some("\"e1\""));
    // Not modified: the body last read, byte for byte.
    let a = v(b.cond_answer(path, 304, "", ""));
    assert_eq!((a["status"].as_u64(), a["same"].as_bool()), (Some(200), Some(true)));
    assert_eq!(a["body"], body);
    // Only the cursor moved (a feed poll): the same rows.
    let moved = json!({ "items": [{ "id": "a" }], "nextCursor": "c2" }).to_string();
    let a = v(b.cond_answer(path, 200, &moved, "\"e2\""));
    assert_eq!(a["same"], true);
    assert_eq!(a["body"], moved);
    // New rows: not the same.
    let new = json!({ "items": [{ "id": "b" }, { "id": "a" }], "nextCursor": null }).to_string();
    assert_eq!(v(b.cond_answer(path, 200, &new, "\"e3\""))["same"], false);
    // A 304 for a path with nothing kept is asked again without a validator.
    assert_eq!(
        v(b.cond_answer("/api/hoppers?limit=100", 304, "", "")),
        json!({ "again": true })
    );
    // Errors pass through as they came.
    let a = v(b.cond_answer(path, 429, "{}", ""));
    assert_eq!((a["status"].as_u64(), a["same"].as_bool()), (Some(429), Some(false)));
}

#[test]
fn without_changes_every_list_is_read_conditionally_every_minute() {
    let mut b = live();
    assert!(b.changes_due(NOW), "a first refresh asks");
    // Contract 6: no route.
    let p = v(b.changes_plan(404, "", None, true, NOW));
    assert_eq!(p["validating"], true);
    assert_eq!(p["every"], 60_000.0);
    assert!(p["rev"].is_null());
    for list in ["items", "reading", "hoppers", "signals", "shelf"] {
        assert_eq!(p[list], true, "{list}");
    }
    // Not asked again for three hours (native's CHANGES_RECHECK); the
    // page plans as if it had answered 404, which keeps the clock.
    assert!(!b.changes_due(NOW + MIN));
    v(b.changes_plan(404, "", None, true, NOW + 90.0 * MIN));
    assert!(!b.changes_due(NOW + 179.0 * MIN));
    assert!(b.changes_due(NOW + 181.0 * MIN));
}

#[test]
fn a_held_reading_list_is_probed_by_its_head_until_its_net_is_due() {
    let mut b = live();
    let rows = json!([reading_row("r1"), reading_row("r2")]).to_string();
    // First read: the whole list.
    let p = v(b.changes_plan(404, "", None, true, NOW));
    assert_eq!(p["reading"], true);
    b.live_reading(&rows, "[]");
    b.changes_accept("reading", "null", NOW, Some(rows.clone()));
    // Then its first page says whether anything moved.
    let p = v(b.changes_plan(404, "", None, true, NOW + MIN));
    assert_eq!(p["reading"], "head");
    assert_eq!(p["items"], true, "every other list is read (conditionally)");
    // A hidden tab never runs the net.
    let p = v(b.changes_plan(404, "", None, false, NOW + 11.0 * MIN));
    assert_eq!(p["reading"], "head");
    // The safety net, ten minutes on: the whole list again.
    let p = v(b.changes_plan(404, "", None, true, NOW + 11.0 * MIN));
    assert_eq!(p["reading"], true);
    // It found nothing new: the net waits twice as long next time.
    b.changes_accept("reading", "null", NOW + 11.0 * MIN, Some(rows.clone()));
    let p = v(b.changes_plan(404, "", None, true, NOW + 22.0 * MIN));
    assert_eq!(p["reading"], "head");
    let p = v(b.changes_plan(404, "", None, true, NOW + 32.0 * MIN));
    assert_eq!(p["reading"], true);
}

#[test]
fn a_studio_with_changes_is_still_planned_by_its_counters() {
    let mut b = live();
    let body = json!({ "epoch": "e1", "domains": {
        "items": 5, "settings": 1, "reading": 7, "subscriptions": 2,
        "signals": 0, "hoppers": 4, "shelf": 1
    } })
    .to_string();
    let p = v(b.changes_plan(200, &body, None, true, NOW));
    assert_eq!(p["validating"], false);
    assert_eq!(p["every"], 15_000.0);
    assert!(b.changes_due(NOW + MIN));
}

#[test]
fn the_page_asks_conditionally_and_never_prefetches_changes() {
    let net = include_str!("../www/net.js");
    assert!(!net.contains("'/api/changes'"), "contract 6 has no /api/changes");
    assert!(net.contains("headers['if-none-match'] = etag;"));
    assert!(net.contains("etag: res.headers.get('etag') ?? ''"));
    let live = include_str!("../www/live.js");
    assert!(live.contains("S.B.changes_due(now())"));
    assert!(live.contains("S.B.cond_answer(path, r.status, r.body, r.etag)"));
    assert!(live.contains("plan.reading === 'head'"));
}

// ---------------------------------------------------------------- reading tabs

fn reading() -> Burrow {
    let mut b = live();
    let mut gone = reading_row("r0");
    gone["withdrawn"] = json!(true);
    gone["contentMd"] = json!("");
    gone["contentHtml"] = json!("");
    let rows = json!([reading_row("r1"), reading_row("r2"), gone]).to_string();
    b.live_reading(&rows, "[]");
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
fn the_reading_tabs_are_studios_in_its_order() {
    let b = reading();
    let s = v(b.wr_sources(NOW));
    let labels: Vec<&str> = s["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "smart" || r["kind"] == "mentions")
        .map(|r| r["label"].as_str().unwrap())
        .collect();
    let tabs = [
        "Feed",
        "Mentions",
        "Agentic feed",
        "Saved",
        "Background",
        "Raw",
    ];
    let at: Vec<usize> = tabs
        .iter()
        .map(|t| {
            labels
                .iter()
                .position(|l| l == t)
                .unwrap_or_else(|| panic!("no {t} in {labels:?}"))
        })
        .collect();
    assert!(at.windows(2).all(|w| w[0] < w[1]), "{labels:?}");
    // Each one is chosen by its key, and its name says so.
    let mut b = b;
    for (key, label) in [
        ("smart:all", "Feed"),
        ("smart:agentic", "Agentic feed"),
        ("smart:shelf", "Saved"),
        ("smart:raw", "Raw"),
    ] {
        b.wr_set_source(key);
        assert_eq!(b.wr_source_label(), label);
        let s = v(b.wr_sources(NOW));
        let on: Vec<&str> = s["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["on"] == true)
            .map(|r| r["key"].as_str().unwrap())
            .collect();
        assert_eq!(on, [key]);
    }
}

#[test]
fn raw_lists_what_came_in_and_the_agentic_feed_nothing_yet() {
    let mut b = reading();
    let mut feed = keys(&b);
    feed.sort();
    assert_eq!(feed, ["sub1|r1", "sub1|r2"], "the Feed: no withdrawn post");
    b.wr_set_source("smart:raw");
    assert_eq!(
        keys(&b),
        ["sub1|r2", "sub1|r1", "sub1|r0"],
        "Raw: everything that came in, withdrawn too, by id when dated the same"
    );
    b.wr_set_source("smart:agentic");
    assert!(keys(&b).is_empty());
    assert_eq!(
        b.wr_empty().as_deref(),
        Some(crate::reading::feed::AGENTIC_NOTE)
    );
}

#[test]
fn the_feed_follows_the_blygs_reader_order() {
    let mut b = reading();
    // Without the setting (an older blyg, the demo): the studio's default,
    // pace-layered, as native's (`ReaderOrder::default`); a choice is kept
    // in this tab.
    assert_eq!(
        v(b.wr_sources(NOW))["order"]["value"],
        blyg_core::order::ReaderOrder::default().as_str()
    );
    assert_eq!(v(b.wr_sources(NOW))["order"]["value"], "pace-layered");
    let r = v(b.wr_set_reader_order("newest"));
    assert!(r.get("req").is_none(), "{r}");
    assert_eq!(v(b.wr_sources(NOW))["order"]["value"], "newest");
    // A blyg that keeps it (studio 0.68): its choice, and a change is sent.
    b.w_set_settings(r#"{"readerOrder":"newest"}"#);
    assert_eq!(v(b.wr_sources(NOW))["order"]["value"], "newest");
    let r = v(b.wr_set_reader_order("pace-layered"));
    assert_eq!(
        r["req"],
        json!({ "method": "PATCH", "path": "/api/settings", "body": { "readerOrder": "pace-layered" } })
    );
    assert_eq!(v(b.wr_sources(NOW))["order"]["value"], "pace-layered", "at once");
    // Refused: back to what the blyg keeps.
    b.wr_reader_order_answer(r["at"].as_f64().unwrap(), 500, "{}");
    assert_eq!(v(b.wr_sources(NOW))["order"]["value"], "newest");
    let o = &v(b.wr_sources(NOW))["order"];
    assert_eq!(o["label"], "Chronological");
}

#[test]
fn a_pace_layered_feed_lists_by_each_rows_place() {
    use crate::reading::feed;
    // No setting: pace-layered all the same (native's default).
    let b = reading();
    // The places are blyg-core's replay (the one the app uses).
    let rows: Vec<blyg_core::ReadingItem> = b.rd.rows.clone();
    let placed = feed::places(
        rows.iter()
            .map(|r| blyg_core::pace::RowFacts::of(r, false))
            .collect(),
        &Default::default(),
    );
    let mut want: Vec<(usize, String)> = rows
        .iter()
        .filter(|r| r.state != "tombstone")
        .map(|r| {
            (
                placed[&crate::reading::vm::key(r)],
                crate::read_api::key_of(r),
            )
        })
        .collect();
    want.sort();
    let want: Vec<String> = want.into_iter().map(|(_, k)| k).collect();
    assert_eq!(keys(&b), want);
}

// ---------------------------------------------------------------- remote HTML

/// A post with only Markdown (a feed item, an older studio's row): the
/// Markdown is someone else's, so what it renders to goes through
/// `sanitize_at` too, like the HTML they send.
#[test]
fn a_markdown_only_post_is_cleaned_against_its_own_origin() {
    let mut b = live();
    let mut row = reading_row("md1");
    row["contentHtml"] = json!("");
    row["contentMd"] = json!(
        "[the owner's API](/api/items) and ![a](https://img.example.com/a.png) \
         and [studio](/studio/login)"
    );
    b.live_reading(&json!([row]).to_string(), "[]");
    let post = v(b.open_reading("sub1|md1", NOW));
    let html = post["html"].as_str().unwrap();
    // A relative link would point at the owner's own blyg (this page's
    // origin, where the Studio cookie is): it points at the author's.
    assert!(
        html.contains("href=\"https://notes.example.com/api/items\""),
        "{html}"
    );
    assert!(
        html.contains("href=\"https://notes.example.com/studio/login\""),
        "{html}"
    );
    assert!(!html.contains("href=\"/"), "{html}");
    // And the allowlist's own marks: images carry no referrer.
    assert!(html.contains("referrerpolicy=\"no-referrer\""), "{html}");
}
