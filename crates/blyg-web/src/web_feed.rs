//! --- reader order --- Reading's tabs as studio 0.68 has them, in a
//! browser: the Feed (the All smart feed, renamed) in the blyg's
//! `readerOrder`, pace-layered or chronological; Raw (everything that came
//! in, withdrawn posts too, newest first); the Agentic feed (a placeholder,
//! as upstream); Saved (the shelf, renamed: the `/shelf` API keeps its
//! name). The rules are the app's `reading/feed_vm.rs`, compiled unchanged
//! (`crate::reading::feed`), over blyg-core's pace-layered replay.
//!
//! - The order is the blyg's `Settings.readerOrder` (studio 0.68), read
//!   with the settings at start; a choice here is shown at once and sent
//!   (`PATCH /api/settings {readerOrder}`), and a refused one goes back to
//!   what the blyg keeps. Without the setting (an older blyg, the demo) it
//!   is the studio's default, pace-layered (`ReaderOrder::default`, as
//!   native's), and a choice stays
//!   in this tab.
//! - The places are worked out again on the next draw after the rows
//!   change (`rows_changed`), over every held row, with no cached pins
//!   (the page keeps no changelogs): the app's replay with fewer facts.
//! - Hoppers, Saved, Raw and the Agentic feed keep their own order
//!   (`feed::follows_feed`), as the studio's do. Raw lists from every held
//!   row, withdrawn ones too, which the other sources never show.

use std::cell::{Cell, RefCell};

use blyg_core::ReadingItem;
use blyg_core::order::ReaderOrder;
use blyg_core::pace::RowFacts;
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

use crate::reading::feed::{self, Feed};
use crate::reading::sources_vm::{Smart, Source};
use crate::reading::stream_vm::ReadMode;
use crate::reading::vm::key as vm_key;
use crate::{Burrow, json};

/// How the page shows the stream: beside its sources pane, the app's
/// Reader (Raw and hoppers keep their own order there).
const MODE: ReadMode = ReadMode::Reader;

/// The Feed's order and places, and whether the blyg keeps the order.
#[derive(Debug)]
pub struct WebFeed {
    feed: RefCell<Feed>,
    /// The rows changed since the places were worked out.
    stale: Cell<bool>,
    /// The blyg's settings carry `readerOrder` (studio 0.68).
    kept_on_blyg: bool,
}

impl Default for WebFeed {
    fn default() -> Self {
        WebFeed {
            feed: RefCell::new(Feed::new(ReaderOrder::default())),
            stale: Cell::new(true),
            kept_on_blyg: false,
        }
    }
}

impl WebFeed {
    /// The rows changed: the places are worked out again when next asked.
    pub(crate) fn rows_changed(&self) {
        self.stale.set(true);
    }

    pub(crate) fn order(&self) -> ReaderOrder {
        self.feed.borrow().order
    }
}

impl Burrow {
    /// `GET /api/settings` (or a save's answer): the blyg's `readerOrder`,
    /// when it has one, is the Feed's.
    pub(crate) fn feed_settings(&mut self, settings: &Value) {
        let Some(order) = ReaderOrder::from_settings(settings) else {
            return;
        };
        self.wr.feed.kept_on_blyg = true;
        let f = self.wr.feed.feed.get_mut();
        // The blyg's word, not a choice made here: a save in flight still
        // lands (its answer is the blyg's word too).
        f.order = order;
        f.confirmed = order;
        f.loaded = true;
        self.wr.feed.stale.set(true);
    }

