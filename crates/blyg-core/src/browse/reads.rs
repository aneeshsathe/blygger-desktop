//! --- browse --- `LiveBackend`'s link classification and browsing log:
//! the blyg's own routes when it serves them (studio 0.49+), else this Mac.
//!
//! Whether the node serves a route is found out the way the lineage
//! extension's is: by asking, remembering a 404 (silently, in `meta`) and
//! asking again after `OFF_RECHECK_MS`. A studio before 0.49 answers a
//! bearer's POST to a route it doesn't know with `403 insufficient_scope`,
//! not 404: for `POST /api/peek/links` (which needs only `owner:read`) that
//! is read as "not served" too. A `403` on the log (a credential without
//! `reading:state`, or such an older studio) keeps the whole log local for
//! the rest of the session, so each step isn't refused again; the next
//! launch or sign-in asks again. Nothing here goes through the outbox: steps are
//! best-effort, from a background thread, and a step that can't be sent
//! (offline) is an error the caller may ignore.

use std::sync::atomic::{AtomicBool, Ordering};

use super::{BrowseStep, CLASSIFY_MAX, LinkTarget, NewBrowseStep, probe};
use crate::api::browse::{BROWSE_PAGE_MAX, is_forbidden};
use crate::api::public::PublicClient;
use crate::backend::{CoreError, Result};
use crate::lineage::OFF_RECHECK_MS;
use crate::store::Store;
use crate::sync::Engine;

/// meta: whether the node serves `POST /api/peek/links`.
pub const PEEK_LINKS_META: &str = "peek_links";
/// meta: whether the node serves `/api/browse`.
pub const BROWSE_META: &str = "browse_log";

/// Session state the store doesn't keep. A sign-in opens a new backend, so
/// it starts over then too.
#[derive(Debug, Default)]
pub struct BrowseState {
    /// The log answered 403 this session: keep it local until relaunch.
    forbidden: AtomicBool,
}

/// `on`, `off:<unix ms>`, or never asked.
fn may_ask(store: &Store, key: &str, now: i64) -> bool {
    match store.meta(key) {
        Some(v) => match v.strip_prefix("off:").and_then(|t| t.parse::<i64>().ok()) {
            Some(t) => now - t >= OFF_RECHECK_MS,
            None => true,
        },
        None => true,
    }
}

fn set_served(store: &Store, key: &str, on: bool, now: i64) {
    let v = if on {
        "on".to_string()
    } else {
        format!("off:{now}")
    };
    if store.meta(key).as_deref() != Some(v.as_str()) {
        let _ = store.set_meta(key, &v);
    }
}

/// The origins whose `f/{id}/`·`t/{id}/` are our own permalinks.
fn own_origins(e: &Engine) -> Vec<String> {
    let mut out = Vec::new();
    let candidates = [e.store.public_origin(), Some(e.api.base_url().to_string())];
    for c in candidates.into_iter().flatten() {
        if let Some(o) = crate::profile::normalize_origin(&c)
            && !out.contains(&o)
        {
            out.push(o);
        }
    }
    out
}

/// `Backend::classify_links`.
pub fn classify(e: &Engine, public: &PublicClient, urls: &[String], now: i64) -> Vec<LinkTarget> {
    let mut answered: std::collections::HashMap<String, Option<super::ItemRef>> =
        Default::default();
    if may_ask(&e.store, PEEK_LINKS_META, now) {
        // The server answers each distinct http(s) URL as sent.
        let mut ask: Vec<String> = Vec::new();
        for u in urls {
            if probe::lookup_key(u).is_some() && !ask.contains(u) {
                ask.push(u.clone());
            }
        }
        for chunk in ask.chunks(CLASSIFY_MAX) {
            match e.api.peek_links(chunk) {
                Ok(Some(links)) => {
                    set_served(&e.store, PEEK_LINKS_META, true, now);
                    answered.extend(links.into_iter().map(|l| (l.url, l.target)));
                }
                // A 404, or the 403 a studio before 0.49 gives a bearer for a
                // POST route it doesn't know (the route needs only
                // `owner:read`, which every Burrow credential has).
                Ok(None) => {
                    set_served(&e.store, PEEK_LINKS_META, false, now);
                    break;
                }
                Err(err) if is_forbidden(&err) => {
                    set_served(&e.store, PEEK_LINKS_META, false, now);
                    break;
                }
                // Offline, refused: what's left is probed here.
                Err(_) => break,
            }
        }
    }
    let rest: Vec<String> = urls
        .iter()
        .filter(|u| !answered.contains_key(*u))
        .cloned()
        .collect();
    let local = if rest.is_empty() {
        Vec::new()
    } else {
        probe::classify(&e.store, public, &own_origins(e), &rest, now)
    };
    answered.extend(local.into_iter().map(|l| (l.url, l.target)));
    urls.iter()
        .map(|u| LinkTarget {
            url: u.clone(),
            target: answered.get(u).cloned().flatten(),
        })
        .collect()
}

