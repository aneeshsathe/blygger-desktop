//! --- syndication --- Sending your posts' copies-elsewhere lists
//! (`crate::syndication`): one queued `syndication` op per post, written in
//! place with `PUT /api/items/:id/syndication {entries, expectedRevision}`.
//! Never a publish: no version, no changelog entry, no feed entry. The
//! `PUT` bumps the item's revision, which is kept from the answer.

use super::Engine;
use crate::backend::{CoreError, Result};
use crate::model::{CoreEvent, LocalId};
use crate::store::Op;
use crate::syndication::{Answer, SERVED_META, SyndicationEntry, SyndicationOp};

impl Engine {
    /// Note whether the blyg serves syndication writes (its capabilities
    /// list `putSyndication`). A failed read leaves what was noted.
    pub(super) fn note_syndication_served(&self) {
        if let Ok(on) = self.api.serves_syndication() {
            let _ = self.store.set_meta(SERVED_META, if on { "1" } else { "0" });
        }
    }

    /// Whether the blyg serves syndication writes, as of the last check.
    pub fn syndication_served(&self) -> bool {
        self.store.meta(SERVED_META).as_deref() == Some("1")
    }

    /// `PUT` the list with the item's revision. A stale revision (409
    /// `revision-conflict`) is read again (merged with conflict detection)
    /// and sent once more: the list doesn't depend on the text, so it is
    /// never held back by a text change. The item's revision follows the
    /// answer.
    fn put_with_revision(
        &self,
        id: &LocalId,
        sid: &str,
        entries: &[SyndicationEntry],
    ) -> Result<Answer> {
        let rev = match self.store.revision(id) {
            Some(r) => r,
            None => self.api.get_item(sid)?.revision,
        };
        let ans = match self.api.put_syndication(sid, rev, entries) {
            Err(CoreError::RevisionConflict) => {
                let rev = match self.reread(id, sid)? {
                    Some(r) => r,
                    // The text is in conflict here: the list still goes,
                    // on the blyg's revision, and the stored one is left
                    // for the conflict to settle.
                    None => {
                        let w = self.api.get_item(sid)?;
                        return self.api.put_syndication(sid, w.revision, entries);
                    }
                };
                self.api.put_syndication(sid, rev, entries)?
            }
            r => r?,
        };
        if ans.revision > 0 {
            self.store.set_revision(id, ans.revision)?;
        }
        Ok(ans)
    }

    /// Send a `syndication` op. A studio that doesn't serve the route
    /// drops it and says so; 404 (the post is gone) drops it; 400 or 409
    /// (an address or version it refuses, a draft or withdrawn post) drops
    /// it and says why. Anything transient keeps it.
    pub(super) fn run_syndication(&self, op: &Op) -> Result<()> {
        let Ok(s) = serde_json::from_str::<SyndicationOp>(&op.payload) else {
            return self.store.drop_op(op.seq);
        };
        if !self.api.serves_syndication()? {
            self.store.drop_op(op.seq)?;
            let _ = self.store.set_meta(SERVED_META, "0");
            self.emit(CoreEvent::Error(
                "This blyg doesn't keep copies elsewhere yet (it needs a newer studio). \
                 The list stays on this Mac only."
                    .into(),
            ));
            return Ok(());
        }
        self.store.set_in_flight(op.seq, true)?;
        let id = LocalId(s.local_id.clone());
        match self.put_with_revision(&id, &s.server_id, &s.entries) {
            Ok(kept) => {
                self.store.syndication_sent(op.seq, &id, &kept.entries)?;
                self.emit(CoreEvent::ItemsChanged);
                Ok(())
            }
            Err(CoreError::NotFound) => self.store.drop_op(op.seq),
            Err(CoreError::Rejected {
                status: status @ (400 | 409 | 422),
                message,
                ..
            }) => {
                self.store.drop_op(op.seq)?;
                self.emit(CoreEvent::Error(if status == 409 {
                    format!("Your blyg didn't take the copies elsewhere list: {message}")
                } else {
                    format!("Your blyg refused a copy's address: {message}")
                }));
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Read your post's list from the blyg (`GET …/syndication`) and keep
    /// it, unless an edit waits. `Ok(None)`: not served.
    pub fn fetch_syndication(
        &self,
        id: &LocalId,
        sid: &str,
    ) -> Result<Option<Vec<SyndicationEntry>>> {
        match self.api.get_syndication(sid) {
            Ok(a) => {
                self.store.syndication_fetched(id, &a.entries)?;
                Ok(Some(self.store.syndication(id)))
            }
            Err(CoreError::NotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
