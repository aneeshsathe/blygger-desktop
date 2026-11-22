//! --- dictionary --- Studio 0.69's dictionary (contract 6.2.0): the words a
//! blyg's spellchecking accepts, shared by every client (`GET
//! /api/dictionary`, `PUT /api/dictionary/{source}`). Burrow's spellcheck
//! accepts them too, and "Learn Spelling" adds to the blyg's `owner`
//! source. The shapes, the word rules and the edits, without a server;
//! e2e/studio069.rs shares a word with a real studio both ways.

use blyg_core::dictionary::{
    Accepted, Dictionary, Edit, OWNER, WORD_MAX, apply, parse, put_body, serves, word_problem,
};
use serde_json::json;

const BLYG_A: &str = "https://blyg.example.com";
const BLYG_B: &str = "https://notes.example.org";

/// A `LiveBackend` on `dir` for the blyg at `url`, sync worker off. Every
/// URL these tests open is unreachable: each call is offline.
fn offline_at(dir: &std::path::Path, url: &str) -> blyg_core::LiveBackend {
    blyg_core::LiveBackend::open_with(
        dir,
        url,
        "unused-test-credential",
        blyg_core::SyncOptions {
            start_worker: false,
            ..blyg_core::SyncOptions::default()
        },
    )
    .unwrap()
}

fn sample() -> Dictionary {
    parse(&json!({
        // Each source is a DictionarySource: its name and its "words".
        "sources": [
            { "source": "owner", "words": ["blyg", "Burrow", "impyrt"] },
            { "source": "maritime", "words": ["bollard", "kedge"] }
        ],
        "revision": 7
    }))
    .expect("a dictionary")
}

#[test]
fn a_dictionary_reads_with_its_sources_and_revision() {
    let d = sample();
    assert_eq!(d.revision, 7);
    assert_eq!(d.sources.len(), 2);
    assert_eq!(d.sources[1].source, "maritime");
    assert_eq!(d.owner_words(), ["blyg", "Burrow", "impyrt"]);
    assert_eq!(d.word_count(), 5);
    // Never written: no sources, revision 0.
    let empty = parse(&json!({ "sources": [], "revision": 0 })).unwrap();
    assert!(empty.owner_words().is_empty());
    assert!(parse(&json!({ "error": "nope" })).is_none());
}

/// Every source counts. A word kept in lowercase accepts any capitals; a
/// word kept with capitals (a name) accepts only those capitals, as
/// written.
#[test]
fn every_source_is_accepted_and_names_keep_their_capitals() {
    let a = Accepted::from(&sample());
    assert!(a.accepts("blyg"));
    assert!(a.accepts("Blyg"), "a sentence's first word");
    assert!(a.accepts("BLYG"));
    assert!(a.accepts("kedge"), "an installed source counts too");
    assert!(a.accepts("Burrow"));
    assert!(!a.accepts("burrow"), "a name keeps its capitals");
    assert!(!a.accepts("tabels"));
    assert!(Accepted::default().is_empty());
    assert!(!Accepted::default().accepts("blyg"));
}

#[test]
fn a_word_is_one_to_64_characters_with_a_letter_and_no_space() {
    assert_eq!(WORD_MAX, 64);
    assert_eq!(word_problem("impyrt"), None);
    assert_eq!(word_problem("Ångström"), None);
    assert_eq!(word_problem("e2e"), None);
    assert_eq!(word_problem(""), Some("is empty"));
    assert_eq!(word_problem("two words"), Some("holds whitespace"));
    assert_eq!(word_problem("1234"), Some("holds no letter"));
    assert_eq!(
        word_problem(&"a".repeat(65)),
        Some("is longer than 64 characters")
    );
    assert_eq!(word_problem(&"é".repeat(64)), None, "characters, not bytes");
}

