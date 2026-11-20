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

use std::collections::{HashMap, HashSet};

use blyg_core::{ReadMarked, RemoteId, SubId};
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
        // Only what this backend's store holds as read, at what version: a
        // mark queued by a blyg just switched away from can name a
        // subscription and post this blyg also has (subscription ids are
        // each studio's own strings), and must change nothing. One lookup
        // per post marked (`reading_with_id`: the rows with that id).
        let mut posts: Vec<&RemoteId> = marked.iter().map(|m| &m.remote_id).collect();
        posts.sort();
        posts.dedup();
        let wanted: HashSet<(&SubId, &RemoteId)> =
            marked.iter().map(|m| (&m.sub, &m.remote_id)).collect();
        // Post id -> (subscription, read version): looked up by a row's
        // post id without copying it.
        let mut read: HashMap<RemoteId, Vec<(SubId, u32)>> = HashMap::new();
        for rid in posts {
            for it in self.backend.reading_with_id(rid) {
                if let Some(v) = it.read_version
                    && wanted.contains(&(&it.subscription_id, &it.remote_id))
                {
                    read.entry(it.remote_id)
                        .or_default()
                        .push((it.subscription_id, v));
                }
            }
        }
        if read.is_empty() {
            return;
        }
        let r = &mut self.reading;
        let mut left_unread = false;
        for row in r.rows.iter_mut() {
            let Some(v) = read
                .get(&row.remote_id)
                .and_then(|subs| subs.iter().find(|(s, _)| *s == row.subscription_id))
                .map(|&(_, v)| v)
            else {
                continue;
            };
            // Never lower: a version read elsewhere may be ahead of the store.
            if row.read_version >= Some(v) {
                continue;
            }
            row.read_version = Some(v);
            let key = (row.subscription_id.clone(), row.remote_id.clone());
            if !r.sticky.contains(&key) {
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
