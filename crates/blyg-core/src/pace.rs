//! The pace-layered order of the Feed (blygger-studio 0.68, ROADMAP row 69):
//! the home page's order (`src/feeds/pace-layered.ts`, RFC-3) run over
//! everything you read, your own posts and every blyg's, as one stream
//! (`src/feeds/reader-order.ts`). Ported step for step; [`crate::order`]
//! is where a list asks for it.
//!
//! - A new item from any blyg is a tick: everything already in the order
//!   sinks, and the newcomer enters on top.
//! - Items sink at `2^-layer` per tick, where the layer is the item's pin
//!   weight, floored, at most 5: the more its blyg pinned it, the slower.
//! - A new version climbs one place, none when it was maintenance.
//! - Only a blyg's own pins count (decision #73), and pin weight travels
//!   only within a blyg (own-origin quotes).
//! - A thread and the parts first published within 15 minutes before it
//!   enter together, the thread on top (RFC-3 R3).
//! - The replay ranks the newest [`WINDOW`] entries; the rest follow newest
//!   first, withdrawn ones last.
//!
//! What Burrow holds differs from the studio's tables, and the order says
//! so: an item's versions are its cached changelog (`Backend` caches the
//! changelogs of posts you opened) or, without one, its first and latest
//! versions, as the studio counts an item imported before its migration
//! 0051. A version is maintenance when the studio's Background says so for
//! the current one ([`crate::background::EntryBackground::maintenance`]):
//! Burrow keeps no unpinned old version of someone's post to measure
//! (spec §8.4).

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::background::Key;
use crate::model::{Kind, OWN_SUBSCRIPTION, ReadingItem, RemoteVersion};
use crate::util::parse_ts_ms;

/// How many entries, by latest activity, the replay ranks (studio
/// `READER_ORDER_WINDOW`).
pub const WINDOW: usize = 500;

/// Hops a pin's weight travels up the quoting tree (studio `WEIGHT_HOPS`).
pub const WEIGHT_HOPS: usize = 6;

/// Brand's pace layers, fastest first (studio `LAYERS`).
pub const LAYERS: [&str; 6] = [
    "fashion",
    "commerce",
    "infrastructure",
    "governance",
    "culture",
    "nature",
];

/// A thread's parts first published at most this long before it enter
/// with it (studio `BURST_WINDOW_MS`).
pub const BURST_WINDOW_MS: i64 = 15 * 60 * 1000;

/// One version as the order reads it (studio `FeedVersion`).
#[derive(Debug, Clone, PartialEq)]
pub struct PaceVersion {
    pub version: u32,
    /// Publication time, unix ms.
    pub at: i64,
    pub pinned: bool,
    /// Items of the same blyg this version quotes (indices into the
    /// history's items, resolved by [`History::new`]).
    pub quotes: Vec<Key>,
    /// The client judged it maintenance: it climbs no place.
    pub maintenance: bool,
}

/// One item's history (studio `FeedHistoryItem`).
#[derive(Debug, Clone, PartialEq)]
pub struct PaceItem {
    pub key: Key,
    pub thread: bool,
    pub withdrawn: bool,
    /// Oldest first.
    pub versions: Vec<PaceVersion>,
}

/// Depth gained per tick: each layer down sinks at half the speed of the
/// one above (studio `sinkSpeed`).
pub fn sink_speed(layer: usize) -> f64 {
    2f64.powi(-(layer as i32))
}

/// An item's layer: its pin weight, floored, at most the last.
pub fn layer_for(weight: f64) -> usize {
    (weight.floor().max(0.0) as usize).min(LAYERS.len() - 1)
}

/// How many places a new version climbs: one, none for maintenance.
pub fn climb_for(v: &PaceVersion) -> usize {
    if v.maintenance { 0 } else { 1 }
}

/// Whether an item first published at `member` belongs to the burst of a
/// thread version published at `thread` (studio `withinBurst`).
pub fn within_burst(member: i64, thread: i64) -> bool {
    member <= thread && thread - member <= BURST_WINDOW_MS
}