/// An edit changes the owner's list only: an addition once (sorted without
/// regard to case, as the studio keeps them), a removal of that exact word.
#[test]
fn edits_add_once_and_remove_exactly() {
    let words: Vec<String> = sample().owner_words().to_vec();
    assert_eq!(
        apply(&words, &Edit::Add("anchorage".into())),
        ["anchorage", "blyg", "Burrow", "impyrt"]
    );
    assert_eq!(apply(&words, &Edit::Add("blyg".into())), words, "kept once");
    assert_eq!(
        apply(&words, &Edit::Add(" Cleat ".into())),
        ["blyg", "Burrow", "Cleat", "impyrt"],
        "trimmed"
    );
    assert_eq!(
        apply(&words, &Edit::Remove("Burrow".into())),
        ["blyg", "impyrt"]
    );
    assert_eq!(
        apply(&words, &Edit::Remove("burrow".into())),
        words,
        "only that exact word"
    );
}

#[test]
fn a_put_sends_the_whole_list_and_the_revision_it_read() {
    assert_eq!(
        put_body(&["blyg".to_string()], 7),
        json!({ "words": ["blyg"], "expectedRevision": 7 })
    );
    assert_eq!(OWNER, "owner");
}

/// Reading follows `getDictionary` alone (a read-only token still gets the
/// accepted words); writing follows `putDictionarySource`.
#[test]
fn reading_and_writing_each_follow_their_own_operation() {
    use blyg_core::dictionary::serves_write;
    let read_only = json!({ "operations": ["getDictionary"] });
    assert!(serves(&read_only));
    assert!(!serves_write(&read_only));
    let both = json!({ "operations": ["getDictionary", "putDictionarySource"] });
    assert!(serves(&both));
    assert!(serves_write(&both));
    assert!(!serves(&json!({ "contractVersion": "6.2.0" })));
    assert!(!serves_write(&json!({ "contractVersion": "6.2.0" })));
}

/// What keeps the offline queue, as the outbox keeps a change: anything a
/// later try can mend (offline, 429, 5xx, a list that kept changing), an
/// ended sign-in, and three conflicts in a row. A refusal (4xx) or a word
/// list the blyg can't take drops it, saying why.
#[test]
fn the_queue_outlasts_whatever_a_later_try_can_mend() {
    use blyg_core::CoreError;
    use blyg_core::dictionary::keeps_queue;
    let rejected = |status| CoreError::Rejected {
        status,
        message: "no".into(),
        details: Vec::new(),
    };
    assert!(keeps_queue(&CoreError::Offline));
    assert!(keeps_queue(&CoreError::RateLimited { retry_after: 60 }));
    assert!(keeps_queue(&CoreError::CursorExpired));
    assert!(keeps_queue(&rejected(503)));
    assert!(keeps_queue(&CoreError::Unauthorized));
    assert!(keeps_queue(&CoreError::RevisionConflict));
    assert!(!keeps_queue(&rejected(400)));
    assert!(!keeps_queue(&CoreError::Other("too many words".into())));
}

