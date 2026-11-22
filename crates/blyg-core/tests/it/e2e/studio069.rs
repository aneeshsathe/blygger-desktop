//! Studio 0.69 e2e (contract 6.2.0, wave E): the Agentic feed's edition as
//! Burrow reads it, the shared dictionary both ways, Catch up's summary
//! asked of the blyg, and the new AI settings written alone. Each test
//! skips, saying so, on a studio whose capabilities don't list the
//! operation (gated on operation names, never on the contract version).

use super::*;
use blyg_core::access_ai::{AiFunction, ServerAiSettings};
use blyg_core::agentic::{DEFAULT_FEED, DEFAULT_TITLE};
use blyg_core::api::dictionary::PutDictionary;
use blyg_core::dictionary::{Accepted, Edit, OWNER};
use blyg_core::shelf::Entry;

/// The blyg's AI settings, put back when the test ends (tests share a Worker).
struct RestoreAi<'a>(&'a LiveBackend, ServerAiSettings);

impl Drop for RestoreAi<'_> {
    fn drop(&mut self) {
        let _ = self.0.save_server_ai_settings(&self.1);
    }
}

/// An assistant writes an edition (`PUT /api/agentic/default`, as an outside
/// assistant with a token would) naming one of your posts and a post that
/// doesn't exist; Burrow reads it back with the post resolved, the missing
/// one left out, and the edition's words as written. The tab's name is
/// `agenticTitle`, saved alone; empty restores "Agentic feed".
#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn an_assistants_edition_is_read_with_its_entries_resolved() {
    let e = e2e();
    let dir = tempfile::tempdir().unwrap();
    let me = manual(dir.path());
    me.sync_now().unwrap();
    let Some(feeds) = me.agentic_feeds().expect("read the feeds") else {
        eprintln!("skipped: this studio doesn't advertise getAgentic");
        return;
    };
    assert!(
        feeds.iter().any(|f| f.id == DEFAULT_FEED),
        "the default feed: {feeds:?}"
    );
    let ai = me.server_ai_settings().expect("AI settings");
    let _restore = RestoreAi(&me, ai.clone());
    let before = owner("GET", &e.url, "/api/settings", None).1;

    // The tab's name, written alone.
    me.save_server_ai_settings(&ServerAiSettings {
        agentic_title: Some("Invented digest".into()),
        ..Default::default()
    })
    .unwrap();
    let raw = owner("GET", &e.url, "/api/settings", None).1;
    assert_eq!(raw["agenticTitle"], "Invented digest");
    assert_eq!(raw["siteTitle"], before["siteTitle"], "nothing else moved");
    assert_eq!(raw["aiModelTk"], before["aiModelTk"]);
    let named = me.agentic_feeds().unwrap().unwrap();
    assert_eq!(
        named.iter().find(|f| f.id == DEFAULT_FEED).unwrap().title,
        "Invented digest"
    );
    me.save_server_ai_settings(&ServerAiSettings {
        agentic_title: Some(String::new()),
        ..Default::default()
    })
    .unwrap();
    let back = me.agentic_feeds().unwrap().unwrap();
    assert_eq!(
        back.iter().find(|f| f.id == DEFAULT_FEED).unwrap().title,
        DEFAULT_TITLE
    );

    // The assistant's edition.
    let t = tag("agentic");
    let (_, sid) = published(&me, Kind::Fragment, &format!("{t} a post for the edition"), None);
    let current = me
        .agentic_edition(DEFAULT_FEED)
        .unwrap()
        .expect("served");
    let (st, put) = owner(
        "PUT",
        &e.url,
        "/api/agentic/default",
        Some(json!({
            "expectedRevision": current.revision,
            "edition": {
                "title": format!("{t} edition"),
                "lede": "An invented lede.",
                "agent": { "name": "e2e-assistant", "model": "invented-model" },
                "sections": [{
                    "title": "Lead", "kind": "lead",
                    "entries": [
                        { "sub": "~own", "remoteId": sid, "headline": "An invented headline",
                          "reason": "You wrote it.", "present": "full", "version": 1 },
                        { "sub": "~own", "remoteId": "01aaaaaaaaaaaaaaaaaaaaaaaa", "headline": "Gone" }
                    ]
                }]
            }
        })),
    );
    assert_eq!(st, 200, "{put}");
    let ed = me.agentic_edition(DEFAULT_FEED).unwrap().unwrap();
    assert_eq!(ed.revision, current.revision + 1);
    let layout = ed.layout.as_ref().expect("laid out");
    assert_eq!(layout.title.as_deref(), Some(format!("{t} edition").as_str()));
    assert_eq!(layout.lede.as_deref(), Some("An invented lede."));
    assert_eq!(layout.agent.name, "e2e-assistant");
    assert_eq!(layout.agent.model.as_deref(), Some("invented-model"));
    assert_eq!(layout.sections.len(), 1);
    let entries = &layout.sections[0].entries;
    assert_eq!(entries.len(), 1, "the missing post is left out");
    assert_eq!(entries[0].headline.as_deref(), Some("An invented headline"));
    assert_eq!(entries[0].reason.as_deref(), Some("You wrote it."));
    assert!(!entries[0].changed_since());
    assert_eq!(
        ed.keys(),
        vec![(SubId::from("~own"), RemoteId::from(sid.as_str()))],
        "what the tab lists, keyed as Burrow keys your own rows"
    );
    assert!(ed.draft.is_some(), "composed into a private draft thread");

    // A stale revision is the studio's to refuse; Burrow never writes one.
    let (st, _) = owner(
        "PUT",
        &e.url,
        "/api/agentic/default",
        Some(json!({ "expectedRevision": current.revision, "edition": { "agent": { "name": "x" }, "scores": [{ "sub": "~own", "remoteId": sid, "score": 1 }] } })),
    );
    assert_eq!(st, 409);
}

