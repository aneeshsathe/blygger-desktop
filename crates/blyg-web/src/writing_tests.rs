//! --- web-writing --- The writing side's rules, driven as `www/writing.js`
//! drives them, with the blyg's answers written out by hand (shapes from
//! Studio's contract 5.7.0 `openapi.json`).

use serde_json::{Value, json};

use super::*;
use crate::Burrow;

/// 2026-11-06T12:00:00Z
const NOW: f64 = 1_793_966_400_000.0;
const BASE: &str = "https://blyg.example.com";

fn v(s: String) -> Value {
    serde_json::from_str(&s).unwrap()
}

fn wire(id: &str, rev: u64, md: &str, status: &str, version: u32) -> Value {
    json!({
        "id": id, "revision": rev, "kind": "fragment", "status": status,
        "created": "2026-11-01T10:00:00Z", "updated": "2026-11-05T10:00:00Z",
        "version": version, "contentMd": md, "dirty": false, "stubOf": null,
        "forkedFrom": null, "provenance": [], "responses": "default", "highlight": "default"
    })
}

const DRAFT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaa";
const PUBLIC: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbb";

fn live() -> Burrow {
    let mut b = Burrow::new_live(NOW, None, None, BASE);
    let items = json!([
        wire(DRAFT, 3, "A draft about tides", "draft", 0),
        wire(PUBLIC, 7, "A published note", "public", 2),
    ]);
    b.live_items(&items.to_string());
    b
}

fn local_of(b: &Burrow, server: &str) -> String {
    b.store.by_server(server).unwrap().0
}

// ------------------------------------------------------------ offsets

#[test]
fn textarea_offsets_are_utf16() {
    let s = "a😀b é";
    assert_eq!(byte_of(s, 0), 0);
    assert_eq!(byte_of(s, 1), 1);
    assert_eq!(byte_of(s, 3), 5, "the emoji is two UTF-16 units");
    assert_eq!(byte_of(s, 99), s.len());
    assert_eq!(u16_of(s, 5), 3);
    assert_eq!(u16_of(s, s.len()), 6);
}

// ------------------------------------------------------------ change notes

#[test]
fn categories_lead_the_note_as_studio_composes_them() {
    assert_eq!(compose_note("", Some("minor")), "minor");
    assert_eq!(
        compose_note("fixed a link", Some("minor")),
        "minor: fixed a link"
    );
    assert_eq!(compose_note("Typo fix.", Some("major")), "Typo fix.");
    assert_eq!(compose_note(" note ", None), "note");
    assert_eq!(compose_note("x", Some("bogus")), "x");
    assert_eq!(leading_category("Typo fix."), Some("typo"));
    assert_eq!(leading_category("typography"), None);
    assert_eq!(strip_category("minor: x"), "x");
    assert_eq!(strip_category("minor"), "");
    assert_eq!(strip_category("Typo fix."), "Typo fix.");
    assert_eq!(
        recategorize("minor: fixed a link", Some("typo")),
        "typo: fixed a link"
    );
    assert_eq!(recategorize("minor: fixed a link", None), "fixed a link");
}

#[test]
fn a_note_is_generated_only_when_it_is_the_machines_unedited() {
    let mut s = NoteSession {
        drafted: Some("Clarified the second paragraph.".into()),
        model: Some("minor".into()),
        in_note: true,
        ..NoteSession::default()
    };
    assert!(s.generated("minor: Clarified the second paragraph."));
    assert!(!s.generated("minor: Clarified the second paragraph a little."));
    s.in_note = false;
    assert!(s.generated("Clarified the second paragraph."));
    // The classifier's bare word, when you left the note empty.
    let s = NoteSession {
        model: Some("typo".into()),
        in_note: true,
        ..NoteSession::default()
    };
    assert!(s.generated("typo"));
    // Your own words with a category in front are still yours.
    let s = NoteSession {
        typed: "fixed it".into(),
        model: Some("typo".into()),
        in_note: true,
        ..NoteSession::default()
    };
    assert!(!s.generated("typo: fixed it"));
}

