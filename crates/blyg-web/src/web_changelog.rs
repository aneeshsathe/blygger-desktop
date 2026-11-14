//! --- reader view / changelog --- Two of the reader's choices in a
//! browser, decided here and drawn by `www/web-reading.js`:
//!
//! - **Open posts on the author's site**: the post's own page (its `page`,
//!   `reading/vm.rs` `page_url`), opened in a new tab (`rel=noopener`); the
//!   choice itself is this browser's (`localStorage`), like the desktop's
//!   config key, with an override per subscription.
//! - **Changelog display** (studio 0.58, contract 5.8.0;
//!   `blyg_core::changelog`): what an opened post shows of its changelog,
//!   as the blyg's `readerChangelogDisplay` says: the latest note (as
//!   before), every note, or the blyg's AI summary, labelled with the model
//!   and "not the author's words". A summary is asked for only when a post
//!   is opened, never for a card. Settings saves both choices on the blyg.
//!
//! #96: nothing here routes, sorts or hides a post by its note.

use blyg_core::changelog::{
    ChangelogDisplay, ChangelogSettings, ChangelogSummary, Shown, history_row, parse_history,
    serves_changelog, shown,
};
use blyg_core::{ReadingItem, SubscriptionKind};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

use crate::api::enc;
use crate::read_api::key_of;
use crate::reading::own::is_own_row;
use crate::reading::vm as rvm;
use crate::{Burrow, json};

impl Burrow {
    fn cl_row(&self, key: &str) -> Option<&ReadingItem> {
        self.rd.rows.iter().find(|r| key_of(r) == key)
    }

    /// A blyg post (not a feed's): only those have a changelog to show.
    fn cl_blyg(&self, r: &ReadingItem) -> bool {
        is_own_row(r)
            || self
                .rd
                .subs
                .iter()
                .find(|s| s.id == r.subscription_id)
                .is_none_or(|s| s.kind == SubscriptionKind::Blyg)
    }

    /// The blyg has changelog display (live: its contract says 5.8; the
    /// demo always does).
    fn cl_served(&self) -> bool {
        if !self.is_live() {
            return true;
        }
        self.wr.contract.as_deref().is_some_and(serves_changelog)
    }

    fn cl_mode(&self) -> ChangelogDisplay {
        self.wr.changelog.reader_mode()
    }
}

#[wasm_bindgen]
impl Burrow {
    /// The open post's own page: `{page, sub}` (`page` null: it has none).
    pub fn wr_post_site(&self, key: &str) -> String {
        let r = self.cl_row(key);
        json(&json!({
            "page": r.filter(|r| r.state != "tombstone").and_then(rvm::page_url),
            "sub": r.map(|r| r.subscription_id.clone()),
            "subTitle": r.map(|r| r.subscription_title.clone()),
        }))
    }

    /// The changelog settings as known: `{served, reader, page, modes:
    /// [{value, label, hint}]}`.
    pub fn wr_changelog_settings(&self) -> String {
        let s = &self.wr.changelog;
        json(&json!({
            "served": self.cl_served() && (s.served() || !self.is_live()),
            "known": s.served() || !self.is_live(),
            "reader": s.reader.unwrap_or_default().as_str(),
            "page": s.page.unwrap_or_default().as_str(),
            "modes": ChangelogDisplay::ALL.iter().map(|d| json!({
                "value": d.as_str(), "label": d.label(), "hint": d.hint(),
            })).collect::<Vec<_>>(),
        }))
    }

    /// Choose `value` for `which` (`reader` or `page`). Live: the request
    /// to send (`{req}`), then `wr_changelog_saved` with the answer; the
    /// demo keeps it here (`{}`). `{error}` for a value that isn't one.
    pub fn wr_set_changelog_display(&mut self, which: &str, value: &str) -> String {
        let Some(d) = ChangelogDisplay::parse(value) else {
            return json(&json!({ "error": "not a changelog display" }));
        };
        let patch = match which {
            "reader" => ChangelogSettings {
                reader: Some(d),
                ..Default::default()
            },
            "page" => ChangelogSettings {
                page: Some(d),
                ..Default::default()
            },
            _ => return json(&json!({ "error": "not a changelog setting" })),
        };
        if !self.is_live() {
            // The demo: kept in this page.
            match which {
                "reader" => self.wr.changelog.reader = Some(d),
                _ => self.wr.changelog.page = Some(d),
            }
            return "{}".into();
        }
        json(
            &json!({ "req": { "method": "PATCH", "path": "/api/settings", "body": patch.to_json() } }),
        )
    }

    /// The answer to the settings save: kept when it's the blyg's settings.
    pub fn wr_changelog_saved(&mut self, status: u16, body: &str) -> bool {
        if !(200..300).contains(&status) {
            return false;
        }
        let Ok(v) = serde_json::from_str::<Value>(body) else {
            return false;
        };
        let s = ChangelogSettings::from_json(&v);
        if s.served() {
            self.wr.changelog = s;
        }
        true
    }

