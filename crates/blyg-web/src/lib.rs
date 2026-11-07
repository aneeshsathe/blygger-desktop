//! Burrow in a browser. The GPUI window can't run in a browser yet (see
//! `README.md`), so this crate compiles the parts of Burrow that aren't
//! pixels to WebAssembly and lets a small page draw them:
//!
//! - the model (`blyg_core::Item`, `Kind`, `Status`, `plain_title`, …);
//! - the view-model, `crates/blyg-app/src/vm.rs`, included as is: the NV
//!   list/omnibar state machine (`ListModel`), the counter, banner, pills,
//!   sync and version labels, highlight ranges, relative time, publish,
//!   delete and discard rules;
//! - the preview: `blyg_render::render_preview` with the app's own quote
//!   resolver and snapshot sanitiser (`crates/blyg-app/src/studio/`);
//! - the app's themes (`crates/blyg-core/themes/`).
//!
//! The data is the native fake's invented sample posts, kept in memory and
//! persisted by the page to IndexedDB. Nothing here opens a socket.

use blyg_core::{Item, Kind, LocalId, Status, SyncStatus};
use chrono::{DateTime, Utc};
use serde::Serialize;
use wasm_bindgen::prelude::*;

/// `vm.rs` calls `crate::keymap::{hint, hint_owned, keyless_here}`.
mod keymap;
mod read_api;
pub mod seed;
pub mod store;
pub mod theme;

/// The native app's view-model, compiled unchanged.
#[path = "../../blyg-app/src/vm.rs"]
#[allow(dead_code)]
pub mod vm;

pub mod studio;

/// The native app's reading view-model, compiled unchanged.
pub mod reading;

/// `crate::app::notes::link`, which the reading view-model calls: a copy of
/// `crates/blyg-app/src/notes/mod.rs` `link` (that module also holds the
/// notes drawer, which needs GPUI).
pub mod app {
    pub mod notes {
        use crate::vm;

        /// `[title](url)`: brackets in the title escaped, newlines folded, a
        /// destination with parentheses wrapped in `<…>`.
        pub fn link(title: &str, url: &str) -> String {
            let t: String = title
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .replace('\\', "\\\\");
            let t = if t.is_empty() { url.to_string() } else { t };
            format!(
                "[{}]({})",
                vm::escape_link_text(&t),
                vm::link_destination(url.trim())
            )
        }
    }
}

/// `crate::fake::reading_seed`, for the reading view-model's own tests.
#[cfg(test)]
pub mod fake {
    pub use crate::seed::reading_seed;
}

use store::WebStore;
use studio::resolver::{StoreResolver, render_kind};
use vm::{EnterAction, ListModel, PublishDecision};

