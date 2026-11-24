//! --- web-parity --- Updates: the studio's Updates tab and Refresh, with
//! the desktop's rules (Blyg › Stale Quotes…, `blyg-app/src/upkeep/
//! stale.rs`, and `blyg_core::upkeep`, shared):
//!
//! - `GET /api/freshness` (`listStaleThreads`): your published threads with
//!   a stale or blocking quote, stalest first;
//! - `GET /api/items/{id}/freshness` (`getItemFreshness`): one thread's
//!   quotes, its blyg asking the quoted blygs for their newest versions;
//! - `POST /api/items/{id}/refresh {expectedRevision, note}`
//!   (`refreshItem`): a publish of the published words with the quotes
//!   brought up to date. Like a publish, the post is read again after
//!   (`live_command_answer`, `live_apply_item`).
//!
//! A refresh is refused here, with the reason, for a thread with a quote
//! that no longer matches, with edits not published (here or on the blyg),
//! or not held in this browser.

use blyg_core::upkeep::{QuoteFreshness, Refreshed, ThreadFreshness, sort_stalest_first};
use blyg_core::{Item, upkeep};
use serde::Serialize;
use serde_json::json;
use wasm_bindgen::prelude::*;

use super::{err_out, err_text};
use crate::api::{self, Request, enc};
use crate::{Burrow, json, vm};

/// The note a refresh publishes with unless another is typed (the studio's
/// and the desktop's).
pub const REFRESH_NOTE: &str = "refreshed quoted snapshots";

/// Why a thread can't be refreshed now (the desktop's `why_not`).
pub fn why_not(report: &ThreadFreshness, here: Option<&Item>) -> Option<&'static str> {
    let Some(it) = here else {
        return Some("This thread isn't in this browser yet");
    };
    if report.blocking > 0 {
        return Some("A quote no longer matches its post: edit the thread first");
    }
    if report.dirty || vm::has_unpublished_edits(it) || it.pending_sync {
        return Some("It has unpublished edits: publish or discard them first");
    }
    if report.stale == 0 {
        return Some("Nothing in it is stale");
    }
    None
}

/// The refresh request (`note` trimmed; the default when empty).
pub fn refresh_request(server_id: &str, revision: u64, note: &str) -> Request {
    let note = note.trim();
    let note = if note.is_empty() { REFRESH_NOTE } else { note };
    Request {
        method: "POST",
        path: format!("/api/items/{}/refresh", enc(server_id)),
        body: Some(json!({
            "expectedRevision": revision,
            "note": blyg_core::text::for_save(note),
        })),
    }
}

#[derive(Serialize)]
struct QuoteRow {
    label: String,
    words: String,
    stale: bool,
    blocking: bool,
    partial: bool,
}

#[derive(Serialize)]
struct UpdateRow {
    sid: String,
    id: Option<String>,
    title: String,
    line: String,
    why: Option<&'static str>,
    quotes: Option<Vec<QuoteRow>>,
    probe_error: Option<String>,
    failed: Option<String>,
}

#[wasm_bindgen]
impl Burrow {
    /// `GET /api/freshness`'s answer. Returns `{n}` or `{error, code}`.
    pub fn upd_loaded(&mut self, status: u16, body: &str) -> String {
        match api::read(status, body, None) {
            Ok(v) => {
                let mut list: Vec<ThreadFreshness> = v
                    .get("items")
                    .and_then(|i| serde_json::from_value(i.clone()).ok())
                    .unwrap_or_default();
                sort_stalest_first(&mut list);
                let n = list.len();
                self.parity.stale = Some(Ok(list));
                self.parity.probed.clear();
                json(&json!({ "n": n }))
            }
            Err(e) => {
                self.parity.stale = Some(Err(err_text(&e)));
                err_out(&e)
            }
        }
    }

    /// Read the list again when it's next shown.
    pub fn upd_forget(&mut self) {
        self.parity.stale = None;
        self.parity.failed.clear();
    }