    /// The request for the open post's changelog (`{path}`), or `null` when
    /// the mode is the latest note or the post has none to show.
    pub fn wr_changelog_ask(&self, key: &str) -> String {
        let Some(r) = self.cl_row(key) else {
            return "null".into();
        };
        if self.cl_mode() == ChangelogDisplay::Latest
            || !self.cl_served()
            || r.state == "tombstone"
            || !self.cl_blyg(r)
        {
            return "null".into();
        }
        let path = if is_own_row(r) {
            format!("/api/items/{}/versions?limit=100", enc(&r.remote_id))
        } else {
            format!(
                "/api/imports/{}/{}/history",
                enc(&r.subscription_id),
                enc(&r.remote_id)
            )
        };
        json(&json!({ "path": path, "version": r.version }))
    }

    /// The summary request for the open post (`{method, path, body}`), or
    /// `null` when the mode isn't summary or its changelog (`history`, the
    /// answer to `wr_changelog_ask`) says nothing beyond its first version.
    pub fn wr_summary_ask(&self, key: &str, history: &str) -> String {
        let Some(r) = self.cl_row(key) else {
            return "null".into();
        };
        if self.cl_mode() != ChangelogDisplay::Summary || !self.cl_served() {
            return "null".into();
        }
        let log = serde_json::from_str::<Value>(history)
            .map(|v| parse_history(&v))
            .unwrap_or_default();
        if !matches!(
            shown(ChangelogDisplay::Summary, &log, r.version),
            Shown::Summary(_)
        ) {
            return "null".into();
        }
        let body = if is_own_row(r) {
            json!({ "itemId": r.remote_id })
        } else {
            json!({ "subscriptionId": r.subscription_id, "remoteId": r.remote_id })
        };
        json(&json!({ "method": "POST", "path": "/api/changelog-summary", "body": body }))
    }