/// The preview's own stylesheet when no blyg's is at hand. Copied from
/// `crates/blyg-app/src/studio/style_cache.rs` (`BUILTIN_CSS`), which also
/// reads the filesystem and so isn't included here.
pub const BUILTIN_CSS: &str = r#"
:root { --paper: #fffff8; --ink: #111; --muted: #6b665d; --accent: #a4271b; --rule: #dcd6c9; --wash: rgba(17,17,17,0.04); color-scheme: light dark; }
@media (prefers-color-scheme: dark) {
  :root { --paper: #161513; --ink: #e4dfd3; --muted: #9a9489; --accent: #e0775a; --rule: #2f2d29; --wash: rgba(228,223,211,0.05); }
}
html { background: var(--paper); }
body { margin: 0; padding: 30px 38px 60px; background: var(--paper); color: var(--ink);
  font: 17px/1.7 Literata, "Iowan Old Style", Charter, Georgia, serif; -webkit-font-smoothing: antialiased; }
article { max-width: 36em; margin: 0 auto; }
a { color: var(--accent); text-decoration-thickness: 1px; text-underline-offset: 2px; }
h1, h2, h3, h4 { font-weight: 400; font-style: italic; line-height: 1.2; margin: 1.4em 0 0.5em; }
h1 { font-size: 1.8rem; } h2 { font-size: 1.5rem; } h3 { font-size: 1.25rem; }
p, ul, ol, pre, table, figure { margin: 0 0 0.9em; }
.item-content > :first-child { margin-top: 0; }
blockquote { margin: 1rem 0; padding: 0.6rem 0.9rem; border-left: 3px solid var(--rule); background: var(--wash); border-radius: 0 4px 4px 0; }
blockquote > :last-child { margin-bottom: 0; }
blockquote.blyg-transclusion { font-size: 0.94rem; }
.blyg-provenance, blockquote.blyg-transclusion > p:last-child small { color: var(--muted); font: 12px/1.4 Inter, -apple-system, system-ui, sans-serif; }
code, pre { font: 0.86em/1.5 "SF Mono", Menlo, monospace; }
pre { padding: 0.7em 0.9em; overflow-x: auto; background: var(--wash); border-radius: 4px; }
hr { border: 0; border-top: 1px solid var(--rule); margin: 1.6em 0; }
img { max-width: 100%; height: auto; }
table { border-collapse: collapse; } th, td { border-bottom: 1px solid var(--rule); padding: 0.2em 0.6em; text-align: left; }
figcaption { color: var(--muted); font: 12px/1.4 Inter, -apple-system, system-ui, sans-serif; margin-top: 4px; word-break: break-all; }
.stub-cite { color: var(--muted); font-size: 0.9rem; }
"#;

#[wasm_bindgen(start)]
pub fn start() {
    #[cfg(target_family = "wasm")]
    console_error_panic_hook::set_once();
}

fn at(ms: f64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(ms as i64).unwrap_or_default()
}

fn rfc(ms: f64) -> String {
    at(ms).to_rfc3339()
}

fn json<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "null".into())
}

/// The status bar's dot, as a class name for the page.
fn dot_class(d: vm::SyncDot) -> &'static str {
    match d {
        vm::SyncDot::Green => "green",
        vm::SyncDot::Amber => "amber",
        vm::SyncDot::Busy => "busy",
        vm::SyncDot::Grey => "grey",
        vm::SyncDot::Red => "red",
    }
}

fn level_class(l: vm::Level) -> &'static str {
    match l {
        vm::Level::Normal => "",
        vm::Level::Warn => "warn",
        vm::Level::Over => "over",
    }
}

/// `text` HTML-escaped, with every match of `query` in `<mark>`.
fn highlighted(text: &str, query: &str) -> String {
    let mut out = String::new();
    let mut last = 0;
    for r in vm::highlight_ranges(text, query) {
        out.push_str(&blyg_render::escape_html(&text[last..r.start]));
        out.push_str("<mark>");
        out.push_str(&blyg_render::escape_html(&text[r.clone()]));
        out.push_str("</mark>");
        last = r.end;
    }
    out.push_str(&blyg_render::escape_html(&text[last..]));
    out
}

#[derive(Serialize)]
struct Row {
    id: String,
    title_html: String,
    kind: &'static str,
    pill: String,
    pill_pub: bool,
    dot: bool,
    dot_tip: Option<String>,
    when: String,
    selected: bool,
    shelved: bool,
}

#[derive(Serialize)]
struct Current {
    id: Option<String>,
    content: String,
    kind: &'static str,
    kind_label: &'static str,
    counter: String,
    level: &'static str,
    banner: Option<&'static str>,
    version: String,
    permalink: Option<String>,
    status: &'static str,
    /// The new-post status line, while a ⌘N post is open.
    new_post: Option<String>,
    shelved: bool,
    /// What a reply stub answers (`host · title`), for the editor's header.
    stub_of: Option<String>,
}

#[derive(Serialize)]
struct View {
    title: String,
    query: String,
    hint: String,
    wants_create: bool,
    rows: Vec<Row>,
    current: Option<Current>,
    sync: String,
    sync_dot: &'static str,
    /// How many rows at the top are shelved (a hairline goes under them).
    shelf_len: usize,
    /// The shelf strip: every shelved post, in order, filtered or not.
    shelf: Vec<read_api::ShelfRow>,
}

