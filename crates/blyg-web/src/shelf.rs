//! --- shelf sync --- The shelf in the browser, as natively
//! (`blyg_core::shelf`, `store/shelf.rs`): your posts and anyone's, in your
//! order, at most fifty. Your own also stay at the top of the Write list;
//! everything shows in Reading's Shelf lens.
//!
//! Live, on a blyg whose capabilities list `getShelf` and `putShelf`
//! (studio 0.61), the shelf syncs: `live::LISTS` reads it again when the
//! `shelf` change counter moved, and a refresh sends it when it differs from
//! what the blyg last agreed (`changes_plan` marks it stale then). The page
//! does the fetching (`www/live.js` `syncShelf`, through `net.js` `call`):
//! `shelf_answer` takes the `GET`, merges this side's changes onto it
//! (`blyg_core::shelf::merge`) and says what to `PUT`; `shelf_stored` takes
//! the `PUT`'s answer, and on a 409 merges onto the list the conflict
//! carries (`details.current`) and says what to send once more. On any
//! other blyg, or in the demo, the shelf is this browser's.

use blyg_core::shelf::{self, Entry, SyncState};
use blyg_core::{Item, LocalId, Status};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wasm_bindgen::prelude::*;

use crate::api::{self, ApiError};
use crate::store::{SHELF_MAX, WebStore};
use crate::{Burrow, Outcome, json};

/// One place on the shelf: a post of yours, by its local id, or a post as
/// the blyg names it (someone else's, or yours before this browser holds
/// it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shelved {
    Own(String),
    Entry(Entry),
}

impl WebStore {
    /// The shelf's place for a post of yours: by its local id, or by its
    /// server id as the blyg named it.
    fn shelf_pos(&self, id: &LocalId) -> Option<usize> {
        let sid = self.item(id).and_then(|i| i.server_id).map(|s| s.0);
        self.shelf.iter().position(|s| match s {
            Shelved::Own(l) => l == &id.0,
            Shelved::Entry(e) => e.is_own() && Some(&e.remote_id) == sid.as_ref(),
        })
    }

    fn own_item(&self, s: &Shelved) -> Option<Item> {
        match s {
            Shelved::Own(l) => self.item(&LocalId(l.clone())),
            Shelved::Entry(e) if e.is_own() => self
                .raw()
                .iter()
                .find(|i| i.server_id.as_ref().is_some_and(|x| x.0 == e.remote_id))
                .cloned(),
            Shelved::Entry(_) => None,
        }
    }

    /// Your posts on the shelf, in order (withdrawn ones left out).
    pub fn own_shelf(&self) -> Vec<LocalId> {
        self.shelf
            .iter()
            .filter_map(|s| self.own_item(s))
            .filter(|i| i.status != Status::Withdrawn)
            .map(|i| i.local_id)
            .collect()
    }

    pub fn is_shelved(&self, id: &LocalId) -> bool {
        self.shelf_pos(id).is_some()
    }

    /// The shelf as the blyg names it: your posts by their server ids (not
    /// withdrawn ones, nor ones it doesn't have yet), others' as they are.
    pub fn wire_shelf(&self) -> Vec<Entry> {
        self.shelf
            .iter()
            .filter_map(|s| match s {
                Shelved::Own(_) => {
                    let it = self.own_item(s)?;
                    (it.status != Status::Withdrawn).then_some(Entry::own(it.server_id?.0))
                }
                Shelved::Entry(e) => Some(e.clone()),
            })
            .collect()
    }

    /// Put a post of yours on the shelf (at the end) or take it off. `Err`
    /// when the shelf is full. Withdrawn posts aren't shelved, as natively.
    pub fn set_on_shelf(&mut self, id: &LocalId, on: bool) -> Result<bool, String> {
        let at = self.shelf_pos(id);
        match (on, at) {
            (true, Some(_)) | (false, None) => Ok(false),
            (false, Some(ix)) => {
                self.shelf.remove(ix);
                Ok(true)
            }
            (true, None) => match self.item(id) {
                Some(it) if it.status == Status::Withdrawn => {
                    Err("Withdrawn posts can't go on the shelf".into())
                }
                None => Err("No such post".into()),
                Some(_) if self.shelf.len() >= SHELF_MAX => Err(format!(
                    "Your shelf is full ({SHELF_MAX} posts). Take one off first."
                )),
                Some(_) => {
                    self.shelf.push(Shelved::Own(id.0.clone()));
                    Ok(true)
                }
            },
        }
    }

    /// Anyone's post, by the reading row's key: on (at the end) or off.
    pub fn set_entry_on_shelf(&mut self, e: &Entry, on: bool) -> Result<bool, String> {
        if e.is_own()
            && let Some(id) = self
                .raw()
                .iter()
                .find(|i| i.server_id.as_ref().is_some_and(|s| s.0 == e.remote_id))
                .map(|i| i.local_id.clone())
        {
            return self.set_on_shelf(&id, on);
        }
        let at = self
            .shelf
            .iter()
            .position(|s| matches!(s, Shelved::Entry(x) if x == e));
        match (on, at) {
            (true, Some(_)) | (false, None) => Ok(false),
            (false, Some(ix)) => {
                self.shelf.remove(ix);
                Ok(true)
            }
            (true, None) if self.shelf.len() >= SHELF_MAX => Err(format!(
                "Your shelf is full ({SHELF_MAX} posts). Take one off first."
            )),
            (true, None) => {
                self.shelf.push(Shelved::Entry(e.clone()));
                Ok(true)
            }
        }
    }

