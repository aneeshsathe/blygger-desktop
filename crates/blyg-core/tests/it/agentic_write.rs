//! --- agentic writer --- Burrow's own AI writes the blyg's Agentic feed
//! edition (`PUT /api/agentic/{feed}`, operation `putAgentic`): which
//! reading entries it's given, how a model's reply becomes an
//! AgenticEditionInput the studio accepts (references it wasn't given
//! dropped, every field clamped to the contract's bounds, at least one
//! section), the body it sends, and the read-then-write with one retry on a
//! 409. Without a server; e2e/studio069.rs writes to a real one.

use blyg_core::agentic::{Edition, Present, SectionKind, parse_edition};
use blyg_core::agentic_write::{
    AgentInput, CANDIDATES_MAX, Candidate, EditionInput, Put, ReplyError, Sent, candidates, due,
    from_reply, has_new, put_body, send_with,
};
use blyg_core::{CoreError, ReadingItem, RemoteId, SubId};
use serde_json::{Value, json};

fn row(sub: &str, id: &str, observed: &str, read: Option<u32>) -> ReadingItem {
    serde_json::from_value(json!({
        "subscription_id": sub, "remote_id": id, "subscription_title": format!("Blyg {sub}"),
        "origin": "https://ada.blyg.example.com/", "kind": "fragment", "state": "current",
        "version": 2, "created": observed, "updated": observed, "observed_at": observed,
        "content_md": format!("# Title {id}\n\nInvented words about tide tables in {id}."),
        "content_html": "", "author": null, "page": null, "thumb": null, "hoppers": [],
        "read_version": read,
    }))
    .unwrap()
}

fn cand(i: usize) -> Candidate {
    Candidate {
        sub: SubId::from("sub-a"),
        remote_id: RemoteId::from(format!("r{i}").as_str()),
        version: 1,
        source: "Ada's harbour".into(),
        title: Some(format!("Post {i}")),
        excerpt: "Invented words.".into(),
        at: "2026-11-22T09:00:00.000Z".into(),
        observed_at: "2026-11-22T09:00:00.000Z".into(),
    }
}

fn agent() -> AgentInput {
    AgentInput {
        name: "Burrow · Codex (local)".into(),
        model: Some("invented-model-2".into()),
    }
}

const NOW: &str = "2026-11-23T08:00:00.000Z";

/// What the writer is given: posts of others you hold, unread or arrived
/// since the last edition, newest first, at most 60. Your own posts,
/// withdrawn ones and ones read before the last edition aren't.
#[test]
fn the_writer_is_given_new_and_unread_posts_newest_first() {
    let since = "2026-11-22T00:00:00.000Z";
    let mut withdrawn = row("sub-a", "gone", "2026-11-22T10:00:00Z", None);
    withdrawn.state = "tombstone".into();
    let rows = vec![
        row("sub-a", "old-read", "2026-11-20T09:00:00Z", Some(2)),
        row("sub-a", "old-unread", "2026-11-20T08:00:00Z", None),
        row("sub-b", "new-read", "2026-11-22T12:00:00Z", Some(2)),
        row("~own", "mine", "2026-11-22T13:00:00Z", None),
        withdrawn,
    ];
    let c = candidates(&rows, Some(since));
    let ids: Vec<&str> = c.iter().map(|c| c.remote_id.as_str()).collect();
    assert_eq!(ids, ["new-read", "old-unread"], "newest first");
    let first = &c[0];
    assert_eq!(first.sub.as_str(), "sub-b");
    assert_eq!(first.version, 2);
    assert_eq!(first.source, "Blyg sub-b");
    assert_eq!(first.title.as_deref(), Some("Title new-read"));
    assert_eq!(
        first.excerpt,
        "Invented words about tide tables in new-read."
    );
    // Never an edition before: everything unread.
    let ids: Vec<String> = candidates(&rows, None)
        .iter()
        .map(|c| c.remote_id.to_string())
        .collect();
    assert_eq!(ids, ["old-unread"]);
    // Capped.
    let many: Vec<ReadingItem> = (0..CANDIDATES_MAX + 15)
        .map(|i| row("sub-a", &format!("m{i}"), "2026-11-22T09:00:00Z", None))
        .collect();
    assert_eq!(CANDIDATES_MAX, 60);
    assert_eq!(candidates(&many, None).len(), CANDIDATES_MAX);
}