#[derive(Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Outcome {
    Open {
        id: String,
    },
    Create {
        id: String,
    },
    Nothing,
    Toast {
        message: String,
    },
    Shake {
        message: String,
    },
    Sheet {
        title: String,
        next_version: u32,
    },
    Confirm {
        title: String,
        body: String,
    },
    Published {
        version: u32,
        permalink: String,
        note: Option<String>,
    },
}

fn status_name(s: Status) -> &'static str {
    match s {
        Status::Draft => "draft",
        Status::Public => "public",
        Status::Withdrawn => "withdrawn",
        Status::Scratch => "scratch",
    }
}

fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Fragment => "fragment",
        Kind::Thread => "thread",
    }
}

/// The main window's state: the store, the NV list, and what's open.
#[wasm_bindgen]
pub struct Burrow {
    store: WebStore,
    list: ListModel,
    rd: seed::Reading,
    /// A ⌘N post: `Some(None)` before its first keystroke, then its id.
    new_post: Option<Option<LocalId>>,
    /// The row selected before ⌘N, where ↑/↓ go on from.
    before_new: Option<LocalId>,
    sync: SyncStatus,
    publishing: bool,
    offline: bool,
}

#[wasm_bindgen]
impl Burrow {
    /// Open the store: `saved` is a previous `to_json()`, else the sample data.
    #[wasm_bindgen(constructor)]
    pub fn new(now_ms: f64, saved: Option<String>) -> Burrow {
        let now = at(now_ms);
        let store = saved
            .as_deref()
            .and_then(WebStore::from_json)
            .unwrap_or_else(|| WebStore::seeded(now));
        let mut list = ListModel::default();
        list.set_query("", store.arrange(store.items()).0);
        let mut rd = seed::reading(now);
        for r in &mut rd.rows {
            let key = read_api::key_of(r);
            if let Some(v) = store.read.get(&key) {
                r.read_version = *v;
            }
            if let Some(h) = store.hopped.get(&key) {
                r.hoppers = h.clone();
            }
        }
        Burrow {
            store,
            list,
            rd,
            new_post: None,
            before_new: None,
            sync: SyncStatus::Synced,
            publishing: false,
            offline: false,
        }
    }

    /// Back to the sample data.
    pub fn reset(&mut self, now_ms: f64) {
        *self = Burrow::new(now_ms, None);
    }

    pub fn to_json(&self) -> String {
        self.store.to_json()
    }

    fn current_id(&self) -> Option<LocalId> {
        match &self.new_post {
            Some(id) => id.clone(),
            None => self.list.selected().cloned(),
        }
    }

    fn current_item(&self) -> Option<Item> {
        self.current_id().and_then(|id| self.store.item(&id))
    }

    /// Leaving a ⌘N post while it's empty deletes it (no blank items).
    fn leave_new_post(&mut self) {
        if let Some(Some(id)) = self.new_post.take()
            && self
                .store
                .item(&id)
                .is_some_and(|i| i.content_md.trim().is_empty())
        {
            self.store.delete(&id);
            self.list.remove(&id);
        }
        self.before_new = None;
    }

    fn refresh(&mut self) {
        let q = self.list.query().to_string();
        self.list.refresh(self.results(&q));
    }

    /// The store's matches for `q`, shelved posts first.
    fn results(&self, q: &str) -> Vec<Item> {
        self.store.arrange(self.store.search(q)).0
    }

    /// After a change to the open post: fresh results, and the post stays
    /// open. Selected if the filter lists it; otherwise it stays open
    /// unlisted, as a ⌘N post does, rather than the editor jumping to
    /// whatever row is first.
    fn keep_open(&mut self, id: &LocalId) {
        self.refresh();
        if self.list.results().iter().any(|i| &i.local_id == id) {
            self.list.select(id);
            self.new_post = None;
            self.before_new = None;
        } else {
            self.new_post = Some(Some(id.clone()));
        }
    }

