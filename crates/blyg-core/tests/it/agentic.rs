//! --- agentic feed --- Studio 0.69's Agentic feed (contract 6.2.0):
//! `GET /api/agentic` (the feeds and their names) and `GET
//! /api/agentic/{feed}` (an edition, every reference resolved). Burrow reads
//! the edition and draws it in Reading's "Agentic feed" tab; it never writes
//! one (`putAgentic` is the assistant's). The shapes, the grouping and the
//! gate, without a server; e2e/studio069.rs reads a real one.

use blyg_core::agentic::{
    DEFAULT_FEED, DEFAULT_TITLE, Edition, Mode, Present, SectionKind, grouped, parse_edition,
    parse_feeds, serves, title_or_default,
};
use blyg_core::{RemoteId, SubId};
use serde_json::{Value, json};

/// A reading entry as `GET /reading` returns it (only what the edition
/// needs is read).
fn entry(key: &str, version: u64) -> Value {
    json!({
        "key": key, "source": "imported", "kind": "fragment", "withdrawn": false, "l0": false,
        "contentHtml": "<p>Invented words about tide tables.</p>",
        "displayAt": "2026-11-20T09:00:00.000Z",
        "imported": { "subscriptionId": "sub-a", "remoteId": key, "version": version },
        "background": { "placement": null, "rule": null }
    })
}

/// A `sections`-mode edition: a lead, a section with a group and a folded
/// entry, and a column of briefs.
fn edition() -> Value {
    json!({
        "revision": 3,
        "updatedAt": "2026-11-21T07:00:00.000Z",
        "mode": "sections",
        "layout": {
            "title": "Morning edition",
            "lede": "Three things worth your time.",
            // AgenticEdition.layout.agent: who wrote it, the "model" optional.
            "agent": { "name": "resident", "model": "invented-model-1" },
            "generatedAt": "2026-11-21T06:55:00.000Z",
            // Each entry is an AgenticResolvedEntry: the assistant's reference
            // and words, and its "entry", the reading entry it resolved to.
            "sections": [
                { "title": "Lead", "kind": "lead", "note": null, "entries": [
                    { "sub": "sub-a", "remoteId": "r1", "version": 2,
                      "headline": "The tide tables were wrong", "summary": "A short dek.",
                      "reason": "You follow tides.", "present": "full", "entry": entry("r1", 3) }
                ]},
                { "title": "Also today", "kind": "section", "note": "Smaller things.", "entries": [
                    { "sub": "sub-a", "remoteId": "r2", "group": "harbour", "entry": entry("r2", 1) },
                    { "sub": "~own", "remoteId": "own-1", "group": "harbour", "present": "title", "entry": entry("own-1", 1) },
                    { "sub": "sub-a", "remoteId": "r3", "entry": entry("r3", 1) },
                    { "sub": "sub-a", "remoteId": "r4", "present": "folded", "entry": entry("r4", 1) }
                ]},
                { "title": "Briefs", "kind": "briefs", "note": null, "entries": [
                    { "sub": "sub-a", "remoteId": "r5", "present": "excerpt", "entry": entry("r5", 1) }
                ]}
            ]
        },
        // AgenticEdition.draft: the private draft thread it was composed as,
        // by its item "id".
        "draft": { "id": "draft-1" }
    })
}

