//! The 0.29 flow pass over Burrow web (`scripts/browser-web-flow-pass.sh`,
//! a real local studio, 300+ posts): how the journeys FEEL. What the pass
//! measured as lost, jumpy or chattier than the studio, driven as
//! `www/*.js` drives it; the page's own files are read where the fault was
//! in them. The pass itself holds the timing and shift budgets
//! (`scripts/web-flow-budgets.json`).

use serde_json::{Value, json};

use crate::Burrow;

/// 2026-11-06T12:00:00Z
const NOW: f64 = 1_793_966_400_000.0;
const BASE: &str = "https://blyg.example.com";

fn v(s: String) -> Value {
    serde_json::from_str(&s).unwrap()
}

fn wire(id: &str, md: &str, status: &str, version: u32) -> Value {
    json!({
        "id": id, "revision": 3, "kind": "thread", "status": status,
        "created": "2026-11-01T10:00:00Z", "updated": "2026-11-05T10:00:00Z",
        "version": version, "contentMd": md, "dirty": false, "stubOf": null,
        "forkedFrom": null, "provenance": [], "responses": "default", "highlight": "default"
    })
}

fn row(rid: &str, kind: &str, md: &str) -> Value {
    json!({
        "subscriptionId": "s1", "remoteId": rid, "subscriptionTitle": "Sample notes",
        "origin": "https://b.blyg.example.com", "kind": kind, "version": 1,
        "created": "2026-11-04T09:00:00Z", "updated": "2026-11-04T09:00:00Z",
        "observedAt": "2026-11-04T09:05:00Z", "contentMd": md, "contentHtml": "",
        "readVersion": null, "hoppers": []
    })
}

