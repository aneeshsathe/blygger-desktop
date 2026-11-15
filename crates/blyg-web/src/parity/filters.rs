//! --- web-parity --- The Posts list's status filters, as the studio's
//! compose page has them (all, drafts, unpublished changes, public,
//! withdrawn, each with its count, `GET /api/items/summary`'s), worked out
//! from the posts this browser holds: no request, and they work offline
//! and in the demo. As in the studio, a search looks through every post
//! whatever the filter, so ⏎ never starts a duplicate of a post the filter
//! hides.

use blyg_core::{Item, Status};
use serde::Serialize;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::{Burrow, json, vm};

/// Which of your posts the Posts list shows.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum PostFilter {
    #[default]
    All,
    /// Never published (version 0): drafts, and Burrow's scratch notes.
    Drafts,
    /// Published, with edits not published yet.
    Changes,
    Public,
    Withdrawn,
}

impl PostFilter {
    pub const EVERY: [PostFilter; 5] = [
        PostFilter::All,
        PostFilter::Drafts,
        PostFilter::Changes,
        PostFilter::Public,
        PostFilter::Withdrawn,
    ];

    pub fn key(self) -> &'static str {
        match self {
            PostFilter::All => "all",
            PostFilter::Drafts => "drafts",
            PostFilter::Changes => "changes",
            PostFilter::Public => "public",
            PostFilter::Withdrawn => "withdrawn",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PostFilter::All => "All",
            PostFilter::Drafts => "Drafts",
            PostFilter::Changes => "Changes",
            PostFilter::Public => "Public",
            PostFilter::Withdrawn => "Withdrawn",
        }
    }

    /// The chip's tooltip.
    pub fn tip(self) -> &'static str {
        match self {
            PostFilter::All => "Every post",
            PostFilter::Drafts => "Never published: drafts and notes",
            PostFilter::Changes => "Published, with unpublished changes",
            PostFilter::Public => "Published and live",
            PostFilter::Withdrawn => "Taken back from your blyg",
        }
    }

    pub fn parse(key: &str) -> Option<PostFilter> {
        PostFilter::EVERY.into_iter().find(|f| f.key() == key)
    }

    /// The next filter, for the key that steps through them.
    pub fn next(self) -> PostFilter {
        let i = PostFilter::EVERY
            .iter()
            .position(|f| *f == self)
            .unwrap_or(0);
        PostFilter::EVERY[(i + 1) % PostFilter::EVERY.len()]
    }

    /// The studio's rules: drafts `version = 0`; changes `dirty` and
    /// public; public and withdrawn by status.
    pub fn keeps(self, it: &Item) -> bool {
        match self {
            PostFilter::All => true,
            PostFilter::Drafts => it.version == 0,
            PostFilter::Changes => vm::has_unpublished_edits(it),
            PostFilter::Public => it.status == Status::Public,
            PostFilter::Withdrawn => it.status == Status::Withdrawn,
        }
    }

    /// What the list says when the filter leaves nothing.
    pub fn empty(self) -> &'static str {
        match self {
            PostFilter::All => "No posts yet.",
            PostFilter::Drafts => "No drafts.",
            PostFilter::Changes => "No published post has unpublished changes.",
            PostFilter::Public => "Nothing published yet.",
            PostFilter::Withdrawn => "Nothing withdrawn.",
        }
    }
}

/// Each filter's count over `items`.
pub fn filter_counts(items: &[Item]) -> Vec<(PostFilter, usize)> {
    PostFilter::EVERY
        .into_iter()
        .map(|f| (f, items.iter().filter(|i| f.keeps(i)).count()))
        .collect()
}

#[derive(Serialize)]
struct Chip {
    key: &'static str,
    label: &'static str,
    tip: &'static str,
    n: usize,
    on: bool,
}

impl Burrow {
    /// What the Posts list shows for `q`: an empty search shows the chosen
    /// filter; a search looks through every post.
    pub(crate) fn filtered(&self, q: &str, mut found: Vec<Item>) -> Vec<Item> {
        if q.trim().is_empty() {
            let f = self.parity.filter;
            found.retain(|i| f.keeps(i));
        }
        found
    }
}

#[wasm_bindgen]
impl Burrow {
    /// The Posts list's filters, with counts: `{on, empty, chips}`.
    pub fn post_filters(&self) -> String {
        let on = self.parity.filter;
        let chips: Vec<Chip> = filter_counts(self.store.raw())
            .into_iter()
            .map(|(f, n)| Chip {
                key: f.key(),
                label: f.label(),
                tip: f.tip(),
                n,
                on: f == on,
            })
            .collect();
        json(&json!({ "on": on.key(), "empty": on.empty(), "chips": chips }))
    }

    /// Choose a filter (`all`, `drafts`, `changes`, `public`, `withdrawn`;
    /// anything else is `all`). With `keep`, the open post stays open even
    /// when the filter doesn't list it (as a ⌘N post does); otherwise the
    /// selection moves into the filtered list. Returns the filter's label.
    pub fn set_post_filter(&mut self, key: &str, keep: bool) -> String {
        self.parity.filter = PostFilter::parse(key).unwrap_or_default();
        match self.current_id() {
            Some(id) if keep && self.store.item(&id).is_some() => self.keep_open(&id),
            _ => {
                let q = self.list.query().to_string();
                let results = self.results(&q);
                self.list.set_query(&q, results);
            }
        }
        self.parity.filter.label().into()
    }

    /// The next filter (the key that steps through them). Returns its label.
    pub fn next_post_filter(&mut self, keep: bool) -> String {
        let next = self.parity.filter.next();
        self.set_post_filter(next.key(), keep)
    }
}
