//! --- web-parity --- Mark all read: every row of the chosen Reading source
//! and lens that isn't read at its current version, as the studio's "mark
//! all read…" does (confirmed by the page; every row of the view, not only
//! those drawn; sent in batches of at most 500 to `POST /api/reading/read`;
//! a batch that fails puts its rows back). Neither the studio nor the
//! desktop offers an undo, so neither does this.
//!
//! The marks go straight to the blyg in batches, not through the outbox,
//! which sends one mark per request: thousands of those would run into the
//! blyg's request budget. Offline, it says so and does nothing.

use std::collections::HashSet;

use blyg_core::api::wire::{READ_BATCH_MAX, ReadMark};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

use crate::api::Request;
use crate::read_api::key_of;
use crate::reading::vm as rvm;
use crate::{Burrow, at, json};

/// The rows (key, version, version read) not read at their version.
pub fn needs_read(rows: &[(String, u32, Option<u32>)]) -> Vec<(String, u32, Option<u32>)> {
    rows.iter()
        .filter(|(_, v, read)| *read != Some(*v))
        .cloned()
        .collect()
}

/// `POST /api/reading/read` requests for `marks`, at most
/// `READ_BATCH_MAX` each.
pub fn read_batches(marks: &[ReadMark]) -> Vec<Request> {
    marks
        .chunks(READ_BATCH_MAX)
        .map(|c| Request {
            method: "POST",
            path: "/api/reading/read".into(),
            body: Some(json!({ "items": c })),
        })
        .collect()
}

/// The read mark for a row key (`sub|remote id`) at `version`.
pub fn mark_of(key: &str, version: u32) -> Option<ReadMark> {
    let (sub, rid) = key.split_once('|')?;
    Some(ReadMark {
        sub: sub.into(),
        remote_id: rid.into(),
        version,
        read_at: None,
    })
}

impl Burrow {
    /// The reading rows the chosen source and lens show, as (key, version,
    /// version read).
    fn view_rows(&self, now_ms: f64) -> Vec<(String, u32, Option<u32>)> {
        let rows = rvm::ordered(self.rd.rows.iter());
        self.wr_filter(rows, at(now_ms))
            .into_iter()
            .map(|r| (key_of(r), r.version, r.read_version))
            .collect()
    }

    /// A row's read state, here only (nothing queued in the outbox).
    fn set_read_quiet(&mut self, key: &str, read: Option<u32>) {
        if let Some(r) = self.rd.rows.iter_mut().find(|r| key_of(r) == key) {
            let changed = r.read_version != read;
            r.read_version = read;
            self.store.read.insert(key.to_string(), read);
            if changed {
                // Read state moves a post in the stream (edited-since-read first).
                self.rows_changed();
            }
            self.reading_dirty = true;
        }
    }
}

#[wasm_bindgen]
impl Burrow {
    /// What "Mark all read" would do here: `{n, label, can, known,
    /// offline}`. `can`: the demo, or a blyg that advertises
    /// `markReadBatch`; `known`: capabilities have been read.
    pub fn mark_all_ask(&self, now_ms: f64) -> String {
        let n = needs_read(&self.view_rows(now_ms)).len();
        json(&json!({
            "n": n,
            "label": self.wr_source_label(),
            "can": !self.is_live() || self.can("markReadBatch"),
            "known": !self.is_live() || self.wr.ops.is_some(),
            "offline": self.is_live() && self.offline,
        }))
    }

    /// Mark every row of the chosen source and lens read, here at once.
    /// Live, returns the batches to send, each with the state to put back
    /// if it fails: `{n, batches: [{req, prior}]}`; or `{error}`.
    pub fn mark_all_read(&mut self, now_ms: f64) -> String {
        if self.is_live() && self.offline {
            return json(
                &json!({ "error": "You're offline. Mark all read when the blyg is back." }),
            );
        }
        if self.is_live() && !self.can("markReadBatch") {
            return json(&json!({ "error": "Your blyg doesn't take read marks in bulk." }));
        }
        let rows = needs_read(&self.view_rows(now_ms));
        let n = rows.len();
        for (key, v, _) in &rows {
            self.set_read_quiet(key, Some(*v));
        }
        if !self.is_live() {
            return json(&json!({ "n": n, "batches": [] }));
        }
        // A mark still waiting in the outbox for one of these would undo it.
        let keys: HashSet<&str> = rows.iter().map(|(k, _, _)| k.as_str()).collect();
        self.store
            .live
            .reads
            .retain(|r| !keys.contains(format!("{}|{}", r.sub, r.remote_id).as_str()));
        let batches: Vec<Value> = rows
            .chunks(READ_BATCH_MAX)
            .map(|chunk| {
                let marks: Vec<ReadMark> = chunk
                    .iter()
                    .filter_map(|(k, v, _)| mark_of(k, *v))
                    .collect();
                let prior: Vec<Value> = chunk.iter().map(|(k, _, p)| json!([k, p])).collect();
                json!({ "req": read_batches(&marks).pop(), "prior": prior })
            })
            .collect();
        json(&json!({ "n": n, "batches": batches }))
    }

    /// A batch failed: its rows go back to how they were (`prior_json` is
    /// the batch's `[[key, version read], …]`).
    pub fn mark_all_rollback(&mut self, prior_json: &str) {
        let prior: Vec<(String, Option<u32>)> =
            serde_json::from_str(prior_json).unwrap_or_default();
        for (k, p) in prior {
            self.set_read_quiet(&k, p);
        }
    }
}
