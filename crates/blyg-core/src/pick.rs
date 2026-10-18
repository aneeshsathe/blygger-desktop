//! The `[[` / `![[` picker's search: studio 0.29's `GET /api/search`,
//! answered locally. It offers what publish will accept as a link or quote
//! target: your own published posts and posts imported from blyg
//! subscriptions (current, or a withdrawn one whose pinned version is
//! retained). Every word of the query must appear somewhere in a post's
//! text or id, in any order, case-insensitively.
//!
//! [`search_in`] is the in-memory version (the `Backend` default); the
//! live backend answers from SQLite's FTS5 indexes and checks each hit with
//! the same [`matches`], so both agree on what matches.

use serde::{Deserialize, Serialize};

use crate::grammar;
use crate::model::{
    Item, Kind, ReadingItem, Status, Subscription, SubscriptionKind, plain_text, strip_tk,
};

/// At most this many words of a query count (upstream's limit).
pub const MAX_WORDS: usize = 8;
/// An excerpt's length in characters (upstream's `clampText(…, 140)`).
pub const EXCERPT_CHARS: usize = 140;
/// How many rows a search returns unless the query says otherwise.
pub const DEFAULT_LIMIT: usize = 50;

/// Which posts the picker searches (`source=all|mine|imported`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PickSource {
    #[default]
    All,
    Mine,
    Imported,
}

impl PickSource {
    pub fn as_str(self) -> &'static str {
        match self {
            PickSource::All => "all",
            PickSource::Mine => "mine",
            PickSource::Imported => "imported",
        }
    }
}

/// `sort=newest|oldest`, by the post's date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PickSort {
    #[default]
    Newest,
    Oldest,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PickQuery {
    pub text: String,
    pub source: PickSource,
    /// Only posts imported through this subscription (implies imported).
    pub sub: Option<String>,
    pub sort: PickSort,
    /// The server id of the post being written (it can't name itself).
    pub exclude: Option<String>,
    /// 0 = [`DEFAULT_LIMIT`].
    pub limit: usize,
}

impl PickQuery {
    pub fn limit(&self) -> usize {
        if self.limit == 0 {
            DEFAULT_LIMIT
        } else {
            self.limit
        }
    }

    /// Whether own posts are searched (not with `imported` or a `sub`).
    pub fn wants_mine(&self) -> bool {
        self.source != PickSource::Imported && self.sub.is_none()
    }

    pub fn wants_imported(&self) -> bool {
        self.source != PickSource::Mine
    }
}

/// One row the picker offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pickable {
    /// What goes between the brackets.
    pub id: String,
    /// The post's title (its first line of its own words).
    pub title: String,
    /// Plain text, at most [`EXCERPT_CHARS`].
    pub excerpt: String,
    /// What the picker writes as the label ([`pick_label`]): the text a bare
    /// `[[id]]` to this post shows (studio 0.47's search-hit `label`, see
    /// [`link_label`]), except that a post with no heading is labelled with
    /// its title rather than a quoted excerpt.
    pub label: String,
    /// `Mine` or `Imported` (never `All`).
    pub source: PickSource,
    pub kind: Kind,
    /// The version a quote would take.
    pub version: u32,
    /// The post's date (ISO-8601), what `sort` orders by.
    pub at: String,
    pub subscription_id: Option<String>,
    /// The subscription's name, else the origin's host; `None` for yours.
    pub source_title: Option<String>,
}

/// The query's words, lowercased (at most [`MAX_WORDS`]).
pub fn words(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .take(MAX_WORDS)
        .map(str::to_lowercase)
        .collect()
}

/// Every word appears in `text` or in `id` (words are lowercased already).
pub fn matches(words: &[String], text: &str, id: &str) -> bool {
    if words.is_empty() {
        return true;
    }
    let text = text.to_lowercase();
    let id = id.to_lowercase();
    words
        .iter()
        .all(|w| text.contains(w.as_str()) || id.contains(w.as_str()))
}

/// The text a `[[id]]` link to a post shows when its author wrote no label
/// (studio 0.47 `link-label.ts`): the post's opening heading as plain text,
/// else a short excerpt in curly quotes, else "a thread" / "a fragment".
pub fn link_label(content_html: &str, kind: Kind) -> String {
    let kind = match kind {
        Kind::Thread => blyg_render::ItemKind::Thread,
        Kind::Fragment => blyg_render::ItemKind::Fragment,
    };
    blyg_render::link_label(content_html, kind)
}