/// Two Learn Spellings at once, on their own threads, while offline: both
/// words wait (the queue's read-change-write is one step).
#[test]
fn words_learned_at_once_are_all_kept() {
    use blyg_core::dictionary::{PENDING_META, Pending};
    use blyg_core::store::Store;
    use blyg_core::{Backend, LiveBackend, SyncOptions};
    use std::sync::{Arc, Barrier};
    let dir = tempfile::tempdir().unwrap();
    let b = Arc::new(
        LiveBackend::open_with(
            dir.path(),
            "http://127.0.0.1:9",
            "unused-test-credential",
            SyncOptions {
                start_worker: false,
                ..SyncOptions::default()
            },
        )
        .unwrap(),
    );
    let words: Vec<String> = (0..8).map(|i| format!("tideword{i}")).collect();
    let gate = Arc::new(Barrier::new(words.len()));
    let threads: Vec<_> = words
        .iter()
        .cloned()
        .map(|w| {
            let (b, gate) = (b.clone(), gate.clone());
            std::thread::spawn(move || {
                gate.wait();
                let _ = b.edit_dictionary(&Edit::Add(w));
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    let store = Store::open(&dir.path().join("blygger.db")).unwrap();
    let p = Pending::parse(store.meta(PENDING_META).as_deref(), "http://127.0.0.1:9");
    let mut kept: Vec<String> = p.edits().iter().map(Edit::word).collect();
    kept.sort();
    assert_eq!(kept, words);
}

/// Learn Spelling offline: the edit waits in the store's meta
/// (`dictionary.pending`). A later edit of the same word replaces an
/// earlier one, and the queue applies onto whatever the blyg holds when it
/// is sent. Junk in the meta is an empty queue.
#[test]
fn queued_edits_keep_the_last_for_each_word_and_round_trip() {
    use blyg_core::dictionary::Pending;
    let mut p = Pending::parse(None, BLYG_A);
    assert!(p.is_empty());
    p.push(Edit::Add("tidepool".into()));
    p.push(Edit::Add("gunwale".into()));
    p.push(Edit::Remove("tidepool".into()));
    assert_eq!(
        p.edits(),
        &[Edit::Add("gunwale".into()), Edit::Remove("tidepool".into())]
    );
    let back = Pending::parse(Some(&p.to_meta()), BLYG_A);
    assert_eq!(back, p);
    assert_eq!(
        Pending::parse(Some(&p.to_meta()), "https://blyg.example.com/").edits(),
        p.edits(),
        "the same blyg, written another way"
    );
    assert!(
        Pending::parse(Some(&p.to_meta()), BLYG_B).is_empty(),
        "another blyg's words are never its"
    );
    let held = vec!["tidepool".to_string(), "blyg".into()];
    assert_eq!(
        back.apply(&held),
        vec!["blyg".to_string(), "gunwale".into()]
    );
    assert!(Pending::parse(Some("not json"), BLYG_A).is_empty());
    assert!(
        Pending::parse(Some(r#"[{"op":"add","word":"tidepool"}]"#), BLYG_A).is_empty(),
        "a queue that names no blyg is no blyg's"
    );
}

/// A real `LiveBackend` with the sync worker off, pointed at the discard
/// port: a word learned offline is refused as offline but kept on this
/// computer, across a restart, and a read that can't reach the blyg keeps
/// it too. A word the blyg would refuse is never queued.
/// e2e/studio069.rs sends a queued word to a real studio.
#[test]
fn a_word_learned_offline_waits_on_this_computer() {
    use blyg_core::dictionary::{PENDING_META, Pending};
    use blyg_core::store::Store;
    use blyg_core::{Backend, CoreError, LiveBackend, SyncOptions};
    let dir = tempfile::tempdir().unwrap();
    let open = || {
        LiveBackend::open_with(
            dir.path(),
            "http://127.0.0.1:9",
            "unused-test-credential",
            SyncOptions {
                start_worker: false,
                ..SyncOptions::default()
            },
        )
        .unwrap()
    };
    let b = open();
    let r = b.edit_dictionary(&Edit::Add("tidepool".into()));
    assert!(matches!(r, Err(CoreError::Offline)), "{r:?}");
    assert!(b.edit_dictionary(&Edit::Add("two words".into())).is_err());
    assert!(b.dictionary().is_err(), "the read can't reach the blyg");
    drop(b);
    let b = open();
    assert!(b.dictionary().is_err());
    drop(b);
    let store = Store::open(&dir.path().join("blygger.db")).unwrap();
    let p = Pending::parse(store.meta(PENDING_META).as_deref(), "http://127.0.0.1:9");
    assert_eq!(p.edits(), &[Edit::Add("tidepool".into())]);
}

/// A database opened for one blyg and later for another (an install from
/// before the data dir named its blyg, or a changed `blyg-url`): the words
/// queued for the first are never the second's. Its own word waits alone,
/// and the first blyg's queue is gone rather than sent to the wrong one.
#[test]
fn queued_words_belong_to_the_blyg_they_were_learned_for() {
    use blyg_core::Backend;
    use blyg_core::dictionary::{PENDING_META, Pending};
    use blyg_core::store::Store;
    const A: &str = "http://127.0.0.1:9";
    const B: &str = "http://localhost:9";
    let dir = tempfile::tempdir().unwrap();
    let a = offline_at(dir.path(), A);
    let _ = a.edit_dictionary(&Edit::Add("tidepool".into()));
    drop(a);
    let b = offline_at(dir.path(), B);
    let _ = b.edit_dictionary(&Edit::Add("gunwale".into()));
    let store = Store::open(&dir.path().join("blygger.db")).unwrap();
    let meta = store.meta(PENDING_META);
    assert_eq!(
        Pending::parse(meta.as_deref(), B).edits(),
        &[Edit::Add("gunwale".into())],
        "only its own word"
    );
    assert!(Pending::parse(meta.as_deref(), A).is_empty());
}

/// Burrow keeps a record of the words its Learn Spelling added to this
/// computer's spelling list, per blyg. A read of the dictionary unlearns
/// those that have left the owner's words (taken out in Studio, say), and
/// nothing else: a word this computer learned some other way is never
/// Burrow's to take out.
#[test]
fn only_words_burrow_taught_and_that_left_the_blyg_are_unlearned() {
    use blyg_core::Backend;
    use blyg_core::dictionary::{Pending, left};
    let d = sample();
    let none = Pending::parse(None, BLYG_A);
    let taught = vec!["impyrt".to_string(), "harbourside".into()];
    assert_eq!(
        left(&taught, &d, &none),
        ["harbourside"],
        "still in the blyg's: kept"
    );
    assert!(
        left(&[], &d, &none).is_empty(),
        "nothing taught, nothing taken out"
    );
    assert_eq!(
        left(&["kedge".to_string()], &d, &none),
        ["kedge"],
        "an installed set isn't the owner's words"
    );

    let dir = tempfile::tempdir().unwrap();
    let b = offline_at(dir.path(), BLYG_A);
    assert!(b.dictionary_taught().unwrap().is_empty());
    assert!(b.dictionary_pending().unwrap().is_empty());
    b.dictionary_teach("harbourside", true).unwrap();
    b.dictionary_teach("impyrt", true).unwrap();
    b.dictionary_teach("impyrt", true).unwrap();
    let mut t = b.dictionary_taught().unwrap();
    t.sort();
    assert_eq!(t, ["harbourside", "impyrt"], "kept once");
    b.dictionary_teach("harbourside", false).unwrap();
    assert_eq!(b.dictionary_taught().unwrap(), ["impyrt"]);
    drop(b);
    assert_eq!(
        offline_at(dir.path(), BLYG_A).dictionary_taught().unwrap(),
        ["impyrt"],
        "kept across a restart"
    );
    assert!(
        offline_at(dir.path(), BLYG_B)
            .dictionary_taught()
            .unwrap()
            .is_empty(),
        "another blyg's record is never this one's"
    );
}

/// A taught word whose send was kept (offline, a 429, conflicts that kept
/// coming) isn't in the blyg's words yet, but it hasn't left them: it's
/// still on its way, so a read in between doesn't unlearn it. A taught
/// word waiting to be taken out has left, as far as this computer goes.
#[test]
fn a_taught_word_still_on_its_way_is_not_unlearned() {
    use blyg_core::Backend;
    use blyg_core::dictionary::{Pending, left};
    let d = sample();
    let mut queued = Pending::parse(None, BLYG_A);
    queued.push(Edit::Add("harbourside".into()));
    queued.push(Edit::Remove("impyrt".into()));
    let taught = vec!["harbourside".to_string(), "impyrt".into()];
    assert_eq!(
        left(&taught, &d, &queued),
        ["impyrt"],
        "the add waits; the removal counts as gone"
    );

    // The backend hands out this blyg's queue.
    let dir = tempfile::tempdir().unwrap();
    let b = offline_at(dir.path(), "http://127.0.0.1:9");
    let _ = b.edit_dictionary(&Edit::Add("harbourside".into()));
    assert_eq!(
        b.dictionary_pending().unwrap().edits(),
        &[Edit::Add("harbourside".into())]
    );
}
