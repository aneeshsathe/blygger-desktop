//! --- shelf sync --- Keeping the shelf the same here and on the blyg
//! (studio 0.61's `GET/PUT /api/shelf`; the shapes and the merge are in
//! `crate::shelf`).
//!
//! - Read when the blyg's capabilities list `getShelf`, and written only
//!   when they list `putShelf` too (noted once a session, with the
//!   contract check; the studio lists only what a sign-in's scopes grant).
//!   Without `getShelf` the shelf stays on this computer, as before, and
//!   nothing is sent. With `getShelf` alone (a read-only sign-in) it's
//!   shown as on a newer contract, below: the union, and changes made here
//!   kept here (`SyncState::HeldReadOnly`).
//! - Read when the `shelf` change counter moved (`SHELF_DEPS`, with the
//!   rest of a pull), and on the first pull of a session.
//! - Sent when it changed here (a queued `shelf` op, or a pull that finds
//!   the shelf differs from what the blyg last agreed): the blyg's list is
//!   read, this side's changes since the last agreement are applied on top
//!   of it (`shelf::merge`), and the result is `PUT` with the revision it
//!   was read at. A 409 (it moved again meanwhile) reads and merges once
//!   more; a second 409 leaves both sides as they are for the next pull.
//! - The first sync takes the union, the blyg's order first.
//! - On a contract newer than Burrow knows (`sync/newer.rs`) the shelf is
//!   read and shown here as the union (nothing comes off on its word; what
//!   this side took off stays off), and never written: a change made here
//!   stays queued until a Burrow that knows the contract sends it. A union
//!   sent back would bring back what another device took off, and the
//!   50-entry cap would trim it. Meanwhile the status bar reads Synced (a
//!   held op isn't due) and Saved says the change is syncing: the
//!   once-a-session notice is the only sign that it's held.
//! - Your posts the blyg can't name yet (scratch notes, drafts not sent)
//!   stay on this computer's shelf in their places, and go once they can.

use super::Engine;
use crate::api::shelf_api::PutShelf;
use crate::backend::{CoreError, Result};
use crate::model::CoreEvent;
use crate::shelf::{
    BASE_META, Entry, REVISION_META, Remote, SERVED_META, SyncState, WRITES_META, cap, merge,
    overflow_message,
};
use crate::store::Op;

/// `meta`: the local list (JSON `[Entry]`) the blyg last refused outright
/// (a 400 or 403): not offered again on its own until it changes.
const REFUSED_META: &str = "shelf_refused";

impl Engine {
    /// Note whether the blyg offers the synced shelf (its capabilities).
    /// A failed read leaves what was noted.
    pub(super) fn note_shelf_served(&self) {
        if let Ok(on) = self.api.serves_shelf() {
            self.set_shelf_served(on);
        }
        if let Ok(on) = self.api.takes_shelf() {
            let _ = self.store.set_meta(WRITES_META, if on { "1" } else { "0" });
        }
    }

    /// Whether this sign-in may write the shelf, as of the last check.
    fn shelf_writes(&self) -> bool {
        self.store.meta(WRITES_META).as_deref() == Some("1")
    }

    /// Why a change made here isn't sent: the blyg's contract is newer than
    /// Burrow knows, or this sign-in may only read the shelf. `None`: it's
    /// sent (or there's no synced shelf to hold it for).
    pub(super) fn shelf_held(&self) -> Option<SyncState> {
        if !self.shelf_served() {
            None
        } else if self.store.lists_prune() == crate::api::Prune::Withheld {
            Some(SyncState::HeldNewer)
        } else if !self.shelf_writes() {
            Some(SyncState::HeldReadOnly)
        } else {
            None
        }
    }

    fn set_shelf_served(&self, on: bool) {
        let _ = self.store.set_meta(SERVED_META, if on { "1" } else { "0" });
        if !on {
            let _ = self.store.drop_shelf_ops();
        }
    }

    /// Whether the blyg offers the synced shelf, as of the last check.
    pub fn shelf_served(&self) -> bool {
        self.store.meta(SERVED_META).as_deref() == Some("1")
    }

    /// Where the shelf stands, for the UI.
    pub fn shelf_state(&self) -> SyncState {
        if !self.shelf_served() {
            SyncState::Local
        } else if let Some(held) = self.shelf_held() {
            // Shown as the blyg has it plus what's kept here; only a change
            // made here is worth a word.
            if self.store.shelf_waiting() {
                held
            } else {
                SyncState::Synced
            }
        } else if self.store.shelf_waiting() || self.shelf_unsent() {
            SyncState::Waiting
        } else {
            SyncState::Synced
        }
    }

    /// The list the blyg last agreed to; `None` before the first sync.
    fn shelf_base(&self) -> Option<Vec<Entry>> {
        self.store
            .meta(BASE_META)
            .and_then(|s| serde_json::from_str(&s).ok())
    }