    /// What to draw above the open post: `{kind: none|history|summary|
    /// pending, heading, rows, summary, label, why}`. `history` is the
    /// changelog's answer (or empty); `summary_status`/`summary_body` the
    /// summary's (`0`: not asked, or not answered yet). Text is plain:
    /// the page escapes it.
    pub fn wr_changelog_view(
        &self,
        key: &str,
        history: &str,
        summary_status: u16,
        summary_body: &str,
    ) -> String {
        let none = || json(&json!({ "kind": "none" }));
        let Some(r) = self.cl_row(key) else {
            return none();
        };
        let Ok(v) = serde_json::from_str::<Value>(history) else {
            return none();
        };
        let log = parse_history(&v);
        let rows = |lines: &[blyg_core::changelog::NoteLine]| {
            lines.iter().map(history_row).collect::<Vec<_>>()
        };
        match shown(self.cl_mode(), &log, r.version) {
            Shown::Nothing | Shown::Latest { .. } => none(),
            Shown::History(lines) => json(&json!({
                "kind": "history", "heading": "Changelog", "rows": rows(&lines),
            })),
            Shown::Summary(lines) => {
                let got = (200..300)
                    .contains(&summary_status)
                    .then(|| serde_json::from_str::<Value>(summary_body).ok())
                    .flatten()
                    .and_then(|v| ChangelogSummary::from_json(&v));
                match got {
                    Some(s) => json(&json!({
                        "kind": "summary", "heading": "Changes so far",
                        "summary": s.summary, "label": s.label(), "model": s.model,
                    })),
                    None => {
                        let why = if summary_status == 0 {
                            "Summarizing the changelog…".to_string()
                        } else {
                            let e = crate::api::read(summary_status, summary_body, None)
                                .err()
                                .map(|e| e.to_json()["text"].as_str().unwrap_or("").to_string())
                                .filter(|t| !t.is_empty())
                                .unwrap_or_else(|| "it couldn't be written".into());
                            format!("No summary: {e}")
                        };
                        json(&json!({
                            "kind": "pending", "why": why, "heading": "Changelog", "rows": rows(&lines),
                        }))
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::Burrow;

    const NOW: f64 = 1_793_966_400_000.0;

    fn v(s: String) -> Value {
        serde_json::from_str(&s).unwrap()
    }

    fn live() -> Burrow {
        let mut b = Burrow::new_live(NOW, None, None, "https://blyg.example.com");
        let rows = json!([{
            "subscriptionId": "a", "remoteId": "a1", "subscriptionTitle": "Ada",
            "origin": "https://ada.blyg.example.com/", "kind": "fragment", "version": 2,
            "created": "2026-11-04T09:00:00Z", "updated": "2026-11-04T09:00:00Z",
            "observedAt": "2026-11-04T09:05:00Z", "contentMd": "Tides again.",
            "contentHtml": "<p>Tides again.</p>", "readVersion": null, "hoppers": [],
            "page": "f/a1"
        }]);
        b.live_reading(&rows.to_string(), "[]");
        b.wr_set_contract("5.8.0");
        b
    }

    fn history() -> String {
        json!({"current": 2, "withdrawn": false, "changelog": [
            {"version": 1, "at": "2026-11-01T00:00:00Z", "note": "first", "pinned": true, "generated": false},
            {"version": 2, "at": "2026-11-02T00:00:00Z", "note": null, "pinned": false, "generated": false}
        ]})
        .to_string()
    }

    fn set_mode(b: &mut Burrow, mode: &str) {
        let body = json!({ "readerChangelogDisplay": mode, "changelogDisplay": "latest" });
        assert!(b.wr_changelog_saved(200, &body.to_string()));
    }

    #[test]
    fn the_page_opens_on_the_authors_site() {
        let b = live();
        let s = v(b.wr_post_site("a|a1"));
        assert_eq!(s["page"], "https://ada.blyg.example.com/f/a1");
        assert_eq!(s["sub"], "a");
        assert_eq!(v(b.wr_post_site("nope|x"))["page"], Value::Null);
    }

    #[test]
    fn latest_asks_for_nothing() {
        let mut b = live();
        set_mode(&mut b, "latest");
        assert_eq!(b.wr_changelog_ask("a|a1"), "null");
        assert_eq!(b.wr_summary_ask("a|a1", &history()), "null");
        // A studio before 5.8: nothing either.
        let mut old = live();
        old.wr_set_contract("5.7.0");
        set_mode(&mut old, "history");
        assert_eq!(old.wr_changelog_ask("a|a1"), "null");
    }

    #[test]
    fn history_lists_every_note_newest_first() {
        let mut b = live();
        set_mode(&mut b, "history");
        let ask = v(b.wr_changelog_ask("a|a1"));
        assert_eq!(ask["path"], "/api/imports/a/a1/history");
        assert_eq!(b.wr_summary_ask("a|a1", &history()), "null");
        let view = v(b.wr_changelog_view("a|a1", &history(), 0, ""));
        assert_eq!(view["kind"], "history");
        assert_eq!(view["rows"][0], "v2 · 2026-11-02 · no note");
        assert_eq!(view["rows"][1], "v1 · 2026-11-01 · “first” · 📌 pinned");
    }

    #[test]
    fn a_summary_is_asked_on_open_and_labelled() {
        let mut b = live();
        set_mode(&mut b, "summary");
        let ask = v(b.wr_summary_ask("a|a1", &history()));
        assert_eq!(ask["path"], "/api/changelog-summary");
        assert_eq!(
            ask["body"],
            json!({"subscriptionId": "a", "remoteId": "a1"})
        );
        let pending = v(b.wr_changelog_view("a|a1", &history(), 0, ""));
        assert_eq!(pending["kind"], "pending");
        assert_eq!(pending["why"], "Summarizing the changelog…");
        let answer = json!({"summary": "A first pinned version, then an unannounced change.",
                            "model": "@cf/example/model", "version": 2, "cached": false});
        let done = v(b.wr_changelog_view("a|a1", &history(), 200, &answer.to_string()));
        assert_eq!(done["kind"], "summary");
        assert!(
            done["label"]
                .as_str()
                .unwrap()
                .contains("@cf/example/model")
        );
        assert!(
            done["label"]
                .as_str()
                .unwrap()
                .contains("not the author's words")
        );
        let failed = v(b.wr_changelog_view(
            "a|a1",
            &history(),
            502,
            &json!({"error": "no key for that provider"}).to_string(),
        ));
        assert_eq!(failed["kind"], "pending");
        assert!(failed["why"].as_str().unwrap().starts_with("No summary"));
        // Only one version and no note: nothing to summarize, not asked.
        let one = json!({"current": 1, "changelog": [
            {"version": 1, "at": "2026-11-01T00:00:00Z", "note": null, "pinned": false}
        ]});
        assert_eq!(b.wr_summary_ask("a|a1", &one.to_string()), "null");
    }

    #[test]
    fn the_settings_save_is_a_patch_live_and_kept_in_the_demo() {
        let mut b = live();
        let r = v(b.wr_set_changelog_display("reader", "history"));
        assert_eq!(r["req"]["method"], "PATCH");
        assert_eq!(
            r["req"]["body"],
            json!({"readerChangelogDisplay": "history"})
        );
        assert!(v(b.wr_set_changelog_display("reader", "all"))["error"].is_string());
        let mut demo = Burrow::new(NOW, None);
        assert_eq!(demo.wr_set_changelog_display("page", "summary"), "{}");
        assert_eq!(v(demo.wr_changelog_settings())["page"], "summary");
    }
}
