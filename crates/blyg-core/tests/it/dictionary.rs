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