    /// The shelf here differs from what the blyg last agreed (and wasn't
    /// just refused as it is).
    pub(super) fn shelf_unsent(&self) -> bool {
        let wire = self.store.shelf_wire();
        let refused: Option<Vec<Entry>> = self
            .store
            .meta(REFUSED_META)
            .and_then(|s| serde_json::from_str(&s).ok());
        self.shelf_base().as_ref() != Some(&wire) && refused.as_ref() != Some(&wire)
    }

    /// A change here: queue it to be sent, when the blyg offers the shelf.
    pub(crate) fn shelf_changed(&self) -> Result<()> {
        if self.shelf_served() {
            self.store.queue_shelf()?;
        }
        Ok(())
    }

    /// Read the blyg's shelf, merge this side's changes onto it, and send
    /// the result when it differs. True when the shelf here changed.
    pub(super) fn sync_shelf(&self) -> Result<bool> {
        match self.api.get_shelf()? {
            // --- newer contract --- and a read-only sign-in: shown, not written.
            Some(server) if self.shelf_held().is_some() => self.show_shelf_union(&server),
            Some(server) => self.settle_shelf(server, true),
            None => {
                // No route after all: the shelf stays here.
                self.set_shelf_served(false);
                Ok(false)
            }
        }
    }

    fn settle_shelf(&self, server: Remote, retry: bool) -> Result<bool> {
        let base = self.shelf_base();
        let local = self.store.shelf_wire();
        let (merged, over) = cap(merge(base.as_deref(), &local, &server.entries));
        if merged == server.entries {
            let changed = self.store.apply_shelf(&server.entries)?;
            self.agreed(&server)?;
            return Ok(changed);
        }
        match self.api.put_shelf(&merged, server.revision) {
            Ok(PutShelf::Stored(stored)) => {
                if over > 0 {
                    self.emit(CoreEvent::Error(overflow_message(over)));
                }
                let changed = self.store.apply_shelf(&stored.entries)?;
                self.agreed(&stored)?;
                Ok(changed)
            }
            Ok(PutShelf::Conflict) if retry => match self.api.get_shelf()? {
                Some(now) => self.settle_shelf(now, false),
                None => {
                    self.set_shelf_served(false);
                    Ok(false)
                }
            },
            // It kept moving: both stay as they are, and the next pull
            // (which sees the shelf still differs) tries again.
            Ok(PutShelf::Conflict) => Ok(false),
            Err(
                e @ CoreError::Rejected {
                    status: 400 | 403 | 422,
                    ..
                },
            ) => {
                let _ = self.store.set_meta(
                    REFUSED_META,
                    &serde_json::to_string(&local).unwrap_or_default(),
                );
                Err(e)
            }
            Err(e) => Err(e),
        }
    }

    /// --- newer contract --- The blyg's shelf, shown here as the union with
    /// this side's (the blyg's order first), less what this side took off
    /// since the blyg last agreed (in the base, not here): that removal
    /// stays, waiting to be sent. Nothing is sent: no `PUT`, no new base.
    /// True when the shelf here changed.
    pub(super) fn show_shelf_union(&self, server: &Remote) -> Result<bool> {
        let local = self.store.shelf_wire();
        let base = self.shelf_base().unwrap_or_default();
        let taken_off: Vec<&Entry> = base.iter().filter(|e| !local.contains(e)).collect();
        let theirs: Vec<Entry> = server
            .entries
            .iter()
            .filter(|e| !taken_off.contains(e))
            .cloned()
            .collect();
        self.store.apply_shelf(&merge(None, &local, &theirs))
    }

    /// The blyg holds `r`: it's the base of the next merge.
    fn agreed(&self, r: &Remote) -> Result<()> {
        self.store.set_meta(
            BASE_META,
            &serde_json::to_string(&r.entries).unwrap_or_default(),
        )?;
        self.store
            .set_meta(REVISION_META, &r.revision.to_string())?;
        self.store.delete_meta(REFUSED_META)
    }

    /// Send a queued `shelf` op. A blyg that doesn't offer the shelf drops
    /// it (the shelf stays here). Anything transient keeps it.
    pub(super) fn run_shelf(&self, op: &Op) -> Result<()> {
        if !self.shelf_served() {
            return self.store.drop_op(op.seq);
        }
        self.store.set_in_flight(op.seq, true)?;
        match self.sync_shelf() {
            Ok(changed) => {
                self.store.drop_op(op.seq)?;
                if changed {
                    self.emit(CoreEvent::ItemsChanged);
                    self.emit(CoreEvent::ReadingChanged);
                }
                Ok(())
            }
            Err(e) => {
                self.store.set_in_flight(op.seq, false)?;
                Err(e)
            }
        }
    }
}
