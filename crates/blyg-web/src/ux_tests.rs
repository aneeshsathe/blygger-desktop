//! The 0.29 usability pass over Burrow web (a real local studio, every
//! persona): what a reader and a writer saw, driven as `www/*.js` drives
//! it, with the blyg's answers written out by hand. The page's own files
//! are read where the fault was in them (keys, field types, fonts, names).

use serde_json::{Value, json};

use crate::Burrow;
use crate::platform_text::{Platform, spelled_as};

/// 2026-11-06T12:00:00Z
const NOW: f64 = 1_793_966_400_000.0;
const BASE: &str = "https://blyg.example.com";

fn v(s: String) -> Value {
    serde_json::from_str(&s).unwrap()
}

fn wire(id: &str, md: &str, status: &str, version: u32, when: &str, dirty: bool) -> Value {
    json!({
        "id": id, "revision": 3, "kind": "fragment", "status": status,
        "created": when, "updated": when,
        "version": version, "contentMd": md, "dirty": dirty, "stubOf": null,
        "forkedFrom": null, "provenance": [], "responses": "default", "highlight": "default"
    })
}

fn row(rid: &str, when: &str) -> Value {
    json!({
        "subscriptionId": "s1", "remoteId": rid, "subscriptionTitle": "Sample notes",
        "origin": "https://b.blyg.example.com", "kind": "fragment", "version": 1,
        "created": when, "updated": when, "observedAt": when,
        "contentMd": format!("Sample note {rid}"), "contentHtml": "", "readVersion": null, "hoppers": []
    })
}

fn at(day: u32, hour: u32) -> String {
    format!("2026-11-{day:02}T{hour:02}:00:00Z")
}

