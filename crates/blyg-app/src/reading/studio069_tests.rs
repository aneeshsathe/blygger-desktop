//! Studio 0.69 in the app (wave E): Catch up asks the blyg for its summary
//! first, the spellcheck accepts the blyg's dictionary, and the AI sheet
//! shows the new rows only where they apply.

use std::ops::Range;

use blyg_core::ReadingItem;
use blyg_core::access_ai::{AiFunction, ServerAiSettings};
use blyg_core::dictionary::{Accepted, parse};
use blyg_core::shelf::Entry;
use serde_json::json;

use crate::app::server_ai::{ROWS, Row, row_shown};
use crate::composer::spell::drop_accepted;

fn row(sub: &str, id: &str) -> ReadingItem {
    serde_json::from_value(json!({
        "subscription_id": sub, "remote_id": id, "subscription_title": "Ada",
        "origin": "https://ada.blyg.example.com/", "kind": "thread", "state": "current",
        "version": 1, "created": "2026-11-20T09:00:00Z", "updated": "2026-11-20T09:00:00Z",
        "observed_at": "2026-11-20T09:00:00Z",
        "content_md": "An answer", "content_html": "", "author": null, "page": null,
        "thumb": null, "hoppers": [],
    }))
    .unwrap()
}

/// The blyg's summary covers the quotes it holds for one entry: asked for
/// a held starting post (yours as `~own`), never for "whole conversation"
/// (its side branches are Burrow's own), and never for a post not held.
#[test]
fn catch_up_asks_the_blyg_only_for_what_its_summary_covers() {
    use super::catchup::blyg_summary_entry;
    let theirs = row("sub-a", "r1");
    assert_eq!(
        blyg_summary_entry(false, Some(&theirs)),
        Some(Entry::new("sub-a", "r1"))
    );
    let mine = row(blyg_core::OWN_SUBSCRIPTION, "01own");
    assert_eq!(
        blyg_summary_entry(false, Some(&mine)),
        Some(Entry::own("01own"))
    );
    assert_eq!(blyg_summary_entry(true, Some(&theirs)), None, "whole");
    assert_eq!(blyg_summary_entry(false, None), None, "not held");
}

/// A word in any of the blyg's sources is no longer underlined; the rest
/// stay as the system checker flagged them.
#[test]
fn the_spellcheck_accepts_the_blygs_words() {
    let d = parse(&json!({ "sources": [
        { "source": "owner", "words": ["impyrt", "Burrow"] },
        { "source": "maritime", "words": ["kedge"] }
    ], "revision": 2 }))
    .unwrap();
    let accepted = Accepted::from(&d);
    let text = "An impyrt of the kedge from burrow and Burrow, with tabels.";
    let find = |w: &str| -> Range<usize> {
        let at = text.find(w).unwrap();
        at..at + w.len()
    };
    let flagged = vec![
        find("impyrt"),
        find("kedge"),
        find("burrow"),
        find("Burrow,").start..find("Burrow,").start + 6,
        find("tabels"),
    ];
    let kept: Vec<&str> = drop_accepted(text, flagged, &accepted)
        .into_iter()
        .map(|r| &text[r])
        .collect();
    assert_eq!(kept, ["burrow", "tabels"]);
    // No dictionary (a blyg without one, or not read yet): nothing dropped.
    assert_eq!(
        drop_accepted(text, vec![find("impyrt")], &Accepted::default()).len(),
        1
    );
}

/// The AI sheet's new rows: the text helpers model, and the classifier's
/// judging of other blygs' changes, shown as Studio's Settings shows them.
#[test]
fn the_ai_sheet_shows_the_new_rows_where_they_apply() {
    assert!(ROWS.contains(&Row::Model(AiFunction::Helpers)));
    assert!(ROWS.contains(&Row::JudgeImported));
    assert!(ROWS.contains(&Row::FlagDisagreement));
    let mut s = ServerAiSettings::from_json(&json!({
        "aiModelHelpers": "", "aiModelClassify": "",
        "bgModelImported": false, "bgInstruction": "", "bgFlagDisagreement": true
    }));
    assert!(row_shown(Row::Model(AiFunction::Helpers), &s));
    assert!(row_shown(Row::JudgeImported, &s), "shown, greyed out");
    assert!(!row_shown(Row::FlagDisagreement, &s));
    s.model_classify = Some("m-judge".into());
    s.bg_model_imported = Some(true);
    s.bg_instruction = Some("Trust notes.".into());
    assert!(row_shown(Row::FlagDisagreement, &s));
    // A studio before 0.69: none of them.
    let old = ServerAiSettings::from_json(&json!({ "aiModelTk": "" }));
    assert!(!row_shown(Row::Model(AiFunction::Helpers), &old));
    assert!(!row_shown(Row::JudgeImported, &old));
}
