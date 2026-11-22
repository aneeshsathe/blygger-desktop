//! --- dictionary --- The blyg's dictionary (blygger-studio 0.69.0, contract
//! 6.2.0, its #156): the words its spellchecking accepts, shared by every
//! client and device. Studio-private.
//!
//! - `GET /api/dictionary` → `{sources: [{source, words}], revision}`:
//!   [`OWNER`] (the author's own words) first, then sets an extension or
//!   client installed, by name; each source's words sorted without regard
//!   to case.
//! - `PUT /api/dictionary/{source} {words, expectedRevision}` replaces one
//!   source's whole list (an empty list removes it). A stale revision is
//!   409 `revision-conflict`; a word that breaks the rules
//!   ([`word_problem`]) is a 400 naming it.
//! - Read when `GET /api/capabilities` lists [`GET_OPERATION`] ([`serves`]),
//!   written when it lists [`PUT_OPERATION`] ([`serves_write`]).
//!
//! Burrow's spellcheck accepts every source's words ([`Accepted`]), and
//! "Learn Spelling" adds to [`OWNER`] as well as to the system's list, as
//! Studio's "add to dictionary" does. Blyg › Dictionary… lists and edits
//! the owner's words. Edits read the dictionary, change the owner's list
//! ([`apply`]) and write it back from that revision, so another client's
//! word is never lost.

use std::collections::HashSet;

use serde_json::{Value, json};

/// The capabilities' operation names.
pub const GET_OPERATION: &str = "getDictionary";
pub const PUT_OPERATION: &str = "putDictionarySource";

/// The source holding the author's own words.
pub const OWNER: &str = "owner";
/// A word's most characters (studio's `WORD_MAX`).
pub const WORD_MAX: usize = 64;
/// A source's most words (studio's `SOURCE_WORDS_MAX`).
pub const SOURCE_WORDS_MAX: usize = 20_000;

/// Whether a capabilities answer offers the dictionary to read (a
/// read-only token still gets the accepted words).
pub fn serves(capabilities: &Value) -> bool {
    crate::api::offers(capabilities, GET_OPERATION)
}

/// Whether it offers changing a source's words.
pub fn serves_write(capabilities: &Value) -> bool {
    crate::api::offers(capabilities, PUT_OPERATION)
}

/// Whether a failed send keeps the queued edits ([`Pending`]), as the
/// outbox keeps a change: whatever a later try can mend (offline, 429,
/// 5xx, a list that kept changing), an ended sign-in (the next sign-in
/// sends them), and conflicts that kept coming. A refusal drops them; its
/// error says why.
pub fn keeps_queue(e: &crate::backend::CoreError) -> bool {
    use crate::backend::CoreError;
    crate::api::is_transient(e)
        || matches!(e, CoreError::Unauthorized | CoreError::RevisionConflict)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub source: String,
    pub words: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Dictionary {
    pub sources: Vec<Source>,
    pub revision: u64,
    /// Whether this sign-in may change it (the capabilities list
    /// [`PUT_OPERATION`]). A read-only sign-in's Learn Spelling stays on
    /// this computer: nothing is sent or queued.
    pub writable: bool,
}

impl Dictionary {
    /// The author's own words.
    pub fn owner_words(&self) -> &[String] {
        self.sources
            .iter()
            .find(|s| s.source == OWNER)
            .map(|s| s.words.as_slice())
            .unwrap_or(&[])
    }

    /// The words in every source.
    pub fn word_count(&self) -> usize {
        self.sources.iter().map(|s| s.words.len()).sum()
    }
}

/// `GET /api/dictionary`'s answer (or a `PUT`'s). `None` when it isn't one.
pub fn parse(v: &Value) -> Option<Dictionary> {
    let revision = v.get("revision")?.as_u64()?;
    let sources = v
        .get("sources")?
        .as_array()?
        .iter()
        .filter_map(|s| {
            Some(Source {
                source: s.get("source")?.as_str()?.to_string(),
                words: s
                    .get("words")?
                    .as_array()?
                    .iter()
                    .filter_map(|w| w.as_str().map(str::to_string))
                    .collect(),
            })
        })
        .collect();
    // The answer doesn't say; `Api::dictionary` sets it from the
    // capabilities.
    Some(Dictionary {
        sources,
        revision,
        writable: true,
    })
}

/// Why a word can't go in the dictionary, or `None` when it can (studio's
/// `wordProblem`, word for word).
pub fn word_problem(word: &str) -> Option<&'static str> {
    if word.is_empty() {
        return Some("is empty");
    }
    if word.chars().count() > WORD_MAX {
        return Some("is longer than 64 characters");
    }
    if word.chars().any(char::is_whitespace) {
        return Some("holds whitespace");
    }
    if !word.chars().any(char::is_alphabetic) {
        return Some("holds no letter");
    }
    None
}

