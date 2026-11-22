//! --- background --- Reading the studio's Background lens, and the
//! owner's choices (`crate::background`). The lens is read whole (`GET
//! /api/reading?background=true`, every page) when something it's worked
//! out from moved: `reading` (imports, withdrawals, placements, and your
//! own posts' status, versions and reasons: the studio's `items` trigger
//! moves `reading` for those), `subscriptions` (a whole blyg moved) or
//! `settings` (a rule turned off). Not `items` alone: a draft edit can't
//! move the lens, and the studio's own lens doesn't depend on it. A choice is sent at once
//! and shows here at once; the next read of the lens settles it.

use super::Engine;
use crate::api::is_transient;
use crate::backend::{CoreError, Result};
use crate::background::{BackgroundSet, Key, Placement};
use crate::model::{CoreEvent, ReadingItem, SubId};

/// What the Background lens depends on (`GET /api/changes` domains).
pub const BACKGROUND_DEPS: &[&str] = &["reading", "subscriptions", "settings"];

impl Engine {
    /// Note whether the blyg's Background lens may be read (its
    /// capabilities list `listReading`) and whether this sign-in may place
    /// posts (`setReadingPlacement`). A failed read leaves what was noted.
    pub(super) fn note_background_served(&self) {
        if let Ok(on) = self.api.serves_background() {
            let _ = self
                .store
                .set_background_served(on && !self.known_without_lens());
        }
        if let Ok(on) = self.api.places_background() {
            let _ = self.store.set_background_placing(on);
        }
    }

    /// Whether this studio's lens read was found plain rows at its current
    /// contract (`NO_LENS_META`): asked again once the contract changes.
    fn known_without_lens(&self) -> bool {
        let Some(seen) = self.store.meta(crate::background::NO_LENS_META) else {
            return false;
        };
        self.lens_memory_key().is_some_and(|v| v == seen)
    }

    /// What a "no lens" finding is remembered under: the blyg's
    /// `contractVersion` as served, compared whole, never parsed. It only
    /// says when to ask again (the API changed); the answer of the lens
    /// itself decides whether Background is there.
    fn lens_memory_key(&self) -> Option<String> {
        self.api.contract_version().ok().flatten()
    }

    /// Whether the blyg serves Background, as of the last check.
    pub fn background_served(&self) -> bool {
        self.store.background_served()
    }

    /// Whether this sign-in may place posts in Background.
    pub fn background_placing(&self) -> bool {
        self.store.background_placing()
    }

    /// Read the lens again and keep it. True when it changed. Nothing is
    /// asked of a blyg that doesn't serve it.
    pub(super) fn fetch_background(&self) -> Result<bool> {
        if !self.store.background_served() {
            return Ok(false);
        }
        // A studio before 0.61 refuses the filter (no Background until the
        // next check), or ignores it and answers plain rows, judged on the
        // first page (no Background until its contract changes).
        let had = self.store.background_set().is_some();
        let lens = match self.api.background_lens() {
            Ok(Some(lens)) => lens,
            Err(e) if is_transient(&e) || matches!(e, CoreError::Unauthorized) => return Err(e),
            Ok(None) => {
                // Remembered for this contract, across sessions.
                if let Some(v) = self.lens_memory_key() {
                    self.store.set_meta(crate::background::NO_LENS_META, &v)?;
                }
                self.store.set_background_served(false)?;
                return Ok(had);
            }
            Err(_) => {
                self.store.set_background_served(false)?;
                return Ok(had);
            }
        };
        let set = BackgroundSet::from_entries(&lens, self.store.background_set().as_ref());
        self.store.put_background_set(&set)
    }

    /// The owner's choice on one row (`withdrawn`: it's a withdrawn post),
    /// or `None` to let the rules decide again. Sent at once; on success it
    /// shows here at once (`ReadingChanged`), and the lens is read again on
    /// the next pull (the write moves `reading`).
    pub fn set_reading_placement(
        &self,
        key: &Key,
        withdrawn: bool,
        placement: Option<Placement>,
    ) -> Result<()> {
        self.track(self.api.set_reading_placement(key, placement))?;
        if let Some(mut set) = self.store.background_set() {
            set.place(key, withdrawn, placement);
            self.store.put_background_set(&set)?;
        }
        self.emit(CoreEvent::ReadingChanged);
        Ok(())
    }

    /// Move the whole blyg `sub` to Background (`on`) or back. Sent at
    /// once; its rows follow here at once, as far as what's held says.
    pub fn set_blyg_background(&self, sub: &SubId, on: bool, rows: &[ReadingItem]) -> Result<()> {
        self.track(self.api.set_subscription_background(sub, on))?;
        if let Some(mut set) = self.store.background_set() {
            set.place_blyg(sub, on, rows);
            self.store.put_background_set(&set)?;
        }
        self.emit(CoreEvent::ReadingChanged);
        Ok(())
    }
}

impl Engine {
    /// Read the lens on this pull: something it depends on moved (`stale`)
    /// and the pull may walk reading lists now (`open`), or the blyg serves
    /// it and it was never read here.
    pub(super) fn background_due(&self, stale: bool, open: bool) -> bool {
        (stale && open)
            || (self.store.background_served()
                && self.store.meta(crate::background::SET_META).is_none())
    }
}
