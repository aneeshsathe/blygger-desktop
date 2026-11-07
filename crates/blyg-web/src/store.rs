//! The browser's store: Burrow's `Item`s in memory, behaving as the native
//! fake backend does (`crates/blyg-app/src/fake.rs`: search, create, save,
//! kind, scratch notes and their promotion, publish, delete, discard). The
//! page persists [`WebStore::to_json`] to IndexedDB and hands it back to
//! [`WebStore::from_json`] on the next visit. Nothing here touches a network.

use std::collections::HashMap;

use blyg_core::{Item, Kind, LocalId, ServerId, Status, promotion_kind};
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

#[derive(Debug, Default)]
pub struct WebStore {
    items: Vec<Item>,
    versions: HashMap<LocalId, Vec<Published>>,
    next_id: u64,
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
}

#[derive(Debug, Serialize, Deserialize)]
struct Snapshot {
    /// Bumped if the stored shape ever changes; older snapshots are dropped.
    format: u32,
    next_id: u64,
    items: Vec<Stored>,
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
        }
    }

    pub fn from_json(json: &str) -> Option<Self> {
        let snap: Snapshot = serde_json::from_str(json).ok()?;
        if snap.format != FORMAT {
            return None;
        }
        let mut store = WebStore {
            next_id: snap.next_id,
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
            })
            .collect();
        serde_json::to_string(&Snapshot {
            format: FORMAT,
            next_id: self.next_id,
            items,
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

    /// Mark everything pushed (the debounced "sync" of the demo).
    pub fn mark_synced(&mut self) {
        for it in &mut self.items {
            it.pending_sync = false;
        }
    }

    pub fn pending(&self) -> usize {
        self.items.iter().filter(|i| i.pending_sync).count()
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