/// [`link_label`] for a post known only by its Markdown (your own, or an
/// imported one without HTML). An approximation of its published HTML:
/// TK reduced to output, quote directives left out (they bake to a quote of
/// someone else's words, which isn't what this post says), links shown as
/// their labels.
pub fn link_label_md(md: &str, kind: Kind) -> String {
    let md = strip_tk(md);
    let mut kept = String::with_capacity(md.len());
    for line in md.split('\n') {
        if grammar::directive_ref(line).is_some() {
            continue;
        }
        kept.push_str(&grammar::replace_links(line, |r| {
            Some(r.label.clone().unwrap_or_else(|| "↗".into()))
        }));
        kept.push('\n');
    }
    link_label(&blyg_render::render_markdown(&kept), kind)
}

/// The longest title [`pick_label`] writes as a label (the studio's cap on
/// a heading label, `anchorText`'s 80).
pub const TITLE_LABEL_CHARS: usize = 80;

/// What the picker writes as a pick's label, from the post's `title` and
/// the text a bare `[[id]]` to it `shows` ([`link_label`]).
///
/// A deliberate difference from the studio (docs/SPEC.md rule 3), whose
/// `paletteInsert` writes the search hit's label as it is: when the post
/// has no heading, that label is its first 60 characters in quotes, the
/// title and the body run together ("“Gardens, not streams A note on…”").
/// Burrow writes the title instead (`[[id | Gardens, not streams]]`), which
/// is what a reader of the link wants to see. A heading's label is kept, and
/// so is the quoted excerpt when the title is too long to be one (a post
/// that is a single paragraph) or there's no text at all ("a thread").
pub fn pick_label(title: Option<&str>, shows: String) -> String {
    let excerpt = shows.starts_with('“') && shows.ends_with('”');
    match title.map(str::trim) {
        Some(t) if excerpt && !t.is_empty() && t.chars().count() <= TITLE_LABEL_CHARS => {
            t.to_string()
        }
        _ => shows,
    }
}

/// The first [`EXCERPT_CHARS`] characters of the plain text, `…` when cut.
pub fn excerpt(md: &str) -> String {
    let t = plain_text(md);
    if t.chars().count() <= EXCERPT_CHARS {
        return t;
    }
    let cut: String = t.chars().take(EXCERPT_CHARS - 1).collect();
    format!("{}…", cut.trim_end())
}

fn host(origin: &str) -> String {
    url::Url::parse(origin)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_else(|| origin.trim_end_matches('/').to_string())
}

/// A row for your own published post, if it's one the query can name.
pub fn mine(it: &Item, q: &PickQuery, words: &[String]) -> Option<Pickable> {
    let sid = it.server_id.as_ref()?;
    if it.status != Status::Public || q.exclude.as_deref() == Some(sid.0.as_str()) {
        return None;
    }
    if !matches(words, &it.content_md, &sid.0) {
        return None;
    }
    Some(Pickable {
        id: sid.0.clone(),
        title: it.title(),
        excerpt: excerpt(&it.content_md),
        label: pick_label(
            crate::model::plain_title(&it.content_md).as_deref(),
            link_label_md(&it.content_md, it.kind),
        ),
        source: PickSource::Mine,
        kind: it.kind,
        version: it.version,
        at: it.updated.clone(),
        subscription_id: None,
        source_title: None,
    })
}

/// A row for an imported post (`blyg` = it came from a blyg subscription).
pub fn imported(r: &ReadingItem, blyg: bool, q: &PickQuery, words: &[String]) -> Option<Pickable> {
    let version = if r.state == "current" {
        r.version
    } else {
        r.pinned_version_retained?
    };
    if !blyg || q.exclude.as_deref() == Some(r.remote_id.as_str()) {
        return None;
    }
    if q.sub.as_deref().is_some_and(|s| s != r.subscription_id) {
        return None;
    }
    if !matches(words, &r.content_md, &r.remote_id) {
        return None;
    }
    let title = r.subscription_title.trim();
    Some(Pickable {
        id: r.remote_id.clone(),
        title: crate::model::plain_title(&r.content_md).unwrap_or_else(|| "…".into()),
        excerpt: excerpt(&r.content_md),
        label: pick_label(
            crate::model::plain_title(&r.content_md).as_deref(),
            if r.content_html.trim().is_empty() {
                link_label_md(&r.content_md, r.kind)
            } else {
                link_label(&r.content_html, r.kind)
            },
        ),
        source: PickSource::Imported,
        kind: r.kind,
        version,
        at: r.sort_at(),
        subscription_id: Some(r.subscription_id.clone()),
        source_title: Some(if title.is_empty() {
            host(&r.origin)
        } else {
            title.to_string()
        }),
    })
}