/// A word Burrow learns reaches the blyg's `owner` source; a word Studio
/// adds is accepted by Burrow's spellcheck; a write from a stale revision
/// is a conflict that hands back the current dictionary, and Burrow's edit
/// merges onto it.
#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn the_dictionary_is_shared_both_ways() {
    let e = e2e();
    let dir = tempfile::tempdir().unwrap();
    let me = manual(dir.path());
    let Some(start) = me.dictionary().expect("read the dictionary") else {
        eprintln!("skipped: this studio doesn't advertise getDictionary/putDictionarySource");
        return;
    };
    let mine = tag("burrowword");
    let theirs = tag("studioword");

    // Burrow → the blyg.
    let d = me.edit_dictionary(&Edit::Add(mine.clone())).unwrap();
    assert!(d.revision > start.revision);
    let (st, raw) = owner("GET", &e.url, "/api/dictionary", None);
    assert_eq!(st, 200, "{raw}");
    let owner_words: Vec<String> = raw["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["source"] == OWNER)
        .map(|s| serde_json::from_value(s["words"].clone()).unwrap())
        .unwrap_or_default();
    assert!(owner_words.contains(&mine), "{raw}");

    // Studio → Burrow: a PUT as Studio's Settings sends it.
    let mut words = owner_words.clone();
    words.push(theirs.clone());
    let (st, v) = owner(
        "PUT",
        &e.url,
        "/api/dictionary/owner",
        Some(json!({ "words": words, "expectedRevision": raw["revision"] })),
    );
    assert_eq!(st, 200, "{v}");
    let accepted = Accepted::from(&me.dictionary().unwrap().unwrap());
    assert!(accepted.accepts(&theirs));
    assert!(accepted.accepts(&mine));

    // A stale revision: a conflict carrying the current dictionary.
    let api = Api::new(&e.url, e.cred.clone());
    match api
        .put_dictionary_source(OWNER, &[mine.clone()], start.revision)
        .unwrap()
    {
        PutDictionary::Conflict(Some(current)) => {
            assert!(current.owner_words().contains(&theirs))
        }
        other => panic!("a stale revision: {other:?}"),
    }
    // Burrow's edits read first and merge: neither word is lost.
    let d = me.edit_dictionary(&Edit::Remove(mine.clone())).unwrap();
    assert!(!d.owner_words().contains(&mine));
    assert!(d.owner_words().contains(&theirs), "Studio's word survives");
    let d = me.edit_dictionary(&Edit::Remove(theirs.clone())).unwrap();
    assert!(!d.owner_words().contains(&theirs));
    // A word the studio would refuse never leaves Burrow.
    assert!(me.edit_dictionary(&Edit::Add("two words".into())).is_err());
}

