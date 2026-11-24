//! --- background --- Driven as `www/web-reading.js` drives it, with the
//! blyg's answers written out by hand (shapes from blygger-studio 0.61.0,
//! contract 5.10.0). Against a real studio: `crates/blyg-core/tests/e2e.rs`
//! `burrows_background_is_the_studios`.

use serde_json::{Value, json};

use crate::Burrow;

/// 2026-11-06T12:00:00Z
const NOW: f64 = 1_793_966_400_000.0;
const BASE: &str = "https://blyg.example.com";

fn v(s: String) -> Value {
    serde_json::from_str(&s).unwrap()
}

fn row(sub: &str, rid: &str, state: &str, day: u32) -> Value {
    json!({
        "subscriptionId": sub, "remoteId": rid, "subscriptionTitle": format!("Sub {sub}"),
        "origin": format!("https://{sub}.example.com"), "kind": "fragment", "version": 2,
        "state": state, "withdrawn": state == "tombstone",
        "created": format!("2026-11-0{day}T09:00:00Z"), "updated": format!("2026-11-0{day}T09:00:00Z"),
        "observedAt": format!("2026-11-0{day}T09:05:00Z"), "contentMd": format!("Sample note {rid}."),
        "contentHtml": format!("<p>Sample note {rid}.</p>"), "readVersion": null, "hoppers": []
    })
}

fn caps(ops: &[&str]) -> String {
    json!({ "contractVersion": "5.10.0", "operations": ops, "extensions": [] }).to_string()
}

fn entry(sub: &str, rid: &str, reasons: &[&str], placement: Option<&str>, blyg: bool) -> Value {
    json!({
        "key": format!("imported:[\"{sub}\",\"{rid}\"]"), "source": "imported",
        "imported": { "subscriptionId": sub, "remoteId": rid },
        "background": { "reasons": reasons, "placement": placement, "blyg": blyg },
    })
}

fn live(ops: &[&str]) -> Burrow {
    let mut b = Burrow::new_live(NOW, None, None, BASE);
    let rows = json!([
        row("a", "a1", "current", 5),
        row("a", "a2", "current", 4),
        row("b", "b1", "current", 3),
        row("b", "gone", "tombstone", 2),
    ]);
    b.live_reading(&rows.to_string(), "[]");
    b.wr_loaded("capabilities", &caps(ops));
    b
}

fn shown(b: &mut Burrow, source: &str) -> Vec<String> {
    b.wr_set_source(source);
    let out = v(b.reading_view(NOW, "", false))["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["key"].as_str().unwrap_or("").to_string())
        .collect();
    b.wr_set_source("smart:all");
    out
}

fn ids(keys: &[String]) -> Vec<&str> {
    keys.iter().map(|k| k.rsplit('|').next().unwrap()).collect()
}

#[test]
fn an_older_studio_shows_no_background_ui_and_withdrawn_stays_background() {
    let mut b = live(&["markReadBatch"]);
    assert!(!b.wr_bg_wanted(), "never asked for the lens");
    assert_eq!(v(b.wr_list("background"))["path"], "/api/reading");
    assert_eq!(v(b.wr_bg("a|a1"))["served"], false);
    assert_eq!(v(b.wr_bg_rules())["served"], false);
    assert!(v(b.wr_bg_act("a|a1", "place"))["error"].is_string());
    assert_eq!(ids(&shown(&mut b, "smart:all")), ["a1", "a2", "b1"]);
}

#[test]
fn the_lens_decides_what_background_and_every_other_source_show() {
    let mut b = live(&["setReadingPlacement"]);
    assert!(b.wr_bg_wanted());
    let lens = json!([
        entry("a", "a2", &[], Some("background"), false),
        entry("b", "b1", &[], None, true),
        entry("b", "gone", &["withdrawn"], None, true),
    ]);
    assert_eq!(v(b.wr_loaded("background", &lens.to_string()))["n"], 3);
    assert!(!b.wr_bg_wanted());
    assert_eq!(ids(&shown(&mut b, "smart:all")), ["a1"]);
    assert_eq!(
        ids(&shown(&mut b, "smart:background")),
        ["a2", "b1", "gone"]
    );
    let x = v(b.wr_bg("a|a2"));
    assert_eq!(x["on"], true);
    assert_eq!(x["placement"], "background");
    assert_eq!(x["why"], "you moved this");
    assert_eq!(v(b.wr_bg("b|b1"))["why"], "blyg in Background");

    // Back to Foreground: the request names the entry, and it moves at once.
    let o = v(b.wr_bg_act("a|a2", "place"));
    assert_eq!(o["req"]["method"], "PUT");
    assert_eq!(o["req"]["path"], "/api/reading/placement");
    assert_eq!(
        o["req"]["body"],
        json!({"source": "imported", "sub": "a", "id": "a2", "placement": "foreground"})
    );
    let d = v(b.wr_bg_done(
        "a|a2",
        "place",
        200,
        r#"{"source":"imported","sub":"a","id":"a2","placement":"foreground"}"#,
    ));
    assert_eq!(d["toast"], "Moved to Foreground");
    assert_eq!(ids(&shown(&mut b, "smart:all")), ["a1", "a2"]);
    assert!(b.wr_bg_wanted(), "and the lens is read again");

    // The whole blyg `b` back to the foreground: a partial subscription write.
    let o = v(b.wr_bg_act("b|b1", "blyg"));
    assert_eq!(o["req"]["method"], "PATCH");
    assert_eq!(o["req"]["path"], "/api/subscriptions/b");
    assert_eq!(o["req"]["body"], json!({"background": false}));
    b.wr_bg_done("b|b1", "blyg", 200, "{}");
    assert_eq!(ids(&shown(&mut b, "smart:all")), ["a1", "a2", "b1"]);
    assert_eq!(
        ids(&shown(&mut b, "smart:background")),
        ["gone"],
        "withdrawn stays"
    );
}

#[test]
fn a_refused_choice_changes_nothing() {
    let mut b = live(&["setReadingPlacement"]);
    let lens = json!([entry("b", "gone", &["withdrawn"], None, false)]);
    b.wr_loaded("background", &lens.to_string());
    let d = v(b.wr_bg_done(
        "a|a1",
        "place",
        404,
        r#"{"error":"no reading entry with that id"}"#,
    ));
    assert!(d["error"].is_string());
    assert_eq!(ids(&shown(&mut b, "smart:all")), ["a1", "a2", "b1"]);
}

#[test]
fn rules_are_read_from_settings_and_written_one_key_at_a_time() {
    let mut b = live(&["setReadingPlacement"]);
    b.wr_loaded(
        "settings",
        r#"{"acceptMentions":true,"backgroundRules":{"minor":false}}"#,
    );
    let r = v(b.wr_bg_rules());
    assert_eq!(r["served"], true);
    // The rules blyg-core knows, in its order (`burst` came with 0.69).
    let minor = |r: &serde_json::Value| -> serde_json::Value {
        r["rules"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["rule"] == "minor")
            .cloned()
            .expect("a minor rule")
    };
    assert_eq!(minor(&r)["on"], false);
    let o = v(b.wr_bg_rule("minor", true));
    assert_eq!(o["req"]["method"], "PATCH");
    assert_eq!(
        o["req"]["body"],
        json!({"backgroundRules": {"minor": true}})
    );
    assert!(b.wr_bg_rule_saved(200, r#"{"backgroundRules":{"minor":true}}"#));
    assert_eq!(minor(&v(b.wr_bg_rules()))["on"], true);
    assert!(v(b.wr_bg_rule("withdrawn", false))["error"].is_string());
}
