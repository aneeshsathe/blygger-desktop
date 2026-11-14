//! Live mode: the same `Burrow`, on the owner's own blyg. Served from the
//! blyg at `/burrow/`, the page reaches the owner API at `/api` on the same
//! origin with the Studio session cookie (no token, nothing stored). The
//! page does the fetching (`www/live.js`); this decides what to send and
//! what the answers mean (`crate::api`), and keeps the store.
//!
//! Local first: every keystroke lands in the store at once, as in the
//! demo. The outbox is the store itself: an item with `pending_sync` is an
//! edit the blyg hasn't got, sent with the revision it was based on
//! (`expectedRevision`); deletes, read marks and hopper changes wait in
//! lists. All of it is persisted, so edits made offline survive a reload
//! and replay when the blyg answers again. A `revision-conflict` never
//! overwrites: the blyg's copy is fetched and kept beside yours until you
//! choose one.

use std::collections::HashSet;

use blyg_core::api::absolute_page;
use blyg_core::api::wire::{ChangeState, WireItem, imported_row};
use blyg_core::sync::{FULL_PULL_EVERY, ITEMS_DEPS, READING_DEPS};
use blyg_core::{Hopper, Item, Kind, LocalId, ReadingItem, ServerId, Status};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

use crate::api::{self, ApiError, Request};
use crate::read_api::key_of;
use crate::store::{Conflict, HopOp, PendingDelete, ReadOp, Seen, WebStore};
use crate::{Burrow, Outcome, at, json, seed};

/// The reading list's own snapshot (IndexedDB key `live-reading`): the
/// rows and hoppers as last read, and the revisions they were read under.
#[derive(Deserialize)]
struct ReadingCache {
    rows: Vec<ReadingItem>,
    #[serde(default)]
    hoppers: Vec<Hopper>,
    #[serde(default)]
    seen: Seen,
}

#[derive(Serialize)]
struct ReadingCacheRef<'a> {
    rows: &'a [ReadingItem],
    hoppers: &'a [Hopper],
    seen: &'a Seen,
}

/// An item as the blyg holds it, in the app's model. Its local id is its
/// server id.
pub fn item_of(w: &WireItem, base: &str) -> Item {
    let mut w = w.clone();
    w.resolve(false, base);
    let mut it = seed::blank(
        &w.id,
        w.local_kind(),
        w.local_status(),
        &w.content_md,
        &w.created,
    );
    it.server_id = Some(ServerId(w.id.clone()));
    it.version = w.version;
    it.dirty = w.dirty && w.version > 0;
    it.updated = if w.updated.is_empty() {
        w.created.clone()
    } else {
        w.updated.clone()
    };
    it.permalink = w.permalink.clone();
    it.stub_of = w.stub_ref();
    it.forked_from = w.fork_ref();
    it.responses_mode = w.responses_mode();
    it.show_responses = w.shows_responses();
    it
}

/// One outbox entry, as the page sends it and hands it back.
#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
pub struct Op {
    /// `create`, `save`, `delete`, `read` or `hop`.
    pub op: String,
    /// The local item id, or the reading row's key.
    pub id: String,
    /// For `create` / `save`: the text sent, so an edit made while it was
    /// in flight stays pending.
    #[serde(default)]
    pub sent: Option<String>,
    #[serde(default)]
    pub kind: Option<Kind>,
    /// For `hop`: which hopper, and in or out.
    #[serde(default)]
    pub hopper: Option<String>,
    #[serde(default)]
    pub on: Option<bool>,
    /// For `read`: the version marked (`None`: unread).
    #[serde(default)]
    pub version: Option<u32>,
}

#[derive(Serialize)]
struct Next {
    #[serde(flatten)]
    op: Op,
    req: Request,
}

/// What an outbox answer means for the page.
#[derive(Debug, Default, Serialize, PartialEq)]
pub struct Step {
    /// Stop sending for now (offline, signed out, rate-limited).
    pub stop: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
    /// Fetch this (`GET`) and hand it to `live_conflict`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fetch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflict: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toast: Option<String>,
    /// Read the item list again.
    pub refresh: bool,
}

impl WebStore {
    fn rev_of(&self, id: &LocalId) -> u64 {
        self.live.rev.get(&id.0).copied().unwrap_or(0)
    }

