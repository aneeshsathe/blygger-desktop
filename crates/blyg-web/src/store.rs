//! The browser's store: Burrow's `Item`s in memory, behaving as the native
//! fake backend does (`crates/blyg-app/src/fake.rs`: search, create, save,
//! kind, scratch notes and their promotion, publish, delete, discard). The
//! page persists [`WebStore::to_json`] to IndexedDB and hands it back to
//! [`WebStore::from_json`] on the next visit. Nothing here touches a network.

use std::collections::HashMap;

use blyg_core::{
    Hopper, Item, Kind, LocalId, ReadingItem, RemoteRef, ServerId, Status, promotion_kind,
};
use serde::{Deserialize, Serialize};

use crate::seed;

/// A published version, as the fake's version list records it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Published {
    pub version: u32,
    pub at: String,
    pub note: Option<String>,
    /// The text that version published (what Discard changes goes back to).
    pub content_md: String,
}

/// The most posts the shelf holds (studio's `PUT /shelf` limit; the
/// native shelf refuses a 51st the same way).
pub const SHELF_MAX: usize = 50;

#[derive(Debug, Default)]
pub struct WebStore {
    items: Vec<Item>,
    versions: HashMap<LocalId, Vec<Published>>,
    next_id: u64,
    /// Your shelf, in order (local only, as the native shelf is today).
    pub shelf: Vec<LocalId>,
    /// Reading rows you've read or marked: `"sub|remote id"` → the version
    /// read (`None`: marked unread). Rows not here keep the seed's state.
    pub read: HashMap<String, Option<u32>>,
    /// Reading rows you've put in hoppers: `"sub|remote id"` → hopper ids.
    pub hopped: HashMap<String, Vec<String>>,
    /// Live mode: the blyg's own state, and what waits to be sent.
    pub live: Live,
}

/// What live mode keeps beside the items (all of it persisted, so it
/// survives a reload with no network): each item's revision on the blyg,
/// conflicts waiting for you, and the outbox. Item edits wait as the items
/// themselves (`pending_sync`), sent with the revision they were based on.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Live {
    pub on: bool,
    /// The blyg's public base (its `siteUrl`, else the origin), for
    /// permalinks and the preview.
    #[serde(default)]
    pub base: String,
    /// local id → the item's revision on the blyg, as last seen.
    #[serde(default)]
    pub rev: HashMap<String, u64>,
    /// local id → the blyg's copy, when it changed under your edit.
    #[serde(default)]
    pub conflicts: HashMap<String, Conflict>,
    /// Drafts deleted here, waiting to be deleted on the blyg.
    #[serde(default)]
    pub deletes: Vec<PendingDelete>,
    /// Read marks waiting to be sent (`None`: mark unread).
    #[serde(default)]
    pub reads: Vec<ReadOp>,
    /// Hopper changes waiting to be sent.
    #[serde(default)]
    pub hops: Vec<HopOp>,
    /// The reading list and hoppers as last read, for offline.
    #[serde(default)]
    pub reading: Vec<ReadingItem>,
    #[serde(default)]
    pub hoppers: Vec<Hopper>,
}

