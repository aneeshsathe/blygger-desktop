//! --- shelf sync --- The studio's synced shelf (blygger-studio 0.61.0,
//! contract 5.10.0, its #135/#137): the owner's short, ordered list of
//! items, own and imported, shared by every client.
//!
//! - `GET /api/shelf` → `{entries: [{sub, remoteId}], revision}`.
//! - `PUT /api/shelf {entries, expectedRevision}` replaces the whole list
//!   (owner:manage). `expectedRevision` is required: a stale one is 409
//!   `revision-conflict` with the current list in `details.current`.
//!   Entries that don't resolve (an unknown subscription, an item not held,
//!   a withdrawn own item, a tombstone without a retained pin) are dropped
//!   quietly, a repeated entry keeps its first place, and at most
//!   [`SHELF_MAX`] entries are taken (a longer list is a 400). The answer is
//!   the list as stored.
//! - An entry names an item as a hopper membership does: `sub` is
//!   [`OWN`] (`"~own"`) with the item's server id for one of your own
//!   items (drafts too), else the subscription id and the imported item's
//!   remote id.
//! - `/api/changes` counts the list in its own domain, `shelf`.
//! - Offered when `GET /api/capabilities` lists both `getShelf` and
//!   `putShelf` ([`serves`]); never judged by the contract version.
//!
//! This module is the shapes and the merge, with no I/O: native sync
//! (`sync::Engine::sync_shelf`) and the web (`blyg-web`) share it.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The `sub` of one of your own items (the studio's `OWN_SUBSCRIPTION`).
pub const OWN: &str = crate::model::OWN_SUBSCRIPTION;

/// The most entries the studio's shelf holds (`SHELF_MAX` in its
/// `src/shelf.ts`); a `PUT` of more is refused outright, so a client never
/// sends more.
pub const SHELF_MAX: usize = 50;

/// The capabilities' operation names that offer the synced shelf.
pub const GET_OPERATION: &str = "getShelf";
pub const PUT_OPERATION: &str = "putShelf";

/// `meta`: "1" while the blyg offers the synced shelf, "0" when it doesn't,
/// absent until its capabilities were read.
pub const SERVED_META: &str = "shelf_served";
/// `meta`: the list as the blyg last agreed (JSON `[Entry]`): the common
/// ancestor of a three-way merge. Absent until the first sync.
pub const BASE_META: &str = "shelf_base";
/// `meta`: the shelf's revision as last read or written.
pub const REVISION_META: &str = "shelf_revision";

/// One shelf entry, as the wire names it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Entry {
    pub sub: String,
    #[serde(rename = "remoteId")]
    pub remote_id: String,
}

impl Entry {
    pub fn new(sub: impl Into<String>, remote_id: impl Into<String>) -> Entry {
        Entry {
            sub: sub.into(),
            remote_id: remote_id.into(),
        }
    }

    /// One of your own items, by its server id.
    pub fn own(server_id: impl Into<String>) -> Entry {
        Entry::new(OWN, server_id)
    }

    pub fn is_own(&self) -> bool {
        self.sub == OWN
    }
}

/// The shelf as the blyg holds it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Remote {
    pub entries: Vec<Entry>,
    pub revision: u64,
}

/// Where the shelf lives, for the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncState {
    /// On this computer only: the blyg doesn't offer a synced shelf (or
    /// this isn't a live blyg).
    #[default]
    Local,
    /// Synced with the blyg; nothing waiting.
    Synced,
    /// Changed here; waiting to reach the blyg.
    Waiting,
}

/// Whether a `GET /api/capabilities` answer offers the synced shelf: its
/// `operations` (an array of ids; a map is read by its keys) list both
/// [`GET_OPERATION`] and [`PUT_OPERATION`].
pub fn serves(capabilities: &Value) -> bool {
    let has = |op: &str| match capabilities.get("operations") {
        Some(Value::Array(a)) => a.iter().any(|o| o.as_str() == Some(op)),
        Some(Value::Object(m)) => m.contains_key(op),
        _ => false,
    };
    has(GET_OPERATION) && has(PUT_OPERATION)
}

/// A `Shelf` answer (`GET`, a `PUT`'s answer, or a 409's
/// `details.current`). `None` when it isn't one.
pub fn parse(v: &Value) -> Option<Remote> {
    let revision = v.get("revision")?.as_u64()?;
    let entries = v
        .get("entries")?
        .as_array()?
        .iter()
        .filter_map(|e| {
            let s = |k: &str| e.get(k).and_then(Value::as_str).filter(|s| !s.is_empty());
            Some(Entry::new(s("sub")?, s("remoteId")?))
        })
        .collect();
    Some(Remote { entries, revision })
}