#[test]
fn the_publish_sheet_offers_ai_only_when_the_blyg_is_set_up_for_it() {
    let mut b = live();
    let pubd = local_of(&b, PUBLIC);
    let draft = local_of(&b, DRAFT);
    let off = v(b.w_note_offer(&pubd));
    assert_eq!(off["draft"], false);
    assert_eq!(off["classify"], false);
    b.w_set_settings(
        &json!({ "aiModelChangelog": "m1", "autoChangeNotes": true, "aiModelClassify": "m2" })
            .to_string(),
    );
    let on = v(b.w_note_offer(&pubd));
    assert_eq!(on["draft"], true);
    assert_eq!(on["auto"], true);
    assert_eq!(on["classify"], true);
    assert_eq!(on["in_note"], true, "changeCategoryInNote defaults to on");
    // A first publish has nothing to compare against.
    assert_eq!(v(b.w_note_offer(&draft))["draft"], false);
    // The demo has no models.
    let demo = Burrow::new(NOW, None);
    assert_eq!(v(demo.w_note_offer("x"))["draft"], false);
}

#[test]
fn drafted_notes_and_categories_reach_the_publish_request() {
    let mut b = live();
    let id = local_of(&b, PUBLIC);
    b.w_set_settings(&json!({ "aiModelChangelog": "m1", "aiModelClassify": "m2" }).to_string());
    b.w_note_begin("", true);
    let r = v(b.w_note_request(&id, "note-draft"));
    assert_eq!(r["req"]["method"], "POST");
    assert_eq!(r["req"]["path"], format!("/api/items/{PUBLIC}/note-draft"));
    assert_eq!(r["req"]["body"]["expectedRevision"], 7);
    let t = v(b.w_note_drafted(
        200,
        &json!({ "note": "Reworded the ending.", "model": "m1", "pinnedPrior": false, "revision": 7 })
            .to_string(),
    ));
    assert_eq!(t["text"], "Reworded the ending.");
    let c = v(b.w_note_classified(
        200,
        &json!({ "revision": 7, "version": 3, "category": "minor", "model": "m2", "probability": 0.81 })
            .to_string(),
        "Reworded the ending.",
    ));
    assert_eq!(c["text"], "minor: Reworded the ending.");
    assert_eq!(c["probability"], 0.81);
    // Choosing another chip swaps the word; pressing it again drops it.
    let t = v(b.w_note_choose("typo", "minor: Reworded the ending."));
    assert_eq!(t["text"], "typo: Reworded the ending.");
    let t = v(b.w_note_choose("minor", "typo: Reworded the ending."));
    assert_eq!(t["text"], "minor: Reworded the ending.");
    let f = v(b.w_note_finish("minor: Reworded the ending."));
    assert_eq!(f["generated"], true);
    assert_eq!(f["classified"], true);
    assert_eq!(f["category"], "minor");
    let p = v(b.w_publish_request(
        &id,
        "minor: Reworded the ending.",
        true,
        true,
        Some("minor".into()),
    ));
    let body = &p["req"]["body"];
    assert_eq!(body["expectedRevision"], 7);
    assert_eq!(body["note"], "minor: Reworded the ending.");
    assert_eq!(body["noteGenerated"], true);
    assert_eq!(body["changeCategory"], "minor");
    // Edited by hand: yours, not generated; no classifier, no category.
    let p = v(b.w_publish_request(&id, "my words", false, false, None));
    assert!(p["req"]["body"].get("noteGenerated").is_none());
    assert!(p["req"]["body"].get("changeCategory").is_none());
    // Classified but none chosen: an explicit null.
    let p = v(b.w_publish_request(&id, "", false, true, None));
    assert_eq!(p["req"]["body"]["changeCategory"], Value::Null);
}

#[test]
fn a_classifier_that_fails_leaves_the_note_and_says_why() {
    let mut b = live();
    b.w_note_begin("fixed a link", true);
    let c = v(b.w_note_classified(
        502,
        &json!({ "code": "provider-error", "message": "the model is unavailable" }).to_string(),
        "fixed a link",
    ));
    assert_eq!(c["category"], Value::Null);
    assert_eq!(c["text"], "fixed a link");
    assert!(c["reason"].as_str().unwrap().contains("unavailable"));
}

