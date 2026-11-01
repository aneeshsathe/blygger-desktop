//! --- browse --- following links out of an entry (blygger-studio 0.49.0).
//!
//! The reader can walk "the thicket": a quoted layer at any depth, a stub's
//! target, or an ordinary link that names a blyg item opens in the reader,
//! and Back retraces the walk. Two things support it:
//!
//! - **Link classification** (`Backend::classify_links`): which of an entry's
//!   links name a blyg item. On a studio 0.49+ node this is
//!   `POST /api/peek/links` (the node looks at each page's §5.8
//!   `rel="alternate"` item document and remembers the answer for a week);
//!   elsewhere Burrow probes the pages itself with the token-less public
//!   client and caches the answers locally.
//! - **The browsing log** (`Backend::log_browse`, `browse_history`,
//!   `clear_browse`): the owner's private walk, a tree (each step names the
//!   step it was reached from). On a 0.49+ node it is the studio's own
//!   `browse_log` (`GET/POST/DELETE /api/browse`), shared with the web
//!   Studio's history pane; elsewhere it is kept in the local store. Never
//!   on the wire to anyone else and never on a public page.

use serde::{Deserialize, Serialize};

/// How a browse step was reached (the studio's `BROWSE_VIA`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BrowseVia {
    /// Opened from the feed: a trail's first step.
    Start,
    /// A stub's "responds to" line.
    Stub,
    /// A quoted layer (at any depth).
    Quote,
    /// An ordinary link that names a blyg item.
    Link,
    /// Back, retracing the walk.
    Back,
}

impl BrowseVia {
    pub fn as_str(self) -> &'static str {
        match self {
            BrowseVia::Start => "start",
            BrowseVia::Stub => "stub",
            BrowseVia::Quote => "quote",
            BrowseVia::Link => "link",
            BrowseVia::Back => "back",
        }
    }

    pub fn parse(s: &str) -> Option<BrowseVia> {
        Some(match s {
            "start" => BrowseVia::Start,
            "stub" => BrowseVia::Stub,
            "quote" => BrowseVia::Quote,
            "link" => BrowseVia::Link,
            "back" => BrowseVia::Back,
            _ => return None,
        })
    }
}

/// A blyg item named by origin + id (§13.1 rule 7): survives an unsubscribe.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemRef {
    pub origin: String,
    pub id: String,
}

/// One link found in an entry, and the blyg item it names, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkTarget {
    pub url: String,
    /// `None`: not a blyg item (open it in the browser).
    pub target: Option<ItemRef>,
}

/// One step of the owner's browsing log (the studio's `BrowseStep`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowseStep {
    pub id: i64,
    /// RFC 3339.
    pub at: String,
    /// One walk: 8–64 characters, made by the client when a walk starts.
    pub trail: String,
    /// The step this one was reached from; `None` for a trail's first.
    pub parent_id: Option<i64>,
    pub origin: String,
    pub remote_id: String,
    pub version: Option<u32>,
    pub via: BrowseVia,
    /// The title as it read then; the item may change or vanish.
    pub title: Option<String>,
}

/// A step to record (`Backend::log_browse`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBrowseStep {
    pub trail: String,
    pub parent_id: Option<i64>,
    pub origin: String,
    pub remote_id: String,
    pub version: Option<u32>,
    pub via: BrowseVia,
    /// At most 400 characters (the server's limit); longer is cut.
    pub title: Option<String>,
}

/// The studio's limit on one `POST /api/peek/links` (`CLASSIFY_MAX`).
pub const CLASSIFY_MAX: usize = 50;

/// A fresh trail id: 16 hex characters (the server wants 8–64).
pub fn new_trail() -> String {
    let n: u64 = rand_u64();
    format!("{n:016x}")
}

fn rand_u64() -> u64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut h = RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn via_round_trips() {
        for v in [
            BrowseVia::Start,
            BrowseVia::Stub,
            BrowseVia::Quote,
            BrowseVia::Link,
            BrowseVia::Back,
        ] {
            assert_eq!(BrowseVia::parse(v.as_str()), Some(v));
        }
        assert_eq!(BrowseVia::parse("nope"), None);
    }

    #[test]
    fn trails_are_server_sized_and_differ() {
        let (a, b) = (new_trail(), new_trail());
        assert_eq!(a.len(), 16);
        assert_ne!(a, b);
    }
}
