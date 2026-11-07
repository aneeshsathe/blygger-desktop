//! The Read tab, the shelf, quick capture and the editor bar's link and quote
//! pickers, as more `Burrow` methods. The rules are the native ones: the
//! reading list's order, titles, badges and dates, reply stubs and partial
//! quotes come from `crates/blyg-app/src/reading/vm.rs` (`crate::reading::vm`);
//! the capture counter from `vm::capture_counter`; the shelf's order from
//! `WebStore::arrange`.

use blyg_core::profile::Relation;
use blyg_core::{Item, Kind, LocalId, ReadingItem, RemoteRef, Status, post_key};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::reading::vm as rvm;
use crate::{Burrow, Outcome, at, json, rfc, vm};

/// One reading row's key, `"subscription id|remote id"`.
pub fn key_of(r: &ReadingItem) -> String {
    format!("{}|{}", r.subscription_id, r.remote_id)
}

#[derive(Serialize)]
pub struct ShelfRow {
    id: String,
    title: String,
    kind: &'static str,
    pill: String,
    when: String,
}

#[derive(Serialize)]
struct ReadRow {
    key: String,
    who: String,
    host: String,
    when: String,
    unread: bool,
    badge: Option<String>,
    title: String,
    excerpt: String,
    /// The lineage glyph: `↩` a reply, `⑂` a fork, `❝2` quotes two posts.
    glyph: String,
    hoppers: Vec<String>,
    kind: &'static str,
}

#[derive(Serialize)]
struct ReadView {
    unread: usize,
    rows: Vec<ReadRow>,
    empty: Option<String>,
}

#[derive(Serialize)]
struct Lineage {
    text: String,
    /// The key of the row it points at, when that post is held here.
    key: Option<String>,
}

#[derive(Serialize)]
struct ReadPost {
    key: String,
    title: String,
    by: String,
    html: String,
    lineage: Vec<Lineage>,
    drawn_on_by: Vec<Lineage>,
    hoppers: Vec<String>,
    kind: &'static str,
    unread: bool,
    /// A thread can take a quote; Quote opens a new thread either way.
    web_url: Option<String>,
}

#[derive(Serialize)]
struct Target {
    id: String,
    title: String,
    sub: String,
    label: String,
}

#[derive(Serialize)]
struct HopperRow {
    id: String,
    name: String,
    on: bool,
}

/// The post's text without its markup, for the stream's excerpt: the
/// published HTML's text after the title line.
fn excerpt(r: &ReadingItem, title: &str) -> String {
    let md = r.content_md.trim();
    let body = md
        .lines()
        .skip_while(|l| l.trim().is_empty())
        .skip(usize::from(md.lines().next().map(str::trim) == Some(title)))
        // A quote on its own line is another post, not this one's words.
        .filter(|l| !l.trim_start().starts_with("![["))
        .collect::<Vec<_>>()
        .join("\n");
    let html = blyg_render::render_markdown(&body);
    let text = blyg_render::plain_text_from_html(&html);
    vm::truncate_chars(
        text.split_whitespace().collect::<Vec<_>>().join(" ").trim(),
        220,
    )
}

fn glyph(r: &ReadingItem) -> String {
    let refs = r.references();
    if refs.iter().any(|p| p.relation == Relation::Forks) {
        return "⑂".into();
    }
    if refs.iter().any(|p| p.relation == Relation::Stubs) {
        return "↩".into();
    }
    let quotes = refs
        .iter()
        .filter(|p| p.relation == Relation::Quotes)
        .count();
    if quotes > 0 {
        format!("❝{quotes}")
    } else {
        String::new()
    }
}

fn author(r: &ReadingItem) -> String {
    r.author
        .as_ref()
        .and_then(|a| a.name.clone())
        .unwrap_or_else(|| r.subscription_title.clone())
}

impl Burrow {
    fn row(&self, key: &str) -> Option<&ReadingItem> {
        self.rd.rows.iter().find(|r| key_of(r) == key)
    }

