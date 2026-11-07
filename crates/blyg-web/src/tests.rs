//! The bindings, driven as the page drives them (plain Rust: every export
//! takes and returns plain values, so these run natively too).

use super::*;
use serde_json::Value;

/// 2026-11-06T12:00:00Z
const NOW: f64 = 1_793_966_400_000.0;

fn view(b: &Burrow) -> Value {
    serde_json::from_str(&b.view(NOW)).unwrap()
}

fn titles(v: &Value) -> Vec<String> {
    v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["title_html"].as_str().unwrap().to_string())
        .collect()
}

fn outcome(s: String) -> Value {
    serde_json::from_str(&s).unwrap()
}

#[test]
fn the_sample_data_is_the_fakes() {
    let b = Burrow::new(NOW, None);
    let v = view(&b);
    let t = titles(&v);
    assert_eq!(t.len(), 7);
    assert!(t[0].starts_with("A lighthouse keeper"), "{}", t[0]);
    assert_eq!(v["title"], "Burrow — blyg.example.com");
    // The newest is selected and open, a draft fragment.
    let cur = &v["current"];
    assert_eq!(cur["kind_label"], "◦ fragment");
    assert_eq!(cur["version"], "draft");
    assert_eq!(cur["counter"], "67 / 1000");
    assert_eq!(v["rows"][0]["pill"], "draft");
    assert_eq!(v["rows"][0]["when"], "2m");
    assert_eq!(v["sync"], "synced");
}

#[test]
fn typing_filters_live_and_highlights() {
    let mut b = Burrow::new(NOW, None);
    b.set_query("GARDEN");
    let v = view(&b);
    assert_eq!(v["hint"], "1 found · ⏎ open");
    let t = titles(&v);
    assert_eq!(t.len(), 1);
    assert!(t[0].contains("<mark>Garden</mark>s"), "{}", t[0]);
    // The selection jumps to the match and the editor shows it (NV).
    assert!(
        v["current"]["content"]
            .as_str()
            .unwrap()
            .starts_with("Notes on The Field Guide")
    );
}

#[test]
fn enter_with_no_match_creates_a_draft_seeded_with_the_query() {
    let mut b = Burrow::new(NOW, None);
    b.set_query("tide tables");
    let v = view(&b);
    assert_eq!(v["hint"], "⏎ new draft");
    assert_eq!(v["wants_create"], true);
    let o = outcome(b.enter(NOW));
    assert_eq!(o["action"], "create");
    let v = view(&b);
    assert_eq!(v["query"], "");
    assert_eq!(v["current"]["content"], "tide tables");
    assert_eq!(v["current"]["id"], o["id"]);
    assert_eq!(v["sync"], "saved in this browser");
    assert_eq!(titles(&v).len(), 8);
}

#[test]
fn arrows_move_the_selection_clamped() {
    let mut b = Burrow::new(NOW, None);
    let first = b.current().unwrap();
    assert_eq!(b.move_selection(-1), None);
    let second = b.move_selection(1).unwrap();
    assert_ne!(first, second);
    assert_eq!(b.current().unwrap(), second);
    for _ in 0..20 {
        b.move_selection(1);
    }
    assert_eq!(b.move_selection(1), None);
}

#[test]
fn editing_a_published_post_marks_unpublished_edits() {
    let mut b = Burrow::new(NOW, None);
    b.set_query("trombone");
    assert!(b.edit("Band on Harbour Road: late, and better.", NOW));
    let v = view(&b);
    assert_eq!(v["current"]["version"], "public v1 · unpublished edits");
    assert_eq!(v["rows"][0]["dot"], true);
    // Discard goes back to v1 as published.
    assert_eq!(outcome(b.discard_check())["action"], "confirm");
    assert_eq!(
        outcome(b.discard(NOW))["message"],
        "Back to v1 as published"
    );
    let v = view(&b);
    assert!(
        v["current"]["content"]
            .as_str()
            .unwrap()
            .contains("trombone")
    );
    assert_eq!(v["current"]["version"], "public v1");
}

#[test]
fn the_counter_and_banner_follow_the_kind() {
    let mut b = Burrow::new(NOW, None);
    b.edit(&"x".repeat(950), NOW);
    let v = view(&b);
    assert_eq!(v["current"]["level"], "warn");
    b.edit(&"x".repeat(1001), NOW);
    let v = view(&b);
    assert_eq!(v["current"]["counter"], "1001 / 1000");
    assert_eq!(v["current"]["level"], "over");
    assert_eq!(
        v["current"]["banner"],
        "Too long for a fragment · ⌘T makes it a thread"
    );
    assert_eq!(outcome(b.publish_check())["action"], "shake");
    b.toggle_kind(NOW);
    let v = view(&b);
    assert_eq!(v["current"]["kind_label"], "≡ thread");
    assert_eq!(v["current"]["counter"], "1,001 chars · no limit");
    assert!(v["current"]["banner"].is_null());
    assert_eq!(outcome(b.publish_check())["action"], "sheet");
}

