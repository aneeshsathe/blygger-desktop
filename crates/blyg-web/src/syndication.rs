//! --- syndication --- Copies elsewhere in the web app (decision #98;
//! `blyg_core::syndication` has the wire shapes and rules).
//!
//! - Reading: an imported post's entries, as its blyg served them with the
//!   row, under the byline: "Also posted at host". The text is escaped, and
//!   an entry is a link only after an `https:` check. Nothing is fetched
//!   from the copies, nothing is folded or hidden, no count is shown.
//! - Your published post (⋯ › Copies elsewhere…): the list as the blyg
//!   has it (`GET /api/items/:id/syndication`), add an `https:` address (the
//!   version defaults to the current one), remove one (the page confirms
//!   first). Each change is one `PUT …/syndication {entries,
//!   expectedRevision}`, an in-place rewrite: no new version.
//!
//! The reading rows' entries are kept here, beside the rows (`note_rows`,
//! from `live_reading`), so the reading model's own types don't change.

use std::cell::RefCell;
use std::collections::HashMap;

use blyg_core::syndication::{self as synd, SyndicationEntry};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

use crate::api::Request;
use crate::{Burrow, json};

thread_local! {
    /// Reading row key (`"sub|remote"`) → its served entries.
    static READING: RefCell<HashMap<String, Vec<SyndicationEntry>>> = RefCell::new(HashMap::new());
}

/// Hook (`live_reading`): keep each raw row's entries (none on an endcap).
pub fn note_rows(rows: &[Value]) {
    let map: HashMap<String, Vec<SyndicationEntry>> = rows
        .iter()
        .filter(|r| !r.get("withdrawn").and_then(Value::as_bool).unwrap_or(false))
        .filter_map(|r| {
            let sub = r.get("subscriptionId")?.as_str()?;
            let rid = r.get("remoteId")?.as_str()?;
            let list = synd::parse_member(r.get(synd::MEMBER))?;
            (!list.is_empty()).then(|| (format!("{sub}|{rid}"), list))
        })
        .collect();
    READING.with(|m| *m.borrow_mut() = map);
}

/// Text for HTML: `&`, `<`, `>`, `"` and `'` escaped.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// "Also posted at …" for `entries`: `https:` ones as links (the host as
/// text), the rest left out; `""` when none is left.
pub fn copies_html(entries: &[SyndicationEntry]) -> String {
    let links: Vec<String> = entries
        .iter()
        .filter(|e| synd::is_https(&e.url))
        .filter(|e| !e.host().is_empty())
        .map(|e| {
            format!(
                r#"<a href="{}" target="_blank" rel="noopener noreferrer nofollow ugc">{} ↗</a>"#,
                esc(&e.url),
                esc(&e.host())
            )
        })
        .collect();
    if links.is_empty() {
        return String::new();
    }
    format!(
        r#"<div class="also-posted" title="Where the author says they posted this too. Not checked or fetched.">Also posted at {}</div>"#,
        links.join(" · ")
    )
}

fn err(text: impl Into<String>) -> String {
    json(&json!({ "error": { "text": text.into() } }))
}

fn rows(entries: &[SyndicationEntry]) -> Vec<Value> {
    entries
        .iter()
        .map(|e| json!({ "url": e.url, "host": e.host(), "version": e.version }))
        .collect()
}

#[wasm_bindgen]
impl Burrow {
    /// The reading post `key`'s copies elsewhere, as safe HTML (`""`: none).
    pub fn reading_copies_html(&self, key: &str) -> String {
        READING.with(|m| {
            m.borrow()
                .get(key)
                .map(|l| copies_html(l))
                .unwrap_or_default()
        })
    }

    /// Before the Copies sheet: the post on screen, published, and where to
    /// read its list (`{id, title, current, path}`), or `{error}`.
    pub fn w_copies_begin(&self) -> String {
        let Some(it) = self.current_item() else {
            return err("Open a post first");
        };
        if it.status != blyg_core::Status::Public {
            return err("Publish it first: only a published post has copies elsewhere");
        }
        if !self.is_live() {
            return err("Copies elsewhere live on your blyg: connect it first");
        }
        let Some(sid) = &it.server_id else {
            return err("That post isn't on the blyg yet");
        };
        json(&json!({
            "id": it.local_id.0,
            "title": crate::vm::truncate_chars(&crate::vm::item_title(&it), 48),
            "current": it.version,
            "path": synd::route(&sid.0),
        }))
    }

    /// `GET …/syndication`'s answer: `{entries, revision, served}`;
    /// `served: false` (and no entries) when the blyg has no such route.
    pub fn w_copies_view(&self, status: u16, body: &str) -> String {
        if status == 404 || status == 405 {
            return json(&json!({ "entries": [], "revision": 0, "served": false }));
        }
        let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
        match (status, synd::parse_answer(&v)) {
            (200, Some(a)) => json(&json!({
                "entries": rows(&a.entries),
                "revision": a.revision,
                "served": true,
            })),
            _ => err("Couldn't read the list from your blyg"),
        }
    }

