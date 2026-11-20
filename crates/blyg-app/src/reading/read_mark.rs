//! A local read mark (`CoreEvent::ReadMarked`: a post opened, a stream card
//! seen): the marked rows change in place. Reading the whole list again
//! (`reading_changed`) to change one row cost tens of milliseconds at 10k
//! posts on every open.
//!
//! What follows a row's read state, and how it keeps up here:
//! - the row's dot, the header's "N to read", the sources pane's counts and
//!   dots, the Mentions dot, Space's next unread and the stream's cards are
//!   worked out from `rows` when drawn: they follow the row;
//! - the status bar's count (`MainView::unread`) is the store's: asked again;
//! - the open post keeps the read state it had when it opened (its "edited
//!   since you read it"), as `reading_changed` keeps it;
//! - which rows are listed changes only in All unread (Reader), and there
//!   only for a post that isn't sticky (the open one is): then `refilter`.

use blyg_core::ReadMarked;
use gpui_kit::Context;

use super::sources_vm::{Smart, Source};
use super::stream_vm::ReadMode;
use crate::app::MainView;

impl MainView {
    /// Hook (`CoreEvent::ReadMarked`). With a `ReadingChanged` in the same
    /// batch (`reading_follows`) the list is read again anyway: nothing to do.
    pub(crate) fn read_marked(
        &mut self,
        marked: &[ReadMarked],
        reading_follows: bool,
        cx: &mut Context<Self>,
    ) {
        if reading_follows || marked.is_empty() {
            return;
        }
        let r = &mut self.reading;
        let mut left_unread = false;
        for row in r.rows.iter_mut() {
            let Some(m) = marked
                .iter()
                .find(|m| m.sub == row.subscription_id && m.remote_id == row.remote_id)
            else {
                continue;
            };
            // Never lower: a version read elsewhere may be ahead of the mark.
            if row.read_version >= Some(m.version) {
                continue;
            }
            row.read_version = Some(m.version);
            if !r
                .sticky
                .contains(&(row.subscription_id.clone(), row.remote_id.clone()))
            {
                left_unread = true;
            }
        }
        if left_unread && r.mode == ReadMode::Reader && r.source == Source::Smart(Smart::Unread) {
            r.refilter();
        }
        self.unread = self.backend.unread_count();
        cx.notify();
    }
}
