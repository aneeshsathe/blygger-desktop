//! --- background --- What this Mac knows of the studio's Background
//! (`crate::background`): whether the blyg serves it, and its Background
//! lens as last read, with this client's Foreground choices. Kept in
//! `meta` (two keys, no table): it's one small document, read whole and
//! replaced whole after each read of the lens.

use super::Store;
use crate::backend::Result;
use crate::background::{BackgroundSet, SERVED_META, SET_META};

impl Store {
    /// Whether the blyg serves Background (its capabilities list
    /// `setReadingPlacement`), as of the last check.
    pub fn background_served(&self) -> bool {
        self.meta(SERVED_META).as_deref() == Some("1")
    }

    /// Note whether it does. When it doesn't (any more), what was read of
    /// its lens is dropped, so the old rule (withdrawn is background) holds.
    pub fn set_background_served(&self, on: bool) -> Result<()> {
        self.set_meta(SERVED_META, if on { "1" } else { "0" })?;
        if !on {
            self.delete_meta(SET_META)?;
        }
        Ok(())
    }

    /// The Background lens as last read; `None` when the blyg doesn't
    /// serve it or it hasn't been read yet.
    pub fn background_set(&self) -> Option<BackgroundSet> {
        if !self.background_served() {
            return None;
        }
        self.meta(SET_META)
            .and_then(|s| BackgroundSet::from_json(&s))
            .map(BackgroundSet::indexed)
    }

    /// Keep `set`; true when it differs from what was kept.
    pub fn put_background_set(&self, set: &BackgroundSet) -> Result<bool> {
        let json = set.to_json();
        if self.meta(SET_META).as_deref() == Some(json.as_str()) {
            return Ok(false);
        }
        self.set_meta(SET_META, &json)?;
        Ok(true)
    }
}