    /// The write for the list `entries_json` (`[{url, version}]`, as shown)
    /// with `remove` taken out (`""`: none) and `add_url` at `add_version`
    /// added (`""`: none; a blank version is `current`), on the item's
    /// `revision`: `{req}`, or `{error}` for an address that isn't
    /// `https:` or a version out of range.
    #[allow(clippy::too_many_arguments)]
    pub fn w_copies_put(
        &self,
        path: &str,
        entries_json: &str,
        remove: &str,
        add_url: &str,
        add_version: &str,
        current: u32,
        revision: f64,
    ) -> String {
        let shown: Vec<SyndicationEntry> = serde_json::from_str::<Value>(entries_json)
            .ok()
            .and_then(|v| synd::parse_member(Some(&v)))
            .unwrap_or_default();
        let mut list = if remove.is_empty() {
            shown
        } else {
            synd::edit(&shown, Some(remove), None)
        };
        if !add_url.trim().is_empty() {
            let url = match synd::check_url(add_url) {
                Ok(u) => u,
                Err(why) => return err(why),
            };
            let v = add_version.trim().trim_start_matches(['v', 'V']);
            let version = if v.is_empty() {
                current.max(1)
            } else {
                match v.parse::<u32>() {
                    Ok(n) if n >= 1 && n <= current.max(1) => n,
                    _ => {
                        return err(format!(
                            "The version is a number from 1 to {}",
                            current.max(1)
                        ));
                    }
                }
            };
            if list.len() >= synd::MAX_ENTRIES && !list.iter().any(|e| e.url == url) {
                return err(format!("A post lists at most {} copies", synd::MAX_ENTRIES));
            }
            list = synd::edit(
                &list,
                Some(&url),
                Some(SyndicationEntry::new(url.clone(), version)),
            );
        }
        // Only one of your items' lists.
        if !path.starts_with("/api/items/")
            || !path.ends_with("/syndication")
            || path.contains("..")
        {
            return err("That isn't one of your posts");
        }
        json(&json!({ "req": Request {
            method: "PUT",
            path: path.to_string(),
            body: Some(synd::put_body(&list, revision.max(0.0) as u64)),
        }}))
    }

    /// The `PUT`'s answer for post `id`, sent on `sent_revision`:
    /// `{entries, revision}` as the blyg keeps them, or `{error}` (404/405:
    /// the blyg doesn't keep copies elsewhere yet; 409: changed elsewhere,
    /// or a version or post it won't list). The post's revision held here
    /// follows the answer when it was the one sent.
    pub fn w_copies_answer(
        &mut self,
        id: &str,
        sent_revision: f64,
        status: u16,
        body: &str,
    ) -> String {
        let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
        let message = || {
            v.get("error")
                .and_then(|e| e.get("message").or(Some(e)))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        match status {
            200..=299 => match synd::parse_answer(&v) {
                Some(a) => {
                    let held = self.store.live.rev.get(id).copied();
                    if a.revision > 0 && held == Some(sent_revision as u64) {
                        self.store.live.rev.insert(id.to_string(), a.revision);
                    }
                    json(&json!({ "entries": rows(&a.entries), "revision": a.revision }))
                }
                None => err("Your blyg answered in a shape Burrow doesn't know"),
            },
            404 | 405 => {
                err("Your blyg doesn't keep copies elsewhere yet (it needs a newer studio)")
            }
            400 | 422 => err(format!("Your blyg refused that address. {}", message())),
            409 if v.to_string().contains("revision") => {
                err("The post changed elsewhere: close this and open it again")
            }
            409 => err(format!("Your blyg didn't take that. {}", message())),
            _ => err("Couldn't save the list; try again"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_are_escaped_and_https_only() {
        let html = copies_html(&[
            SyndicationEntry::new("https://social.example.net/@a/1?x=\"><script>", 2),
            SyndicationEntry::new("javascript:alert(1)", 1),
            SyndicationEntry::new("http://plain.example.net/1", 1),
        ]);
        assert!(html.contains("Also posted at"));
        assert!(!html.contains("<script>"), "{html}");
        assert!(!html.contains("javascript:"));
        assert!(!html.contains("plain.example.net"));
        assert!(html.contains(r#"rel="noopener noreferrer nofollow ugc""#));
        assert_eq!(copies_html(&[]), "");
        assert_eq!(
            copies_html(&[SyndicationEntry::new("http://x.example/", 1)]),
            ""
        );
    }

    #[test]
    fn rows_are_noted_by_key_and_endcaps_carry_none() {
        note_rows(&[
            json!({"subscriptionId": "s", "remoteId": "r1",
                   "syndication": [{"url": "https://bsky.example/p/1", "version": 1}]}),
            json!({"subscriptionId": "s", "remoteId": "r2", "withdrawn": true,
                   "syndication": [{"url": "https://bsky.example/p/2", "version": 1}]}),
            json!({"subscriptionId": "s", "remoteId": "r3"}),
        ]);
        READING.with(|m| {
            let m = m.borrow();
            assert_eq!(m.len(), 1);
            assert_eq!(m["s|r1"][0].url, "https://bsky.example/p/1");
        });
    }

    #[test]
    fn text_escapes() {
        assert_eq!(
            esc(r#"<a href="x">'&'"#),
            "&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;"
        );
    }
}