    fn row_by_post(&self, origin: &str, id: &str) -> Option<&ReadingItem> {
        let k = post_key(origin, id);
        self.rd
            .rows
            .iter()
            .find(|r| post_key(&r.origin, &r.remote_id) == k)
    }

    pub(crate) fn shelf_rows(&self, now: chrono::DateTime<chrono::Utc>) -> Vec<ShelfRow> {
        self.store
            .shelf
            .iter()
            .filter_map(|id| self.store.item(id))
            .map(|it| ShelfRow {
                id: it.local_id.0.clone(),
                title: vm::truncate_chars(&vm::item_title(&it), 60),
                kind: vm::kind_label(it.kind),
                pill: vm::pill(&it).0,
                when: vm::relative_time(&it.updated, now),
            })
            .collect()
    }

    /// "Replying to rue.blyg.example.com · title" for a reply stub.
    pub(crate) fn stub_label(&self, it: &Item) -> Option<String> {
        let of = it.stub_of.as_ref()?;
        let host = rvm::host(&of.origin);
        Some(match self.row_by_post(&of.origin, &of.id) {
            Some(r) => format!("↩ Replying to {} · {}", author(r), rvm::post_title(r)),
            None => format!("↩ Replying to {host}"),
        })
    }

    fn open_new(&mut self, id: LocalId) {
        self.new_post = None;
        self.before_new = None;
        self.list.set_query("", self.results(""));
        self.list.select(&id);
        self.touched();
    }

    fn set_read_state(&mut self, key: &str, read: Option<u32>) {
        if let Some(r) = self.rd.rows.iter_mut().find(|r| key_of(r) == key) {
            let changed = r.read_version != read;
            r.read_version = read;
            self.store.read.insert(key.to_string(), read);
            if changed {
                self.queue_read(key, read);
                self.store.live.reading = self.rd.rows.clone();
            }
        }
    }
}

#[wasm_bindgen]
impl Burrow {
    // ------------------------------------------------------------ reading

    /// The stream: the native order (edited-since-read first, withdrawn
    /// posts hidden unless kept), the reading search, unread only or all.
    pub fn reading_view(&self, now_ms: f64, query: &str, unread_only: bool) -> String {
        let now = at(now_ms);
        let rows = rvm::order(self.rd.rows.clone());
        let unread = rows.iter().filter(|r| r.is_unread()).count();
        let shown: Vec<ReadRow> = rows
            .iter()
            .filter(|r| rvm::matches_query(r, query))
            .filter(|r| !unread_only || r.is_unread())
            .map(|r| {
                let title = rvm::post_title(r);
                ReadRow {
                    key: key_of(r),
                    who: author(r),
                    host: rvm::host(&r.origin),
                    when: rvm::when_label(r, now),
                    unread: r.is_unread(),
                    badge: rvm::badge(r),
                    excerpt: excerpt(r, &title),
                    title,
                    glyph: glyph(r),
                    hoppers: r
                        .hoppers
                        .iter()
                        .filter_map(|h| self.rd.hoppers.iter().find(|x| &x.id == h))
                        .map(|h| h.name.clone())
                        .collect(),
                    kind: vm::kind_label(r.kind),
                }
            })
            .collect();
        let empty = shown.is_empty().then(|| {
            if !query.trim().is_empty() {
                rvm::no_match(query)
            } else if unread_only {
                "Nothing unread. ● shows everything again.".into()
            } else {
                "Nothing to read yet".into()
            }
        });
        json(&ReadView {
            unread,
            rows: shown,
            empty,
        })
    }