#[test]
fn publish_warnings_name_unresolved_handles() {
    let w = Burrow::w_publish_warning(
        200,
        &json!({ "ok": true, "revision": 9, "version": 3, "unresolvedHandles": ["sam", "kit"] })
            .to_string(),
    )
    .unwrap();
    assert_eq!(w, "Not in your contacts, left as written: @sam, @kit");
    assert_eq!(
        Burrow::w_publish_warning(200, r#"{"ok":true,"revision":9,"version":3}"#),
        None
    );
}

// ------------------------------------------------------------ @handles

fn cards() -> Vec<Card> {
    parse_cards(&json!({ "contacts": [
        { "id": "c1", "handle": "robin", "name": "Robin Example", "url": "https://robin.blyg.example.com/",
          "ids": [{ "id": "https://robin.example.org/", "provenance": "self" },
                  { "id": "https://typed.example.org/", "provenance": "entered" }],
          "created": "2026-11-01T00:00:00Z", "updated": "2026-11-01T00:00:00Z" },
    ]}))
}

#[test]
fn contacts_read_with_the_ids_that_go_on_the_wire() {
    let c = cards();
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].handle, "robin");
    assert_eq!(c[0].ids, vec!["https://robin.example.org/".to_string()]);
}

#[test]
fn handles_are_found_where_studio_finds_them() {
    let spots = |md: &str| -> Vec<String> { handle_spots(md).into_iter().map(|s| s.2).collect() };
    assert_eq!(spots("hi @robin, and @kit."), vec!["robin", "kit"]);
    assert!(spots("mail me@robin.example").is_empty(), "not an email");
    assert!(spots("see /@robin").is_empty(), "not a path");
    assert!(spots("@a@b.example").is_empty());
    assert!(spots("@robin.example").is_empty(), "a domain, not a handle");
    assert_eq!(spots("end with @robin."), vec!["robin"]);
    assert!(spots("`@robin`").is_empty(), "code is left alone");
    assert!(spots("```\n@robin\n```\n").is_empty());
    assert!(
        spots("[@robin](https://x.example)").is_empty(),
        "a link is left alone"
    );
    assert!(
        spots("> @robin said").is_empty(),
        "a quoted line is someone else's"
    );
    assert_eq!(spots("@robin-"), vec!["robin-"]);
}

#[test]
fn the_preview_shows_a_contacts_hcard_as_published() {
    let mut b = live();
    b.w_set_contacts(
        200,
        &json!({ "contacts": [{ "id": "c1", "handle": "robin", "name": "Robin Example",
            "url": "https://robin.blyg.example.com/", "ids": [{ "id": "https://robin.example.org/", "provenance": "self" }],
            "created": "x", "updated": "x" }] })
        .to_string(),
    );
    assert!(b.w_contacts_read());
    let id = b.create_draft("Thanks @robin and @nobody for the bread.", NOW);
    assert_eq!(b.current().as_deref(), Some(id.as_str()));
    let page = b.preview_page("n0nce", false);
    assert!(
        page.contains("<span class=\"h-card\"><a class=\"u-url\" href=\"https://robin.blyg.example.com/\">@<span class=\"p-name\">Robin Example</span></a><data class=\"u-url\" value=\"https://robin.example.org/\"></data></span>"),
        "{page}"
    );
    assert!(
        page.contains("@nobody"),
        "an unknown handle stays as written"
    );
    // No contacts: as written.
    let mut b = live();
    b.create_draft("Thanks @robin", NOW);
    assert!(b.preview_page("n0nce", false).contains("@robin"));
    assert!(!b.preview_page("n0nce", false).contains("h-card"));
}