#[test]
fn an_edition_reads_with_its_sections_entries_and_words() {
    let e: Edition = parse_edition(&edition()).expect("an edition");
    assert_eq!(e.revision, 3);
    assert_eq!(e.updated_at.as_deref(), Some("2026-11-21T07:00:00.000Z"));
    assert_eq!(e.mode, Some(Mode::Sections));
    assert_eq!(e.draft.as_deref(), Some("draft-1"));
    let l = e.layout.as_ref().expect("laid out");
    assert_eq!(l.title.as_deref(), Some("Morning edition"));
    assert_eq!(l.lede.as_deref(), Some("Three things worth your time."));
    assert_eq!(l.agent.name, "resident");
    assert_eq!(l.agent.model.as_deref(), Some("invented-model-1"));
    assert_eq!(l.generated_at.as_deref(), Some("2026-11-21T06:55:00.000Z"));
    let kinds: Vec<SectionKind> = l.sections.iter().map(|s| s.kind).collect();
    assert_eq!(
        kinds,
        [SectionKind::Lead, SectionKind::Section, SectionKind::Briefs]
    );
    assert_eq!(l.sections[1].note.as_deref(), Some("Smaller things."));
    let lead = &l.sections[0].entries[0];
    assert_eq!(lead.sub, SubId::from("sub-a"));
    assert_eq!(lead.remote_id, RemoteId::from("r1"));
    assert_eq!(lead.version, Some(2));
    assert_eq!(lead.headline.as_deref(), Some("The tide tables were wrong"));
    assert_eq!(lead.summary.as_deref(), Some("A short dek."));
    assert_eq!(lead.reason.as_deref(), Some("You follow tides."));
    assert_eq!(lead.present, Present::Full);
    // The version the entry holds now, from its resolved reading entry.
    assert_eq!(lead.held_version, Some(3));
    assert!(
        lead.changed_since(),
        "the assistant read v2; it's at v3 now"
    );
    let also = &l.sections[1].entries;
    assert_eq!(also[0].present, Present::Excerpt, "the default");
    assert_eq!(also[0].group.as_deref(), Some("harbour"));
    assert_eq!(also[1].present, Present::Title);
    assert_eq!(also[3].present, Present::Folded);
    assert!(
        !also[0].changed_since(),
        "no version judged: never 'changed'"
    );
}

#[test]
fn the_keys_are_every_entry_once_in_edition_order() {
    let e = parse_edition(&edition()).unwrap();
    let keys: Vec<(String, String)> = e
        .keys()
        .into_iter()
        .map(|(s, r)| (s.to_string(), r.to_string()))
        .collect();
    let want = [
        ("sub-a", "r1"),
        ("sub-a", "r2"),
        ("~own", "own-1"),
        ("sub-a", "r3"),
        ("sub-a", "r4"),
        ("sub-a", "r5"),
    ];
    assert_eq!(
        keys,
        want.map(|(s, r)| (s.to_string(), r.to_string())).to_vec()
    );
}

/// Studio's `grouped` (src/ui/agentic.tsx): a group gathers under its first
/// entry; a folded entry joins the one above it.
#[test]
fn entries_group_under_their_first_and_folded_ones_join_the_one_above() {
    let e = parse_edition(&edition()).unwrap();
    let section = &e.layout.unwrap().sections[1];
    let g = grouped(&section.entries);
    let shape: Vec<(usize, Vec<usize>)> = g.into_iter().map(|g| (g.lead, g.also)).collect();
    assert_eq!(shape, vec![(0, vec![1]), (2, vec![3])]);
}

#[test]
fn a_never_written_edition_has_no_layout() {
    let v =
        json!({ "revision": 0, "updatedAt": null, "mode": null, "layout": null, "draft": null });
    let e = parse_edition(&v).expect("still an edition");
    assert_eq!(e.revision, 0);
    assert!(e.layout.is_none());
    assert!(e.is_empty());
    assert!(e.keys().is_empty());
    // Scores mode lays out one section the same way.
    let mut s = edition();
    s["mode"] = json!("scores");
    assert_eq!(parse_edition(&s).unwrap().mode, Some(Mode::Scores));
    // Not an edition at all.
    assert!(parse_edition(&json!({ "error": "no such agentic feed" })).is_none());
}