fn keys(b: &Burrow) -> Vec<String> {
    v(b.reading_view(NOW, "", false))["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["key"].as_str().unwrap().to_string())
        .collect()
}

const OWN: &str = "01OWNPUBLISHED0000000000AA";
const OWN_KEY: &str = "~own|01OWNPUBLISHED0000000000AA";

/// One post of yours, published on day 3, among a blyg's posts of days 2
/// and 5: distinct times, unlike the pass's seed (all within 2 s).
fn feed(order: &str, dirty: bool) -> Burrow {
    let mut b = Burrow::new_live(NOW, None, None, BASE);
    b.w_set_settings(&json!({ "readerOrder": order }).to_string());
    let md = if dirty {
        "Drift wood, published. A revision."
    } else {
        "Drift wood, published."
    };
    b.live_items(&json!([wire(OWN, md, "public", 1, &at(3, 12), dirty)]).to_string());
    let rows = json!([row("01K5", &at(5, 9)), row("01K2", &at(2, 9))]);
    b.live_reading(&rows.to_string(), "[]");
    b
}

/// The www file `name`, as served.
fn www(name: &str) -> String {
    std::fs::read_to_string(format!("{}/www/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// Every page file (scripts, styles, the HTML) with its name.
fn page_files() -> Vec<(String, String)> {
    let dir = format!("{}/www", env!("CARGO_MANIFEST_DIR"));
    let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| {
            let name = e.unwrap().file_name().into_string().unwrap();
            [".js", ".html", ".css"]
                .iter()
                .any(|x| name.ends_with(x))
                .then(|| (name.clone(), www(&name)))
        })
        .collect();
    out.sort();
    out
}

/// `src` without its comments (`/* */`, `<!-- -->`, and `// …` that starts
/// a line or follows a space, so `https://` stays).
fn code_of(src: &str) -> String {
    let mut s = src.to_string();
    for (open, close) in [("/*", "*/"), ("<!--", "-->")] {
        while let Some(a) = s.find(open) {
            let b = s[a..].find(close).map_or(s.len(), |b| a + b + close.len());
            s.replace_range(a..b, "");
        }
    }
    s.lines()
        .map(|l| {
            let t = l.trim_start();
            if t.starts_with("//") {
                ""
            } else if let Some(i) = l.find(" // ") {
                &l[..i]
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ------------------------------------------------------------ 1, 10: the Feed

/// The studio's reader replays your items and every subscription's as one
/// stream (blygger-studio `src/feeds/reader-order.ts`, other repo:
/// blygger-studio), so yours take their place by date, in either order.
#[test]
fn your_posts_take_their_place_in_the_feed_by_date_as_the_studios_do() {
    for order in ["newest", "pace-layered"] {
        let b = feed(order, false);
        assert_eq!(
            keys(&b),
            ["s1|01K5", OWN_KEY, "s1|01K2"],
            "{order}: a followed post newer than yours comes first"
        );
    }
}

#[test]
fn chronological_is_newest_first_by_the_posts_own_times() {
    let mut b = Burrow::new_live(NOW, None, None, BASE);
    b.w_set_settings(r#"{"readerOrder":"newest"}"#);
    let rows = json!([
        row("01K1", "2026-11-05T09:00:01Z"),
        row("01K3", "2026-11-05T09:00:03Z"),
        row("01K2", "2026-11-05T09:00:02Z"),
    ]);
    b.live_reading(&rows.to_string(), "[]");
    assert_eq!(keys(&b), ["s1|01K3", "s1|01K2", "s1|01K1"]);
}

/// The Feed's order control says what the order does, so choosing one
/// shows a difference even when the top of the list looks the same.
#[test]
fn the_order_control_says_what_each_order_does() {
    let b = feed("pace-layered", false);
    let c = v(b.wr_sources(NOW));
    let o = &c["readerOrder"];
    assert!(
        o["note"].as_str().is_some_and(|n| n.contains("newest")),
        "{o}"
    );
}

// ------------------------------------------------------------ 1: your own posts

#[test]
fn your_own_posts_offer_no_read_or_background_controls() {
    let mut b = feed("newest", false);
    b.wr_loaded(
        "capabilities",
        &json!({ "contractVersion": "6.2.0", "operations": ["setReadingPlacement", "listReading"] })
            .to_string(),
    );
    let card = v(b.reading_view(NOW, "", false))["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == OWN_KEY)
        .cloned()
        .unwrap();
    assert_eq!(card["own"], true, "{card}");
    let post = v(b.open_reading(OWN_KEY, NOW));
    assert_eq!(post["own"], true, "the reader hides ● Unread: {post}");
    assert_eq!(
        v(b.wr_bg(OWN_KEY))["served"],
        false,
        "no ↓ Background on your own post"
    );
    // Theirs keep both.
    assert_eq!(v(b.open_reading("s1|01K5", NOW))["own"], false);
    assert_eq!(v(b.wr_bg("s1|01K5"))["served"], true);
}

// ------------------------------------------------------------ 2: unpublished edits

#[test]
fn your_post_with_unpublished_edits_says_so_in_reading() {
    let b = feed("newest", true);
    let card = v(b.reading_view(NOW, "", false))["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == OWN_KEY)
        .cloned()
        .unwrap();
    assert_eq!(card["badge"], "unpublished edits", "{card}");
    let mut b = b;
    let by = v(b.open_reading(OWN_KEY, NOW))["by"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        by.ends_with("v1 · unpublished edits"),
        "not the public v1's label over the working copy: {by}"
    );
    // Published as it is: just its version.
    let mut clean = feed("newest", false);
    let by = v(clean.open_reading(OWN_KEY, NOW))["by"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(by.ends_with("· v1"), "{by}");
}

// ------------------------------------------------------------ 3: one name

#[test]
fn saved_is_the_one_name_for_what_you_keep() {
    let mut b = feed("newest", false);
    let on = v(b.toggle_reading_shelf("s1|01K5"));
    assert_eq!(on["message"], "Saved");
    let off = v(b.toggle_reading_shelf("s1|01K5"));
    assert_eq!(off["message"], "Removed from Saved");
    let first = b.store.items()[0].local_id.0.clone();
    b.open(&first);
    assert_eq!(v(b.toggle_shelf())["message"], "Saved 📚");
    assert_eq!(v(b.toggle_shelf())["message"], "Removed from Saved");
    // The empty Saved list names the button that exists, and the key the page binds.
    b.wr_set_source("smart:shelf");
    let empty = spelled_as(Platform::Browser, || {
        v(b.reading_view(NOW, "", false))["empty"]
            .as_str()
            .unwrap()
            .to_string()
    });
    assert!(empty.contains("“+ Save”"), "{empty}");
    assert!(!empty.contains('⌘'), "{empty}");
    let core = www("core.js");
    assert!(
        core.contains("'+ Save'"),
        "the button says what the empty state names"
    );
    for (name, src) in page_files() {
        let code = code_of(&src);
        for old in [
            "Add to shelf",
            "Remove from shelf",
            "Your shelf",
            "Shelve",
            "› Shelf",
            ">Shelf<",
            "your shelf",
            "the shelf",
        ] {
            assert!(!code.contains(old), "{name} still says “{old}”");
        }
    }
}

// ------------------------------------------------------------ 4: the › glyph

/// The ET Book faces docs/prototype carries map › and ‹ to the ffl and ffi
/// ligatures; the desktop's (fixed in 0.25.1) carry real guillemets. The
/// page serves the desktop's.
#[test]
fn the_page_serves_the_fixed_et_book() {
    let css = www("style.css");
    let serve =
        std::fs::read_to_string(format!("{}/serve.sh", env!("CARGO_MANIFEST_DIR"))).unwrap();
    assert!(!css.contains(".woff"), "style.css names the old faces");
    for face in [
        "et-book-roman-line-figures.ttf",
        "et-book-display-italic-old-style-figures.ttf",
    ] {
        assert!(css.contains(&format!("fonts/{face}")), "style.css: {face}");
        assert!(
            serve.contains(&format!("crates/blyg-app/assets/fonts/et-book/{face}"))
                || serve.contains("crates/blyg-app/assets/fonts/et-book/\"*.ttf")
                || serve.contains("assets/fonts/et-book"),
            "serve.sh copies the fixed {face}"
        );
    }
    assert!(!serve.contains("docs/prototype/*.woff"));
}

// ------------------------------------------------------------ 5, 6: posts' tables and code

#[test]
fn tables_and_wide_code_in_posts_are_drawn_readably() {
    let css = code_of(&www("web-reading.css"));
    for want in [
        "border-collapse",
        ".item-content th",
        ".item-content td",
        ".item-content pre",
    ] {
        assert!(css.contains(want), "web-reading.css: {want}");
    }
    // A wide block scrolls with a visible bar, not an overlay one.
    assert!(css.contains("::-webkit-scrollbar"), "a visible scroll bar");
    assert!(css.contains("overflow-x: auto"), "it scrolls");
}

// ------------------------------------------------------------ 7, 9: the status bar

#[test]
fn a_scratch_note_is_never_shown_as_synced() {
    let mut b = feed("newest", false);
    b.new_post();
    b.edit("A quick invented thought from a phone.", NOW);
    b.sync_phase("synced");
    let view = v(b.view(NOW));
    assert_eq!(view["sync"], "on this device only", "{view}");
    assert_ne!(view["sync_dot"], "green");
}

#[test]
fn a_conflict_is_edited_elsewhere_not_a_sync_error() {
    let mut b = feed("newest", false);
    let id = b.store.items()[0].local_id.0.clone();
    b.open(&id);
    b.edit("Drift wood, published. Mine.", NOW);
    let mut n = v(b.outbox_next("[]"));
    n.as_object_mut().unwrap().remove("req");
    b.outbox_answer(
        &n.to_string(),
        409,
        &json!({ "code": "revision-conflict", "message": "stale", "details": {} }).to_string(),
        None,
    );
    let theirs = wire(OWN, "Theirs.", "public", 1, &at(3, 12), true);
    b.live_conflict(OWN, 200, &theirs.to_string());
    b.sync_phase("error");
    let view = v(b.view(NOW));
    assert_eq!(view["sync"], "edited elsewhere · choose a copy", "{view}");
    assert_eq!(view["sync_dot"], "amber");
    assert!(view["current"]["conflict"].is_object(), "the banner stays");
}

// ------------------------------------------------------------ 8: an over-long fragment

#[test]
fn an_over_long_fragment_shakes_instead_of_opening_publish() {
    let mut b = feed("newest", false);
    b.new_post();
    b.edit(&"Long invented text. ".repeat(55), NOW);
    let o = spelled_as(Platform::Browser, || v(b.publish_check()));
    assert_eq!(o["action"], "shake", "{o}");
    assert!(o["message"].as_str().unwrap().contains("⌃T"), "{o}");
    // ⌃D keeps it, and says it's a thread now.
    let d = v(b.make_draft(NOW));
    assert!(
        d["message"].as_str().unwrap().contains("thread"),
        "the kind change is said: {d}"
    );
    assert_eq!(v(b.view(NOW))["current"]["kind"], "thread");
}

// ------------------------------------------------------------ 11: the Agentic feed

#[test]
fn the_agentic_feed_says_what_makes_an_edition() {
    let mut b = feed("newest", false);
    b.wr_set_source("smart:agentic");
    let empty = v(b.reading_view(NOW, "", false))["empty"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(!empty.contains("studio 0."), "no version numbers: {empty}");
    assert!(empty.contains("Burrow"), "the next step: {empty}");
    assert!(empty.contains("assistant"), "{empty}");
}

// ------------------------------------------------------------ 12: bare addresses

#[test]
fn subscribe_and_peek_take_a_bare_host() {
    let mut b = feed("newest", false);
    for action in ["preview", "subscribe"] {
        let o = v(b.wr_do(action, r#"{"url":"blyg.example.com"}"#, NOW));
        assert_eq!(
            o["req"]["body"]["url"], "https://blyg.example.com",
            "{action}: {o}"
        );
    }
    let o = v(b.wr_do("peek_url", r#"{"url":"blyg.example.com/t/01K9"}"#, NOW));
    assert_eq!(
        o["req"]["body"]["urls"][0], "https://blyg.example.com/t/01K9",
        "{o}"
    );
    // Words still aren't an address.
    let o = v(b.wr_do("subscribe", r#"{"url":"not a url at all"}"#, NOW));
    assert!(o["error"].is_string(), "{o}");
    // The fields let a bare host through to Burrow (no browser bubble).
    for (name, src) in page_files() {
        assert!(!src.contains("type=\"url\""), "{name}: a type=url field");
    }
}

// ------------------------------------------------------------ 13: keys

#[test]
fn hints_spell_the_keys_the_page_binds() {
    spelled_as(Platform::Browser, || {
        assert_eq!(
            crate::keymap::hint("⌥⌘Z discards them"),
            "⌃⌥Z discards them"
        );
        assert_eq!(crate::keymap::hint("⇧⌘⌫ deletes"), "⌃⇧⌫ deletes");
        assert_eq!(crate::keymap::hint("⌘⏎ publishes"), "⌃⏎ publishes");
        assert_eq!(crate::keymap::hint("⌃⇧Y"), "⌃⇧Y");
        assert_eq!(
            crate::keymap::hint_owned("⌘⏎ publishes them as v2".into()),
            "⌃⏎ publishes them as v2"
        );
        // What the writer sees, start to end.
        let mut b = feed("newest", false);
        b.new_post();
        b.edit("x", NOW);
        let view = b.view(NOW);
        assert!(!view.contains('⌘'), "{view}");
    });
    // The page's own text, and the HUD it draws, spell ⌃ too.
    for (name, src) in page_files() {
        let code = code_of(&src);
        assert!(
            !code.contains('⌘'),
            "{name} shows ⌘, which the browser keeps: say ⌃ (keys.js)"
        );
    }
}

// ------------------------------------------------------------ 15: /api/changes

#[test]
fn a_blyg_without_change_counters_is_never_asked_for_them() {
    let mut b = feed("newest", false);
    assert!(b.changes_due(NOW), "not known yet: asked");
    b.wr_loaded(
        "capabilities",
        &json!({ "contractVersion": "6.2.0", "operations": ["listItems"] }).to_string(),
    );
    assert!(!b.changes_due(NOW), "contract 6 lists no getChanges");
    let mut old = feed("newest", false);
    old.wr_loaded(
        "capabilities",
        &json!({ "contractVersion": "5.7.0", "operations": ["getChanges"] }).to_string(),
    );
    assert!(old.changes_due(NOW));
}

// ------------------------------------------------------------ 16: nits

#[test]
fn the_reading_settings_read_as_english() {
    let js = www("web-reading.js");
    assert!(!js.contains("show of a changelog"));
}
