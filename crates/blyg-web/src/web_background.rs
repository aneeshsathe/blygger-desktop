//! --- background --- Background and Foreground in a browser (studio 0.61,
//! contract 5.10.0; `blyg_core::background`), as the desktop has them.
//!
//! The Background source lists what the studio's Background lens lists
//! (`GET /api/reading?background=true`, every page, read through
//! `web-reading.js` `ensure('background')` after the reading list or your
//! posts were read again); every other source leaves it out. Under an open
//! post: Background / Foreground, Clear my choice, and the whole blyg;
//! ⚙ Reading has the rules. All of it only when the blyg's capabilities
//! list `setReadingPlacement`; an older studio's Background holds withdrawn
//! posts, as before. Every request goes through `net.js` `call`.

use blyg_core::background::{
    BackgroundRules, BackgroundSet, Key, NOT_SERVED, OPERATION, Placement, RULES, placement_body,
    rule_label,
};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

use crate::api::enc;
use crate::read_api::key_of;
use crate::reading::own::is_own_row;
use crate::{Burrow, json};

/// What the open post's Background row needs.
fn not_served() -> String {
    json(&json!({ "error": NOT_SERVED }))
}

impl Burrow {
    /// The blyg serves Background (live, with capabilities read).
    pub(crate) fn bg_served(&self) -> bool {
        self.is_live() && self.can(OPERATION)
    }

    /// Put each row's place on it, from the lens as last read (nothing
    /// before it's read, or on a blyg without it).
    pub(crate) fn bg_annotate(&mut self) {
        let Some(set) = self.wr.bg.take() else {
            return;
        };
        let set = set.indexed();
        if self.bg_served() {
            set.annotate(&mut self.rd.rows);
        } else {
            for r in &mut self.rd.rows {
                r.background = None;
            }
        }
        self.wr.bg = Some(set);
        self.rows_changed();
        self.reading_dirty = true;
    }

    /// The lens's entries, read: kept (with this page's Foreground
    /// choices), and put on the rows.
    pub(crate) fn bg_loaded(&mut self, items: &[Value]) -> usize {
        let set = BackgroundSet::from_entries(items, self.wr.bg.as_ref());
        let n = set.len();
        self.wr.bg = Some(set);
        self.wr.bg_fresh = true;
        self.bg_annotate();
        n
    }

    fn bg_row_key(&self, key: &str) -> Option<(Key, bool, bool, String)> {
        let r = self.rd.rows.iter().find(|r| key_of(r) == key)?;
        Some((
            (r.subscription_id.clone(), r.remote_id.clone()),
            r.state == "tombstone",
            r.is_background(),
            r.subscription_title.clone(),
        ))
    }
}

#[wasm_bindgen]
impl Burrow {
    /// Whether the Background lens should be read now: the blyg serves it
    /// and what's held is stale (the reading list or your posts were read
    /// again, or a choice was made). `web-reading.js` asks on each draw.
    pub fn wr_bg_wanted(&self) -> bool {
        self.bg_served() && !self.wr.bg_fresh
    }

    /// Under an open post: `{served, on, placement, blyg, own, why}`.
    pub fn wr_bg(&self, key: &str) -> String {
        let r = self.rd.rows.iter().find(|r| key_of(r) == key);
        let b = r.and_then(|r| r.background.clone()).unwrap_or_default();
        json(&json!({
            "served": self.bg_served() && r.is_some(),
            "on": r.is_some_and(|r| r.is_background()),
            "placement": b.placement.map(Placement::as_str),
            "blyg": b.blyg,
            "own": r.is_some_and(is_own_row),
            "why": if b.on { b.why() } else { None },
        }))
    }