    /// Everything the page draws, as JSON.
    pub fn view(&self, now_ms: f64) -> String {
        let now = at(now_ms);
        let q = self.list.trimmed_query().to_string();
        let sel = self.current_id();
        let rows = self
            .list
            .results()
            .iter()
            .map(|it| Row {
                id: it.local_id.0.clone(),
                title_html: highlighted(&vm::truncate_chars(&vm::item_title(it), 140), &q),
                kind: vm::kind_label(it.kind),
                pill: vm::pill(it).0,
                pill_pub: vm::pill(it).1,
                dot: vm::has_unpublished_edits(it),
                dot_tip: vm::edits_tip(it),
                when: vm::relative_time(&it.updated, now),
                selected: sel.as_ref() == Some(&it.local_id),
                shelved: self.store.shelf.contains(&it.local_id),
            })
            .collect();
        let shelf_len = self
            .list
            .results()
            .iter()
            .take_while(|i| self.store.shelf.contains(&i.local_id))
            .count();
        let current = match (&self.new_post, self.current_item()) {
            (Some(None), _) => Some(Current {
                id: None,
                content: String::new(),
                kind: "fragment",
                kind_label: vm::kind_label(Kind::Fragment),
                counter: vm::counter(Kind::Fragment, 0).0,
                level: "",
                banner: None,
                version: String::new(),
                permalink: None,
                status: "new",
                new_post: Some(NEW_NOTE.into()),
                shelved: false,
                stub_of: None,
            }),
            (_, Some(it)) => {
                let chars = it.char_count();
                let (counter, level) = vm::counter(it.kind, chars);
                Some(Current {
                    id: Some(it.local_id.0.clone()),
                    kind: kind_name(it.kind),
                    kind_label: vm::kind_label(it.kind),
                    counter,
                    level: level_class(level),
                    banner: vm::banner(it.kind, chars),
                    version: vm::version_label(&it),
                    permalink: it.permalink.clone(),
                    status: status_name(it.status),
                    new_post: (self.new_post.is_some() && it.status == Status::Scratch)
                        .then(|| NEW_NOTE.into()),
                    shelved: self.store.shelf.contains(&it.local_id),
                    stub_of: self.stub_label(&it),
                    content: it.content_md,
                })
            }
            _ => None,
        };
        let sync = if self.offline {
            SyncStatus::Offline {
                pending: self.store.pending(),
            }
        } else {
            self.sync
        };
        let (label, dot) = vm::sync_label(sync, self.publishing);
        json(&View {
            title: vm::window_title(Some(seed::ORIGIN)),
            query: self.list.query().to_string(),
            hint: self.list.hint(),
            wants_create: self.list.wants_create(),
            rows,
            current,
            sync: label,
            sync_dot: dot_class(dot),
            shelf_len,
            shelf: self.shelf_rows(now),
        })
    }

    /// The omnibar changed: filter live, the selection jumps to the first
    /// match and the editor shows it (NV).
    pub fn set_query(&mut self, query: &str) {
        self.leave_new_post();
        let results = self.results(query);
        self.list.set_query(query, results);
    }

    /// ↑/↓ in the omnibar. Returns the newly selected id, if it changed.
    pub fn move_selection(&mut self, delta: i32) -> Option<String> {
        if self.new_post.is_some() {
            let back = self.before_new.clone();
            self.leave_new_post();
            if let Some(id) = back {
                self.list.select(&id);
            }
        }
        self.list.move_selection(delta as isize).map(|id| id.0)
    }

    /// ⏎ in the omnibar: open the selection, or create a draft from the
    /// query when nothing matches.
    pub fn enter(&mut self, now_ms: f64) -> String {
        match self.list.enter() {
            EnterAction::Open(id) => json(&Outcome::Open { id: id.0 }),
            EnterAction::Create(text) => {
                let id = self.create_draft(&text, now_ms);
                json(&Outcome::Create { id })
            }
            EnterAction::Nothing => json(&Outcome::Nothing),
        }
    }

    /// A row clicked.
    pub fn open(&mut self, id: &str) {
        self.leave_new_post();
        self.list.select(&LocalId(id.into()));
    }

    /// A draft seeded with `text`; the omnibar clears and it's selected.
    pub fn create_draft(&mut self, text: &str, now_ms: f64) -> String {
        self.leave_new_post();
        let id = self.store.insert(
            Kind::Fragment,
            text,
            Status::Draft,
            &rfc(now_ms),
            now_ms as i64,
        );
        self.list.set_query("", self.results(""));
        self.list.select(&id);
        self.touched();
        id.0
    }

