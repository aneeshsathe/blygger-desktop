//! --- background --- Reading the studio's Background lens, and the
//! owner's choices (`crate::background`). The lens is read whole (`GET
//! /api/reading?background=true`, every page) when something it's worked
//! out from moved: `reading` (imports, withdrawals, placements, own
//! versions' reasons), `subscriptions` (a whole blyg moved), `settings`
//! (a rule turned off) or `items` (own versions). A choice is sent at once
//! and shows here at once; the next read of the lens settles it.

use super::Engine;
use crate::backend::Result;
use crate::background::{BackgroundSet, Key, Placement};
use crate::model::{CoreEvent, ReadingItem};

/// What the Background lens depends on (`GET /api/changes` domains).
pub const BACKGROUND_DEPS: &[&str] = &["reading", "subscriptions", "settings", "items"];

impl Engine {
    /// Note whether the blyg serves Background (its capabilities list
    /// `setReadingPlacement`). A failed read leaves what was noted.
    pub(super) fn note_background_served(&self) {
        if let Ok(on) = self.api.serves_background() {
            let _ = self.store.set_background_served(on);
        }
    }

    /// Whether the blyg serves Background, as of the last check.
    pub fn background_served(&self) -> bool {
        self.store.background_served()
    }

    /// Read the lens again and keep it. True when it changed. Nothing is
    /// asked of a blyg that doesn't serve it.
    pub(super) fn fetch_background(&self) -> Result<bool> {
        if !self.store.background_served() {
            return Ok(false);
        }
        let lens = self.api.background_lens()?;
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
    pub fn set_blyg_background(&self, sub: &str, on: bool, rows: &[ReadingItem]) -> Result<()> {
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