    /// A Background action on the post `key`: `place` (to Background, or
    /// back to Foreground), `clear` (the rules decide) or `blyg` (its whole
    /// blyg). `{req}` to send, then `wr_bg_done`; `{error}` otherwise.
    pub fn wr_bg_act(&self, key: &str, action: &str) -> String {
        if !self.bg_served() {
            return not_served();
        }
        let Some((k, _, on, _)) = self.bg_row_key(key) else {
            return json(&json!({ "error": "That post isn't here any more" }));
        };
        let req = match action {
            "place" => json!({
                "method": "PUT", "path": "/api/reading/placement",
                "body": placement_body(&k, Some(if on { Placement::Foreground } else { Placement::Background })),
            }),
            "clear" => json!({
                "method": "PUT", "path": "/api/reading/placement",
                "body": placement_body(&k, None),
            }),
            "blyg" if k.0 != blyg_core::OWN_SUBSCRIPTION => {
                let blyg = self
                    .rd
                    .rows
                    .iter()
                    .find(|r| key_of(r) == key)
                    .and_then(|r| r.background.as_ref())
                    .is_some_and(|b| b.blyg);
                json!({
                    "method": "PATCH",
                    "path": format!("/api/subscriptions/{}", enc(&k.0)),
                    "body": { "background": !blyg },
                })
            }
            _ => return json(&json!({ "error": "not a Background action" })),
        };
        json(&json!({ "req": req }))
    }

    /// The answer to `wr_bg_act`'s request: on success the post (or blyg)
    /// moves here at once and the lens is read again; `{toast}` or `{error}`.
    pub fn wr_bg_done(&mut self, key: &str, action: &str, status: u16, body: &str) -> String {
        if !(200..300).contains(&status) {
            let text = crate::api::read(status, body, None)
                .err()
                .map(|e| e.message())
                .unwrap_or_else(|| "Couldn't move it".into());
            return json(&json!({ "error": text }));
        }
        let Some((k, withdrawn, on, title)) = self.bg_row_key(key) else {
            return "{}".into();
        };
        let mut set = self.wr.bg.take().unwrap_or_default();
        let toast = match action {
            "place" => {
                let to = if on {
                    Placement::Foreground
                } else {
                    Placement::Background
                };
                set.place(&k, withdrawn, Some(to));
                if on {
                    "Moved to Foreground"
                } else {
                    "Moved to Background"
                }
                .to_string()
            }
            "clear" => {
                set.place(&k, withdrawn, None);
                "The rules decide again".to_string()
            }
            "blyg" => {
                let now = !set.blyg_on(&k.0);
                set.place_blyg(&k.0, now, &self.rd.rows);
                if now {
                    format!("{title} moved to Background")
                } else {
                    format!("{title} is back in the main lists")
                }
            }
            _ => String::new(),
        };
        self.wr.bg = Some(set);
        self.wr.bg_fresh = false;
        self.bg_annotate();
        json(&json!({ "toast": toast }))
    }

    /// ⚙ Reading's Background card: `{served, known, rules: [{rule, label,
    /// hint, on}]}`.
    pub fn wr_bg_rules(&self) -> String {
        let rules = self.wr.bg_rules.clone().unwrap_or_default();
        json(&json!({
            "served": self.bg_served(),
            "known": self.wr.bg_rules.is_some(),
            "rules": RULES.iter().map(|r| {
                let (label, hint) = rule_label(r);
                json!({ "rule": r, "label": label, "hint": hint, "on": rules.on(r) })
            }).collect::<Vec<_>>(),
        }))
    }

    /// Turn a rule on or off: `{req}` (only that key, a partial settings
    /// write), then `wr_bg_rule_saved`.
    pub fn wr_bg_rule(&self, rule: &str, on: bool) -> String {
        if !self.bg_served() {
            return not_served();
        }
        if !RULES.contains(&rule) {
            return json(&json!({ "error": "not a Background rule" }));
        }
        json(&json!({ "req": {
            "method": "PATCH", "path": "/api/settings", "body": BackgroundRules::patch(rule, on),
        }}))
    }

    /// The settings answer: the rules kept, and the lens read again.
    pub fn wr_bg_rule_saved(&mut self, status: u16, body: &str) -> bool {
        if !(200..300).contains(&status) {
            return false;
        }
        let Some(rules) = serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|v| BackgroundRules::from_settings(&v))
        else {
            return false;
        };
        self.wr.bg_rules = Some(rules);
        self.wr.bg_fresh = false;
        true
    }
}

#[cfg(test)]
#[path = "web_background_tests.rs"]
mod tests;