    /// Move a post of yours to `index` among your shelved posts (clamped);
    /// others' posts keep their places.
    pub fn move_on_shelf(&mut self, id: &LocalId, index: usize) {
        let Some(from) = self.shelf_pos(id) else {
            return;
        };
        let visible = self.own_shelf();
        let anchor = visible.iter().filter(|v| *v != id).nth(index).cloned();
        let last = visible.iter().rfind(|v| *v != id).cloned();
        let it = self.shelf.remove(from);
        let to = match (anchor, last) {
            (Some(a), _) => self.shelf_pos(&a).unwrap_or(self.shelf.len()),
            (None, Some(l)) => self.shelf_pos(&l).map_or(self.shelf.len(), |p| p + 1),
            (None, None) => self.shelf.len(),
        };
        self.shelf.insert(to.min(self.shelf.len()), it);
    }

    /// Shelved posts first, in shelf order; the rest in the order given.
    /// The same rule as the native list (`vm::ListModel::set_shelf`),
    /// applied before the list sees the results.
    pub fn arrange(&self, mut results: Vec<Item>) -> (Vec<Item>, usize) {
        let mut top = Vec::new();
        for id in self.own_shelf() {
            if let Some(ix) = results.iter().position(|i| i.local_id == id) {
                top.push(results.remove(ix));
            }
        }
        let n = top.len();
        top.extend(results);
        (top, n)
    }

    /// Lay the blyg's list over the shelf: what it names, in its order;
    /// your posts it can't name (not sent yet, withdrawn) keep their
    /// places. True when anything changed.
    pub fn apply_shelf(&mut self, server: &[Entry]) -> bool {
        let old = self.shelf.clone();
        let mut next: Vec<Shelved> = Vec::with_capacity(server.len());
        for e in server {
            let s = match e.is_own().then(|| {
                self.raw()
                    .iter()
                    .find(|i| i.server_id.as_ref().is_some_and(|s| s.0 == e.remote_id))
                    .map(|i| i.local_id.0.clone())
            }) {
                Some(Some(l)) => Shelved::Own(l),
                _ => Shelved::Entry(e.clone()),
            };
            if !next.contains(&s) {
                next.push(s);
            }
        }
        let local_only = |s: &Shelved| match s {
            Shelved::Own(_) => self
                .own_item(s)
                .is_none_or(|i| i.server_id.is_none() || i.status == Status::Withdrawn),
            Shelved::Entry(_) => false,
        };
        let kept: Vec<Shelved> = old.iter().filter(|s| local_only(s)).cloned().collect();
        shelf::place_after(&old, &mut next, |s| kept.contains(s));
        let changed = next != old;
        self.shelf = next;
        changed
    }

    /// The shelf differs from what the blyg last agreed.
    /// (Not when the blyg refused this very list: it waits for a change.)
    pub fn shelf_unsent(&self) -> bool {
        let wire = self.wire_shelf();
        self.live.shelf_base.as_ref() != Some(&wire)
            && self.live.shelf_refused.as_ref() != Some(&wire)
    }
}

fn unexpected() -> ApiError {
    ApiError::Rejected {
        status: 200,
        message: "unexpected response from /api/shelf".into(),
        details: vec![],
    }
}

/// What the page does next about the shelf.
#[derive(Debug, Default, Serialize, PartialEq)]
pub struct ShelfStep {
    /// `PUT` this.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub put: Option<api::Request>,
    /// The shelf here changed: redraw.
    pub changed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toast: Option<String>,
}

impl Burrow {
    /// Live, on a blyg that keeps a shelf.
    pub(crate) fn shelf_synced(&self) -> bool {
        self.is_live()
            && !self.store.live.shelf_off
            && self.can(shelf::GET_OPERATION)
            && self.can(shelf::PUT_OPERATION)
    }

    /// The blyg holds `r` now.
    fn shelf_agreed(&mut self, r: &shelf::Remote) -> bool {
        let changed = self.store.apply_shelf(&r.entries);
        self.store.live.shelf_base = Some(r.entries.clone());
        self.store.live.shelf_rev = r.revision;
        self.store.live.shelf_refused = None;
        if changed {
            self.rows_changed();
            self.refresh();
        }
        changed
    }