#[test]
fn a_404_means_no_address_book() {
    let mut b = live();
    assert_eq!(
        b.w_set_contacts(404, r#"{"code":"not-found","message":"not found"}"#),
        0
    );
    assert!(b.w_contacts_read());
    let mut b = live();
    b.w_set_contacts(0, "");
    assert!(!b.w_contacts_read(), "offline: try again later");
}

#[test]
fn typing_at_opens_the_list_with_contacts_first() {
    let mut b = live();
    b.w_set_contacts(
        200,
        &json!({ "contacts": [{ "id": "c1", "handle": "robin", "name": "Robin Example",
            "url": "https://robin.blyg.example.com/", "ids": [], "created": "x", "updated": "x" }] })
        .to_string(),
    );
    let old = "Hi ";
    let new = "Hi @";
    let r = v(b.w_mention(old, new, 4, NOW));
    assert_eq!(r["rows"][0]["label"], "Robin Example");
    assert_eq!(r["rows"][0]["contact"], true);
    // Narrowing.
    let r = v(b.w_mention("Hi @", "Hi @ro", 6, NOW));
    assert_eq!(r["query"], "ro");
    assert_eq!(r["rows"][0]["label"], "Robin Example");
    let p = v(b.w_mention_pick("Hi @ro", 6, 0));
    assert_eq!(p["from"], 3);
    assert_eq!(p["to"], 6);
    assert_eq!(
        p["insert"], "@robin",
        "the blyg rewrites a contact's handle"
    );
    // Closed: a newline ends it.
    assert_eq!(b.w_mention("Hi @ro", "Hi @ro\n", 7, NOW), "null");
    // An @ inside a word (an email) doesn't open it.
    assert_eq!(b.w_mention("me", "me@", 3, NOW), "null");
}

// ------------------------------------------------------------ images

#[test]
fn images_go_up_as_they_are_or_reencoded() {
    assert_eq!(image_plan("image/png", 1000.0), ImagePlan::AsIs);
    assert_eq!(
        image_plan("image/jpeg", 6e6),
        ImagePlan::Reencode,
        "a big photo shrinks"
    );
    assert_eq!(
        image_plan("image/heic", 2e6),
        ImagePlan::Reencode,
        "a type the blyg doesn't take"
    );
    assert!(matches!(
        image_plan("image/gif", 6e6),
        ImagePlan::Refuse { .. }
    ));
    assert!(matches!(
        image_plan("text/plain", 10.0),
        ImagePlan::Refuse { .. }
    ));
    // The demo and offline say why.
    let demo = Burrow::new(NOW, None);
    assert!(
        v(demo.w_image_plan("image/png", 10.0))["why"]
            .as_str()
            .unwrap()
            .contains("demo")
    );
    let mut b = live();
    b.set_offline(true);
    assert!(
        v(b.w_image_plan("image/png", 10.0))["why"]
            .as_str()
            .unwrap()
            .contains("connection")
    );
}

#[test]
fn an_upload_lands_where_its_placeholder_was() {
    let mut b = live();
    let id = local_of(&b, DRAFT);
    let n = v(b.w_upload_begin());
    let ph = n["placeholder"].as_str().unwrap().to_string();
    let n = n["n"].as_u64().unwrap() as u32;
    let text = format!("Before\n\n{ph}\n\nafter");
    b.open(&id);
    assert!(b.edit(&text, NOW));
    let at = v(b.w_placeholder_at(&text, n));
    assert_eq!(at["from"], 8);
    let a = v(b.w_upload_answer(
        201,
        &json!({ "id": "M1", "url": "/media/M1/ab12.png", "mime": "image/png" }).to_string(),
        "A [small] garden",
        "data:image/jpeg;base64,AAAA",
    ));
    assert_eq!(a["url"], "https://blyg.example.com/media/M1/ab12.png");
    assert_eq!(
        a["md"],
        "![A \\[small\\] garden](https://blyg.example.com/media/M1/ab12.png)"
    );
    // The editor moved on: it goes into the post itself.
    assert!(b.w_replace_placeholder(&id, n, a["md"].as_str().unwrap(), NOW));
    let it = b.store.item(&LocalId(id.clone())).unwrap();
    assert_eq!(
        it.content_md,
        "Before\n\n![A \\[small\\] garden](https://blyg.example.com/media/M1/ab12.png)\n\nafter"
    );
    assert!(it.pending_sync, "it goes to the blyg with the next send");
    // The preview shows it from this page's thumbnail.
    b.open(&id);
    let page = b.preview_page("n0nce", false);
    assert!(page.contains("data:image/jpeg;base64,AAAA"), "{page}");
    // Gone meanwhile: nothing to replace, and the upload is deleted.
    assert!(!b.w_replace_placeholder(&id, n, "x", NOW));
    let d = v(b.w_media_delete("M1"));
    assert_eq!(d["req"]["method"], "DELETE");
    assert_eq!(d["req"]["path"], "/api/media/M1");
}

#[test]
fn a_refused_upload_says_why() {
    let mut b = live();
    let a = v(b.w_upload_answer(
        413,
        &json!({ "code": "too-large", "message": "file exceeds 5 MB" }).to_string(),
        "",
        "",
    ));
    assert!(a["error"]["text"].as_str().unwrap().contains("5 MB"));
}

// ------------------------------------------------------------ versions

fn versions_page() -> String {
    json!([
        { "kind": "fragment", "itemId": PUBLIC, "version": 1, "contentMd": "x", "contentHtml": "<p>x</p>",
          "contentHash": "h1", "publishedAt": "2026-11-01T10:00:00Z", "note": null, "transclusions": [],
          "pinned": true, "pinnedAt": "2026-11-02T10:00:00Z", "stubOf": null, "stubCite": null,
          "noteGenerated": false, "generated": [] },
        { "kind": "fragment", "itemId": PUBLIC, "version": 2, "contentMd": "y", "contentHtml": "<p>y</p>",
          "contentHash": "h2", "publishedAt": "2026-11-04T10:00:00Z", "note": "tightened", "transclusions": [],
          "pinned": false, "pinnedAt": null, "stubOf": null, "stubCite": null,
          "noteGenerated": false, "generated": [] },
    ])
    .to_string()
}

#[test]
fn your_history_lists_versions_with_what_you_can_do() {
    let mut b = live();
    let id = local_of(&b, PUBLIC);
    b.open(&id);
    let begin = v(b.w_versions_begin());
    assert_eq!(begin["path"], format!("/api/items/{PUBLIC}/versions"));
    let h = v(b.w_versions_view(&id, &versions_page(), NOW));
    assert_eq!(h["current"], 2);
    let rows = h["rows"].as_array().unwrap();
    assert_eq!(rows[0]["version"], 2, "newest first");
    assert_eq!(rows[0]["badge"], "now");
    assert_eq!(rows[0]["note"], "“tightened”");
    assert_eq!(rows[1]["badge"], "📌");
    let acts = |r: &Value| -> Vec<String> {
        r["actions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["act"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(acts(&rows[0]), ["view", "diff", "pin", "fork"]);
    assert_eq!(rows[0]["actions"][3]["on"], false, "fork needs a pin");
    assert_eq!(
        acts(&rows[1]),
        ["view", "diff", "pinned", "fork", "restore"]
    );
    assert_eq!(rows[1]["actions"][3]["on"], true);
}

#[test]
fn a_version_is_read_shown_and_diffed() {
    let mut b = live();
    let id = local_of(&b, PUBLIC);
    b.open(&id);
    let r = v(b.w_version_begin(&id, 1));
    assert_eq!(r["path"], format!("/api/items/{PUBLIC}/versions/1"));
    assert_eq!(
        b.w_version_answer(
            &id,
            1,
            200,
            &json!({ "kind": "fragment", "itemId": PUBLIC, "version": 1, "contentMd": "An early note",
                "contentHtml": "<p>An early note</p>", "contentHash": "h", "publishedAt": "2026-11-01T10:00:00Z",
                "note": null, "transclusions": [], "pinned": true, "pinnedAt": null, "stubOf": null,
                "stubCite": null, "noteGenerated": false, "generated": [] })
            .to_string()
        ),
        "null"
    );
    assert_eq!(v(b.w_version_begin(&id, 1))["ready"], true);
    assert!(
        b.w_version_page(&id, 1, "n0nce", false)
            .contains("An early note")
    );
    let d = b.w_version_diff(&id, 1);
    assert!(d.contains("<del>"), "{d}");
    assert!(d.contains("<ins>"), "{d}");
    assert!(d.contains("note"), "{d}");
}

#[test]
fn pinning_takes_the_typed_word() {
    let mut b = live();
    let id = local_of(&b, PUBLIC);
    assert!(v(b.w_pin(&id, 2, "yes"))["error"].is_object());
    let r = v(b.w_pin(&id, 2, " PIN "));
    assert_eq!(r["req"]["method"], "PUT");
    assert_eq!(
        r["req"]["path"],
        format!("/api/items/{PUBLIC}/versions/2/pin")
    );
    assert_eq!(r["req"]["body"]["expectedRevision"], 7);
    // Not while edits wait to be sent.
    b.open(&id);
    assert!(b.edit("A published note, edited", NOW));
    assert!(v(b.w_pin(&id, 2, "pin"))["error"].is_object());
}

#[test]
fn the_demo_keeps_its_own_history() {
    let mut b = Burrow::new(NOW, None);
    let id = b.create_draft("A demo note", NOW);
    let _ = b.publish("first", NOW);
    assert!(b.edit("A demo note, again", NOW));
    let _ = b.publish("", NOW);
    assert_eq!(v(b.w_versions_begin())["id"], id);
    let h = v(b.w_versions_view(&id, "", NOW));
    assert_eq!(h["rows"].as_array().unwrap().len(), 2);
    assert_eq!(v(b.w_pin(&id, 1, "pin"))["done"], "Pinned v1");
    let h = v(b.w_versions_view(&id, "", NOW));
    assert_eq!(h["rows"][1]["badge"], "📌");
    // Restore v1: its text is back, to publish as v3.
    let o = v(b.w_restore(&id, 1, NOW));
    assert!(o["message"].as_str().unwrap().contains("v3"), "{o}");
    let it = b.store.item(&LocalId(id.clone())).unwrap();
    assert_eq!(it.content_md, "A demo note");
    assert!(it.dirty);
    assert!(b.w_version_diff(&id, 2).contains("<del>"));
    // Withdraw.
    let c = v(b.w_withdraw_check());
    assert_eq!(c["action"], "confirm");
    let w = v(b.w_withdraw(&id, "no longer true", NOW));
    assert_eq!(
        w["done"],
        "Withdrawn as v3 · “no longer true”. It stays listed as withdrawn."
    );
    let h = v(b.w_versions_view(&id, "", NOW));
    assert_eq!(h["rows"][0]["badge"], "withdrawn");
    assert_eq!(v(b.w_withdraw_check())["message"], "Already withdrawn");
}

#[test]
fn deleting_a_published_post_asks_to_withdraw_it() {
    let mut b = live();
    let id = local_of(&b, PUBLIC);
    b.open(&id);
    assert_eq!(v(b.delete_check())["action"], "ask_withdraw");
    let w = v(b.w_withdraw(&id, "", NOW));
    assert_eq!(w["req"]["path"], format!("/api/items/{PUBLIC}/withdraw"));
    assert_eq!(w["req"]["body"], json!({ "expectedRevision": 7 }));
    let w = v(b.w_withdraw(&id, " outdated ", NOW));
    assert_eq!(w["req"]["body"]["note"], "outdated");
    // A draft has nothing to withdraw.
    let d = local_of(&b, DRAFT);
    b.open(&d);
    assert_eq!(v(b.w_withdraw_check())["action"], "toast");
}

// ------------------------------------------------------------ fork

#[test]
fn forking_lists_pins_then_opens_the_new_draft() {
    let mut b = live();
    let p = v(b.w_fork_options_path("sub0|r12", false));
    assert_eq!(p["path"], "/api/fork-options?id=r12&sub=sub0");
    let own = local_of(&b, PUBLIC);
    let p = v(b.w_fork_options_path(&own, true));
    assert_eq!(p["path"], format!("/api/fork-options?id={PUBLIC}"));
    let o = v(b.w_fork_options(
        200,
        &json!({ "origin": "https://writer1.blyg.example.com", "ourOrigin": BASE,
            "versions": [{ "version": 1, "at": "2026-11-01T10:00:00Z", "note": null },
                         { "version": 3, "at": "2026-11-03T10:00:00Z", "note": "settled" }] })
        .to_string(),
        "r12",
        NOW,
    ));
    assert_eq!(o["versions"][0]["version"], 3, "newest first");
    assert_eq!(o["versions"][0]["note"], "“settled”");
    let none = v(b.w_fork_options(
        200,
        &json!({ "origin": "https://writer1.blyg.example.com", "ourOrigin": BASE, "versions": [] })
            .to_string(),
        "r12",
        NOW,
    ));
    assert!(
        none["error"]["text"]
            .as_str()
            .unwrap()
            .contains("no pinned versions")
    );
    let r = v(b.w_fork_request("https://writer1.blyg.example.com", "r12", 3));
    assert_eq!(
        r["req"]["body"],
        json!({ "mode": "fork", "source": { "origin": "https://writer1.blyg.example.com", "id": "r12", "version": 3 } })
    );
    let mut w = wire(
        "cccccccccccccccccccccccccc",
        1,
        "Their pinned words",
        "draft",
        0,
    );
    w["forkedFrom"] =
        json!({ "origin": "https://writer1.blyg.example.com", "id": "r12", "version": 3 });
    let a = v(b.w_fork_answer(201, &w.to_string()));
    let id = a["id"].as_str().unwrap();
    assert_eq!(b.current().as_deref(), Some(id));
    let it = b.store.item(&LocalId(id.into())).unwrap();
    assert_eq!(it.content_md, "Their pinned words");
    assert!(it.forked_from.is_some());
    // The demo has no pins to fork.
    let demo = Burrow::new(NOW, None);
    assert!(v(demo.w_fork_options_path("sub0|r1", false))["error"].is_object());
}

// ------------------------------------------------------------ TK

#[test]
fn tk_scopes_are_listed_and_generated_on_the_blyg() {
    let mut b = live();
    let id = local_of(&b, PUBLIC);
    b.open(&id);
    assert!(b.edit(
        "Intro.\n\n[TK]a line about tides[/TK]\n\n[TK]x[=]done[/TK]",
        NOW
    ));
    let t = v(b.w_tk_view());
    assert_eq!(t["scopes"].as_array().unwrap().len(), 2);
    assert_eq!(t["scopes"][0]["instruction"], "a line about tides");
    assert_eq!(t["scopes"][0]["done"], false);
    assert_eq!(t["scopes"][1]["done"], true);
    // Not until the edit is on the blyg.
    assert!(v(b.w_generate_request(0))["error"].is_object());
    // Sent: the blyg has it.
    let mut n = v(b.outbox_next("[]"));
    n.as_object_mut().unwrap().remove("req");
    let mut w = wire(
        PUBLIC,
        8,
        "Intro.\n\n[TK]a line about tides[/TK]\n\n[TK]x[=]done[/TK]",
        "public",
        2,
    );
    w["dirty"] = json!(true);
    b.outbox_answer(&n.to_string(), 200, &w.to_string(), None);
    let r = v(b.w_generate_request(0));
    assert_eq!(r["req"]["path"], format!("/api/items/{PUBLIC}/generate"));
    assert_eq!(
        r["req"]["body"],
        json!({ "scope": 0, "expectedRevision": 8 })
    );
    assert_eq!(
        Burrow::w_generated_with(r#"{"text":"t","model":"m-small","contentMd":"x","revision":9}"#),
        "m-small"
    );
    let demo = Burrow::new(NOW, None);
    let mut demo = demo;
    demo.create_draft("[TK]x[/TK]", NOW);
    assert!(
        v(demo.w_generate_request(0))["error"]["text"]
            .as_str()
            .unwrap()
            .contains("demo")
    );
}

// ------------------------------------------------------------ settings

#[test]
fn settings_send_only_what_changed_and_check_it() {
    let mut b = live();
    let cur = json!({
        "siteTitle": "Notes", "authorName": "Sam", "authorBio": "", "authorUrl": "", "siteUrl": "",
        "timezone": "", "authorLinks": [{ "label": "Home", "url": "https://example.org/" }],
        "pickerTyping": "auto", "showResponsesDefault": false, "highlightGeneratedDefault": false,
        "aiModel": "", "theme": "default"
    });
    // Not read yet: no save.
    assert!(v(b.w_settings_save("{}"))["error"].is_object());
    b.w_set_settings(&cur.to_string());
    let f = v(b.w_settings_form());
    assert_eq!(f["form"]["siteTitle"], "Notes");
    assert_eq!(f["form"]["links"], "Home | https://example.org/");
    let mut form = f["form"].clone();
    assert_eq!(
        v(b.w_settings_save(&form.to_string()))["nothing"],
        "Nothing changed"
    );
    form["siteTitle"] = json!("Field notes");
    form["showResponsesDefault"] = json!(true);
    form["links"] = json!("Home | https://example.org/\nCode | https://code.example.org/");
    form["timezone"] = json!("Europe/Lisbon");
    let r = v(b.w_settings_save(&form.to_string()));
    assert_eq!(r["req"]["method"], "PATCH");
    assert_eq!(r["req"]["path"], "/api/settings");
    assert_eq!(
        r["req"]["body"],
        json!({ "siteTitle": "Field notes", "timezone": "Europe/Lisbon", "showResponsesDefault": true,
            "authorLinks": [{ "label": "Home", "url": "https://example.org/" }, { "label": "Code", "url": "https://code.example.org/" }] })
    );
    form["authorUrl"] = json!("not a url");
    assert!(
        v(b.w_settings_save(&form.to_string()))["error"]["text"]
            .as_str()
            .unwrap()
            .contains("https://")
    );
    form["authorUrl"] = json!("");
    form["links"] = json!("just words");
    assert!(
        v(b.w_settings_save(&form.to_string()))["error"]["text"]
            .as_str()
            .unwrap()
            .contains("Label | https://")
    );
    // The answer is the stored settings; a new siteUrl moves the base.
    let mut saved = cur.clone();
    saved["siteUrl"] = json!("https://notes.example.com/");
    let a = v(b.w_settings_answer(200, &saved.to_string(), BASE));
    assert_eq!(a["saved"], "Settings saved");
    assert_eq!(b.origin(), "https://notes.example.com");
    // Offline: kept for later is not a thing for settings.
    b.set_offline(true);
    form["links"] = json!("");
    assert!(
        v(b.w_settings_save(&form.to_string()))["error"]["text"]
            .as_str()
            .unwrap()
            .contains("connection")
    );
}

#[test]
fn the_demo_saves_settings_in_the_page() {
    let mut b = Burrow::new(NOW, None);
    let mut form = v(b.w_settings_form())["form"].clone();
    form["authorName"] = json!("Kit Example");
    assert!(v(b.w_settings_save(&form.to_string()))["saved"].is_string());
    assert_eq!(v(b.w_settings_form())["form"]["authorName"], "Kit Example");
}

// ------------------------------------------------------------ the ⋯ menu

#[test]
fn the_post_menu_offers_what_fits_the_post() {
    let mut b = live();
    let id = local_of(&b, PUBLIC);
    b.open(&id);
    let m = v(b.w_post_menu());
    let acts: Vec<&str> = m["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["act"].as_str().unwrap())
        .collect();
    // --- syndication --- a public post lists its copies elsewhere
    assert_eq!(acts, ["image", "tk", "history", "copies", "withdraw"]);
    let d = local_of(&b, DRAFT);
    b.open(&d);
    let m = v(b.w_post_menu());
    let acts: Vec<&str> = m["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["act"].as_str().unwrap())
        .collect();
    assert_eq!(acts, ["image", "tk", "history", "delete"]);
    assert_eq!(m["items"][2]["on"], false, "a draft has no versions");
}

#[test]
fn diffs_escape_html() {
    let d = diff_html("a <b> c", "a <i> c");
    assert_eq!(d, "a &lt;<del>b</del><ins>i</ins>&gt; c");
}
