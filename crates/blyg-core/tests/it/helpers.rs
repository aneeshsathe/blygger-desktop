//! --- helpers --- Studio 0.69's helpers (contract 6.2.0) and the settings it
//! adds. Burrow's Catch up asks the blyg for its summary
//! (`summarizeCatchUp`, written once per set of versions by the blyg's text
//! helpers model) when the blyg offers it, else its own AI. The AI sheet
//! gains the text helpers model, the Agentic feed's name and the classifier's
//! judging of other blygs' changes, each written alone. Without a server;
//! e2e/studio069.rs asks a real one.

use blyg_core::access_ai::{AiFunction, ServerAiSettings};
use blyg_core::helpers::{SUMMARIZE_OPERATION, parse_summary, serves_summary, summary_body};
use blyg_core::shelf::Entry;
use serde_json::json;

#[test]
fn a_summary_reads_with_its_model_sources_and_cache_mark() {
    let v = json!({
        "summary": "Ada asked about tides; Ben answered with tables.",
        "model": "invented-model-1",
        "at": "2026-11-21T07:00:00.000Z",
        "sources": [
            { "id": "01aaa", "version": 1, "origin": "https://ada.blyg.example.com/" },
            { "id": "01bbb", "version": null }
        ],
        "cached": true
    });
    let s = parse_summary(&v).expect("a summary");
    assert_eq!(
        s.summary,
        "Ada asked about tides; Ben answered with tables."
    );
    assert_eq!(s.model, "invented-model-1");
    assert_eq!(s.at, "2026-11-21T07:00:00.000Z");
    assert!(s.cached);
    assert_eq!(s.sources.len(), 2);
    assert_eq!(
        s.sources[0].origin.as_deref(),
        Some("https://ada.blyg.example.com/")
    );
    assert_eq!(s.sources[1].version, None);
    assert_eq!(s.sources[1].origin, None, "your own: no origin (§5.9)");
    assert!(parse_summary(&json!({ "error": "x" })).is_none());
}

#[test]
fn a_summary_is_asked_by_the_entry_it_starts_from() {
    assert_eq!(
        summary_body(&Entry::new("sub-a", "r1")),
        json!({ "sub": "sub-a", "remoteId": "r1" })
    );
    assert_eq!(
        summary_body(&Entry::own("01own")),
        json!({ "sub": "~own", "remoteId": "01own" })
    );
    assert_eq!(SUMMARIZE_OPERATION, "summarizeCatchUp");
    assert!(serves_summary(
        &json!({ "operations": ["getShelf", "summarizeCatchUp"] })
    ));
    assert!(!serves_summary(&json!({ "operations": ["getShelf"] })));
}

/// The new settings read as the studio sends them and are written alone: a
/// key the studio didn't report is never sent (an older studio refuses it).
#[test]
fn the_new_ai_settings_round_trip_and_stay_partial() {
    assert_eq!(AiFunction::Helpers.key(), "aiModelHelpers");
    assert_eq!(AiFunction::Helpers.label(), "Text helpers");
    assert!(AiFunction::ALL.contains(&AiFunction::Helpers));
    let v = json!({
        "aiModelTk": "", "aiModelChangelog": "m-notes", "aiModelHelpers": "m-help",
        "aiModelClassify": "m-judge", "aiModelFeed": "", "feedPrompt": "",
        "agenticTitle": "Daily digest",
        "bgModelImported": true, "bgInstruction": "Trust notes that say typo.",
        "bgFlagDisagreement": false
    });
    let s = ServerAiSettings::from_json(&v);
    assert_eq!(s.model(AiFunction::Helpers), Some("m-help"));
    assert_eq!(s.agentic_title.as_deref(), Some("Daily digest"));
    assert_eq!(s.bg_model_imported, Some(true));
    assert_eq!(
        s.bg_instruction.as_deref(),
        Some("Trust notes that say typo.")
    );
    assert_eq!(s.bg_flag_disagreement, Some(false));
    let back = s.to_json();
    assert_eq!(back["aiModelHelpers"], "m-help");
    assert_eq!(back["agenticTitle"], "Daily digest");
    assert_eq!(back["bgModelImported"], true);
    assert_eq!(back["bgInstruction"], "Trust notes that say typo.");
    assert_eq!(back["bgFlagDisagreement"], false);

    // A studio before 0.69: none of the new keys read, none sent.
    let old = ServerAiSettings::from_json(&json!({ "aiModelTk": "", "aiModelChangelog": "" }));
    let sent = old.to_json();
    for k in [
        "aiModelHelpers",
        "agenticTitle",
        "bgModelImported",
        "bgInstruction",
        "bgFlagDisagreement",
    ] {
        assert!(sent.get(k).is_none(), "{k} sent to a studio without it");
    }
}

/// What the classifier's judging of other blygs' changes may show, as
/// Studio's Settings › Background does: the toggle only with a classifier,
/// the instruction only while judging, the keep-in-view switch only with an
/// instruction.
#[test]
fn judging_other_blygs_changes_shows_only_what_applies() {
    let mut s = ServerAiSettings::from_json(&json!({
        "aiModelClassify": "", "bgModelImported": false, "bgInstruction": "", "bgFlagDisagreement": true
    }));
    let j = s.judging();
    assert!(j.offered, "the studio has it");
    assert!(!j.can_judge, "no classifier chosen");
    assert!(!j.instruction && !j.flag);
    s.model_classify = Some("m-judge".into());
    s.bg_model_imported = Some(true);
    let j = s.judging();
    assert!(j.can_judge && j.instruction && !j.flag);
    s.bg_instruction = Some("Trust notes.".into());
    assert!(s.judging().flag);
    // The keep-in-view default is on unless the studio says off.
    assert!(s.judging().flag_on);
    assert!(
        !ServerAiSettings::from_json(&json!({ "aiModelTk": "" }))
            .judging()
            .offered
    );
}