#[test]
fn publishing_makes_the_next_version() {
    let mut b = Burrow::new(NOW, None);
    let o = outcome(b.publish_check());
    assert_eq!(o["action"], "sheet");
    assert_eq!(o["next_version"], 1);
    let o = outcome(b.publish("first version", NOW));
    assert_eq!(o["action"], "published");
    assert_eq!(o["version"], 1);
    assert!(
        o["permalink"]
            .as_str()
            .unwrap()
            .starts_with("https://blyg.example.com/f/")
    );
    let v = view(&b);
    assert_eq!(v["current"]["version"], "public v1");
    assert_eq!(
        outcome(b.publish_check())["message"],
        "Already published as v1"
    );
    // Published posts are never deleted.
    assert_eq!(outcome(b.delete_check())["action"], "toast");
}

#[test]
fn a_new_post_exists_only_after_a_keystroke_and_vanishes_empty() {
    let mut b = Burrow::new(NOW, None);
    b.new_post();
    let v = view(&b);
    assert_eq!(v["current"]["status"], "new");
    assert!(
        v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["selected"] == false)
    );
    assert_eq!(titles(&v).len(), 7);
    b.edit("S", NOW);
    let v = view(&b);
    assert_eq!(titles(&v).len(), 8);
    assert_eq!(v["current"]["version"], "scratch · only in this browser");
    // Emptied, then left: gone.
    b.edit("", NOW);
    b.set_query("");
    assert_eq!(titles(&view(&b)).len(), 7);
    // ⌘D makes one a draft.
    b.new_post();
    b.edit("Kept", NOW);
    assert_eq!(outcome(b.make_draft(NOW))["message"], "Saved as a draft");
    assert_eq!(view(&b)["current"]["version"], "draft");
}

#[test]
fn deleting_a_draft_moves_the_list_on() {
    let mut b = Burrow::new(NOW, None);
    let gone = b.current().unwrap();
    assert_eq!(outcome(b.delete_check())["action"], "confirm");
    assert_eq!(outcome(b.delete())["message"], "Draft deleted");
    assert_ne!(b.current().unwrap(), gone);
    assert_eq!(titles(&view(&b)).len(), 6);
}

#[test]
fn the_store_round_trips_through_json() {
    let mut b = Burrow::new(NOW, None);
    b.edit("Changed in the browser", NOW + 1000.0);
    b.publish("", NOW + 2000.0);
    let saved = b.to_json();
    let b2 = Burrow::new(NOW, Some(saved));
    let v = view(&b2);
    assert_eq!(v["current"]["content"], "Changed in the browser");
    assert_eq!(v["current"]["version"], "public v1");
    // Junk falls back to the sample data.
    let b3 = Burrow::new(NOW, Some("{".into()));
    assert_eq!(titles(&view(&b3)).len(), 7);
}

#[test]
fn offline_keeps_changes_waiting() {
    let mut b = Burrow::new(NOW, None);
    b.set_offline(true);
    b.edit("Written on a plane.", NOW);
    let v = view(&b);
    assert_eq!(v["sync"], "offline · 1 change waiting");
    assert_eq!(v["sync_dot"], "grey");
    assert_eq!(b.set_offline(false), 1);
    b.sync_phase("synced");
    assert_eq!(view(&b)["sync"], "synced");
}

#[test]
fn the_preview_is_blyg_render_with_quotes_resolved() {
    let mut b = Burrow::new(NOW, None);
    b.set_query("tide pools");
    let page = b.preview_page("abc123", false);
    assert!(page.starts_with("<!doctype html>"));
    assert!(page.contains("nonce-abc123"));
    assert!(page.contains("<script nonce=\"abc123\">"));
    assert!(page.contains("<article class=\"thread\">"));
    // The imported post is quoted; the unknown id is the unresolved marker.
    assert!(page.contains("A tide pool is a small ocean"), "{page}");
    assert!(page.contains("blyg-transclusion unresolved"));
    assert!(page.contains("blyg-tk-gen"));
    let stats = b.preview_stats();
    assert!(stats.contains("1 quote"), "{stats}");
    assert!(stats.contains("1 AI span"), "{stats}");
    assert!(stats.contains("1 video"), "{stats}");
    assert!(stats.contains("1 issue would stop publish"), "{stats}");
}

#[test]
fn every_builtin_theme_resolves() {
    let themes: Value = serde_json::from_str(&Burrow::themes()).unwrap();
    let themes = themes.as_array().unwrap();
    assert_eq!(themes.len(), blyg_core::config::theme::BUILTIN.len());
    let paper = &themes[0];
    assert_eq!(paper["id"], "light");
    let vars = paper["vars"].as_array().unwrap();
    assert!(vars.iter().any(|v| v[0] == "--c-bg" && v[1] == "#fffff8"));
}

/// A post the filter doesn't list stays open through ⌘D and delete; the
/// editor never jumps to (and acts on) the first listed row.
#[test]
fn an_unlisted_post_stays_the_one_acted_on() {
    let mut b = Burrow::new(NOW, None);
    b.set_query("tide pools");
    b.new_post();
    b.edit("A note", NOW);
    let id = b.current().unwrap();
    b.make_draft(NOW);
    assert_eq!(b.current().unwrap(), id);
    assert_eq!(view(&b)["current"]["content"], "A note");
    assert_eq!(outcome(b.delete())["message"], "Draft deleted");
    // The listed thread is untouched.
    assert_eq!(titles(&view(&b)).len(), 1);
}