#[test]
fn feeds_read_with_their_names_and_the_default_title() {
    // listAgenticFeeds' answer (GET /api/agentic): the "feeds" in order.
    let v = json!({ "feeds": [
        { "id": "default", "title": "Daily digest", "revision": 2, "updatedAt": "2026-11-21T07:00:00.000Z" },
        { "id": "weekly", "title": "weekly", "revision": 0, "updatedAt": null }
    ]});
    let feeds = parse_feeds(&v).expect("feeds");
    assert_eq!(feeds.len(), 2);
    assert_eq!(feeds[0].id, DEFAULT_FEED);
    assert_eq!(feeds[0].title, "Daily digest");
    assert_eq!(feeds[0].revision, 2);
    assert_eq!(
        feeds[0].updated_at.as_deref(),
        Some("2026-11-21T07:00:00.000Z")
    );
    assert_eq!(feeds[1].updated_at, None);
    // `agenticTitle`: empty restores the default name.
    assert_eq!(DEFAULT_TITLE, "Agentic feed");
    assert_eq!(title_or_default(""), "Agentic feed");
    assert_eq!(title_or_default("  "), "Agentic feed");
    assert_eq!(title_or_default(" My briefing "), "My briefing");
}

#[test]
fn offered_only_when_the_capabilities_list_get_agentic() {
    assert!(serves(
        &json!({ "operations": ["listAgenticFeeds", "getAgentic", "putAgentic"] })
    ));
    assert!(serves(&json!({ "operations": { "getAgentic": {} } })));
    assert!(!serves(&json!({ "operations": ["getShelf"] })));
    assert!(
        !serves(&json!({ "contractVersion": "6.2.0" })),
        "never by version"
    );
}

/// The feed list (`GET /api/agentic`) follows its own operation,
/// `listAgenticFeeds`; an edition follows `getAgentic`.
#[test]
fn the_feed_list_follows_list_agentic_feeds() {
    use blyg_core::agentic::serves_feeds;
    assert!(serves_feeds(&json!({ "operations": ["listAgenticFeeds"] })));
    assert!(!serves_feeds(&json!({ "operations": ["getAgentic"] })));
    assert!(!serves_feeds(&json!({ "contractVersion": "6.2.0" })));
}

/// An edition is an outside assistant's writing: whatever it sends, each
/// text is clipped and the sections and their entries are capped, so a
/// runaway edition can't swamp the tab.
#[test]
fn an_edition_is_bounded_however_much_it_says() {
    use blyg_core::agentic::{ENTRIES_MAX, SECTIONS_MAX, SHORT_MAX, TEXT_MAX};
    let long = "w".repeat(20_000);
    let mut entries = vec![json!({
        "sub": "sub-a", "remoteId": "r-long",
        "headline": long, "summary": long, "reason": long
    })];
    entries.extend(
        (0..ENTRIES_MAX + 20).map(|i| json!({ "sub": "sub-a", "remoteId": format!("r{i}") })),
    );
    let mut sections =
        vec![json!({ "title": long, "kind": "section", "note": long, "entries": entries })];
    sections.extend((0..SECTIONS_MAX + 5).map(|_| json!({ "title": "More", "entries": [] })));
    let v = json!({
        "revision": 1,
        "layout": {
            "title": long, "lede": long,
            "agent": { "name": long, "model": long },
            "sections": sections
        }
    });
    let l = parse_edition(&v).unwrap().layout.unwrap();
    let n = |s: &str| s.chars().count();
    assert_eq!(l.sections.len(), SECTIONS_MAX);
    assert_eq!(l.sections[0].entries.len(), ENTRIES_MAX);
    assert!(n(l.title.as_deref().unwrap()) <= SHORT_MAX);
    assert!(n(l.lede.as_deref().unwrap()) <= TEXT_MAX);
    assert!(n(&l.agent.name) <= SHORT_MAX);
    assert!(n(l.agent.model.as_deref().unwrap()) <= SHORT_MAX);
    let s = &l.sections[0];
    assert!(n(&s.title) <= SHORT_MAX);
    assert!(n(s.note.as_deref().unwrap()) <= TEXT_MAX);
    let p = &s.entries[0];
    assert!(n(p.headline.as_deref().unwrap()) <= SHORT_MAX);
    assert!(n(p.summary.as_deref().unwrap()) <= TEXT_MAX);
    assert!(n(p.reason.as_deref().unwrap()) <= TEXT_MAX);
    const { assert!(SHORT_MAX < TEXT_MAX && TEXT_MAX < 20_000) };
}