    /// A fresh list from the blyg. Items with edits waiting keep their text
    /// (and the revision they're based on, so a stale one still conflicts
    /// when it's sent); the rest take the blyg's copy. Items the blyg no
    /// longer has go, unless they hold unsent words.
    pub fn merge_server(&mut self, list: Vec<WireItem>) {
        let base = self.live.base.clone();
        let deleting: HashSet<String> = self
            .live
            .deletes
            .iter()
            .map(|d| d.server_id.clone())
            .collect();
        let mut seen = HashSet::new();
        for w in list {
            if deleting.contains(&w.id) {
                continue;
            }
            seen.insert(w.id.clone());
            let fresh = item_of(&w, &base);
            match self.by_server(&w.id) {
                Some(id) => {
                    let Some(local) = self.item(&id) else {
                        continue;
                    };
                    if local.pending_sync || local.conflict {
                        if let Some(it) = self.get_mut_pub(&id) {
                            it.status = fresh.status;
                            it.version = fresh.version;
                            it.permalink = fresh.permalink;
                        }
                    } else {
                        let mut it = fresh;
                        it.local_id = id.clone();
                        self.upsert(it);
                        self.live.rev.insert(id.0, w.revision);
                    }
                }
                None => {
                    self.live.rev.insert(w.id.clone(), w.revision);
                    self.upsert(fresh);
                }
            }
        }
        let gone: Vec<LocalId> = self
            .raw()
            .iter()
            .filter(|i| {
                i.server_id.as_ref().is_some_and(|s| !seen.contains(&s.0))
                    && !i.pending_sync
                    && !i.conflict
            })
            .map(|i| i.local_id.clone())
            .collect();
        for id in gone {
            self.live.rev.remove(&id.0);
            self.delete(&id);
        }
    }

    /// The blyg's answer to a command on `id`: its revision and state.
    /// `sent`: the text that went; when the editor still holds it, the
    /// item is no longer pending. `force`: the blyg's text wins (after a
    /// publish, a restore, or "use theirs").
    pub fn apply_server(&mut self, id: &LocalId, w: &WireItem, sent: Option<&str>, force: bool) {
        let base = self.live.base.clone();
        let fresh = item_of(w, &base);
        self.live.rev.insert(id.0.clone(), w.revision);
        let Some(it) = self.get_mut_pub(id) else {
            return;
        };
        let same = sent.is_none_or(|s| s == it.content_md);
        it.server_id = Some(ServerId(w.id.clone()));
        it.status = fresh.status;
        it.version = fresh.version;
        it.permalink = fresh.permalink;
        it.stub_of = fresh.stub_of.or(it.stub_of.take());
        if force || same {
            it.kind = fresh.kind;
            it.content_md = fresh.content_md;
            it.dirty = fresh.dirty;
            it.updated = fresh.updated;
            it.pending_sync = false;
        } else if it.status == Status::Public {
            it.dirty = true;
        }
    }

    /// The next thing the outbox would send, skipping `skip` (`op:id`
    /// entries that failed this round).
    fn outbox_next(&self, skip: &HashSet<String>) -> Option<(Op, Request)> {
        let op = |op: &str, id: &str| Op {
            op: op.into(),
            id: id.into(),
            sent: None,
            kind: None,
            hopper: None,
            on: None,
            version: None,
        };
        let free = |o: &Op| !skip.contains(&format!("{}:{}", o.op, o.id));
        for d in &self.live.deletes {
            let o = op("delete", &d.server_id);
            if free(&o) {
                return Some((o, api::delete_item(&d.server_id, d.revision)));
            }
        }
        let mut items: Vec<&Item> = self
            .raw()
            .iter()
            .filter(|i| i.pending_sync && !i.conflict && i.status != Status::Scratch)
            .collect();
        items.sort_by(|a, b| a.updated.cmp(&b.updated));
        for it in items {
            let kind = (it.version == 0).then_some(it.kind);
            let o = match &it.server_id {
                None => Op {
                    sent: Some(it.content_md.clone()),
                    kind,
                    ..op("create", &it.local_id.0)
                },
                Some(_) => Op {
                    sent: Some(it.content_md.clone()),
                    kind,
                    ..op("save", &it.local_id.0)
                },
            };
            if !free(&o) {
                continue;
            }
            let req = match &it.server_id {
                None => api::create_item(&it.content_md, it.kind, it.stub_of.as_ref()),
                Some(sid) => {
                    api::save_item(&sid.0, self.rev_of(&it.local_id), &it.content_md, kind)
                }
            };
            return Some((o, req));
        }
        for r in &self.live.reads {
            let key = format!("{}|{}", r.sub, r.remote_id);
            let o = Op {
                version: r.version,
                ..op("read", &key)
            };
            if free(&o) {
                return Some((o, api::read_mark(&r.sub, &r.remote_id, r.version)));
            }
        }
        for h in &self.live.hops {
            let key = format!("{}|{}", h.sub, h.remote_id);
            let o = Op {
                hopper: Some(h.hopper.clone()),
                on: Some(h.on),
                ..op("hop", &key)
            };
            if free(&o) {
                return Some((o, api::hopper_item(&h.hopper, &h.sub, &h.remote_id, h.on)));
            }
        }
        None
    }