/// After a sync it runs only when posts arrived since the last edition and
/// the gap since that edition has passed.
#[test]
fn it_runs_after_a_sync_only_when_due_and_something_is_new() {
    let hour = 3_600_000;
    let at = |s: &str| blyg_core::agentic_write::ms(s).unwrap();
    let now = at("2026-11-23T08:00:00Z");
    assert!(due(None, now, 3 * hour), "never written");
    assert!(due(Some("2026-11-23T04:59:00Z"), now, 3 * hour));
    assert!(!due(Some("2026-11-23T06:00:00Z"), now, 3 * hour));
    let c = vec![cand(1)];
    assert!(has_new(&c, None));
    assert!(has_new(&c, Some("2026-11-22T08:00:00Z")));
    assert!(!has_new(&c, Some("2026-11-22T10:00:00Z")), "nothing since");
    assert!(!has_new(&[], None));
}

/// A model's reply (fenced or not) becomes an edition: references by the
/// number each post was given, the ones it wasn't given dropped, a post
/// named twice kept once, sections left empty dropped, unknown kinds and
/// presents left out, and the agent, model and time Burrow's.
#[test]
fn a_reply_becomes_an_edition_of_only_the_posts_it_was_given() {
    let given = vec![cand(1), cand(2), cand(3)];
    let reply = r#"Here it is:
```json
{"title": "The Monday paper", "lede": "Tides, mostly.",
 "sections": [
   {"title": "Lead", "kind": "lead", "entries": [
      {"ref": 2, "headline": "Tides were wrong", "summary": "A dek.", "reason": "You follow tides.", "present": "full"}
   ]},
   {"title": "Also", "kind": "sidebar", "note": "Smaller things.", "entries": [
      {"ref": 1, "group": "harbour", "present": "sideways"},
      {"ref": 99, "headline": "Invented"},
      {"ref": 2, "headline": "Again"},
      {"ref": "3", "present": "title"}
   ]},
   {"title": "Nothing", "entries": [{"ref": 42}]}
 ]}
```"#;
    let e = from_reply(reply, &given, agent(), NOW).expect("an edition");
    assert_eq!(e.title.as_deref(), Some("The Monday paper"));
    assert_eq!(e.lede.as_deref(), Some("Tides, mostly."));
    assert_eq!(e.agent, agent());
    assert_eq!(e.generated_at.as_deref(), Some(NOW));
    assert_eq!(e.sections.len(), 2, "the empty section is dropped");
    let lead = &e.sections[0];
    assert_eq!(lead.kind, Some(SectionKind::Lead));
    assert_eq!(lead.entries[0].remote_id.as_str(), "r2");
    assert_eq!(lead.entries[0].version, Some(1), "the version it was given");
    assert_eq!(lead.entries[0].present, Some(Present::Full));
    let also = &e.sections[1];
    assert_eq!(also.kind, None, "an unknown kind is left out");
    assert_eq!(also.note.as_deref(), Some("Smaller things."));
    let ids: Vec<&str> = also.entries.iter().map(|p| p.remote_id.as_str()).collect();
    assert_eq!(ids, ["r1", "r3"], "99 wasn't given; r2 is already in");
    assert_eq!(also.entries[0].present, None);
    assert_eq!(also.entries[0].group.as_deref(), Some("harbour"));
    assert_eq!(also.entries[1].present, Some(Present::Title));
    assert_eq!(e.entry_count(), 3);
}

