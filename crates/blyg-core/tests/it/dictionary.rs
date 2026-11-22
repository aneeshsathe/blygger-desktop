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

fn sample() -> Dictionary {
    parse(&json!({
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

#[test]
fn offered_only_when_the_capabilities_list_both_operations() {
    assert!(serves(
        &json!({ "operations": ["getDictionary", "putDictionarySource"] })
    ));
    assert!(!serves(&json!({ "operations": ["getDictionary"] })));
    assert!(!serves(&json!({ "contractVersion": "6.2.0" })));
}

/// Learn Spelling offline: the edit waits in the store's meta
/// (`dictionary.pending`). A later edit of the same word replaces an
/// earlier one, and the queue applies onto whatever the blyg holds when it
/// is sent. Junk in the meta is an empty queue.
#[test]
fn queued_edits_keep_the_last_for_each_word_and_round_trip() {
    use blyg_core::dictionary::Pending;
    let mut p = Pending::parse(None);
    assert!(p.is_empty());
    p.push(Edit::Add("tidepool".into()));
    p.push(Edit::Add("gunwale".into()));
    p.push(Edit::Remove("tidepool".into()));
    assert_eq!(
        p.edits(),
        &[Edit::Add("gunwale".into()), Edit::Remove("tidepool".into())]
    );
    let back = Pending::parse(Some(&p.to_meta()));
    assert_eq!(back, p);
    let held = vec!["tidepool".to_string(), "blyg".into()];
    assert_eq!(back.apply(&held), vec!["blyg".to_string(), "gunwale".into()]);
    assert!(Pending::parse(Some("not json")).is_empty());
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
    let p = Pending::parse(store.meta(PENDING_META).as_deref());
    assert_eq!(p.edits(), &[Edit::Add("tidepool".into())]);
}