/// A change to the owner's words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    Add(String),
    Remove(String),
}

impl Edit {
    /// The word, trimmed and in NFC (as the studio keeps it).
    pub fn word(&self) -> String {
        let (Edit::Add(w) | Edit::Remove(w)) = self;
        crate::text::nfc(w.trim()).into_owned()
    }
}

/// The owner's list after `edit`: an addition kept once, a removal of that
/// exact word; sorted without regard to case, then as written (the
/// studio's order).
pub fn apply(words: &[String], edit: &Edit) -> Vec<String> {
    let w = edit.word();
    sorted(match edit {
        Edit::Add(_) if words.contains(&w) => words.to_vec(),
        Edit::Add(_) => words.iter().cloned().chain([w]).collect(),
        Edit::Remove(_) => words.iter().filter(|x| **x != w).cloned().collect(),
    })
}

/// The studio's order: without regard to case, then as written.
fn sorted(mut words: Vec<String>) -> Vec<String> {
    words.sort_by(|a, b| {
        a.to_lowercase()
            .cmp(&b.to_lowercase())
            .then_with(|| a.cmp(b))
    });
    words
}

/// The store meta key holding edits made while the blyg couldn't be
/// reached ([`Pending`], JSON).
pub const PENDING_META: &str = "dictionary.pending";

/// A meta value kept for one blyg: `{blyg, items}`. A database is one
/// blyg's (`config::blygs::data_dir_for`), but one opened for another blyg
/// (an install from before the data dir named its blyg, a changed
/// `blyg-url`) must never hand the first blyg's words to the second.
fn keyed_items(meta: Option<&str>, blyg: &str) -> Vec<Value> {
    let Some(v) = meta.and_then(|m| serde_json::from_str::<Value>(m).ok()) else {
        return Vec::new();
    };
    match v.get("blyg").and_then(Value::as_str) {
        Some(b) if crate::config::blygs::same(b, blyg) => v
            .get("items")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        // Another blyg's, or naming none: not this one's.
        _ => Vec::new(),
    }
}

fn keyed_meta(blyg: &str, items: Vec<Value>) -> String {
    json!({ "blyg": blyg, "items": items }).to_string()
}

/// Edits waiting for the blyg: Learn Spelling (or the Dictionary sheet)
/// offline. Kept in the store's meta ([`PENDING_META`]) for the blyg they
/// were made for, and sent, applied onto what it holds then, by the next
/// read of its dictionary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pending {
    blyg: String,
    edits: Vec<Edit>,
}

impl Pending {
    /// `blyg`'s queue from the meta's value; missing, unreadable or
    /// another blyg's is an empty queue.
    pub fn parse(meta: Option<&str>, blyg: &str) -> Pending {
        let edit = |e: &Value| -> Option<Edit> {
            let w = e.get("word")?.as_str()?.to_string();
            match e.get("op")?.as_str()? {
                "add" => Some(Edit::Add(w)),
                "remove" => Some(Edit::Remove(w)),
                _ => None,
            }
        };
        Pending {
            blyg: blyg.to_string(),
            edits: keyed_items(meta, blyg).iter().filter_map(edit).collect(),
        }
    }