    /// Take an op off its list once the blyg has it (or never will).
    fn settle(&mut self, o: &Op) {
        match o.op.as_str() {
            "delete" => self.live.deletes.retain(|d| d.server_id != o.id),
            "read" => {
                let ix = self.live.reads.iter().position(|r| {
                    format!("{}|{}", r.sub, r.remote_id) == o.id && r.version == o.version
                });
                if let Some(ix) = ix {
                    self.live.reads.remove(ix);
                }
            }
            "hop" => {
                let ix = self.live.hops.iter().position(|h| {
                    format!("{}|{}", h.sub, h.remote_id) == o.id
                        && Some(&h.hopper) == o.hopper.as_ref()
                        && Some(h.on) == o.on
                });
                if let Some(ix) = ix {
                    self.live.hops.remove(ix);
                }
            }
            _ => {}
        }
    }

    /// What the blyg said to one outbox entry.
    pub fn outbox_answer(
        &mut self,
        o: &Op,
        status: u16,
        body: &str,
        retry_after: Option<&str>,
    ) -> Step {
        let res = api::read(status, body, retry_after);
        let id = LocalId(o.id.clone());
        match (o.op.as_str(), res) {
            (
                _,
                Err(
                    e @ (ApiError::Offline | ApiError::Unauthorized | ApiError::RateLimited { .. }),
                ),
            ) => Step {
                stop: true,
                error: Some(e.to_json()),
                ..Step::default()
            },
            ("create" | "save", Ok(v)) => match api::parse_item(&v) {
                Ok(w) => {
                    self.apply_server(&id, &w, o.sent.as_deref(), false);
                    Step::default()
                }
                Err(e) => Step {
                    error: Some(e.to_json()),
                    ..Step::default()
                },
            },
            ("save", Err(ApiError::RevisionConflict)) => {
                if let Some(it) = self.get_mut_pub(&id) {
                    it.conflict = true;
                }
                let sid = self
                    .item(&id)
                    .and_then(|i| i.server_id)
                    .map(|s| s.0)
                    .unwrap_or_default();
                Step {
                    fetch: Some(api::item_path(&sid)),
                    conflict: Some(o.id.clone()),
                    toast: Some(
                        "Changed elsewhere since you opened it. Choose which copy to keep.".into(),
                    ),
                    ..Step::default()
                }
            }
            ("save", Err(ApiError::NotFound)) => {
                // Deleted on the blyg under your edit: your words stay, as a
                // new draft on the next send.
                self.live.rev.remove(&o.id);
                if let Some(it) = self.get_mut_pub(&id) {
                    it.server_id = None;
                    it.version = 0;
                    it.status = Status::Draft;
                    it.permalink = None;
                    it.dirty = false;
                }
                Step {
                    toast: Some(
                        "That post was deleted elsewhere. Your text is kept as a new draft.".into(),
                    ),
                    ..Step::default()
                }
            }
            ("delete", Ok(_) | Err(ApiError::NotFound)) => {
                self.settle(o);
                Step::default()
            }
            ("delete", Err(ApiError::RevisionConflict)) => {
                // It changed elsewhere after you deleted it here: keep it.
                self.settle(o);
                Step {
                    toast: Some(
                        "That draft changed elsewhere, so it wasn't deleted. It's back in your list."
                            .into(),
                    ),
                    refresh: true,
                    ..Step::default()
                }
            }
            ("read" | "hop", Ok(_) | Err(ApiError::NotFound)) => {
                self.settle(o);
                Step::default()
            }
            (_, Err(e)) => Step {
                error: Some(e.to_json()),
                ..Step::default()
            },
            (_, Ok(_)) => {
                self.settle(o);
                Step::default()
            }
        }
    }