    /// `shown` (the list's rows, in its order) in the Feed's order: by
    /// each row's place when the Feed is pace-layered and `source` follows
    /// it; a row with no place yet (it arrived since) above every place, as
    /// the studio's does (`feed::arrange`, whose rule this is, over
    /// references rather than indices). Otherwise as they are.
    pub(crate) fn feed_arrange<'a>(
        &self,
        mut shown: Vec<&'a ReadingItem>,
        source: &Source,
    ) -> Vec<&'a ReadingItem> {
        if self.wr.feed.order() != ReaderOrder::PaceLayered || !feed::follows_feed(MODE, source) {
            return shown;
        }
        self.feed_places(&self.rd.rows);
        let f = self.wr.feed.feed.borrow();
        // Stable: rows with no place keep the list's order.
        shown.sort_by_key(|r| f.place(&vm_key(r)).map_or(0, |p| p + 1));
        shown
    }

    /// Raw: every row that came in (withdrawn ones too; yours aren't),
    /// one per post, in Raw's own order (`feed::raw_order`).
    pub(crate) fn raw_rows(&self, now: chrono::DateTime<chrono::Utc>) -> Vec<&ReadingItem> {
        let rows = &self.rd.rows;
        let raw = Source::Smart(Smart::Raw);
        let filed = Default::default();
        let mut seen = std::collections::HashSet::new();
        let mut idx: Vec<usize> = (0..rows.len())
            .filter(|&i| {
                let r = &rows[i];
                crate::reading::sources_vm::in_source(r, &raw, &filed, now)
                    && seen.insert(vm_key(r))
            })
            .collect();
        feed::raw_order(rows, &mut idx);
        idx.into_iter().filter_map(|i| rows.get(i)).collect()
    }

    /// Work the Feed's places out again if the rows changed since.
    fn feed_places(&self, rows: &[ReadingItem]) {
        let mut f = self.wr.feed.feed.borrow_mut();
        if !self.wr.feed.stale.get() && !f.held() {
            return;
        }
        let facts = rows
            .iter()
            .map(|r| RowFacts::of(r, feed::is_l0(r, &self.rd.subs)))
            .collect();
        let ticket = f.start_rebuild();
        f.land(ticket, feed::places(facts, &Default::default()));
        self.wr.feed.stale.set(false);
    }

    /// What the order control shows: `{value, label, shown, options}`;
    /// shown where the list follows the Feed.
    pub(crate) fn feed_control(&self) -> Value {
        let order = self.wr.feed.order();
        json!({
            "value": order.as_str(),
            "label": order.label(),
            "shown": feed::follows_feed(MODE, &self.wr.source),
            "options": ReaderOrder::ALL.iter().map(|o| json!({
                "value": o.as_str(),
                "label": o.label(),
                "on": *o == order,
            })).collect::<Vec<_>>(),
        })
    }
}

#[wasm_bindgen]
impl Burrow {
    /// Choose the Feed's order (`pace-layered`, `newest`): shown at once.
    /// Returns `{at, req?}`: on a blyg that keeps it, the `PATCH` to send,
    /// whose answer goes to `wr_reader_order_answer` with `at`.
    pub fn wr_set_reader_order(&mut self, value: &str) -> String {
        let Some(order) = ReaderOrder::parse(value) else {
            return json(&json!({ "error": "No such order" }));
        };
        self.wr.feed.stale.set(true);
        let send = self.wr.feed.kept_on_blyg && self.is_live();
        let f = self.wr.feed.feed.get_mut();
        let at = f.choose(order);
        if !send {
            f.answer(at, Some(order));
            return json(&json!({ "at": at }));
        }
        json(&json!({
            "at": at,
            "req": { "method": "PATCH", "path": "/api/settings", "body": order.patch() },
        }))
    }

    /// The answer to the order's `PATCH` for the choice `at`: kept, the
    /// blyg's word stands; refused, back to the order it keeps. A later
    /// choice's own answer decides instead.
    pub fn wr_reader_order_answer(&mut self, at: f64, status: u16, body: &str) {
        let f = self.wr.feed.feed.get_mut();
        let kept = (200..300).contains(&status).then(|| {
            serde_json::from_str::<Value>(body)
                .ok()
                .as_ref()
                .and_then(ReaderOrder::from_settings)
                .unwrap_or(f.order)
        });
        if let Some(order) = f.answer(at as u64, kept) {
            f.order = order;
            self.wr.feed.stale.set(true);
        }
    }
}