/// Catch up's summary, asked of the blyg for a thread that quotes one of
/// your posts: a summary from the text helpers model, or a plain refusal
/// when no key is set (never "not offered" on a studio that lists the
/// operation). A post that quotes nothing has nothing to summarize (409).
#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn catch_up_summary_is_asked_of_the_blyg() {
    let dir = tempfile::tempdir().unwrap();
    let me = manual(dir.path());
    let t = tag("catchup");
    let (_, quoted) = published(&me, Kind::Fragment, &format!("{t} the first words"), None);
    let (_, thread) = published(
        &me,
        Kind::Thread,
        &format!("{t} an answer\n\n![[{quoted}]]\n\nAnd more of my own."),
        None,
    );
    let ai = me.server_ai_settings().expect("AI settings");
    let _restore = RestoreAi(&me, ai.clone());
    if ai.model(AiFunction::Helpers).is_none() {
        eprintln!("skipped: this studio has no text helpers model setting");
        return;
    }
    if let Some(m) = me
        .ai_models()
        .unwrap_or_default()
        .for_function(AiFunction::Helpers)
        .first()
    {
        me.save_server_ai_settings(&ServerAiSettings {
            model_helpers: Some(m.id.clone()),
            ..Default::default()
        })
        .unwrap();
    }
    match me.summarize_catch_up(&Entry::own(thread.clone())) {
        Ok(Some(s)) => {
            assert!(!s.summary.is_empty());
            assert!(!s.model.is_empty());
            assert!(s.sources.iter().any(|x| x.id == quoted), "{s:?}");
        }
        Ok(None) => panic!("a studio listing summarizeCatchUp offers it"),
        Err(CoreError::Rejected { status, .. }) => assert!(status >= 400, "{status}"),
        Err(CoreError::RateLimited { .. }) => {}
        Err(other) => panic!("summary: {other:?}"),
    }
    match me.summarize_catch_up(&Entry::own(quoted)) {
        Err(CoreError::Rejected { status: 409, .. }) => {}
        other => panic!("nothing quoted, nothing to summarize: {other:?}"),
    }
}

/// The classifier's judging of other blygs' changes (`bgModelImported`,
/// `bgInstruction`, `bgFlagDisagreement`) and the text helpers model, each
/// written alone, as the AI sheet saves them.
#[test]
#[ignore = "needs a local Worker: scripts/e2e-local.sh"]
fn judging_settings_and_the_helpers_model_are_written_alone() {
    let e = e2e();
    let dir = tempfile::tempdir().unwrap();
    let me = manual(dir.path());
    let ai = me.server_ai_settings().expect("AI settings");
    if !ai.judging().offered {
        eprintln!("skipped: this studio has no bgModelImported setting");
        return;
    }
    let _restore = RestoreAi(&me, ai.clone());
    let before = owner("GET", &e.url, "/api/settings", None).1;
    me.save_server_ai_settings(&ServerAiSettings {
        bg_model_imported: Some(true),
        bg_instruction: Some("Trust a note that says typo.".into()),
        bg_flag_disagreement: Some(false),
        model_helpers: Some(String::new()),
        ..Default::default()
    })
    .unwrap();
    let raw = owner("GET", &e.url, "/api/settings", None).1;
    assert_eq!(raw["bgModelImported"], true);
    assert_eq!(raw["bgInstruction"], "Trust a note that says typo.");
    assert_eq!(raw["bgFlagDisagreement"], false);
    assert_eq!(raw["aiModelHelpers"], "");
    for k in ["siteTitle", "aiModelClassify", "backgroundRules", "agenticTitle"] {
        assert_eq!(raw[k], before[k], "{k} moved");
    }
    let read = me.server_ai_settings().unwrap();
    assert_eq!(read.bg_model_imported, Some(true));
    assert_eq!(read.bg_flag_disagreement, Some(false));
}