/// Pin weight (studio `pinWeights`): an item's own pins, plus half the
/// weight of each item that quotes it in a pinned version, a quarter of
/// what quotes those, out to [`WEIGHT_HOPS`].
fn pin_weights(
    pins: &HashMap<usize, f64>,
    quoters: &HashMap<usize, BTreeSet<usize>>,
) -> HashMap<usize, f64> {
    let mut weights = pins.clone();
    for _ in 0..WEIGHT_HOPS {
        let mut next = pins.clone();
        for (quoted, by) in quoters {
            let inherited: f64 = by
                .iter()
                .map(|q| weights.get(q).copied().unwrap_or(0.0))
                .sum();
            *next.entry(*quoted).or_insert(0.0) += inherited / 2.0;
        }
        weights = next;
    }
    weights
}

/// The order of `items` (studio `paceLayeredOrder`): keys top first,
/// withdrawn items left out. `items` is in listing order, which breaks
/// ties between versions published in the same millisecond.
pub fn pace_layered_order(items: &[PaceItem]) -> Vec<Key> {
    let index: HashMap<&Key, usize> = items
        .iter()
        .enumerate()
        .map(|(i, it)| (&it.key, i))
        .collect();
    let resolve =
        |qs: &[Key]| -> Vec<usize> { qs.iter().filter_map(|q| index.get(q).copied()).collect() };
    let visible: Vec<bool> = items.iter().map(|i| !i.withdrawn).collect();

    // Bursts (studio `findBursts`): a thread version and the items it is
    // the first to name, first published within the window before it.
    let first: HashMap<usize, i64> = items
        .iter()
        .enumerate()
        .filter_map(|(i, it)| it.versions.first().map(|v| (i, v.at)))
        .collect();
    let mut thread_versions: Vec<(usize, &PaceVersion)> = items
        .iter()
        .enumerate()
        .filter(|(i, it)| it.thread && visible[*i])
        .flat_map(|(i, it)| it.versions.iter().map(move |v| (i, v)))
        .collect();
    thread_versions.sort_by_key(|(_, v)| v.at);
    let mut by_thread: HashMap<(usize, u32), Vec<usize>> = HashMap::new();
    let mut member: HashSet<usize> = HashSet::new();
    for (id, v) in thread_versions {
        let mut members: Vec<usize> = Vec::new();
        for q in resolve(&v.quotes) {
            if q == id || member.contains(&q) || members.contains(&q) {
                continue;
            }
            if first.get(&q).is_some_and(|&t| within_burst(t, v.at)) {
                members.push(q);
            }
        }
        if members.is_empty() {
            continue;
        }
        members.sort_by_key(|m| first[m]);
        member.extend(members.iter().copied());
        by_thread.insert((id, v.version), members);
    }

    // Replay every publication in time order; at the same time, listing
    // order, and an item's own versions in sequence (a stable sort).
    let mut events: Vec<(usize, &PaceVersion)> = items
        .iter()
        .enumerate()
        .flat_map(|(i, it)| it.versions.iter().map(move |v| (i, v)))
        .collect();
    events.sort_by_key(|(_, v)| v.at);

    let mut depth: HashMap<usize, f64> = HashMap::new();
    let mut entered: HashMap<usize, u64> = HashMap::new();
    let mut clock: u64 = 0;
    let mut pins: HashMap<usize, f64> = HashMap::new();
    let mut quoters: HashMap<usize, BTreeSet<usize>> = HashMap::new();
    let mut layers: HashMap<usize, usize> = HashMap::new();
    // Shallowest first; at equal depth, the later entrant on top.
    let ranked = |depth: &HashMap<usize, f64>, entered: &HashMap<usize, u64>| -> Vec<usize> {
        let mut v: Vec<usize> = depth.keys().copied().collect();
        v.sort_by(|a, b| {
            depth[a]
                .partial_cmp(&depth[b])
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| entered[b].cmp(&entered[a]))
        });
        v
    };
    let sink_all_but =
        |depth: &mut HashMap<usize, f64>, layers: &HashMap<usize, usize>, but: usize| {
            for (other, d) in depth.iter_mut() {
                if *other != but {
                    *d += sink_speed(layers.get(other).copied().unwrap_or(0));
                }
            }
        };

    for (id, v) in events {
        // A pin counts from its version's publication; a withdrawn item's
        // pins still count (pinned versions are served forever).
        if v.pinned {
            *pins.entry(id).or_insert(0.0) += 1.0;
            for q in resolve(&v.quotes) {
                if q != id {
                    quoters.entry(q).or_default().insert(id);
                }
            }
            layers = pin_weights(&pins, &quoters)
                .into_iter()
                .map(|(k, w)| (k, layer_for(w)))
                .collect();
        }
        if !visible[id] {
            continue;
        }
        // A burst's member enters with its thread, not on its own.
        if !depth.contains_key(&id) && member.contains(&id) {
            continue;
        }
        let burst: Vec<usize> = by_thread
            .get(&(id, v.version))
            .map(|ms| {
                ms.iter()
                    .copied()
                    .filter(|m| visible[*m] && !depth.contains_key(m))
                    .collect()
            })
            .unwrap_or_default();
        if !burst.is_empty() {
            // One tick for the whole burst: the parts enter in reverse
            // publication order and the thread last, on top.
            sink_all_but(&mut depth, &layers, id);
            for m in burst.iter().rev() {
                depth.insert(*m, 0.0);
                entered.insert(*m, clock);
                clock += 1;
            }
            depth.insert(id, 0.0);
            entered.insert(id, clock);
            clock += 1;
            continue;
        }
        if !depth.contains_key(&id) {
            // A tick: everything sinks, and the newcomer enters on top.
            sink_all_but(&mut depth, &layers, id);
            depth.insert(id, 0.0);
            entered.insert(id, clock);
            clock += 1;
        } else {
            for _ in 0..climb_for(v) {
                let order = ranked(&depth, &entered);
                let Some(at) = order.iter().position(|x| *x == id) else {
                    break;
                };
                if at == 0 {
                    break;
                }
                let above = order[at - 1];
                let own = depth[&id];
                let theirs = depth[&above];
                depth.insert(id, theirs);
                depth.insert(above, own);
                // At equal depths the swap changes nothing; the tie-break
                // decides, so the climber becomes the later entrant.
                if theirs == own {
                    entered.insert(id, clock);
                    clock += 1;
                }
            }
        }
    }
    ranked(&depth, &entered)
        .into_iter()
        .map(|i| items[i].key.clone())
        .collect()
}