    /// A post at full size, marked read. Its HTML is the author's, cleaned
    /// by the app's own sanitiser; a feed item with only text is rendered.
    pub fn open_reading(&mut self, key: &str, now_ms: f64) -> String {
        let Some(r) = self.row(key).cloned() else {
            return "null".into();
        };
        let now = at(now_ms);
        self.set_read_state(key, Some(r.version));
        let html = if r.content_html.trim().is_empty() {
            blyg_render::render_markdown(&r.content_md)
        } else {
            crate::studio::sanitize::sanitize_at(&r.content_html, &r.origin)
        };
        let label = |rel: Relation, partial: bool| match (rel, partial) {
            (Relation::Stubs, _) => "↩ replies to",
            (Relation::Forks, _) => "⑂ forked from",
            (Relation::Quotes, true) => "❝ quotes a passage of",
            (Relation::Quotes, false) => "❝ quotes",
        };
        let lineage = r
            .references()
            .into_iter()
            .map(|p| {
                let held = self.row_by_post(&p.origin, &p.id);
                let what = match held {
                    Some(t) => format!("{} · {}", author(t), rvm::post_title(t)),
                    None => rvm::host(&p.origin),
                };
                let v = p.version.map(|v| format!(" v{v}")).unwrap_or_default();
                Lineage {
                    text: format!("{} {what}{v}", label(p.relation, p.partial)),
                    key: held.map(key_of),
                }
            })
            .collect();
        let me = post_key(&r.origin, &r.remote_id);
        let drawn_on_by = self
            .rd
            .rows
            .iter()
            .filter(|o| rvm::visible(o))
            .filter_map(|o| {
                let p = o
                    .references()
                    .into_iter()
                    .find(|p| (p.origin.clone(), p.id.clone()) == me)?;
                let verb = match p.relation {
                    Relation::Stubs => "↩ replied to by",
                    Relation::Forks => "⑂ forked by",
                    Relation::Quotes => "❝ quoted by",
                };
                Some(Lineage {
                    text: format!("{verb} {} · {}", author(o), rvm::post_title(o)),
                    key: Some(key_of(o)),
                })
            })
            .collect();
        json(&ReadPost {
            key: key.to_string(),
            title: rvm::post_title(&r),
            by: format!(
                "{} · {} · {} · v{}",
                author(&r),
                rvm::host(&r.origin),
                rvm::when_label(&r, now),
                r.version
            ),
            html,
            lineage,
            drawn_on_by,
            hoppers: r.hoppers.clone(),
            kind: vm::kind_label(r.kind),
            unread: false,
            web_url: rvm::web_url(&r),
        })
    }

    /// Swipe right (read) or "mark unread".
    pub fn set_read(&mut self, key: &str, read: bool) -> String {
        let Some(v) = self.row(key).map(|r| r.version) else {
            return json(&Outcome::Nothing);
        };
        self.set_read_state(key, read.then_some(v));
        json(&Outcome::Toast {
            message: if read { "Marked read" } else { "Marked unread" }.into(),
        })
    }

    /// ↩ Reply: a stub thread answering the post, quoting `passage` of it
    /// when one is given (a partial quote, spec §16.4), else the whole post.
    /// It opens in the editor with the caret at the end.
    pub fn reply_to(&mut self, key: &str, passage: &str, now_ms: f64) -> String {
        let Some(r) = self.row(key).cloned() else {
            return json(&Outcome::Nothing);
        };
        let body = self
            .quote_md(&r, passage)
            .unwrap_or_else(|| rvm::stub_body(&r.remote_id));
        let stub_of = RemoteRef {
            origin: r.origin.clone(),
            id: r.remote_id.clone(),
            version: r.version,
            cited: None,
        };
        let id = self.store.insert_thread(
            &format!("{}\n\n", body.trim_end()),
            Some(stub_of),
            &rfc(now_ms),
            now_ms as i64,
        );
        self.open_new(id.clone());
        json(&Outcome::Create { id: id.0 })
    }

    /// ❝ Quote: a new thread quoting the post (labelled, as the picker
    /// labels it) or just `passage` of it.
    pub fn quote_new(&mut self, key: &str, passage: &str, now_ms: f64) -> String {
        let Some(r) = self.row(key).cloned() else {
            return json(&Outcome::Nothing);
        };
        let quote = self
            .quote_md(&r, passage)
            .unwrap_or_else(|| rvm::quote_directive(&r, false));
        let id = self.store.insert_thread(
            &format!("\n\n{quote}\n\n"),
            None,
            &rfc(now_ms),
            now_ms as i64,
        );
        self.open_new(id.clone());
        json(&Outcome::Create { id: id.0 })
    }

