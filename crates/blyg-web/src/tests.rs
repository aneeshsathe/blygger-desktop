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

// ---------------------------------------------------------------- phone

#[test]
fn the_stream_orders_without_copying_as_the_native_list_does() {
    let rows = crate::seed::reading(at(NOW)).rows;
    let key = |r: &blyg_core::ReadingItem| crate::read_api::key_of(r);
    let owned: Vec<String> = crate::reading::vm::order(rows.clone())
        .iter()
        .map(key)
        .collect();
    let borrowed: Vec<String> = crate::reading::vm::order_refs(&rows)
        .into_iter()
        .map(key)
        .collect();
    assert!(!owned.is_empty());
    assert_eq!(owned, borrowed);
}

fn reading(b: &Burrow, q: &str, unread_only: bool) -> Value {
    serde_json::from_str(&b.reading_view(NOW, q, unread_only)).unwrap()
}

fn ids(v: &Value) -> Vec<String> {
    v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn reading_marks_read_by_swipe_and_by_opening_and_remembers() {
    let mut b = Burrow::new(NOW, None);
    let r = reading(&b, "", false);
    let unread = r["unread"].as_u64().unwrap();
    assert!(unread >= 2, "{r}");
    // Excerpts are the post's own words, without markup or quoted posts.
    for x in r["rows"].as_array().unwrap() {
        let e = x["excerpt"].as_str().unwrap();
        assert!(!e.contains("![[") && !e.contains("## "), "{e}");
    }
    let unread_keys: Vec<String> = r["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|x| x["unread"] == true)
        .map(|x| x["key"].as_str().unwrap().to_string())
        .collect();
    // Swipe right.
    assert_eq!(
        outcome(b.set_read(&unread_keys[0], true))["message"],
        "Marked read"
    );
    // Open.
    let post: Value = serde_json::from_str(&b.open_reading(&unread_keys[1], NOW)).unwrap();
    assert!(!post["html"].as_str().unwrap().is_empty());
    assert!(post["by"].as_str().unwrap().contains(" · v"), "{post}");
    assert_eq!(
        reading(&b, "", false)["unread"].as_u64().unwrap(),
        unread - 2
    );
    // Unread only leaves them out.
    let only = reading(&b, "", true);
    assert!(
        only["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["unread"] == true)
    );
    // Kept with the store.
    let again = Burrow::new(NOW, Some(b.to_json()));
    assert_eq!(
        reading(&again, "", false)["unread"].as_u64().unwrap(),
        unread - 2
    );
}

#[test]
fn holding_a_sentence_replies_to_just_that_passage() {
    let mut b = Burrow::new(NOW, None);
    let r = reading(&b, "", false);
    let row = r["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| {
            x["excerpt"]
                .as_str()
                .is_some_and(|e| e.split_whitespace().count() > 6)
        })
        .unwrap()
        .clone();
    let key = row["key"].as_str().unwrap();
    // The first few words of the post's text, as a held passage.
    let passage = row["excerpt"]
        .as_str()
        .unwrap()
        .split_whitespace()
        .take(5)
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(outcome(b.reply_to(key, &passage, NOW))["action"], "create");
    let v = view(&b);
    let cur = &v["current"];
    assert_eq!(cur["kind_label"], "≡ thread");
    let content = cur["content"].as_str().unwrap();
    assert!(content.contains(&passage), "{content}");
    let stub = cur["stub_of"].as_str().unwrap();
    assert!(stub.starts_with("↩ Replying to"), "{stub}");
    // It survives a reload.
    let again = Burrow::new(NOW, Some(b.to_json()));
    assert!(
        view(&again)["rows"].as_array().unwrap().len()
            > titles(&view(&Burrow::new(NOW, None))).len()
    );
    assert!(again.to_json().contains("stub_of"));
}

#[test]
fn the_shelf_comes_first_keeps_its_order_and_holds_fifty() {
    let mut b = Burrow::new(NOW, None);
    let all = ids(&view(&b));
    let (a, c) = (&all[3], &all[5]);
    assert_eq!(outcome(b.set_on_shelf(a, true))["action"], "nothing");
    assert_eq!(outcome(b.set_on_shelf(c, true))["action"], "nothing");
    let v = view(&b);
    assert_eq!(v["shelf_len"], 2);
    assert_eq!(v["rows"][0]["id"], a.as_str());
    assert_eq!(v["rows"][1]["id"], c.as_str());
    assert_eq!(v["rows"][0]["shelved"], true);
    b.move_on_shelf(c, 0);
    let v = view(&b);
    assert_eq!(v["shelf"][0]["id"], c.as_str());
    assert_eq!(v["rows"][0]["id"], c.as_str());
    // The open post's 📚 toggles it.
    b.open_from_shelf(a);
    assert_eq!(view(&b)["current"]["shelved"], true);
    assert_eq!(outcome(b.toggle_shelf())["message"], "Taken off your shelf");
    assert_eq!(view(&b)["shelf_len"], 1);
    // Fifty at most.
    for i in 0..60 {
        b.capture(&format!("note {i}"), false, NOW);
    }
    let rest: Vec<String> = ids(&view(&b)).into_iter().filter(|id| id != c).collect();
    for id in &rest[..49] {
        assert_eq!(outcome(b.set_on_shelf(id, true))["action"], "nothing");
    }
    assert_eq!(view(&b)["shelf"].as_array().unwrap().len(), 50);
    let full = outcome(b.set_on_shelf(&rest[49], true));
    assert_eq!(full["action"], "toast", "{full}");
}

#[test]
fn quick_capture_keeps_or_publishes() {
    let mut b = Burrow::new(NOW, None);
    let c: Value = serde_json::from_str(&Burrow::capture_counter("Low tide")).unwrap();
    assert!(c["label"].as_str().unwrap().contains("/ 1000"), "{c}");
    assert_eq!(
        outcome(b.capture("  ", false, NOW))["message"],
        "Nothing to keep yet"
    );
    let kept = outcome(b.capture("Low tide this morning.", false, NOW));
    assert_eq!(kept["message"], "Kept as a note in this browser");
    let p = outcome(b.capture("A map nobody reads.", true, NOW));
    assert_eq!(p["action"], "published");
    assert_eq!(p["version"], 1);
    assert!(
        p["permalink"]
            .as_str()
            .unwrap()
            .starts_with("https://blyg.example.com")
    );
    b.set_offline(true);
    let off = outcome(b.capture("On a train.", true, NOW));
    assert!(
        off["message"].as_str().unwrap().starts_with("Offline"),
        "{off}"
    );
}

#[test]
fn the_pickers_link_anything_and_quote_into_threads() {
    let mut b = Burrow::new(NOW, None);
    let t: Value = serde_json::from_str(&b.pick_targets("")).unwrap();
    let t = t.as_array().unwrap();
    assert!(
        t.iter()
            .any(|x| x["sub"].as_str().unwrap().starts_with("yours"))
    );
    let theirs = t
        .iter()
        .find(|x| !x["sub"].as_str().unwrap().starts_with("yours"))
        .unwrap();
    let (id, label) = (
        theirs["id"].as_str().unwrap(),
        theirs["label"].as_str().unwrap(),
    );
    let link = outcome(b.pick_markup(id, label, false));
    assert_eq!(link["action"], "insert");
    assert!(
        link["text"]
            .as_str()
            .unwrap()
            .starts_with(&format!("[[{id}")),
        "{link}"
    );
    // The open post is a fragment: a quote needs a thread.
    assert_eq!(view(&b)["current"]["kind_label"], "◦ fragment");
    assert_eq!(outcome(b.pick_markup(id, label, true))["action"], "toast");
    b.toggle_kind(NOW);
    let q = outcome(b.pick_markup(id, label, true));
    assert_eq!(q["action"], "insert");
    assert_eq!(q["block"], true);
    assert!(q["text"].as_str().unwrap().starts_with("![["), "{q}");
}