fn last_at(i: &PaceItem) -> i64 {
    i.versions.last().map_or(i64::MIN, |v| v.at)
}

/// Every key's place, top first (studio `readerPositions`): the replay
/// over the newest [`WINDOW`] live entries (reading pins and quotes from
/// withdrawn ones too), then the rest newest first, withdrawn last.
pub fn reader_positions(items: &[PaceItem]) -> Vec<Key> {
    let mut live: Vec<&PaceItem> = items
        .iter()
        .filter(|i| !i.withdrawn && !i.versions.is_empty())
        .collect();
    live.sort_by_key(|i| std::cmp::Reverse(last_at(i)));
    let window: HashSet<&Key> = live.iter().take(WINDOW).map(|i| &i.key).collect();
    let mut replay: Vec<PaceItem> = items
        .iter()
        .filter(|i| window.contains(&i.key) || i.withdrawn)
        .cloned()
        .collect();
    replay.sort_by_key(|i| i.versions.first().map_or(i64::MIN, |v| v.at));
    let mut out = pace_layered_order(&replay);
    out.extend(live.iter().skip(WINDOW).map(|i| i.key.clone()));
    let mut withdrawn: Vec<&PaceItem> = items.iter().filter(|i| i.withdrawn).collect();
    withdrawn.sort_by_key(|i| std::cmp::Reverse(last_at(i)));
    out.extend(withdrawn.into_iter().map(|i| i.key.clone()));
    out
}