    /// Merge onto the blyg's list `r`: settled (nothing to send), or what to
    /// `PUT`.
    fn shelf_merge(&mut self, r: &shelf::Remote) -> ShelfStep {
        let local = self.store.wire_shelf();
        let base = self.store.live.shelf_base.clone();
        let (merged, over) = shelf::cap(shelf::merge(base.as_deref(), &local, &r.entries));
        if merged == r.entries {
            return ShelfStep {
                changed: self.shelf_agreed(r),
                ..ShelfStep::default()
            };
        }
        ShelfStep {
            put: Some(api::Request {
                method: "PUT",
                path: "/api/shelf".into(),
                body: Some(shelf::put_body(&merged, r.revision)),
            }),
            toast: (over > 0).then(|| shelf::overflow_message(over)),
            ..ShelfStep::default()
        }
    }

    fn shelf_err(e: ApiError) -> ShelfStep {
        ShelfStep {
            error: Some(e.to_json()),
            ..ShelfStep::default()
        }
    }
}

#[wasm_bindgen]
impl Burrow {
    /// Whether the shelf syncs with this blyg (live, and its capabilities
    /// offer it).
    pub fn shelf_served(&self) -> bool {
        self.shelf_synced()
    }

    /// `local`, `synced` or `waiting`, for the shelf's heading.
    pub fn shelf_state(&self) -> String {
        match self.shelf_sync_state() {
            SyncState::Local => "local",
            SyncState::Synced => "synced",
            SyncState::Waiting => "waiting",
        }
        .into()
    }

    /// The answer to `GET /api/shelf`: `{changed}` when it's settled, or
    /// `{put}` to send; `{error}` when it failed. A 404: the blyg doesn't
    /// keep one after all; the shelf stays here.
    pub fn shelf_answer(&mut self, status: u16, body: &str) -> String {
        match api::read(status, body, None) {
            Ok(v) => match shelf::parse(&v) {
                Some(r) => json(&self.shelf_merge(&r)),
                None => json(&Self::shelf_err(unexpected())),
            },
            Err(ApiError::NotFound) => {
                self.store.live.shelf_off = true;
                json(&ShelfStep::default())
            }
            Err(e) => json(&Self::shelf_err(e)),
        }
    }

    /// The answer to the `PUT`. Stored: `{changed}`. A 409 with the list
    /// that's there now: merged onto it, `{put}` once more when `retry`,
    /// else left for the next refresh. Refused: `{error}`.
    pub fn shelf_stored(&mut self, status: u16, body: &str, retry: bool) -> String {
        if status == 409 {
            let current = serde_json::from_str::<Value>(body)
                .ok()
                .and_then(|v| shelf::conflict_current(&v));
            return json(&match current {
                Some(r) if retry => self.shelf_merge(&r),
                _ => ShelfStep::default(),
            });
        }
        match api::read(status, body, None) {
            Ok(v) => match shelf::parse(&v) {
                Some(r) => json(&ShelfStep {
                    changed: self.shelf_agreed(&r),
                    ..ShelfStep::default()
                }),
                None => json(&Self::shelf_err(unexpected())),
            },
            Err(e) => {
                // Refused outright: not sent again until the shelf changes.
                if matches!(
                    e,
                    ApiError::Forbidden { .. }
                        | ApiError::Rejected {
                            status: 400 | 422,
                            ..
                        }
                ) {
                    self.store.live.shelf_refused = Some(self.store.wire_shelf());
                }
                json(&Self::shelf_err(e))
            }
        }
    }

    /// "+ Add to shelf" / "− Remove from shelf" on a reading row (`key`:
    /// `sub|remote id`).
    pub fn toggle_reading_shelf(&mut self, key: &str) -> String {
        let Some((sub, rid)) = key.split_once('|') else {
            return json(&Outcome::Nothing);
        };
        let e = Entry::new(sub, rid);
        let on = !self.store.wire_shelf().contains(&e);
        match self.store.set_entry_on_shelf(&e, on) {
            Ok(_) => {
                self.rows_changed();
                self.refresh();
                json(&Outcome::Toast {
                    message: if on {
                        "Added to shelf"
                    } else {
                        "Removed from shelf"
                    }
                    .into(),
                })
            }
            Err(message) => json(&Outcome::Toast { message }),
        }
    }

    /// Whether the reading row `key` is on the shelf.
    pub fn reading_shelved(&self, key: &str) -> bool {
        key.split_once('|')
            .is_some_and(|(s, r)| self.store.wire_shelf().contains(&Entry::new(s, r)))
    }
}

impl Burrow {
    pub(crate) fn shelf_sync_state(&self) -> SyncState {
        if !self.shelf_synced() {
            SyncState::Local
        } else if self.store.shelf_unsent() {
            SyncState::Waiting
        } else {
            SyncState::Synced
        }
    }

    /// The Shelf lens's rows, in the shelf's order.
    pub(crate) fn shelf_order<'a>(
        &self,
        rows: Vec<&'a blyg_core::ReadingItem>,
    ) -> Vec<&'a blyg_core::ReadingItem> {
        self.store
            .wire_shelf()
            .iter()
            .filter_map(|e| {
                rows.iter()
                    .find(|r| r.subscription_id == e.sub && r.remote_id == e.remote_id)
                    .copied()
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "shelf_tests.rs"]
mod tests;