    /// The Markdown a passage of `r` becomes in a thread, when there is one.
    fn quote_md(&self, r: &ReadingItem, passage: &str) -> Option<String> {
        if passage.trim().is_empty() {
            return None;
        }
        let html = if r.content_html.trim().is_empty() {
            blyg_render::render_markdown(&r.content_md)
        } else {
            r.content_html.clone()
        };
        let source = rvm::QuoteSource::Blyg {
            id: r.remote_id.clone(),
            html,
            title: rvm::post_title(r),
            url: rvm::web_url(r),
        };
        rvm::quote_block(passage, &source, true)
    }

    /// [[ Link: the post as a labelled link, for the clipboard.
    pub fn link_markup(&self, key: &str) -> String {
        match self.row(key) {
            Some(r) => blyg_render::link_source(&r.remote_id, Some(&rvm::quote_label(r))),
            None => String::new(),
        }
    }

    /// The hoppers, and whether `key` is in each.
    pub fn hoppers(&self, key: &str) -> String {
        let on = self.row(key).map(|r| r.hoppers.clone()).unwrap_or_default();
        json(
            &self
                .rd
                .hoppers
                .iter()
                .map(|h| HopperRow {
                    id: h.id.clone(),
                    name: h.name.clone(),
                    on: on.contains(&h.id),
                })
                .collect::<Vec<_>>(),
        )
    }

    pub fn toggle_hopper(&mut self, key: &str, hopper: &str) -> String {
        let Some(name) = self
            .rd
            .hoppers
            .iter()
            .find(|h| h.id == hopper)
            .map(|h| h.name.clone())
        else {
            return json(&Outcome::Nothing);
        };
        let Some(r) = self.rd.rows.iter_mut().find(|r| key_of(r) == key) else {
            return json(&Outcome::Nothing);
        };
        let added = if let Some(ix) = r.hoppers.iter().position(|h| h == hopper) {
            r.hoppers.remove(ix);
            false
        } else {
            r.hoppers.push(hopper.to_string());
            true
        };
        self.store.hopped.insert(key.to_string(), r.hoppers.clone());
        self.queue_hop(key, hopper, added);
        json(&Outcome::Toast {
            message: if added {
                format!("In {name}")
            } else {
                format!("Taken out of {name}")
            },
        })
    }

    // ------------------------------------------------------------ pickers

    /// What [[ link and ❝ quote offer: your published posts, then what you
    /// read, matching `query`.
    pub fn pick_targets(&self, query: &str) -> String {
        let q = query.trim().to_lowercase();
        let own = self
            .store
            .items()
            .into_iter()
            .filter(|i| i.status == Status::Public)
            .filter_map(|i| {
                let sid = i.server_id.clone()?;
                let title = vm::item_title(&i);
                Some(Target {
                    id: sid.0,
                    sub: format!("yours · {}", vm::kind_label(i.kind)),
                    label: blyg_core::pick::pick_label(
                        Some(&title),
                        blyg_core::pick::link_label_md(&i.content_md, i.kind),
                    ),
                    title,
                })
            });
        let theirs = rvm::order(self.rd.rows.clone())
            .into_iter()
            .map(|r| Target {
                id: r.remote_id.clone(),
                title: rvm::post_title(&r),
                sub: format!("{} · {}", author(&r), rvm::host(&r.origin)),
                label: rvm::quote_label(&r),
            });
        let rows: Vec<Target> = own
            .chain(theirs)
            .filter(|t| {
                q.is_empty()
                    || t.title.to_lowercase().contains(&q)
                    || t.sub.to_lowercase().contains(&q)
            })
            .take(40)
            .collect();
        json(&rows)
    }

