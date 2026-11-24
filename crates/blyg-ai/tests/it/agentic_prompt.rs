//! --- agentic writer --- The prompt Burrow's own AI writes the Agentic
//! feed's edition from: the blyg's brief (its Agentic feed prompt) when it
//! has one, else Burrow's; each post it's given, numbered, with its source,
//! title and opening words; JSON only.

use blyg_ai::prompts::{AGENTIC_DEFAULT_BRIEF, agentic_edition_prompt};
use blyg_core::agentic_write::Candidate;
use blyg_core::{RemoteId, SubId};

fn cand(i: usize, title: Option<&str>) -> Candidate {
    Candidate {
        sub: SubId::from("sub-a"),
        remote_id: RemoteId::from(format!("r{i}").as_str()),
        version: 3,
        source: "Ada's harbour".into(),
        title: title.map(str::to_string),
        excerpt: format!("Invented words {i}."),
        at: "2026-11-22T09:00:00.000Z".into(),
        observed_at: "2026-11-22T09:00:00.000Z".into(),
    }
}

#[test]
fn the_prompt_numbers_each_post_and_asks_for_json_only() {
    let given = [cand(1, Some("Tide tables")), cand(2, None)];
    let p = agentic_edition_prompt(None, &given);
    // A newspaper: title, lede, a lead, sections, briefs, why each is there.
    for w in [
        "title",
        "lede",
        "\"lead\"",
        "\"briefs\"",
        "reason",
        "\"ref\"",
    ] {
        assert!(p.system.contains(w), "the system prompt names {w}");
    }
    assert!(p.system.contains("ONLY"), "JSON only");
    assert!(p.user.contains(AGENTIC_DEFAULT_BRIEF), "Burrow's brief");
    assert!(
        p.user.contains("[1] Ada's harbour · Tide tables"),
        "{}",
        p.user
    );
    assert!(p.user.contains("Invented words 1."));
    assert!(
        p.user.contains("[2] Ada's harbour · untitled"),
        "{}",
        p.user
    );
    assert!(
        !p.user.contains("r1") && !p.user.contains("sub-a"),
        "ids stay with Burrow: the model names posts by number"
    );
    // The blyg's own brief replaces Burrow's.
    let p = agentic_edition_prompt(Some("Only boats, please."), &given);
    assert!(p.user.contains("Only boats, please."));
    assert!(!p.user.contains(AGENTIC_DEFAULT_BRIEF));
    // A blank brief is no brief.
    let p = agentic_edition_prompt(Some("  "), &given);
    assert!(p.user.contains(AGENTIC_DEFAULT_BRIEF));
}