/// The blyg's copy of an item that changed under a local edit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conflict {
    pub server_md: String,
    pub server_rev: u64,
    pub server_kind: Kind,
    pub server_status: Status,
    pub server_version: u32,
    pub server_dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingDelete {
    pub server_id: String,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadOp {
    pub sub: String,
    pub remote_id: String,
    pub version: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HopOp {
    pub hopper: String,
    pub sub: String,
    pub remote_id: String,
    pub on: bool,
}

/// The persisted form of an `Item` (the model's `Item` has no serde).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Stored {
    id: String,
    server_id: Option<String>,
    kind: Kind,
    status: Status,
    version: u32,
    dirty: bool,
    content_md: String,
    created: String,
    updated: String,
    permalink: Option<String>,
    #[serde(default)]
    versions: Vec<Published>,
    /// What a reply stub answers.
    #[serde(default)]
    stub_of: Option<RemoteRef>,
    /// Live mode: an edit not yet on the blyg (the outbox).
    #[serde(default)]
    pending: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct Snapshot {
    /// Bumped if the stored shape ever changes; older snapshots are dropped.
    format: u32,
    next_id: u64,
    items: Vec<Stored>,
    #[serde(default)]
    shelf: Vec<String>,
    #[serde(default)]
    read: HashMap<String, Option<u32>>,
    #[serde(default)]
    hopped: HashMap<String, Vec<String>>,
    #[serde(default)]
    live: Live,
}

const FORMAT: u32 = 1;

impl WebStore {
    /// The sample data, as of `now` (RFC 3339).
    pub fn seeded(now: chrono::DateTime<chrono::Utc>) -> Self {
        let items = seed::items(now);
        let versions = items
            .iter()
            .filter(|i| i.status == Status::Public)
            .map(|i| {
                let vs = (1..=i.version)
                    .map(|v| Published {
                        version: v,
                        at: i.updated.clone(),
                        note: None,
                        content_md: i.content_md.clone(),
                    })
                    .collect();
                (i.local_id.clone(), vs)
            })
            .collect();
        WebStore {
            items,
            versions,
            next_id: 1,
            ..WebStore::default()
        }
    }

    pub fn from_json(json: &str) -> Option<Self> {
        let snap: Snapshot = serde_json::from_str(json).ok()?;
        if snap.format != FORMAT {
            return None;
        }
        let mut store = WebStore {
            next_id: snap.next_id,
            shelf: snap.shelf.into_iter().map(LocalId).collect(),
            read: snap.read,
            hopped: snap.hopped,
            live: snap.live,
            ..WebStore::default()
        };
        for s in snap.items {
            let id = LocalId(s.id.clone());
            let mut it = seed::blank(&s.id, s.kind, s.status, &s.content_md, &s.created);
            it.server_id = s.server_id.map(ServerId);
            it.version = s.version;
            it.dirty = s.dirty;
            it.updated = s.updated;
            it.permalink = s.permalink;
            it.stub_of = s.stub_of;
            // The demo's "sync" is pretend, so only live mode keeps it.
            it.pending_sync = s.pending && store.live.on;
            it.conflict = store.live.conflicts.contains_key(&s.id);
            if !s.versions.is_empty() {
                store.versions.insert(id, s.versions);
            }
            store.items.push(it);
        }
        Some(store)
    }

    pub fn to_json(&self) -> String {
        let items = self
            .items
            .iter()
            .map(|i| Stored {
                id: i.local_id.0.clone(),
                server_id: i.server_id.as_ref().map(|s| s.0.clone()),
                kind: i.kind,
                status: i.status,
                version: i.version,
                dirty: i.dirty,
                content_md: i.content_md.clone(),
                created: i.created.clone(),
                updated: i.updated.clone(),
                permalink: i.permalink.clone(),
                versions: self.versions.get(&i.local_id).cloned().unwrap_or_default(),
                stub_of: i.stub_of.clone(),
                pending: i.pending_sync,
            })
            .collect();
        serde_json::to_string(&Snapshot {
            format: FORMAT,
            next_id: self.next_id,
            items,
            shelf: self.shelf.iter().map(|i| i.0.clone()).collect(),
            read: self.read.clone(),
            hopped: self.hopped.clone(),
            live: self.live.clone(),
        })
        .unwrap_or_default()
    }

    fn sorted(mut v: Vec<Item>) -> Vec<Item> {
        v.sort_by(|a, b| b.updated.cmp(&a.updated));
        v
    }

    pub fn items(&self) -> Vec<Item> {
        Self::sorted(self.items.clone())
    }

    /// Case-insensitive substring over the whole text, newest first (the
    /// fake's `search`; the SQLite store's FTS behaves the same for this).
    pub fn search(&self, query: &str) -> Vec<Item> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return self.items();
        }
        Self::sorted(
            self.items
                .iter()
                .filter(|i| i.content_md.to_lowercase().contains(&q))
                .cloned()
                .collect(),
        )
    }

    /// Live mode: an empty store for the blyg at `base`.
    pub fn live(base: &str) -> Self {
        WebStore {
            next_id: 1,
            live: Live {
                on: true,
                base: base.to_string(),
                ..Live::default()
            },
            ..WebStore::default()
        }
    }

    pub(crate) fn get_mut_pub(&mut self, id: &LocalId) -> Option<&mut Item> {
        self.get_mut(id)
    }

    /// Put an item in (or replace it), keeping its place.
    pub(crate) fn upsert(&mut self, it: Item) {
        match self.items.iter_mut().find(|i| i.local_id == it.local_id) {
            Some(slot) => *slot = it,
            None => self.items.push(it),
        }
    }

    /// The local id of the item the blyg knows as `server_id`.
    pub fn by_server(&self, server_id: &str) -> Option<LocalId> {
        self.items
            .iter()
            .find(|i| i.server_id.as_ref().is_some_and(|s| s.0 == server_id))
            .map(|i| i.local_id.clone())
    }

    /// Every item, in no particular order (for the outbox).
    pub(crate) fn raw(&self) -> &[Item] {
        &self.items
    }

    pub fn item(&self, id: &LocalId) -> Option<Item> {
        self.items.iter().find(|i| &i.local_id == id).cloned()
    }

    fn get_mut(&mut self, id: &LocalId) -> Option<&mut Item> {
        self.items.iter_mut().find(|i| &i.local_id == id)
    }

    pub fn versions(&self, id: &LocalId) -> Vec<Published> {
        self.versions.get(id).cloned().unwrap_or_default()
    }

    pub fn insert(
        &mut self,
        kind: Kind,
        content_md: &str,
        status: Status,
        now: &str,
        ms: i64,
    ) -> LocalId {
        let n = self.next_id;
        self.next_id += 1;
        let id = format!("local-{n}-{ms}");
        let mut it = seed::blank(&id, kind, status, content_md, now);
        it.pending_sync = status != Status::Scratch;
        self.items.push(it);
        LocalId(id)
    }

    /// Every keystroke lands here. Returns false if nothing changed.
    pub fn save(&mut self, id: &LocalId, content_md: &str, now: &str) -> bool {
        let Some(it) = self.get_mut(id) else {
            return false;
        };
        if it.content_md == content_md {
            return false;
        }
        it.content_md = content_md.to_string();
        it.updated = now.to_string();
        it.pending_sync = it.status != Status::Scratch;
        if it.status == Status::Public {
            it.dirty = true;
        }
        true
    }

    pub fn set_kind(&mut self, id: &LocalId, kind: Kind, now: &str) -> bool {
        let Some(it) = self.get_mut(id) else {
            return false;
        };
        if it.kind == kind {
            return false;
        }
        it.kind = kind;
        it.updated = now.to_string();
        if it.status == Status::Public {
            it.dirty = true;
            if let Some(sid) = &it.server_id {
                it.permalink = Some(seed::permalink(kind, &sid.0));
            }
        }
        true
    }

    /// ⌘D: a scratch note becomes a draft (same id), its kind chosen as the
    /// native app chooses it (`blyg_core::promotion_kind`).
    pub fn promote(&mut self, id: &LocalId, now: &str) -> Option<Kind> {
        let it = self.get_mut(id)?;
        if it.status == Status::Scratch {
            it.kind = promotion_kind(it.kind, &it.content_md);
            it.status = Status::Draft;
            it.pending_sync = true;
            it.updated = now.to_string();
        }
        Some(it.kind)
    }

    /// Publish the working copy as the next version.
    pub fn publish(&mut self, id: &LocalId, note: Option<String>, now: &str) -> Option<Item> {
        let it = self.get_mut(id)?;
        if it.status == Status::Scratch {
            it.kind = promotion_kind(it.kind, &it.content_md);
        }
        let sid = it
            .server_id
            .clone()
            .unwrap_or_else(|| ServerId(server_id_for(&it.local_id.0)));
        it.server_id = Some(sid.clone());
        it.version += 1;
        it.status = Status::Public;
        it.dirty = false;
        it.pending_sync = false;
        it.updated = now.to_string();
        it.permalink = Some(seed::permalink(it.kind, &sid.0));
        let done = it.clone();
        self.versions
            .entry(id.clone())
            .or_default()
            .push(Published {
                version: done.version,
                at: now.to_string(),
                note,
                content_md: done.content_md.clone(),
            });
        Some(done)
    }

    pub fn delete(&mut self, id: &LocalId) -> bool {
        let before = self.items.len();
        self.items.retain(|i| &i.local_id != id);
        self.versions.remove(id);
        self.shelf.retain(|s| s != id);
        self.items.len() != before
    }

    /// Discard changes: the working copy goes back to `version` as published.
    pub fn restore(&mut self, id: &LocalId, version: u32, now: &str) -> Option<Item> {
        let md = self
            .versions
            .get(id)?
            .iter()
            .find(|v| v.version == version)?
            .content_md
            .clone();
        let it = self.get_mut(id)?;
        it.content_md = md;
        it.dirty = false;
        it.updated = now.to_string();
        Some(it.clone())
    }

    /// A reply stub or quote thread: a draft thread with this text, which
    /// answers `stub_of` when it's a reply.
    pub fn insert_thread(
        &mut self,
        content_md: &str,
        stub_of: Option<RemoteRef>,
        now: &str,
        ms: i64,
    ) -> LocalId {
        let id = self.insert(Kind::Thread, content_md, Status::Draft, now, ms);
        if let Some(it) = self.get_mut(&id) {
            it.stub_of = stub_of;
        }
        id
    }

    /// Put a post on the shelf (at the end) or take it off. `Err` when the
    /// shelf is full. Withdrawn posts aren't shelved, as natively.
    pub fn set_on_shelf(&mut self, id: &LocalId, on: bool) -> Result<bool, String> {
        let here = self.shelf.contains(id);
        if on == here {
            return Ok(false);
        }
        if !on {
            self.shelf.retain(|s| s != id);
            return Ok(true);
        }
        match self.item(id) {
            Some(it) if it.status == Status::Withdrawn => {
                Err("Withdrawn posts can't go on the shelf".into())
            }
            None => Err("No such post".into()),
            Some(_) if self.shelf.len() >= SHELF_MAX => Err(format!(
                "Your shelf is full ({SHELF_MAX} posts). Take one off first."
            )),
            Some(_) => {
                self.shelf.push(id.clone());
                Ok(true)
            }
        }
    }

    /// Move a shelved post to `index` (clamped).
    pub fn move_on_shelf(&mut self, id: &LocalId, index: usize) {
        if let Some(ix) = self.shelf.iter().position(|s| s == id) {
            let it = self.shelf.remove(ix);
            let to = index.min(self.shelf.len());
            self.shelf.insert(to, it);
        }
    }

    /// Shelved posts first, in shelf order; the rest in the order given.
    /// The same rule as the native list (`vm::ListModel::set_shelf` on the
    /// `shelf` branch), applied before the list sees the results.
    pub fn arrange(&self, mut results: Vec<Item>) -> (Vec<Item>, usize) {
        let mut top = Vec::new();
        for id in &self.shelf {
            if let Some(ix) = results.iter().position(|i| &i.local_id == id) {
                top.push(results.remove(ix));
            }
        }
        let n = top.len();
        top.extend(results);
        (top, n)
    }

    /// Mark everything pushed (the debounced "sync" of the demo).
    pub fn mark_synced(&mut self) {
        for it in &mut self.items {
            it.pending_sync = false;
        }
    }

    pub fn pending(&self) -> usize {
        self.items.iter().filter(|i| i.pending_sync).count()
            + self.live.deletes.len()
            + self.live.reads.len()
            + self.live.hops.len()
    }
}

/// A 26-character id in the blyg alphabet, derived from the local id (the
/// sample blyg's stand-in for the id the Worker would assign).
fn server_id_for(local: &str) -> String {
    let alphabet = blyg_render::ID_ALPHABET.as_bytes();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut out = String::with_capacity(26);
    for i in 0..26u64 {
        for b in local.bytes().chain(i.to_le_bytes()) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        out.push(alphabet[(h % 32) as usize] as char);
    }
    out
}