    /// The Updates screen: `{loaded, error, can, canRefresh, canProbe,
    /// note, rows}`. `can`: the blyg lists stale threads at all.
    pub fn upd_view(&self) -> String {
        let can = self.can(super::STALE_OPERATION);
        let list = match &self.parity.stale {
            None => return json(&json!({ "loaded": false, "can": can, "rows": [] })),
            Some(Err(e)) => {
                return json(&json!({ "loaded": true, "can": can, "error": e, "rows": [] }));
            }
            Some(Ok(l)) => l,
        };
        let own = |sid: &str| self.store.by_server(sid).and_then(|l| self.store.item(&l));
        let rows: Vec<UpdateRow> = list
            .iter()
            .map(|t| {
                let it = own(&t.id);
                let probed = self.parity.probed.get(&t.id);
                let report = match probed {
                    Some(Ok(p)) => p,
                    _ => t,
                };
                let quotes = probed.and_then(|p| p.as_ref().ok()).map(|p| {
                    p.quotes
                        .iter()
                        .map(|q: &QuoteFreshness| QuoteRow {
                            label: match &q.origin {
                                Some(o) => upkeep::host(o),
                                None => own(&q.id)
                                    .map(|i| format!("yours: {}", vm::item_title(&i)))
                                    .unwrap_or_else(|| "a post of yours".into()),
                            },
                            words: q.words(),
                            stale: q.status.is_stale(),
                            blocking: q.status.is_blocking(),
                            partial: q.partial,
                        })
                        .collect()
                });
                UpdateRow {
                    sid: t.id.clone(),
                    id: it.as_ref().map(|i| i.local_id.0.clone()),
                    title: it.as_ref().map(vm::item_title).unwrap_or_else(|| {
                        format!("Thread {}", t.id.chars().take(8).collect::<String>())
                    }),
                    line: report.line(),
                    why: why_not(report, it.as_ref()),
                    quotes,
                    probe_error: probed.and_then(|p| p.as_ref().err().cloned()),
                    failed: self.parity.failed.get(&t.id).cloned(),
                }
            })
            .collect();
        json(&json!({
            "loaded": true,
            "can": can,
            "canRefresh": self.can(super::REFRESH_OPERATION),
            "canProbe": self.can(super::FRESHNESS_OPERATION),
            "note": REFRESH_NOTE,
            "rows": rows,
        }))
    }

    /// Where one thread's quotes are asked of their blygs.
    pub fn upd_probe_path(&self, sid: &str) -> String {
        format!("/api/items/{}/freshness", enc(sid))
    }

    /// The answer to [`Burrow::upd_probe_path`].
    pub fn upd_probed(&mut self, sid: &str, status: u16, body: &str) {
        let r = api::read(status, body, None)
            .map_err(|e| err_text(&e))
            .and_then(|v| {
                serde_json::from_value::<ThreadFreshness>(v)
                    .map_err(|_| "The blyg's answer wasn't a freshness report".to_string())
            });
        self.parity.probed.insert(sid.to_string(), r);
    }

    /// The refresh request for thread `sid`: `{req, id}` (`id`, the local
    /// id, for `live_command_answer`), or `{error}`.
    pub fn upd_refresh_request(&self, sid: &str, note: &str) -> String {
        if !self.can(super::REFRESH_OPERATION) {
            return json(&json!({ "error": "Your blyg doesn't refresh quotes" }));
        }
        let Some(lid) = self.store.by_server(sid) else {
            return json(&json!({ "error": "This thread isn't in this browser yet" }));
        };
        let it = self.store.item(&lid);
        let probed = self.parity.probed.get(sid).and_then(|p| p.as_ref().ok());
        let listed = self
            .parity
            .stale
            .as_ref()
            .and_then(|s| s.as_ref().ok())
            .and_then(|l| l.iter().find(|t| t.id == sid));
        let Some(report) = probed.or(listed) else {
            return json(&json!({ "error": "That thread isn't stale" }));
        };
        if let Some(why) = why_not(report, it.as_ref()) {
            return json(&json!({ "error": why }));
        }
        let rev = self.store.live.rev.get(&lid.0).copied().unwrap_or(0);
        json(&json!({ "req": refresh_request(sid, rev, note), "id": lid.0 }))
    }

    /// What a refresh did: `{toast}` (the thread leaves the list), or
    /// `{error, code}` (kept, with the reason, on its row).
    pub fn upd_refreshed(&mut self, sid: &str, status: u16, body: &str) -> String {
        match api::read(status, body, None) {
            Ok(v) => {
                let words = serde_json::from_value::<Refreshed>(v)
                    .map(|r| r.words())
                    .unwrap_or_else(|_| "a new version".into());
                if let Some(Ok(l)) = &mut self.parity.stale {
                    l.retain(|t| t.id != sid);
                }
                self.parity.probed.remove(sid);
                self.parity.failed.remove(sid);
                json(&json!({ "toast": words }))
            }
            Err(e) => {
                self.parity.failed.insert(sid.to_string(), err_text(&e));
                err_out(&e)
            }
        }
    }
}