/// A publisher's time clamped to when this reader first saw the item
/// (studio `clampedTime`), so a future-dated item can't hold the top.
pub fn clamped(asserted: Option<&str>, observed: &str) -> Option<i64> {
    let a = asserted.and_then(parse_ts_ms);
    let o = parse_ts_ms(observed);
    match (a, o) {
        (Some(a), Some(o)) => Some(a.min(o)),
        (a, o) => a.or(o),
    }
}

fn same_origin(a: &str, b: &str) -> bool {
    a.trim_end_matches('/') == b.trim_end_matches('/')
}

/// One reading row's history (studio `importedVersions`): its cached
/// changelog's versions up to the current one, else its first and latest
/// versions; times clamped to when it was seen; the current quotes stand
/// for every pinned version's and the current one's.
pub fn row_versions(row: &ReadingItem, changelog: Option<&[RemoteVersion]>) -> Vec<PaceVersion> {
    let key_of = |id: &str| {
        (
            row.subscription_id.clone(),
            crate::model::RemoteId::from(id),
        )
    };
    let own = row.subscription_id == OWN_SUBSCRIPTION;
    let mut seen = HashSet::new();
    let quotes: Vec<Key> = row
        .transclusions
        .iter()
        .filter(|q| match &q.origin {
            None => true,
            Some(o) => !own && same_origin(o, &row.origin),
        })
        .filter(|q| seen.insert(q.id.clone()))
        .map(|q| key_of(&q.id))
        .collect();
    let Some(fallback) = parse_ts_ms(&row.observed_at)
        .or_else(|| row.updated.as_deref().and_then(parse_ts_ms))
        .or_else(|| row.created.as_deref().and_then(parse_ts_ms))
    else {
        return Vec::new();
    };
    let at = |asserted: Option<&str>| clamped(asserted, &row.observed_at).unwrap_or(fallback);
    let maintenance = row.background.as_ref().is_some_and(|b| b.maintenance());
    let logged: Vec<&RemoteVersion> = changelog
        .unwrap_or_default()
        .iter()
        .filter(|v| v.version <= row.version)
        .collect();
    let mut out: Vec<PaceVersion> = if logged.is_empty() {
        let mut v = vec![PaceVersion {
            version: 1,
            at: at(row.created.as_deref().or(row.updated.as_deref())),
            pinned: false,
            quotes: if row.version <= 1 {
                quotes.clone()
            } else {
                vec![]
            },
            maintenance: false,
        }];
        if row.version > 1 {
            v.push(PaceVersion {
                version: row.version,
                at: at(row.updated.as_deref()),
                pinned: false,
                quotes: quotes.clone(),
                maintenance,
            });
        }
        v
    } else {
        logged
            .into_iter()
            .map(|e| PaceVersion {
                version: e.version,
                at: at(Some(e.at.as_str())
                    .filter(|s| !s.is_empty())
                    .or(row.updated.as_deref())),
                pinned: e.pinned,
                quotes: if e.pinned || e.version == row.version {
                    quotes.clone()
                } else {
                    vec![]
                },
                maintenance: maintenance && e.version == row.version && e.version > 1,
            })
            .collect()
    };
    out.sort_by_key(|v| v.version);
    out
}

/// The history of every reading row (studio `loadReaderHistory`), in a
/// fixed listing order (by key), with each row's cached changelog when
/// `changelog` has one.
pub fn history<'a>(
    rows: impl IntoIterator<Item = &'a ReadingItem>,
    changelog: impl Fn(&ReadingItem) -> Option<Vec<RemoteVersion>>,
) -> Vec<PaceItem> {
    let mut items: Vec<PaceItem> = rows
        .into_iter()
        .map(|r| PaceItem {
            key: (r.subscription_id.clone(), r.remote_id.clone()),
            thread: r.kind == Kind::Thread,
            withdrawn: r.state != "current",
            versions: row_versions(r, changelog(r).as_deref()),
        })
        .collect();
    items.sort_by(|a, b| a.key.cmp(&b.key));
    items.dedup_by(|a, b| a.key == b.key);
    items
}

#[cfg(test)]
#[path = "pace_tests.rs"]
mod tests;
