//! The one list order for every client (SPEC "Reading order"): the reading
//! list and your own posts are ordered here, by the SQLite store, by the
//! app, and by Burrow in a browser alike. No client keeps the order a
//! backend or a server answered in: `/api/reading/imported` comes in fetch
//! order (`observed_at`, the same second for a whole backfill), and the
//! studio's own timestamps tie to the second.
//!
//! - **Reading rows** ([`reading_cmp`]): newest first by the post's own date,
//!   [`ReadingItem::sort_at`] (updated, else created, else when your blyg
//!   first saw it), then subscription id and remote id, both descending, so
//!   rows dated the same come out the same way every time.
//! - **Your posts** ([`own_cmp`]): newest first by `updated` read as an
//!   instant (never compared as text: a local edit says `+00:00` with
//!   milliseconds, the server `Z` without), then the server id descending:
//!   the studio's `updated DESC, id DESC`.

use std::borrow::Borrow;
use std::cmp::Ordering;

use crate::model::{Item, ReadingItem};
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

#[cfg(test)]
#[path = "order_tests.rs"]
mod tests;