    /// ⌘N: an empty editor; nothing exists until the first keystroke.
    pub fn new_post(&mut self) {
        self.leave_new_post();
        self.before_new = self.list.selected().cloned();
        self.list.clear_selection();
        self.new_post = Some(None);
    }

    /// Every keystroke in the editor. Returns true if the store changed.
    pub fn edit(&mut self, text: &str, now_ms: f64) -> bool {
        if let Some(None) = self.new_post {
            if text.is_empty() {
                return false;
            }
            let id = self.store.insert(
                Kind::Fragment,
                text,
                Status::Scratch,
                &rfc(now_ms),
                now_ms as i64,
            );
            self.new_post = Some(Some(id.clone()));
            self.refresh();
            self.list.select(&id);
            return true;
        }
        let Some(id) = self.current_id() else {
            return false;
        };
        if !self.store.save(&id, text, &rfc(now_ms)) {
            return false;
        }
        if let Some(it) = self.store.item(&id) {
            if it.status != Status::Scratch {
                self.touched();
            }
            self.list.update_item(it);
        }
        true
    }

    fn touched(&mut self) {
        if !self.offline {
            self.sync = SyncStatus::Saving;
        }
    }

    /// The debounce ran out and the page wrote the store to IndexedDB: the
    /// demo's push. `phase` is `"syncing"` then `"synced"`.
    pub fn sync_phase(&mut self, phase: &str) {
        if self.offline {
            return;
        }
        self.sync = match phase {
            "syncing" => SyncStatus::Syncing,
            "saving" => SyncStatus::Saving,
            "error" => SyncStatus::Error,
            _ => {
                self.store.mark_synced();
                SyncStatus::Synced
            }
        };
    }

    /// The demo's network switch: offline keeps changes waiting.
    pub fn set_offline(&mut self, offline: bool) -> usize {
        let pending = self.store.pending();
        self.offline = offline;
        if !offline {
            self.sync = if pending > 0 {
                SyncStatus::Syncing
            } else {
                SyncStatus::Synced
            };
        }
        pending
    }

    pub fn is_offline(&self) -> bool {
        self.offline
    }

    /// ⌘T: fragment ⇄ thread.
    pub fn toggle_kind(&mut self, now_ms: f64) -> String {
        let Some(it) = self.current_item() else {
            return json(&Outcome::Nothing);
        };
        let kind = match it.kind {
            Kind::Fragment => Kind::Thread,
            Kind::Thread => Kind::Fragment,
        };
        self.store.set_kind(&it.local_id, kind, &rfc(now_ms));
        if let Some(it) = self.store.item(&it.local_id) {
            if it.status != Status::Scratch {
                self.touched();
            }
            self.list.update_item(it);
        }
        json(&Outcome::Toast {
            message: match kind {
                Kind::Thread => {
                    "Now a thread: no length limit, and it can quote other blygs".into()
                }
                Kind::Fragment => "Now a fragment: 1000 characters max".into(),
            },
        })
    }

    /// ⌘⏎: what publishing the open post would do (`vm::publish_decision`).
    pub fn publish_check(&self) -> String {
        let Some(it) = self.current_item() else {
            return json(&Outcome::Toast {
                message: "Nothing to publish yet".into(),
            });
        };
        if self.offline {
            return json(&Outcome::Toast {
                message: "Publishing needs the blyg. Your edits are kept until you're back online."
                    .into(),
            });
        }
        json(&match vm::publish_decision(&it) {
            PublishDecision::Shake => Outcome::Shake {
                message: keymap::hint(
                    "Too long to publish as a fragment. Press ⌘T to make it a thread.",
                )
                .into(),
            },
            PublishDecision::AlreadyPublished(v) => Outcome::Toast {
                message: format!("Already published as v{v}"),
            },
            PublishDecision::Empty => Outcome::Toast {
                message: "Nothing to publish yet".into(),
            },
            PublishDecision::Sheet {
                title,
                next_version,
            } => Outcome::Sheet {
                title,
                next_version,
            },
        })
    }