/// Unusable output is an error and nothing is sent.
#[test]
fn unusable_output_is_an_error() {
    let given = vec![cand(1)];
    assert_eq!(
        from_reply("I couldn't do that.", &given, agent(), NOW),
        Err(ReplyError::NotJson)
    );
    assert_eq!(
        from_reply(
            r#"{"sections": [{"title": "Lead", "entries": [{"ref": 7}]}]}"#,
            &given,
            agent(),
            NOW
        ),
        Err(ReplyError::NoSections)
    );
    assert_eq!(
        from_reply(r#"{"title": "Empty"}"#, &given, agent(), NOW),
        Err(ReplyError::NoSections)
    );
    for e in [ReplyError::NotJson, ReplyError::NoSections] {
        assert!(!e.to_string().is_empty());
    }
}

/// Every field is clamped to the contract's bounds (AgenticEditionInput,
/// the studio's `AGENTIC_LIMITS`), counted as the studio counts (UTF-16),
/// and at most 20 sections of 50 entries.
#[test]
fn every_field_is_clamped_to_the_contracts_bounds() {
    let given: Vec<Candidate> = (0..60).map(cand).collect();
    let long = "w".repeat(9_000);
    let wide = "🌊".repeat(3_000); // two UTF-16 units each
    let entries: Vec<Value> = (0..60)
        .map(|i| json!({ "ref": i + 1, "headline": long, "summary": long, "reason": wide, "group": long }))
        .collect();
    let mut sections = vec![json!({ "title": long, "note": wide, "entries": entries })];
    sections.extend((0..30).map(|i| json!({ "title": "", "entries": [{ "ref": (i % 60) + 1 }] })));
    let reply = json!({ "title": wide, "lede": long, "sections": sections }).to_string();
    let a = AgentInput {
        name: "n".repeat(500),
        model: Some("m".repeat(500)),
    };
    let e = from_reply(&reply, &given, a, NOW).unwrap();
    let u = |s: &str| s.encode_utf16().count();
    assert!(u(e.title.as_deref().unwrap()) <= 200);
    assert!(u(e.lede.as_deref().unwrap()) <= 4_000);
    assert!(u(&e.agent.name) <= 80);
    assert!(u(e.agent.model.as_deref().unwrap()) <= 120);
    assert_eq!(e.sections.len(), 1, "every later entry was already placed");
    let s = &e.sections[0];
    assert!(u(&s.title) <= 200);
    assert!(u(s.note.as_deref().unwrap()) <= 4_000);
    assert_eq!(s.entries.len(), 50);
    for p in &s.entries {
        assert!(u(p.headline.as_deref().unwrap()) <= 200);
        assert!(u(p.summary.as_deref().unwrap()) <= 2_000);
        assert!(u(p.reason.as_deref().unwrap()) <= 500);
        assert!(u(p.group.as_deref().unwrap()) <= 60);
    }
    // Sections beyond 20 are dropped; an untitled one gets a title.
    let many: Vec<Value> = (0..30)
        .map(|i| json!({ "title": "", "entries": [{ "ref": i + 1 }] }))
        .collect();
    let e = from_reply(
        &json!({ "sections": many }).to_string(),
        &given,
        agent(),
        NOW,
    )
    .unwrap();
    assert_eq!(e.sections.len(), 20);
    assert!(e.sections.iter().all(|s| !s.title.is_empty()));
}

/// The body is `{edition, expectedRevision}` with an AgenticEditionInput
/// in sections mode: absent fields left out (the studio refuses unknown
/// ones and empty strings).
#[test]
fn the_body_is_the_contracts_shape() {
    let given = vec![cand(1), cand(2)];
    let reply = r#"{"title": "Paper", "sections": [{"title": "Lead", "kind": "briefs",
        "entries": [{"ref": 1, "reason": "Because."}, {"ref": 2}]}]}"#;
    let e: EditionInput = from_reply(reply, &given, agent(), NOW).unwrap();
    let body = put_body(&e, 4);
    assert_eq!(
        body,
        json!({
            "expectedRevision": 4,
            "edition": {
                "title": "Paper",
                "agent": { "name": "Burrow · Codex (local)", "model": "invented-model-2" },
                "generatedAt": NOW,
                "sections": [{
                    "title": "Lead", "kind": "briefs",
                    "entries": [
                        { "sub": "sub-a", "remoteId": "r1", "version": 1, "reason": "Because." },
                        { "sub": "sub-a", "remoteId": "r2", "version": 1 }
                    ]
                }]
            }
        })
    );
    assert!(
        body["edition"].get("scores").is_none(),
        "sections, never both"
    );
}

fn written(revision: u64) -> Edition {
    parse_edition(&json!({ "revision": revision, "layout": null })).unwrap()
}

/// Read the revision, write from it; on a 409 read again and write once
/// more; a second 409 gives up. Not offered is said, and nothing written.
#[test]
fn a_conflict_is_read_again_and_written_once_more() {
    // Written first time.
    let mut puts = vec![];
    let r = send_with(
        || Ok(Some(3)),
        |rev| {
            puts.push(rev);
            Ok(Put::Written(written(rev + 1)))
        },
    )
    .unwrap();
    assert!(matches!(r, Sent::Written(e) if e.revision == 4));
    assert_eq!(puts, [3]);

    // A conflict, read again, written.
    let mut reads = vec![Some(5), Some(3)];
    let mut puts = vec![];
    let r = send_with(
        || Ok(reads.pop().flatten()),
        |rev| {
            puts.push(rev);
            Ok(if rev == 3 {
                Put::Conflict
            } else {
                Put::Written(written(rev + 1))
            })
        },
    )
    .unwrap();
    assert!(matches!(r, Sent::Written(e) if e.revision == 6));
    assert_eq!(puts, [3, 5]);

    // Two conflicts: give up.
    let mut n = 0;
    let r = send_with(
        || Ok(Some(1)),
        |_| {
            n += 1;
            Ok(Put::Conflict)
        },
    )
    .unwrap();
    assert!(matches!(r, Sent::Conflicted));
    assert_eq!(n, 2);

    // Not offered here (no feed, or no putAgentic): nothing written.
    let r = send_with(|| Ok(None), |_| panic!("never written")).unwrap();
    assert!(matches!(r, Sent::NotOffered));
    let r = send_with(|| Ok(Some(0)), |_| Ok(Put::NotOffered)).unwrap();
    assert!(matches!(r, Sent::NotOffered));
    // A failed read is the error.
    assert!(send_with(|| Err(CoreError::Offline), |_| panic!()).is_err());
}