/// The www file `name`, as served.
fn www(name: &str) -> String {
    std::fs::read_to_string(format!("{}/www/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

const DRAFT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaa";

fn live_with_a_draft() -> Burrow {
    let mut b = Burrow::new_live(NOW, None, None, BASE);
    b.live_items(&json!([wire(DRAFT, "A draft about tides", "draft", 0)]).to_string());
    b
}

// ------------------------------------------------------------ typing and sync

/// A check of the blyg while you type (the tab shown again, a poll) sent
/// the post being typed once per keystroke burst: 18 saves for one
/// sentence in the pass. The page's own save waits for typing to stop; a
/// background send leaves that post alone until then, and still sends the
/// rest of the outbox.
#[test]
fn a_background_send_leaves_the_post_being_typed_until_typing_stops() {
    let mut b = live_with_a_draft();
    b.open(DRAFT);
    assert!(b.edit("A draft about tides, still", NOW));
    // Mid-word: nothing to send yet.
    assert_eq!(b.outbox_next_settled("[]", NOW + 100.0), "null");
    // Once typing has stopped, it goes.
    let n = v(b.outbox_next_settled("[]", NOW + 900.0));
    assert_eq!(n["op"], "save");
    assert_eq!(n["req"]["body"]["contentMd"], "A draft about tides, still");
    // Publishing sends everything at once (the plain outbox).
    assert_eq!(v(b.outbox_next("[]"))["op"], "save");
}

#[test]
fn a_background_send_still_sends_read_marks_while_you_type() {
    let mut b = live_with_a_draft();
    b.live_reading(
        &json!([row("01R1", "fragment", "A note")]).to_string(),
        "[]",
    );
    let key = v(b.reading_view(NOW, "", false))["rows"][0]["key"]
        .as_str()
        .unwrap()
        .to_string();
    b.open_reading(&key, NOW);
    b.open(DRAFT);
    b.edit("A draft about tides, typing", NOW);
    let n = v(b.outbox_next_settled("[]", NOW + 50.0));
    assert_eq!(n["op"], "read", "the read mark goes; the typed post waits");
}

// ------------------------------------------------------------ loading, not empty

/// On a first start the stream drew "Nothing here" (the source's empty
/// note) for a moment before the first page arrived: an empty state
/// flashing before data. Until the reading list has been read once, the
/// view says it's loading, and has no empty note.
#[test]
fn the_stream_is_loading_not_empty_until_the_list_first_arrives() {
    let mut b = Burrow::new_live(NOW, None, None, BASE);
    let r = v(b.reading_page(NOW, "", false, 40));
    assert_eq!(r["loading"], true);
    assert_eq!(r["empty"], Value::Null, "no empty note while loading");
    b.live_reading("[]", "[]");
    let r = v(b.reading_page(NOW, "", false, 40));
    assert_eq!(r["loading"], false);
    assert!(r["empty"].is_string(), "read, and empty: say so");
    // The demo has its list from the start.
    let d = Burrow::new(NOW, None);
    assert_eq!(v(d.reading_page(NOW, "", false, 40))["loading"], false);
}

#[test]
fn a_held_reading_list_is_not_loading() {
    let mut b = Burrow::new_live(NOW, None, None, BASE);
    b.live_reading(
        &json!([row("01R1", "fragment", "A note")]).to_string(),
        "[]",
    );
    let held = b.reading_json();
    let again = Burrow::new_live(NOW, None, Some(held), BASE);
    assert_eq!(v(again.reading_page(NOW, "", false, 40))["loading"], false);
}

/// The phone's Read tab and the desk's stream draw a placeholder while
/// loading, not the empty note.
#[test]
fn the_page_draws_a_placeholder_while_the_stream_loads() {
    for f in ["desk.js", "phone.js"] {
        assert!(www(f).contains("R.loading"), "{f} draws R.loading");
    }
    assert!(www("style.css").contains(".stream-loading"));
}

// ------------------------------------------------------------ leftovers of the last pass

/// A thread's card repeated its title as the start of its excerpt ("A
/// third view on tide pools A third view on tide pools Quoting…"): the
/// heading line wasn't recognised as the title.
#[test]
fn a_thread_card_does_not_repeat_its_title_in_the_excerpt() {
    let mut b = Burrow::new_live(NOW, None, None, BASE);
    let md = "# A third view on tide pools\n\nQuoting the reply, then the third view.";
    b.live_reading(&json!([row("01T1", "thread", md)]).to_string(), "[]");
    let r = &v(b.reading_view(NOW, "", false))["rows"][0];
    assert_eq!(r["title"], "A third view on tide pools");
    let ex = r["excerpt"].as_str().unwrap();
    assert!(ex.starts_with("Quoting the reply"), "excerpt: {ex}");
}

/// The studio's messages start lowercase ("invalid url"); shown as they
/// came, a toast read "invalid url". They're capitalised for display.
#[test]
fn studio_messages_are_shown_with_a_capital() {
    let e = crate::api::read(
        422,
        r#"{"code":"validation","message":"invalid url"}"#,
        None,
    )
    .unwrap_err();
    assert_eq!(e.message(), "Invalid url");
    let e = crate::api::read(
        403,
        r#"{"code":"forbidden","message":"cross-origin owner request denied"}"#,
        None,
    )
    .unwrap_err();
    assert!(
        e.message().ends_with("Cross-origin owner request denied"),
        "{}",
        e.message()
    );
}

/// The Agentic feed's empty state named a version ("comes in Burrow
/// 0.29"); it now names where the setting is.
#[test]
fn the_agentic_feed_names_burrow_desktops_setting() {
    let t = crate::web_reading::AGENTIC_EMPTY;
    assert!(
        t.contains("Burrow desktop can write editions: Settings › Agentic feed"),
        "{t}"
    );
    assert!(!t.contains("0.29"), "{t}");
}

// ------------------------------------------------------------ one tap, kept place

/// Switching between Feed, the Agentic feed, Saved, Background and Raw on
/// the phone took two taps each (☰ Sources, then the source) and a sheet;
/// the studio's is one tap on a row of named buttons. The Read screen has
/// that row.
#[test]
fn the_phone_switches_smart_sources_in_one_tap() {
    let html = www("index.html");
    let strip = html
        .find("id=\"p-srcStrip\"")
        .map(|i| &html[i..i + html[i..].find("</div>").unwrap()])
        .expect("a source strip on the Read screen");
    for s in [
        "smart:all",
        "smart:agentic",
        "smart:shelf",
        "smart:background",
        "smart:raw",
    ] {
        assert!(
            strip.contains(&format!("data-src=\"{s}\"")),
            "{s} in the strip"
        );
    }
}

/// A reload put the phone back on Write and the desk back on Posts (the
/// studio keeps its place in the address). The view is kept in the
/// address's fragment and restored on start.
#[test]
fn a_reload_keeps_the_view() {
    let core = www("core.js");
    assert!(
        core.contains("export function keepPlace"),
        "one place that writes the view"
    );
    for f in ["desk.js", "phone.js"] {
        assert!(www(f).contains("keepPlace("), "{f} keeps its view");
    }
    assert!(
        www("app.js").contains("placeOnStart"),
        "and the start reads it back"
    );
}

/// "Loading Burrow…" flashed for a few frames on a fast start (a spinner
/// shorter than 150 ms). It appears only when the start is slow.
#[test]
fn the_loading_note_waits_before_it_shows() {
    let css = www("style.css") + &www("phone.css");
    assert!(
        css.contains("--loading-delay"),
        "the loading notes are delayed"
    );
}

/// A failed look-up in Subscribe went by as a toast; the form then looked
/// as if nothing had happened. The reason stays in the form.
#[test]
fn a_failed_subscribe_says_why_in_the_form() {
    let js = www("web-reading.js");
    assert!(
        js.contains("screen.error"),
        "the subscribe screen keeps its error"
    );
    assert!(js.contains("class=\"wr-error\""), "and draws it");
}

/// On a first open the desk's list said "No matches. Press ⏎ to start “”."
/// for a few frames before your posts arrived. Until they first arrive the
/// list says it's loading; with no query there's no "No matches".
#[test]
fn your_posts_are_loading_not_missing_until_they_first_arrive() {
    let mut b = Burrow::new_live(NOW, None, None, BASE);
    assert_eq!(v(b.view(NOW))["loading"], true);
    b.live_items("[]");
    assert_eq!(v(b.view(NOW))["loading"], false);
    let held = b.to_json();
    let again = Burrow::new_live(NOW, Some(held), None, BASE);
    assert_eq!(v(again.view(NOW))["loading"], false, "held from a last visit");
    assert_eq!(v(Burrow::new(NOW, None).view(NOW))["loading"], false);
    let desk = www("desk.js");
    assert!(desk.contains("V.loading"), "the desk draws it");
    assert!(desk.contains("Nothing written yet"), "an empty query isn't a search");
}