    /// The publish sheet's ⏎. On the sample blyg only: nothing leaves the page.
    pub fn publish(&mut self, note: &str, now_ms: f64) -> String {
        let Some(id) = self.current_id() else {
            return json(&Outcome::Nothing);
        };
        let note = Some(note.trim().to_string()).filter(|n| !n.is_empty());
        let Some(it) = self.store.publish(&id, note.clone(), &rfc(now_ms)) else {
            return json(&Outcome::Nothing);
        };
        self.keep_open(&id);
        json(&Outcome::Published {
            version: it.version,
            permalink: it.permalink.unwrap_or_default(),
            note,
        })
    }

    /// ⌘D: the open scratch note becomes a draft.
    pub fn make_draft(&mut self, now_ms: f64) -> String {
        let Some(it) = self.current_item() else {
            return json(&Outcome::Toast {
                message: "Nothing to save as a draft yet".into(),
            });
        };
        if let Some(why) = vm::make_draft_blocked(&it) {
            return json(&Outcome::Toast {
                message: why.into(),
            });
        }
        self.store.promote(&it.local_id, &rfc(now_ms));
        self.touched();
        self.keep_open(&it.local_id);
        json(&Outcome::Toast {
            message: "Saved as a draft".into(),
        })
    }

    /// ⇧⌘⌫: what deleting the open post means (`vm::discard`).
    pub fn delete_check(&self) -> String {
        let Some(it) = self.current_item() else {
            return json(&Outcome::Nothing);
        };
        json(&match vm::discard(&it) {
            vm::Discard::Delete => Outcome::Confirm {
                title: format!("Delete this {}?", vm::discard_noun(&it)),
                body: format!(
                    "“{}” will be gone. This can't be undone.",
                    vm::truncate_chars(&vm::item_title(&it), 48)
                ),
            },
            vm::Discard::Withdraw => Outcome::Toast {
                message:
                    "Published posts are never deleted. Withdraw is in the Post menu of the app."
                        .into(),
            },
            vm::Discard::AlreadyWithdrawn => Outcome::Toast {
                message: "Already withdrawn".into(),
            },
        })
    }

    pub fn delete(&mut self) -> String {
        let Some(it) = self.current_item() else {
            return json(&Outcome::Nothing);
        };
        let noun = vm::discard_noun(&it);
        self.new_post = None;
        self.before_new = None;
        self.store.delete(&it.local_id);
        self.list.remove(&it.local_id);
        json(&Outcome::Toast {
            message: format!("{} deleted", capitalise(noun)),
        })
    }

    /// ⌥⌘Z: whether the open post's unpublished changes can be discarded.
    pub fn discard_check(&self) -> String {
        let Some(it) = self.current_item() else {
            return json(&Outcome::Nothing);
        };
        json(&match vm::discard_changes(&it, self.offline) {
            Ok(v) => Outcome::Confirm {
                title: "Discard unpublished changes?".into(),
                body: format!(
                    "“{}”: the editor goes back to v{v} as published. You can't undo this.",
                    vm::truncate_chars(&vm::item_title(&it), 48)
                ),
            },
            Err(why) => Outcome::Toast {
                message: why.into(),
            },
        })
    }

    pub fn discard(&mut self, now_ms: f64) -> String {
        let Some(it) = self.current_item() else {
            return json(&Outcome::Nothing);
        };
        let Ok(v) = vm::discard_changes(&it, self.offline) else {
            return json(&Outcome::Nothing);
        };
        match self.store.restore(&it.local_id, v, &rfc(now_ms)) {
            Some(it) => {
                self.list.update_item(it);
                json(&Outcome::Toast {
                    message: format!("Back to v{v} as published"),
                })
            }
            None => json(&Outcome::Nothing),
        }
    }