    /// The blyg's copy of a conflicted item, kept beside yours.
    pub fn record_conflict(&mut self, id: &LocalId, w: &WireItem) {
        let c = Conflict {
            server_md: w.content_md.clone(),
            server_rev: w.revision,
            server_kind: w.local_kind(),
            server_status: w.local_status(),
            server_version: w.version,
            server_dirty: w.dirty,
        };
        if let Some(it) = self.get_mut_pub(id) {
            it.conflict = true;
            self.live.conflicts.insert(id.0.clone(), c);
        }
    }

    /// Your choice: `mine` sends your text over theirs (based on their
    /// revision now); otherwise their text replaces yours.
    pub fn resolve(&mut self, id: &LocalId, mine: bool) -> bool {
        let Some(c) = self.live.conflicts.remove(&id.0) else {
            return false;
        };
        self.live.rev.insert(id.0.clone(), c.server_rev);
        let Some(it) = self.get_mut_pub(id) else {
            return false;
        };
        it.conflict = false;
        it.status = c.server_status;
        it.version = c.server_version;
        if mine {
            it.pending_sync = true;
            if it.status == Status::Public {
                it.dirty = true;
            }
        } else {
            it.content_md = c.server_md;
            it.kind = c.server_kind;
            it.dirty = c.server_dirty && c.server_version > 0;
            it.pending_sync = false;
        }
        true
    }
}

#[derive(Serialize)]
pub struct ConflictView {
    pub server_rev: u64,
    pub excerpt: String,
}

