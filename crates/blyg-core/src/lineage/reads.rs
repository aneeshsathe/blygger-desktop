//! The `lineage-glyph` extension's reads for `LiveBackend`, over the store's
//! cache. Whether the node serves the extension is found out the way the
//! other server extensions are: by asking, and remembering a 404 (silently)
//! as "not enabled there", asked again after `OFF_RECHECK_MS`.

use std::collections::HashMap;

use super::{Centre, LineageGraph, LineageSummary, MAX_SUMMARY_KEYS, OFF_RECHECK_MS};
use crate::store::LineageServed as Served;
use crate::sync::Engine;

/// Whether to ask the node now: not while a recent 404 stands.
fn may_ask(e: &Engine, now: i64) -> bool {
    !matches!(e.store.lineage_served(), Served::Off(t) if now - t < OFF_RECHECK_MS)
}

fn served(e: &Engine) -> bool {
    e.store.lineage_served() == Served::On
}

/// Find out whether the node serves the extension (a summaries request for
/// no keys). An error (offline) leaves what's known as it is.
fn probe(e: &Engine, now: i64) -> bool {
    if served(e) {
        return true;
    }
    if !may_ask(e, now) {
        return false;
    }
    match e.api.lineage_summaries(&[]) {
        Ok(Some(_)) => e.store.set_lineage_served(true, now).is_ok(),
        Ok(None) => {
            let _ = e.store.set_lineage_served(false, now);
            false
        }
        Err(_) => false,
    }
}

pub fn cached_summaries(e: &Engine, keys: &[String]) -> Option<HashMap<String, LineageSummary>> {
    if !served(e) {
        return None;
    }
    let cached = e.store.cached_lineage_summaries(keys).ok()?;
    Some(
        cached
            .into_iter()
            .filter_map(|(k, (s, _))| s.map(|s| (k, s)))
            .collect(),
    )
}

pub fn summaries(
    e: &Engine,
    keys: &[String],
    max_age_ms: i64,
    now: i64,
) -> Option<HashMap<String, LineageSummary>> {
    if !may_ask(e, now) {
        return None;
    }
    let cached = e.store.cached_lineage_summaries(keys).unwrap_or_default();
    let stale: Vec<String> = keys
        .iter()
        .filter(|k| cached.get(*k).is_none_or(|(_, at)| now - at >= max_age_ms))
        .cloned()
        .collect();
    let mut out: HashMap<String, LineageSummary> = cached
        .into_iter()
        .filter_map(|(k, (s, _))| s.map(|s| (k, s)))
        .collect();
    if stale.is_empty() {
        probe(e, now);
    }
    for chunk in stale.chunks(MAX_SUMMARY_KEYS) {
        match e.api.lineage_summaries(chunk) {
            Ok(Some(got)) => {
                let _ = e.store.set_lineage_served(true, now);
                let _ = e.store.put_lineage_summaries(chunk, &got, now);
                for k in chunk {
                    out.remove(k);
                }
                out.extend(got);
            }
            Ok(None) => {
                // Not enabled there (or no longer): count locally.
                let _ = e.store.set_lineage_served(false, now);
                return None;
            }
            // Offline or refused: keep what's cached.
            Err(_) => break,
        }
    }
    served(e).then_some(out)
}

pub fn cached_graph(e: &Engine, centre: &Centre) -> Option<LineageGraph> {
    if !served(e) {
        return None;
    }
    e.store
        .cached_lineage_graph(&centre.query())
        .ok()
        .flatten()
        .and_then(|(g, _)| g)
}

pub fn graph(e: &Engine, centre: &Centre, max_age_ms: i64, now: i64) -> Option<LineageGraph> {
    // The lineage route's 404 also means "no such subscription", so whether
    // the extension is on is the summaries route's to say.
    if !probe(e, now) {
        return None;
    }
    let q = centre.query();
    let cached = e.store.cached_lineage_graph(&q).ok().flatten();
    if let Some((g, at)) = &cached
        && now - at < max_age_ms
    {
        return g.clone();
    }
    match e.api.lineage_graph(centre) {
        Ok(g) => {
            let _ = e.store.put_lineage_graph(&q, g.as_ref(), now);
            g
        }
        Err(_) => cached.and_then(|(g, _)| g),
    }
}