/// Whether the log lives on the server right now.
fn on_server(e: &Engine, st: &BrowseState, now: i64) -> bool {
    !st.forbidden.load(Ordering::Relaxed) && may_ask(&e.store, BROWSE_META, now)
}

/// `Backend::log_browse`.
pub fn log(e: &Engine, st: &BrowseState, step: &NewBrowseStep, now: i64) -> Result<i64> {
    if on_server(e, st, now) {
        match e.api.log_browse(step) {
            Ok(Some(id)) => {
                set_served(&e.store, BROWSE_META, true, now);
                return Ok(id);
            }
            Ok(None) => set_served(&e.store, BROWSE_META, false, now),
            Err(err) if is_forbidden(&err) => st.forbidden.store(true, Ordering::Relaxed),
            Err(err) => return Err(err),
        }
    }
    e.store.record_browse(step, &crate::util::iso_from_ms(now))
}

/// `Backend::browse_history`.
pub fn history(
    e: &Engine,
    st: &BrowseState,
    trail: Option<&str>,
    offset: usize,
    limit: usize,
    now: i64,
) -> Result<Vec<BrowseStep>> {
    if on_server(e, st, now) {
        match server_history(e, trail, offset, limit) {
            Ok(Some(out)) => {
                set_served(&e.store, BROWSE_META, true, now);
                return Ok(out);
            }
            Ok(None) => set_served(&e.store, BROWSE_META, false, now),
            Err(err) if is_forbidden(&err) => st.forbidden.store(true, Ordering::Relaxed),
            Err(err) => return Err(err),
        }
    }
    e.store.browse_log(trail, offset, limit)
}

/// Steps `offset..offset + limit` of the server's log, newest first: one
/// walk of its cursor pages from the start (the log has no offsets). A walk
/// the log changed under (`CursorExpired`) is dropped and started again.
/// `None` on 404.
fn server_history(
    e: &Engine,
    trail: Option<&str>,
    offset: usize,
    limit: usize,
) -> Result<Option<Vec<BrowseStep>>> {
    let end = offset.saturating_add(limit);
    'walk: for _ in 0..=crate::api::CURSOR_RESTARTS {
        let mut seen: Vec<BrowseStep> = Vec::new();
        let mut cursor: Option<String> = None;
        while seen.len() < end {
            let want = (end - seen.len()).min(BROWSE_PAGE_MAX);
            match e.api.list_browse(trail, cursor.as_deref(), want) {
                Ok(Some(page)) => {
                    seen.extend(page.items);
                    match page.next {
                        Some(n) => cursor = Some(n),
                        None => break,
                    }
                }
                Ok(None) => return Ok(None),
                Err(CoreError::CursorExpired) => continue 'walk,
                Err(err) => return Err(err),
            }
        }
        return Ok(Some(seen.into_iter().skip(offset).take(limit).collect()));
    }
    Err(CoreError::CursorExpired)
}

/// `Backend::clear_browse`: the server's log when it keeps one, and this
/// Mac's always (steps kept here while the server couldn't take them).
pub fn clear(e: &Engine, st: &BrowseState, trail: Option<&str>, now: i64) -> Result<()> {
    if on_server(e, st, now) {
        match e.api.clear_browse(trail) {
            Ok(Some(())) => set_served(&e.store, BROWSE_META, true, now),
            Ok(None) => set_served(&e.store, BROWSE_META, false, now),
            Err(err) if is_forbidden(&err) => st.forbidden.store(true, Ordering::Relaxed),
            Err(err) => return Err(err),
        }
    }
    e.store.clear_browse_log(trail)
}