/// The current list a 409 `revision-conflict` carries
/// (`{code, message, details: {current}}`).
pub fn conflict_current(body: &Value) -> Option<Remote> {
    parse(body.get("details")?.get("current")?)
}

/// The `PUT /api/shelf` body. Never more than [`SHELF_MAX`] entries.
pub fn put_body(entries: &[Entry], expected_revision: u64) -> Value {
    let entries: Vec<&Entry> = entries.iter().take(SHELF_MAX).collect();
    json!({ "entries": entries, "expectedRevision": expected_revision })
}

/// The list to send: `server` with this side's intent applied on top.
///
/// - `base`: the list both sides last agreed on (`None`: never synced). A
///   first sync takes the union, the blyg's order first, then what only
///   this side holds, in its order.
/// - Otherwise a three-way merge. What this side took off since `base`
///   comes off; what it added goes in after the entry it followed here (at
///   the top when it was first); and when it reordered what both hold, the
///   places those entries take on the blyg's list are filled in this side's
///   order. The blyg's own additions and removals stand. With `server ==
///   base` (nothing changed there) the answer is `local` itself.
///
/// Repeats are dropped (the first place wins, as the studio keeps it).
pub fn merge(base: Option<&[Entry]>, local: &[Entry], server: &[Entry]) -> Vec<Entry> {
    let Some(base) = base else {
        let mut out = dedupe(server);
        let have: HashSet<Entry> = out.iter().cloned().collect();
        let mut seen = have;
        for e in local {
            if seen.insert(e.clone()) {
                out.push(e.clone());
            }
        }
        return out;
    };
    let base_set: HashSet<&Entry> = base.iter().collect();
    let local_set: HashSet<&Entry> = local.iter().collect();
    // Taken off here since the last sync.
    let mut out: Vec<Entry> = dedupe(server)
        .into_iter()
        .filter(|e| !base_set.contains(e) || local_set.contains(e))
        .collect();
    // Reordered here: what both base and local hold, in local's order.
    let kept_base: Vec<&Entry> = base.iter().filter(|e| local_set.contains(e)).collect();
    let kept_local: Vec<&Entry> = local.iter().filter(|e| base_set.contains(e)).collect();
    if kept_base != kept_local {
        let kept: HashSet<&Entry> = kept_local.iter().copied().collect();
        let slots: Vec<usize> = (0..out.len()).filter(|&i| kept.contains(&out[i])).collect();
        let present: HashSet<Entry> = slots.iter().map(|&i| out[i].clone()).collect();
        let order: Vec<Entry> = kept_local
            .into_iter()
            .filter(|e| present.contains(*e))
            .cloned()
            .collect();
        for (slot, e) in slots.into_iter().zip(order) {
            out[slot] = e;
        }
    }
    // Added here: after the entry it follows here.
    let added = |e: &Entry| !base_set.contains(e);
    place_after(local, &mut out, added);
    dedupe(&out)
}

/// Put each entry of `order` that `extra` picks and `out` lacks into `out`,
/// just after the nearest entry before it in `order` that `out` holds (at
/// the top when there's none). `order`'s relative order of those entries is
/// kept.
pub fn place_after<T: Clone + PartialEq>(
    order: &[T],
    out: &mut Vec<T>,
    extra: impl Fn(&T) -> bool,
) {
    for (i, e) in order.iter().enumerate() {
        if !extra(e) || out.contains(e) {
            continue;
        }
        let at = order[..i]
            .iter()
            .rev()
            .find_map(|p| out.iter().position(|x| x == p))
            .map_or(0, |p| p + 1);
        out.insert(at, e.clone());
    }
}

/// At most [`SHELF_MAX`] entries: the list to send, and how many were left
/// off its end.
pub fn cap(mut list: Vec<Entry>) -> (Vec<Entry>, usize) {
    let over = list.len().saturating_sub(SHELF_MAX);
    list.truncate(SHELF_MAX);
    (list, over)
}

fn dedupe(list: &[Entry]) -> Vec<Entry> {
    let mut seen = HashSet::new();
    list.iter()
        .filter(|e| seen.insert((*e).clone()))
        .cloned()
        .collect()
}

/// What to say when a merged shelf didn't fit.
pub fn overflow_message(over: usize) -> String {
    let (n, were) = if over == 1 {
        ("1 post".to_string(), "was")
    } else {
        (format!("{over} posts"), "were")
    };
    format!(
        "Your shelf holds at most {SHELF_MAX} posts. Merging it with your blyg's, {n} didn't \
         fit and {were} left off. Take something off your shelf to add {} again.",
        if over == 1 { "it" } else { "them" }
    )
}

#[cfg(test)]
#[path = "shelf_tests.rs"]
mod tests;
