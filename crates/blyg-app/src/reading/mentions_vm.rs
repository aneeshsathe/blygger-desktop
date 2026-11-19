//! --- contacts --- Why a post is in the Mentions smart feed. Pure: no UI,
//! no store, no network.
//!
//! The feed is both sources at once: the blyg's lens (`GET
//! /api/reading?mentions=true`: the post's own h-cards name you) and what
//! this Mac works out from the held posts (a quote, stub or fork of one of
//! your posts, or a link to your blyg). Each row says why with a tag or
//! two; never a count.

use std::collections::HashMap;

use blyg_core::ReadingItem;
use blyg_core::profile::Relation;

use super::sources_vm::is_background;
use super::vm::{Key, key, relation_label};

/// Why a held post mentions you, in the order the tags show (the lens
/// first, then the strongest relation: fork > stub > quote > a link).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Why {
    /// The blyg's lens: the post's own h-cards name you (an @-mention).
    Named,
    Forked,
    Replied,
    Quoted,
    Linked,
}

impl Why {
    /// The row's tag. The relation words are the lineage's
    /// (`vm::relation_label`), said of you.
    pub fn label(self) -> &'static str {
        match self {
            Why::Named => "named you",
            Why::Forked => "forked you",
            Why::Replied => relation_label(Some("stub")),
            Why::Quoted => "quoted you",
            Why::Linked => "linked you",
        }
    }
}

/// The tags for a row: "named you" when the lens holds it, then the local
/// reason. At most two.
pub fn tags(named: bool, local: Option<Why>) -> Vec<&'static str> {
    named
        .then_some(Why::Named)
        .into_iter()
        .chain(local)
        .map(Why::label)
        .collect()
}

/// Why `r` points at you (`own`: your normalized origin; `needle`:
/// `://your host`), the strongest reason; `None` when it doesn't.
fn why(r: &ReadingItem, own: &str, needle: &str) -> Option<Why> {
    let rel = r
        .references()
        .into_iter()
        .filter(|p| p.origin == own)
        .map(|p| match p.relation {
            Relation::Forks => Why::Forked,
            Relation::Stubs => Why::Replied,
            Relation::Quotes => Why::Quoted,
        })
        .min();
    rel.or_else(|| {
        (links_to(&r.content_md, needle)
            || (r.content_md.is_empty() && links_to(&r.content_html, needle)))
        .then_some(Why::Linked)
    })
}

/// The held posts (not yours, not Background) that point at `own_origin`,
/// each with why: a quote, stub or fork of one of your posts, or a link to
/// your blyg's address.
pub fn local_mentions(rows: &[ReadingItem], own_origin: &str) -> HashMap<Key, Why> {
    let Some(own) = blyg_core::profile::normalize_origin(own_origin) else {
        return HashMap::new();
    };
    let host = crate::vm::url_host(&own).unwrap_or_default();
    if host.is_empty() {
        return HashMap::new();
    }
    let needle = format!("://{host}");
    rows.iter()
        .filter(|r| !super::own::is_own_row(r) && !is_background(r))
        .filter_map(|r| why(r, &own, &needle).map(|w| (key(r), w)))
        .collect()
}

/// `text` holds `://host` ending where a host ends (not `://host.other`).
fn links_to(text: &str, needle: &str) -> bool {
    text.match_indices(needle).any(|(i, _)| {
        text[i + needle.len()..]
            .chars()
            .next()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '.' || c == '-'))
    })
}

#[cfg(test)]
mod tests {
    use super::super::sources_vm::is_background;
    use super::super::vm::key;
    use super::{Why, local_mentions, tags};
    use crate::fake::reading_seed;

    #[test]
    fn the_tags_name_the_lens_first_and_use_the_lineage_words() {
        assert_eq!(tags(true, Some(Why::Quoted)), ["named you", "quoted you"]);
        assert_eq!(tags(false, Some(Why::Replied)), ["replied"]);
        assert_eq!(tags(true, None), ["named you"]);
        assert!(tags(false, None).is_empty());
        for (w, rel) in [
            (Why::Forked, "fork"),
            (Why::Quoted, "transclusion"),
            (Why::Replied, "stub"),
        ] {
            let word = super::super::vm::relation_label(Some(rel));
            assert!(w.label().starts_with(word), "{w:?}");
        }
    }

    #[test]
    fn a_link_a_quote_and_neither() {
        let now = chrono::Utc::now();
        let mut rows = reading_seed::seed(now).reading;
        rows.retain(|r| !super::super::own::is_own_row(r) && !is_background(r));
        for r in rows.iter_mut() {
            r.content_md.clear();
            r.content_html.clear();
            r.transclusions.clear();
            r.stub_of = None;
            r.forked_from = None;
        }
        rows[0].content_md = "As [Sam](https://blyg.example.com/t/x/) wrote".into();
        rows[1].content_md = "see https://blyg.example.community/".into();
        let got = local_mentions(&rows, "https://blyg.example.com/");
        assert_eq!(got.get(&key(&rows[0])), Some(&Why::Linked));
        // A host that merely starts the same isn't you.
        assert!(!got.contains_key(&key(&rows[1])));
        assert_eq!(got.len(), 1);
    }
}
