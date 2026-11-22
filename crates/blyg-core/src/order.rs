//! The one list order for every client (SPEC "Reading order"): the reading
//! list and your own posts are ordered here, by the SQLite store and by the
//! app alike. No client keeps the order a backend or a server answered in:
//! `/api/reading/imported` comes in fetch order (`observed_at`, the same
//! second for a whole backfill), and the studio's own timestamps tie to the
//! second.
//!
//! - **Reading rows** ([`reading_cmp`]): newest first by the post's own date,
//!   [`ReadingItem::sort_at`] (updated, else created, else when your blyg
//!   first saw it), then subscription id and remote id, both descending, so
//!   rows dated the same come out the same way every time.
//! - **Your posts** ([`own_cmp`]): newest first by `updated` read as an
//!   instant (never compared as text: a local edit says `+00:00` with
//!   milliseconds, the server `Z` without), then the server id descending:
//!   the studio's `updated DESC, id DESC`.
//! - **The Feed** ([`ReaderOrder`], the `readerOrder` setting, studio 0.68):
//!   pace-layered by default ([`feed_positions`], the replay in
//!   [`crate::pace`]), or chronological, which is the reading order above.
//!   Hoppers, Saved, trails and Raw keep their own order.

use std::borrow::Borrow;
use std::cmp::Ordering;

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::background::Key;
use crate::model::{Item, ReadingItem, RemoteVersion};
use crate::util::parse_ts_ms;

/// Two reading rows in list order, given each one's [`ReadingItem::sort_at`]
/// (worked out once by a caller that sorts many rows).
pub fn reading_cmp_at(a_at: &str, a: &ReadingItem, b_at: &str, b: &ReadingItem) -> Ordering {
    b_at.cmp(a_at)
        .then_with(|| b.subscription_id.cmp(&a.subscription_id))
        .then_with(|| b.remote_id.cmp(&a.remote_id))
}

/// Two reading rows in list order: newest first by the post's own date,
/// then subscription id and remote id, descending.
pub fn reading_cmp(a: &ReadingItem, b: &ReadingItem) -> Ordering {
    reading_cmp_at(&a.sort_at(), a, &b.sort_at(), b)
}

/// Sort reading rows (owned or borrowed) into list order, working out each
/// row's date once.
pub fn sort_reading<T: Borrow<ReadingItem>>(rows: &mut Vec<T>) {
    let mut keyed: Vec<(String, T)> = rows.drain(..).map(|r| (r.borrow().sort_at(), r)).collect();
    keyed.sort_by(|(a_at, a), (b_at, b)| reading_cmp_at(a_at, a.borrow(), b_at, b.borrow()));
    rows.extend(keyed.into_iter().map(|(_, r)| r));
}

fn server_id(i: &Item) -> Option<&str> {
    i.server_id.as_ref().map(|s| s.0.as_str())
}

fn own_cmp_at(ta: Option<i64>, a: &Item, tb: Option<i64>, b: &Item) -> Ordering {
    // A date that parses is newer than one that doesn't; two that don't
    // fall back to their text, so the order stays total.
    tb.cmp(&ta)
        .then_with(|| match (ta, tb) {
            (None, None) => b.updated.cmp(&a.updated),
            _ => Ordering::Equal,
        })
        .then_with(|| server_id(b).cmp(&server_id(a)))
}

/// Two of your posts in list order: `updated` newest first, as instants,
/// then the server id descending. Posts the server hasn't numbered yet tie
/// last, keeping the order they came in (sorts here are stable).
pub fn own_cmp(a: &Item, b: &Item) -> Ordering {
    own_cmp_at(parse_ts_ms(&a.updated), a, parse_ts_ms(&b.updated), b)
}

/// Sort your posts into list order, parsing each date once.
pub fn sort_own(items: &mut Vec<Item>) {
    let mut keyed: Vec<(Option<i64>, Item)> = items
        .drain(..)
        .map(|i| (parse_ts_ms(&i.updated), i))
        .collect();
    keyed.sort_by(|(ta, a), (tb, b)| own_cmp_at(*ta, a, *tb, b));
    items.extend(keyed.into_iter().map(|(_, i)| i));
}

/// Store meta: the Feed's order as last read or chosen.
pub const READER_ORDER_META: &str = "reader.order";
/// Store meta: "1" when the blyg's settings have `readerOrder`.
pub const READER_ORDER_SERVED_META: &str = "reader.order.served";

/// The Feed's order (`Settings.readerOrder`, studio 0.68, contract 6.1.0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReaderOrder {
    /// Newest first, shown as "Chronological".
    Newest,
    /// The home page's order over everything you read: the default.
    #[default]
    PaceLayered,
}

impl ReaderOrder {
    pub const ALL: [ReaderOrder; 2] = [ReaderOrder::PaceLayered, ReaderOrder::Newest];

    /// Its value in Settings.
    pub fn as_str(self) -> &'static str {
        match self {
            ReaderOrder::Newest => "newest",
            ReaderOrder::PaceLayered => "pace-layered",
        }
    }

    pub fn parse(s: &str) -> Option<ReaderOrder> {
        match s {
            "newest" => Some(ReaderOrder::Newest),
            "pace-layered" => Some(ReaderOrder::PaceLayered),
            _ => None,
        }
    }

    /// From `GET /api/settings`: `None` when the studio doesn't have it
    /// (before 0.68). A value it has but Burrow doesn't know reads as the
    /// default, as the studio's own reading of its stored value does.
    pub fn from_settings(v: &Value) -> Option<ReaderOrder> {
        let o = v.get("readerOrder")?;
        Some(o.as_str().and_then(ReaderOrder::parse).unwrap_or_default())
    }

    /// The partial `PATCH /api/settings` body: this key and no other.
    pub fn patch(self) -> Value {
        json!({ "readerOrder": self.as_str() })
    }

    /// What the Feed's own control says (the studio's words).
    pub fn label(self) -> &'static str {
        match self {
            ReaderOrder::Newest => "Chronological",
            ReaderOrder::PaceLayered => "Pace-layered",
        }
    }

    /// What Settings › Reader order says of it.
    pub fn hint(self) -> &'static str {
        match self {
            ReaderOrder::Newest => "by when each item or version arrived",
            ReaderOrder::PaceLayered => {
                "the home page's order over everything you read: new items arrive on top \
                 and sink as others come, more slowly the more their blyg has pinned \
                 them, and a new version climbs one place"
            }
        }
    }
}

/// Every row's place in the pace-layered Feed, top first
/// ([`crate::pace::reader_positions`] over `rows`, with each row's cached
/// changelog when `changelog` has one, and `l0` saying which rows are a
/// plain feed's). A row missing from it (it arrived after) goes above every
/// place, as the studio's does.
pub fn feed_positions<'a>(
    rows: impl IntoIterator<Item = &'a ReadingItem>,
    changelog: impl Fn(&ReadingItem) -> Option<Vec<RemoteVersion>>,
    l0: impl Fn(&ReadingItem) -> bool,
) -> HashMap<Key, usize> {
    crate::pace::reader_positions(&crate::pace::history(rows, changelog, l0))
        .into_iter()
        .enumerate()
        .map(|(i, k)| (k, i))
        .collect()
}

#[cfg(test)]
#[path = "order_tests.rs"]
mod tests;
