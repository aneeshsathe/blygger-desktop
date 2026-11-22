//! --- agentic feed --- Studio 0.69's edition in Reading's Agentic feed tab:
//! what each card says (the assistant's words marked as its own), the
//! masthead, what the tab lists and in what order, and its name.

use blyg_core::ReadingItem;
use blyg_core::agentic::{Edition, parse_edition};
use serde_json::{Value, json};

use super::agentic::{ASSISTANT_BADGE, Agentic, Card, card, empty_note, masthead_meta, tab_label};
use super::sources_vm::{Smart, Source};

fn row(sub: &str, id: &str, version: u32, md: &str) -> ReadingItem {
    serde_json::from_value(json!({
        "subscription_id": sub, "remote_id": id, "subscription_title": "Ada's harbour",
        "origin": "https://ada.blyg.example.com/", "kind": "fragment", "state": "current",
        "version": version, "created": "2026-11-20T09:00:00Z", "updated": "2026-11-20T09:00:00Z",
        "observed_at": "2026-11-20T09:00:00Z",
        "content_md": md, "content_html": "", "author": null, "page": null,
        "thumb": null, "hoppers": [],
    }))
    .unwrap()
}

fn placed(extra: Value) -> Value {
    let mut p = json!({ "sub": "sub-a", "remoteId": "r1", "entry": {} });
    p.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    p
}

fn edition_of(entries: Vec<Value>) -> Edition {
    parse_edition(&json!({
        "revision": 1, "updatedAt": "2026-11-21T07:00:00Z", "mode": "sections", "draft": null,
        "layout": {
            "title": "Morning edition", "lede": null,
            "agent": { "name": "resident", "model": "invented-model-1" },
            "generatedAt": "2026-11-21T06:55:00Z",
            "sections": [{ "title": "Lead", "kind": "lead", "note": null, "entries": entries }]
        }
    }))
    .unwrap()
}

fn first(e: &Edition) -> &blyg_core::agentic::Placed {
    &e.layout.as_ref().unwrap().sections[0].entries[0]
}

/// The assistant's headline is its own (marked); without one the card
/// takes the post's own title. An excerpt shows only without a summary,
/// and "changed since" when the post moved past the version it judged.
#[test]
fn a_card_says_whose_words_are_whose() {
    let held = row(
        "sub-a",
        "r1",
        3,
        "# Tide tables\n\nThe tables for the harbour were a day out all month.",
    );
    let e = edition_of(vec![placed(json!({
        "headline": "The tide tables were wrong", "reason": "You follow tides.", "version": 2
    }))]);
    let c: Card = card(first(&e), Some(&held));
    assert_eq!(c.headline, "The tide tables were wrong");
    assert!(c.headline_generated);
    assert!(c.byline.starts_with("Ada's harbour · "), "{}", c.byline);
    assert!(c.byline.ends_with(" · changed since the assistant read it"));
    assert_eq!(
        c.excerpt.as_deref(),
        Some("The tables for the harbour were a day out all month.")
    );
    assert_eq!(c.reason.as_deref(), Some("You follow tides."));
    assert!(c.held);

    // No headline: the post's own title, not marked.
    let e = edition_of(vec![placed(json!({ "summary": "A short dek." }))]);
    let c = card(first(&e), Some(&held));
    assert_eq!(c.headline, "Tide tables");
    assert!(!c.headline_generated);
    assert_eq!(c.summary.as_deref(), Some("A short dek."));
    assert_eq!(c.excerpt, None, "a summary stands in for the excerpt");
    assert!(!c.byline.contains("changed since"));

    // Title only: no excerpt.
    let e = edition_of(vec![placed(json!({ "present": "title" }))]);
    assert_eq!(card(first(&e), Some(&held)).excerpt, None);
}

#[test]
fn a_post_not_on_this_computer_says_so() {
    let e = edition_of(vec![placed(json!({ "headline": "Elsewhere" }))]);
    let c = card(first(&e), None);
    assert_eq!(c.headline, "Elsewhere");
    assert!(!c.held);
    assert_eq!(
        c.byline,
        "Not on this computer yet · it arrives with the next sync"
    );
    let e = edition_of(vec![placed(json!({}))]);
    assert_eq!(card(first(&e), None).headline, "untitled");
}

#[test]
fn the_masthead_names_the_assistant_and_marks_its_words() {
    let e = edition_of(vec![]);
    let meta = masthead_meta(e.layout.as_ref().unwrap(), Some("21 Nov 2026".into()));
    assert_eq!(
        meta,
        format!("by resident (invented-model-1) · 21 Nov 2026 · {ASSISTANT_BADGE}")
    );
    assert_eq!(ASSISTANT_BADGE, "✦ written by your assistant");
    let note = empty_note("Daily digest");
    assert!(note.starts_with("No edition yet."), "{note}");
    assert!(note.contains("PUT /api/agentic/default"), "{note}");
}

/// The tab lists the edition's posts held here, in the edition's order,
/// each once, and is named as the blyg names it.
#[test]
fn the_tab_lists_the_editions_posts_and_wears_its_name() {
    let mut a = Agentic::default();
    assert!(a.keys().is_empty());
    assert_eq!(tab_label(&a), "Agentic feed");
    a.set_edition(Some(edition_of(vec![
        placed(json!({ "remoteId": "r2" })),
        placed(json!({ "remoteId": "r1" })),
        placed(json!({ "sub": "~own", "remoteId": "mine" })),
    ])));
    let keys: Vec<String> = a
        .keys()
        .iter()
        .map(|k| format!("{}/{}", k.0, k.1))
        .collect();
    assert_eq!(keys, ["sub-a/r2", "sub-a/r1", "~own/mine"]);
    a.set_title(Some("Daily digest".into()));
    assert_eq!(tab_label(&a), "Daily digest");
    a.set_title(Some(String::new()));
    assert_eq!(tab_label(&a), "Agentic feed");
    // Its own source, listed in the edition's order (not the Feed's).
    assert!(!super::feed::follows_feed(
        super::stream_vm::ReadMode::Reader,
        &Source::Smart(Smart::Agentic)
    ));
}