/// A hopper from `GET /api/hoppers` (the contract's `Hopper`).
fn hopper_of(v: &Value) -> Option<Hopper> {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    Some(Hopper {
        id: s("id")?,
        name: s("name").unwrap_or_default(),
        slug: s("slug"),
        public: v.get("public").and_then(Value::as_bool).unwrap_or(false),
        count: 0,
        description: s("description"),
        slug_frozen: v
            .get("slugFrozen")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

impl Burrow {
    pub(crate) fn is_live(&self) -> bool {
        self.store.live.on
    }

    /// Where permalinks and the preview point: the blyg, or the sample one.
    pub(crate) fn origin_str(&self) -> String {
        if self.is_live() && !self.store.live.base.is_empty() {
            self.store.live.base.clone()
        } else {
            seed::ORIGIN.to_string()
        }
    }

    pub(crate) fn conflict_view(&self, id: &LocalId) -> Option<ConflictView> {
        let c = self.store.live.conflicts.get(&id.0)?;
        Some(ConflictView {
            server_rev: c.server_rev,
            excerpt: crate::vm::truncate_chars(
                c.server_md
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .trim(),
                160,
            ),
        })
    }

    /// Live mode: a read mark to send.
    pub(crate) fn queue_read(&mut self, key: &str, version: Option<u32>) {
        if !self.is_live() {
            return;
        }
        let Some((sub, rid)) = key.split_once('|') else {
            return;
        };
        let reads = &mut self.store.live.reads;
        reads.retain(|r| !(r.sub == sub && r.remote_id == rid));
        reads.push(ReadOp {
            sub: sub.into(),
            remote_id: rid.into(),
            version,
        });
    }

    pub(crate) fn queue_hop(&mut self, key: &str, hopper: &str, on: bool) {
        if !self.is_live() {
            return;
        }
        let Some((sub, rid)) = key.split_once('|') else {
            return;
        };
        let hops = &mut self.store.live.hops;
        // In then out again (or the reverse) before sending is nothing.
        if let Some(ix) = hops
            .iter()
            .position(|h| h.sub == sub && h.remote_id == rid && h.hopper == hopper)
        {
            hops.remove(ix);
            return;
        }
        hops.push(HopOp {
            hopper: hopper.into(),
            sub: sub.into(),
            remote_id: rid.into(),
            on,
        });
    }

    /// Live mode: a deleted draft the blyg should delete too.
    pub(crate) fn queue_delete(&mut self, it: &Item) {
        if !self.is_live() {
            return;
        }
        if let Some(sid) = &it.server_id {
            let revision = self
                .store
                .live
                .rev
                .get(&it.local_id.0)
                .copied()
                .unwrap_or(0);
            self.store.live.deletes.push(PendingDelete {
                server_id: sid.0.clone(),
                revision,
            });
        }
        self.store.live.rev.remove(&it.local_id.0);
        self.store.live.conflicts.remove(&it.local_id.0);
    }

    fn after_change(&mut self, id: Option<&LocalId>) {
        let open = self.current_id();
        self.refresh();
        if let Some(id) = id.or(open.as_ref())
            && self.store.item(id).is_some()
            && self.list.results().iter().any(|i| &i.local_id == id)
        {
            self.list.select(id);
        }
    }
}

#[wasm_bindgen]
impl Burrow {
    /// Live mode on the blyg whose public base is `base`. `saved`: the
    /// live store as last persisted (a different store from the demo's);
    /// `reading`: the reading list as `reading_json` last wrote it.
    pub fn new_live(
        now_ms: f64,
        saved: Option<String>,
        reading: Option<String>,
        base: &str,
    ) -> Burrow {
        let mut store = saved
            .as_deref()
            .and_then(WebStore::from_json)
            .filter(|s| s.live.on)
            .unwrap_or_else(|| WebStore::live(base));
        if !base.is_empty() {
            store.live.base = base.to_string();
        }
        let mut b = Burrow::new(at(now_ms).timestamp_millis() as f64, None);
        let cache = reading
            .as_deref()
            .and_then(|s| serde_json::from_str::<ReadingCache>(s).ok());
        // A snapshot from before 0.18.1 holds the list itself: it moves to
        // its own snapshot on the next write.
        let legacy = std::mem::take(&mut store.live.reading);
        let legacy_hoppers = std::mem::take(&mut store.live.hoppers);
        b.reading_dirty = cache.is_none() && !legacy.is_empty();
        let cache = cache.unwrap_or(ReadingCache {
            rows: legacy,
            hoppers: legacy_hoppers,
            seen: Default::default(),
        });
        b.rd = seed::Reading {
            rows: cache.rows,
            subs: vec![],
            hoppers: cache.hoppers,
        };
        b.reading_seen = cache.seen;
        b.store = store;
        // Marks made since the list was written, sent or not.
        for r in &mut b.rd.rows {
            let key = key_of(r);
            if let Some(v) = b.store.read.get(&key) {
                r.read_version = *v;
            }
            if let Some(h) = b.store.hopped.get(&key) {
                r.hoppers = h.clone();
            }
        }
        b.overlay_reading();
        b.list = crate::vm::ListModel::default();
        b.list.set_query("", b.store.arrange(b.store.items()).0);
        b
    }

    pub fn is_live_mode(&self) -> bool {
        self.is_live()
    }

    /// The list from the blyg: every item, as `GET /api/items` pages
    /// collected (a JSON array). Returns how many items there are now.
    pub fn live_items(&mut self, items_json: &str) -> String {
        let vals: Vec<Value> = match serde_json::from_str(items_json) {
            Ok(v) => v,
            Err(e) => return json(&json!({ "error": e.to_string() })),
        };
        let mut list = Vec::with_capacity(vals.len());
        for v in &vals {
            match api::parse_item(v) {
                Ok(w) => list.push(w),
                Err(e) => return json(&json!({ "error": e.to_json() })),
            }
        }
        self.store.merge_server(list);
        self.after_change(None);
        json(&json!({ "items": self.store.items().len() }))
    }

    /// The reading list (`GET /api/reading/imported` pages) and the hoppers
    /// (`GET /api/hoppers` pages), as JSON arrays. Marks not yet sent stay
    /// on top of what the blyg says.
    pub fn live_reading(&mut self, rows_json: &str, hoppers_json: &str) -> String {
        let rows: Vec<Value> = serde_json::from_str(rows_json).unwrap_or_default();
        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            if let Ok(mut it) = imported_row(r) {
                it.page = it.page.take().and_then(|pg| absolute_page(&it.origin, &pg));
                out.push(it);
            }
        }
        let hoppers: Vec<Value> = serde_json::from_str(hoppers_json).unwrap_or_default();
        self.rd.hoppers = hoppers.iter().filter_map(hopper_of).collect();
        self.rd.rows = out;
        // The blyg's list has every mark it was sent; the ones it hasn't
        // been sent yet stay on top (`overlay_reading`).
        self.store.read.clear();
        self.store.hopped.clear();
        self.excerpts.borrow_mut().clear();
        self.reading_dirty = true;
        self.overlay_reading();
        json(&json!({ "rows": self.rd.rows.len(), "hoppers": self.rd.hoppers.len() }))
    }

    /// The reading list and hoppers, for their own IndexedDB snapshot
    /// (apart from `to_json`'s, which is written on every change). It is
    /// written only when the list changed: `reading_dirty`.
    pub fn reading_json(&mut self) -> String {
        self.reading_dirty = false;
        json(&ReadingCacheRef {
            rows: &self.rd.rows,
            hoppers: &self.rd.hoppers,
            seen: &self.reading_seen,
        })
    }

    /// Whether the reading list changed since `reading_json` last ran.
    pub fn reading_dirty(&self) -> bool {
        self.reading_dirty
    }

    /// Before a refresh: the blyg's `GET /api/changes` answer, and `now`.
    /// Returns which lists to read again, `{items, reading}`: those whose
    /// revisions moved since they were last read whole (the native sync's
    /// rule, `blyg_core::sync`), and all of them on a new epoch, without a
    /// usable answer, or every `FULL_PULL_EVERY` as a safety net.
    pub fn changes_plan(&mut self, status: u16, body: &str, now_ms: f64) -> String {
        self.changes = (status == 200)
            .then(|| serde_json::from_str::<ChangeState>(body).ok())
            .flatten();
        let every = FULL_PULL_EVERY.as_millis() as f64;
        let stale = |seen: &Seen, deps: &[&str]| match &self.changes {
            None => true,
            Some(c) => {
                c.epoch != seen.epoch
                    || now_ms - seen.at_ms >= every
                    || !deps.iter().all(|d| {
                        c.domains
                            .get(*d)
                            .is_some_and(|n| seen.revisions.get(*d) == Some(n))
                    })
            }
        };
        json(&json!({
            "items": stale(&self.store.live.seen, ITEMS_DEPS),
            "reading": stale(&self.reading_seen, READING_DEPS),
        }))
    }

    /// A list (`items` or `reading`) was read whole and installed: record
    /// the revisions `changes_plan` captured before it was read.
    pub fn changes_accept(&mut self, list: &str, now_ms: f64) {
        let Some(c) = &self.changes else {
            return;
        };
        let (seen, deps) = match list {
            "items" => (&mut self.store.live.seen, ITEMS_DEPS),
            "reading" => {
                self.reading_dirty = true;
                (&mut self.reading_seen, READING_DEPS)
            }
            _ => return,
        };
        *seen = Seen {
            epoch: c.epoch.clone(),
            revisions: deps
                .iter()
                .filter_map(|d| c.domains.get(*d).map(|n| (d.to_string(), *n)))
                .collect(),
            at_ms: now_ms,
        };
    }

    /// The next outbox entry to send, as `{op, id, …, req: {method, path,
    /// body}}`, or `null`. `skip_json`: `["op:id", …]` that failed this round.
    pub fn outbox_next(&self, skip_json: &str) -> String {
        let skip: HashSet<String> = serde_json::from_str::<Vec<String>>(skip_json)
            .unwrap_or_default()
            .into_iter()
            .collect();
        match self.store.outbox_next(&skip) {
            Some((op, req)) => json(&Next { op, req }),
            None => "null".into(),
        }
    }

    /// The blyg's answer to the entry `op_json` (as `outbox_next` gave it).
    pub fn outbox_answer(
        &mut self,
        op_json: &str,
        status: u16,
        body: &str,
        retry_after: Option<String>,
    ) -> String {
        let Ok(op) = serde_json::from_str::<Op>(op_json) else {
            return json(&Step {
                stop: true,
                ..Step::default()
            });
        };
        let step = self
            .store
            .outbox_answer(&op, status, body, retry_after.as_deref());
        self.after_change(None);
        json(&step)
    }

    /// The blyg's copy of a conflicted item (`GET /api/items/:id`).
    pub fn live_conflict(&mut self, id: &str, status: u16, body: &str) -> String {
        let id = LocalId(id.into());
        match api::read(status, body, None).and_then(|v| api::parse_item(&v)) {
            Ok(w) => {
                self.store.record_conflict(&id, &w);
                "true".into()
            }
            Err(e) => {
                // Without the blyg's copy there's nothing to choose
                // between: the edit stays pending, and the next send meets
                // the conflict again and fetches again.
                if !self.store.live.conflicts.contains_key(&id.0)
                    && let Some(it) = self.store.get_mut_pub(&id)
                {
                    it.conflict = false;
                }
                json(&e.to_json())
            }
        }
    }

    /// The conflict banner's choice on the open post: keep `mine` (sent
    /// over theirs next), or take theirs.
    pub fn resolve_conflict(&mut self, mine: bool) -> String {
        let Some(id) = self.current_id() else {
            return json(&Outcome::Nothing);
        };
        if !self.store.resolve(&id, mine) {
            return json(&Outcome::Nothing);
        }
        if let Some(it) = self.store.item(&id) {
            self.list.update_item(it);
        }
        json(&Outcome::Toast {
            message: if mine {
                "Keeping yours. It goes to the blyg now."
            } else {
                "Took the blyg's copy"
            }
            .into(),
        })
    }

    /// Publish on the blyg: the request for the open post (`id`), once its
    /// edits are there. `Err` toast when they aren't yet.
    pub fn live_publish_request(&self, id: &str, note: &str) -> String {
        let id = LocalId(id.into());
        let Some(it) = self.store.item(&id) else {
            return json(&json!({ "error": "That post isn't here any more" }));
        };
        if it.conflict {
            return json(&json!({ "error": "Choose which copy to keep before publishing" }));
        }
        let (Some(sid), false) = (&it.server_id, it.pending_sync) else {
            return json(
                &json!({ "error": "Your last edits haven't reached the blyg yet. Try again when it's back." }),
            );
        };
        let note = Some(note).filter(|n| !n.trim().is_empty());
        json(&json!({ "req": api::publish(&sid.0, self.store.rev_of(&id), note) }))
    }

    /// Discard changes on the blyg: the restore request for the open post.
    pub fn live_restore_request(&self, id: &str, version: u32) -> String {
        let id = LocalId(id.into());
        match self.store.item(&id).and_then(|i| i.server_id) {
            Some(sid) => {
                json(&json!({ "req": api::restore(&sid.0, self.store.rev_of(&id), version) }))
            }
            None => json(&json!({ "error": "That post isn't on the blyg" })),
        }
    }

    /// The answer to a publish or restore: `{fetch}` to read the item
    /// again, or `{error}`.
    pub fn live_command_answer(&mut self, id: &str, status: u16, body: &str) -> String {
        let lid = LocalId(id.into());
        match api::read(status, body, None) {
            Ok(_) => {
                let sid = self
                    .store
                    .item(&lid)
                    .and_then(|i| i.server_id)
                    .map(|s| s.0)
                    .unwrap_or_default();
                json(&json!({ "fetch": api::item_path(&sid) }))
            }
            Err(ApiError::RevisionConflict) => {
                if let Some(it) = self.store.get_mut_pub(&lid) {
                    it.conflict = true;
                }
                let sid = self
                    .store
                    .item(&lid)
                    .and_then(|i| i.server_id)
                    .map(|s| s.0)
                    .unwrap_or_default();
                json(
                    &json!({ "conflict": id, "fetch": api::item_path(&sid), "error": ApiError::RevisionConflict.to_json() }),
                )
            }
            Err(e) => json(&json!({ "error": e.to_json() })),
        }
    }

    /// An item read again after a publish or restore: the blyg's copy wins.
    /// Returns the `published` outcome (version and permalink).
    pub fn live_apply_item(&mut self, id: &str, status: u16, body: &str, note: &str) -> String {
        let lid = LocalId(id.into());
        match api::read(status, body, None).and_then(|v| api::parse_item(&v)) {
            Ok(w) => {
                self.store.apply_server(&lid, &w, None, true);
                if let Some(it) = self.store.item(&lid) {
                    self.list.update_item(it.clone());
                    self.keep_open(&lid);
                    json(&Outcome::Published {
                        version: it.version,
                        permalink: it.permalink.unwrap_or_default(),
                        note: Some(note.trim().to_string()).filter(|n| !n.is_empty()),
                    })
                } else {
                    json(&Outcome::Nothing)
                }
            }
            Err(e) => json(&json!({ "error": e.to_json() })),
        }
    }

    /// `GET /api/capabilities`: `{mode: live|signin|unsupported|demo|offline}`.
    pub fn detect(status: u16, body: &str, www_authenticate: bool) -> String {
        json(&match api::detect(status, body, www_authenticate) {
            api::Mode::Live { contract } => json!({ "mode": "live", "contract": contract }),
            api::Mode::SignIn => json!({ "mode": "signin" }),
            api::Mode::Unsupported { why } => json!({ "mode": "unsupported", "why": why }),
            api::Mode::Demo => json!({ "mode": "demo" }),
            api::Mode::Offline => json!({ "mode": "offline" }),
        })
    }

    /// The blyg's public base from `GET /api/settings` (`siteUrl`), else
    /// `origin`.
    ///
    /// Folded to the origin's one spelling (§12.2: lowercase scheme and host,
    /// no default port, uppercase escapes), then held without its final `/`
    /// because it is the base URLs are built on; comparisons go through
    /// `blyg_core::origin::same`, which puts the `/` back.
    pub fn public_base(settings_json: &str, origin: &str) -> String {
        let base = |s: &str| {
            blyg_core::origin::canonical(s)
                .map(|o| o.trim_end_matches('/').to_string())
                .unwrap_or_else(|| s.trim_end_matches('/').to_string())
        };
        serde_json::from_str::<Value>(settings_json)
            .ok()
            .and_then(|v| v.get("siteUrl").and_then(Value::as_str).map(base))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| base(origin))
    }

    /// What one API answer means, as `{code, text, …}` (or `null` for 2xx).
    pub fn api_error(status: u16, body: &str) -> String {
        match api::read(status, body, None) {
            Ok(_) => "null".into(),
            Err(e) => json(&e.to_json()),
        }
    }
}

/// A list read page by page (`crate::api::Pager`), for the page.
#[wasm_bindgen]
pub struct Pager(api::Pager);

#[wasm_bindgen]
impl Pager {
    #[wasm_bindgen(constructor)]
    pub fn new(path: &str, query: &str, limit: u32) -> Pager {
        Pager(api::Pager::new(path, query, limit))
    }

    /// The next page to fetch, or `undefined` when the list is read.
    pub fn next_path(&self) -> Option<String> {
        self.0.next_path()
    }

    /// One page's answer: `null` when fine, else the error.
    pub fn feed(&mut self, status: u16, body: &str, retry_after: Option<String>) -> String {
        match self.0.feed(status, body, retry_after.as_deref()) {
            Ok(_) => "null".into(),
            Err(e) => json(&e.to_json()),
        }
    }

    /// Everything read, as a JSON array (once `next_path` is `undefined`).
    pub fn items(&mut self) -> String {
        json(&self.0.take_items())
    }
}

impl Burrow {
    /// Put unsent read marks and hopper changes over the blyg's rows.
    pub(crate) fn overlay_reading(&mut self) {
        for r in &mut self.rd.rows {
            let key = key_of(r);
            if let Some(op) = self
                .store
                .live
                .reads
                .iter()
                .rev()
                .find(|o| format!("{}|{}", o.sub, o.remote_id) == key)
            {
                r.read_version = op.version;
            }
            for h in &self.store.live.hops {
                if format!("{}|{}", h.sub, h.remote_id) != key {
                    continue;
                }
                let has = r.hoppers.contains(&h.hopper);
                if h.on && !has {
                    r.hoppers.push(h.hopper.clone());
                } else if !h.on && has {
                    r.hoppers.retain(|x| x != &h.hopper);
                }
            }
        }
    }
}