    /// The open post's preview: a complete, sandboxable document, rendered
    /// as the native studio preview renders it. `nonce` comes from the
    /// page's `crypto.getRandomValues` (std has no clock or RNG here).
    pub fn preview_page(&self, nonce: &str, dark: bool) -> String {
        let (md, kind, item) = match self.current_item() {
            Some(it) => (it.content_md.clone(), it.kind, Some(it)),
            None => (String::new(), Kind::Fragment, None),
        };
        let out = self.render(&md, kind, item.as_ref());
        let body = blyg_render::article_html(render_kind(kind), &out.html, None, None);
        let scheme = if dark {
            ":root { --paper: #161513; --ink: #e4dfd3; --muted: #9a9489; --accent: #e0775a; --rule: #2f2d29; --wash: rgba(228,223,211,0.05); color-scheme: dark; }"
        } else {
            ":root { --paper: #fffff8; --ink: #111; --muted: #6b665d; --accent: #a4271b; --rule: #dcd6c9; --wash: rgba(17,17,17,0.04); color-scheme: light; }"
        };
        page_shell(&format!("{BUILTIN_CSS}\n{scheme}\n"), &body, nonce)
    }

    /// The preview's counts, for the status bar (quotes, links, AI spans,
    /// what would stop publish).
    pub fn preview_stats(&self) -> String {
        let Some(it) = self.current_item() else {
            return String::new();
        };
        let s = self.render(&it.content_md, it.kind, Some(&it)).stats;
        let mut parts = Vec::new();
        let n =
            |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        if s.quotes > 0 {
            parts.push(n(s.quotes, "quote", "quotes"));
        }
        if s.links > 0 {
            parts.push(n(s.links, "link", "links"));
        }
        if s.ai_spans > 0 {
            parts.push(n(s.ai_spans, "AI span", "AI spans"));
        }
        if s.videos > 0 {
            parts.push(n(s.videos, "video", "videos"));
        }
        if s.images > 0 {
            parts.push(n(s.images, "image", "images"));
        }
        let blocking = s.unresolved.len() + s.ungenerated + s.tk_errors.len();
        if blocking > 0 {
            parts.push(n(
                blocking,
                "issue would stop publish",
                "issues would stop publish",
            ));
        }
        parts.join(" · ")
    }

    /// Burrow's themes (`crates/blyg-core/themes/`), as JSON.
    pub fn themes() -> String {
        json(&theme::all())
    }
}

impl Burrow {
    fn render(&self, md: &str, kind: Kind, item: Option<&Item>) -> blyg_render::Rendered {
        let opts = blyg_render::RenderOpts {
            mount: seed::ORIGIN.to_string(),
            self_id: item.and_then(|i| i.server_id.as_ref()).map(|s| s.0.clone()),
            ..blyg_render::RenderOpts::default()
        };
        let resolver = if md.contains("[[") {
            StoreResolver::new(self.store.items(), self.rd.rows.clone(), &[], &opts.mount)
        } else {
            StoreResolver::empty(&opts.mount)
        };
        blyg_render::render_preview(md, render_kind(kind), &resolver, &opts)
    }

    /// For tests: the open post's id.
    pub fn current(&self) -> Option<String> {
        self.current_id().map(|id| id.0)
    }
}

const NEW_NOTE: &str = "New note · saved in this browser · ⌘D draft · ⌘⏎ publish";

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// `blyg_render::page_shell_with`, with the nonce passed in: the original
/// seeds its nonce from `SystemTime::now()`, which panics in
/// `wasm32-unknown-unknown`. Same document otherwise.
pub fn page_shell(theme_css: &str, body: &str, nonce: &str) -> String {
    let style_safe = |css: &str| css.replace("</", "<\\/");
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta http-equiv=\"Content-Security-Policy\" content=\"{csp}\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>Preview</title>\n<style>{theme}</style>\n<style>{studio}</style>\n</head>\n<body>\n{body}\n\
         <script nonce=\"{nonce}\">{script}</script>\n</body>\n</html>\n",
        csp = blyg_render::escape_html(&blyg_render::csp(nonce)),
        theme = style_safe(theme_css),
        studio = style_safe(&blyg_render::studio_css()),
        script = blyg_render::preview_script(),
    )
}

#[cfg(test)]
mod tests;