    /// The markup a pick inserts: `[[id | label]]`, or `![[id | label]]` for
    /// a quote. A quote needs a thread: `Err` toast for a fragment.
    pub fn pick_markup(&self, id: &str, label: &str, quote: bool) -> String {
        if quote && !rvm::can_quote_into(self.current_item().as_ref()) {
            return json(&Outcome::Toast {
                message: "Quotes live in threads. ◦⇄≡ makes this one a thread.".into(),
            });
        }
        let label = Some(label).filter(|l| !l.trim().is_empty());
        let text = if quote {
            blyg_core::grammar::directive_source(id, label)
        } else {
            blyg_render::link_source(id, label)
        };
        json(&serde_json::json!({ "action": "insert", "text": text, "block": quote }))
    }

    // ------------------------------------------------------------ shelf

    /// Shelve the open post, or take it off.
    pub fn toggle_shelf(&mut self) -> String {
        let Some(id) = self.current_id() else {
            return json(&Outcome::Toast {
                message: "Open a post to shelve it".into(),
            });
        };
        let on = !self.store.shelf.contains(&id);
        match self.store.set_on_shelf(&id, on) {
            Ok(_) => {
                self.keep_open(&id);
                json(&Outcome::Toast {
                    message: if on {
                        "On your shelf 📚"
                    } else {
                        "Taken off your shelf"
                    }
                    .into(),
                })
            }
            Err(message) => json(&Outcome::Toast { message }),
        }
    }

    pub fn set_on_shelf(&mut self, id: &str, on: bool) -> String {
        let id = LocalId(id.into());
        match self.store.set_on_shelf(&id, on) {
            Ok(_) => {
                self.refresh();
                json(&Outcome::Nothing)
            }
            Err(message) => json(&Outcome::Toast { message }),
        }
    }

    pub fn move_on_shelf(&mut self, id: &str, index: usize) {
        self.store.move_on_shelf(&LocalId(id.into()), index);
        self.refresh();
    }

    /// Open a shelved post, clearing a search that hides it.
    pub fn open_from_shelf(&mut self, id: &str) {
        let id = LocalId(id.into());
        self.leave_new_post();
        if !self.list.results().iter().any(|i| i.local_id == id) {
            self.list.set_query("", self.results(""));
        }
        self.list.select(&id);
    }

    // ------------------------------------------------------------ capture

    /// The capture sheet's counter: `fragment · 214 / 1000` and its level.
    pub fn capture_counter(text: &str) -> String {
        let (label, level) = vm::capture_counter(blyg_core::published_len(text));
        json(&serde_json::json!({ "label": label, "level": crate::level_class(level) }))
    }

    /// Quick capture: keep the text as a scratch note, or publish it (a
    /// long one as a thread, as promotion chooses). Offline, publishing
    /// keeps it as a note.
    pub fn capture(&mut self, text: &str, publish: bool, now_ms: f64) -> String {
        if text.trim().is_empty() {
            return json(&Outcome::Toast {
                message: "Nothing to keep yet".into(),
            });
        }
        let now = rfc(now_ms);
        if !publish || self.offline {
            self.store
                .insert(Kind::Fragment, text, Status::Scratch, &now, now_ms as i64);
            self.refresh();
            return json(&Outcome::Toast {
                message: if publish {
                    "Offline, so it's kept as a note. Publish it when you're back."
                } else {
                    "Kept as a note in this browser"
                }
                .into(),
            });
        }
        let kind = blyg_core::promotion_kind(Kind::Fragment, text);
        let id = self
            .store
            .insert(kind, text, Status::Draft, &now, now_ms as i64);
        if self.is_live() {
            self.refresh();
            return json(&Outcome::RemotePublish {
                id: id.0,
                note: None,
            });
        }
        let done = self.store.publish(&id, None, &now);
        self.refresh();
        match done {
            Some(it) => json(&Outcome::Published {
                version: it.version,
                permalink: it.permalink.unwrap_or_default(),
                note: None,
            }),
            None => json(&Outcome::Nothing),
        }
    }

    /// For the phone's editor: the sample blyg's address for a permalink.
    pub fn origin(&self) -> String {
        self.origin_str()
    }
}