/// Sort by date (then id) and keep the first `limit`, one row per id.
pub fn finish(mut rows: Vec<Pickable>, q: &PickQuery) -> Vec<Pickable> {
    let mut seen = std::collections::HashSet::new();
    rows.retain(|r| seen.insert(r.id.clone()));
    rows.sort_by(|a, b| {
        let o = crate::util::parse_ts_ms(&a.at)
            .cmp(&crate::util::parse_ts_ms(&b.at))
            .then_with(|| a.id.cmp(&b.id));
        match q.sort {
            PickSort::Newest => o.reverse(),
            PickSort::Oldest => o,
        }
    });
    rows.truncate(q.limit());
    rows
}

/// The search over what's in memory (the `Backend` default).
pub fn search_in(
    items: &[Item],
    reading: &[ReadingItem],
    subs: &[Subscription],
    q: &PickQuery,
) -> Vec<Pickable> {
    let w = words(&q.text);
    let mut rows = Vec::new();
    if q.wants_mine() {
        rows.extend(items.iter().filter_map(|it| mine(it, q, &w)));
    }
    if q.wants_imported() {
        let blyg = |id: &str| {
            subs.iter()
                .any(|s| s.id == id && s.kind == SubscriptionKind::Blyg)
        };
        rows.extend(
            reading
                .iter()
                .filter_map(|r| imported(r, blyg(&r.subscription_id), q, &w)),
        );
    }
    finish(rows, q)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_word_anywhere_in_text_or_id() {
        let w = words("  Garden  BEDS ");
        assert_eq!(w, vec!["garden", "beds"]);
        assert!(matches(&w, "The beds in the garden", "x"));
        assert!(!matches(&w, "The garden", "x"));
        assert!(matches(&words("01ARZ beds"), "beds", "01arzabc"));
        assert!(matches(&words("ÉTÉ"), "un été", "x"));
        assert_eq!(words("a b c d e f g h i j").len(), MAX_WORDS);
    }

    #[test]
    fn excerpts_are_plain_and_clamped() {
        assert_eq!(
            excerpt("# Title\n\n*Some* [text](https://x.example)"),
            "Title Some text"
        );
        let long = "word ".repeat(60);
        let e = excerpt(&long);
        assert!(e.chars().count() <= EXCERPT_CHARS && e.ends_with('…'));
    }

    // test/labels.test.ts "search hits carry the label a link would show".
    #[test]
    fn labels_are_the_heading_else_a_quoted_excerpt() {
        assert_eq!(
            link_label_md("# Labeltoken heading\n\nBody text.", Kind::Fragment),
            "Labeltoken heading"
        );
        assert_eq!(
            link_label_md("labeltoken with no heading at all", Kind::Fragment),
            "“labeltoken with no heading at all”"
        );
        assert_eq!(link_label_md("", Kind::Thread), "a thread");
        assert_eq!(link_label("", Kind::Fragment), "a fragment");
        assert_eq!(
            link_label("<h2>Their <em>title</em></h2><p>Body</p>", Kind::Thread),
            "Their title"
        );
        let id = "0a1b2c3d4e5f6g7h8j9k0m1n2p";
        assert_eq!(
            link_label_md(
                &format!("![[{id} | a note]]\n\nMy reply, after [[{id} | that post]]."),
                Kind::Thread
            ),
            "“My reply, after that post.”"
        );
        assert_eq!(
            link_label_md("[TK]an opener[=]Generated words[/TK]", Kind::Fragment),
            "“Generated words”"
        );
    }

    /// Burrow's difference from the studio: no heading, so the title.
    #[test]
    fn a_pick_is_labelled_with_the_title_when_there_is_no_heading() {
        let label = |md: &str| {
            pick_label(
                crate::model::plain_title(md).as_deref(),
                link_label_md(md, Kind::Thread),
            )
        };
        assert_eq!(
            label("Gardens, not streams\n\nA note on how I write in public."),
            "Gardens, not streams"
        );
        assert_eq!(label("# A heading\n\nBody."), "A heading");
        assert_eq!(label(""), "a thread");
        let para = "word ".repeat(30);
        assert!(label(&para).starts_with('“'), "too long to be a title");
    }
}
