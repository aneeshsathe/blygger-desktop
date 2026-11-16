//! --- shelf sync --- The shelf on the reading side (studio 0.61's
//! parity): "+ Add to shelf" on every post, yours and others', and the
//! Shelf lens in the sources pane, which lists what's on the shelf in your
//! order. Your own shelved posts also stay at the top of the Write list
//! (`crate::app::shelf`). With a blyg that keeps a shelf, every change
//! syncs (`blyg_core::shelf`); otherwise the shelf is this computer's.

use blyg_core::shelf::{Entry, SyncState};
use gpui_kit::*;

use super::vm::{self, Key};
use crate::app::MainView;

/// The shelf's posts as reading rows' keys, in order.
pub fn keys(backend: &dyn blyg_core::Backend) -> Vec<Key> {
    backend
        .shelf_entries()
        .into_iter()
        .map(|e| (e.sub, e.remote_id))
        .collect()
}

/// The toast after "+ Add to shelf" / "− Remove from shelf" (the studio's
/// words).
pub fn toggled_toast(on: bool) -> &'static str {
    if on {
        "Added to shelf"
    } else {
        "Removed from shelf"
    }
}

/// What the Shelf lens says about where the shelf lives.
pub fn state_note(s: SyncState) -> &'static str {
    match s {
        SyncState::Local => "On this computer only: your blyg doesn't keep a shelf.",
        SyncState::Synced => "Synced with your blyg.",
        SyncState::Waiting => "Changed here · syncing with your blyg…",
    }
}

impl MainView {
    /// "+ Add to shelf" on a reading post, or "− Remove from shelf".
    pub(crate) fn toggle_reading_shelf(&mut self, key: Key, cx: &mut Context<Self>) {
        let on = !self.reading.shelved.contains(&key);
        let entry = Entry::new(key.0.clone(), key.1.clone());
        match self.backend.set_entry_on_shelf(&entry, on) {
            Ok(()) => {
                self.reload_reading_shelf();
                self.reload_shelf(); // the Write list's order
                self.reading.refilter();
                self.show_toast(toggled_toast(on), None, cx);
            }
            Err(e) => self.show_toast(format!("{e}"), None, cx),
        }
        cx.notify();
    }

    /// Hook: read the shelf's keys again (after a sync, or a change here).
    pub(crate) fn reload_reading_shelf(&mut self) {
        self.reading.shelved = keys(self.backend.as_ref());
    }

    /// Whether `item` is on the shelf.
    pub(crate) fn on_reading_shelf(&self, item: &blyg_core::ReadingItem) -> bool {
        self.reading.shelved.contains(&vm::key(item))
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: it brings in GPUI's own `test`.
    use super::{SyncState, state_note, toggled_toast};

    #[test]
    fn the_toasts_and_notes_say_where_the_shelf_is() {
        assert_eq!(toggled_toast(true), "Added to shelf");
        assert_eq!(toggled_toast(false), "Removed from shelf");
        assert!(state_note(SyncState::Local).contains("this computer"));
        assert!(state_note(SyncState::Synced).contains("Synced"));
    }
}