    /// The meta's value.
    pub fn to_meta(&self) -> String {
        let items = self
            .edits
            .iter()
            .map(|e| {
                let op = if matches!(e, Edit::Add(_)) {
                    "add"
                } else {
                    "remove"
                };
                json!({ "op": op, "word": e.word() })
            })
            .collect();
        keyed_meta(&self.blyg, items)
    }

    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    pub fn edits(&self) -> &[Edit] {
        &self.edits
    }

    /// One more; it replaces an earlier edit of the same word.
    pub fn push(&mut self, edit: Edit) {
        let w = edit.word();
        self.edits.retain(|e| e.word() != w);
        self.edits.push(edit);
    }

    /// `words` with every queued edit applied, in order.
    pub fn apply(&self, words: &[String]) -> Vec<String> {
        self.edits
            .iter()
            .fold(sorted(words.to_vec()), |out, e| apply(&out, e))
    }

    /// The words it adds.
    pub fn added(&self) -> impl Iterator<Item = String> + '_ {
        self.edits
            .iter()
            .filter(|e| matches!(e, Edit::Add(_)))
            .map(Edit::word)
    }
}

/// The store meta key recording the words Burrow's Learn Spelling added
/// to this computer's spelling list ([`Taught`], JSON), per blyg.
pub const TAUGHT_META: &str = "dictionary.taught";

/// The words Burrow's Learn Spelling taught this computer's spelling list
/// for a blyg. Only these are ever unlearned here (a word that left the
/// owner's words: [`left`]); a word this computer learned any other way is
/// never Burrow's to take out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Taught {
    blyg: String,
    words: Vec<String>,
}

impl Taught {
    /// `blyg`'s record from the meta's value (another blyg's is empty).
    pub fn parse(meta: Option<&str>, blyg: &str) -> Taught {
        Taught {
            blyg: blyg.to_string(),
            words: keyed_items(meta, blyg)
                .iter()
                .filter_map(|w| w.as_str().map(str::to_string))
                .collect(),
        }
    }

    pub fn to_meta(&self) -> String {
        keyed_meta(
            &self.blyg,
            self.words.iter().map(|w| Value::from(w.as_str())).collect(),
        )
    }

    pub fn words(&self) -> &[String] {
        &self.words
    }

    /// Record `word` as taught (`true`, once) or forget it.
    pub fn set(&mut self, word: &str, taught: bool) {
        let w = crate::text::nfc(word.trim()).into_owned();
        self.words.retain(|x| *x != w);
        if taught {
            self.words.push(w);
        }
    }
}

/// The taught words that have left the owner's words: the ones to unlearn
/// on this computer (and forget).
pub fn left(taught: &[String], d: &Dictionary) -> Vec<String> {
    let owner: HashSet<&str> = d.owner_words().iter().map(String::as_str).collect();
    taught
        .iter()
        .filter(|w| !owner.contains(w.as_str()))
        .cloned()
        .collect()
}

/// The `PUT /api/dictionary/{source}` body.
pub fn put_body(words: &[String], expected_revision: u64) -> Value {
    json!({ "words": words, "expectedRevision": expected_revision })
}

/// The words a spellcheck accepts: every source's. A word kept in lower
/// case accepts any capitals ("Blyg" at a sentence's start); a word kept
/// with capitals (a name) accepts only those, as written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Accepted {
    exact: HashSet<String>,
    any_case: HashSet<String>,
}

impl Accepted {
    pub fn is_empty(&self) -> bool {
        self.exact.is_empty() && self.any_case.is_empty()
    }

    pub fn accepts(&self, word: &str) -> bool {
        self.exact.contains(word) || self.any_case.contains(&word.to_lowercase())
    }

    /// One more word (a word just learned, before the blyg answers).
    pub fn add(&mut self, word: &str) {
        if word.chars().any(char::is_uppercase) {
            self.exact.insert(word.to_string());
        } else {
            self.any_case.insert(word.to_string());
        }
    }
}

impl From<&Dictionary> for Accepted {
    fn from(d: &Dictionary) -> Self {
        let mut a = Accepted::default();
        for w in d.sources.iter().flat_map(|s| &s.words) {
            a.add(w);
        }
        a
    }
}
