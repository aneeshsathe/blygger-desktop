//! The reading list's two reads, and the pure decisions behind them
//! (`Engine::read_reading` in `sync/mod.rs` does the I/O).
//!
//! - A **head read** (a pull after `reading` moved): newest-first pages,
//!   each merged as it arrives, stopping at the first page that changed
//!   nothing here, or at one that reaches back past what the last read
//!   covered ([`HEAD_SLACK_MS`] before its newest row). Every change the
//!   studio makes to a row's content (an import, an edit, a withdrawal)
//!   stamps it with a new `observedAt`, so it comes back at the head.
//!   Nothing is pruned, and the `reading` revision is accepted. What the
//!   head can't see (an edit that kept its stamp, a read mark from another
//!   device on an old post, a row the studio wrote late) is left to a whole
//!   walk: a head read that finds nothing for a moved counter, or a late
//!   row ([`late`]), asks for one, paced by `full_pull_every`.
//! - A **whole walk** (the safety net, a first sync, a real subscription
//!   change, or a move the head couldn't explain): pages from the first to
//!   `nextCursor: null`, merged as they arrive, `reading_pages` of them per
//!   pull, continued on the next pull from the cursor. Only a walk that
//!   reached the end prunes, with the keys of that one coherent walk.

use std::collections::HashSet;
use std::time::Duration;

use crate::api::wire::ChangeState;
use crate::model::Subscription;

/// `meta` key: the newest `observedAt` (ms) the last accepted read of the
/// reading list saw. A head read stops once a page reaches back past it,
/// less [`HEAD_SLACK_MS`].
pub const HEAD_MARK: &str = "reading_head_mark";

/// How far behind the last read's newest row a head read still looks. The
/// studio stamps a poll's rows with the time the poll started, and writes
/// them as it goes, so a slow poll (a big backfill) can land rows a little
/// older than ones already read. An hour is well past any one poll.
pub const HEAD_SLACK_MS: i64 = 60 * 60 * 1000;

/// While a whole walk is part-way through, the worker pulls again this
/// soon to carry it on (it flushes the outbox first, and the net lock is
/// free in between). The sooner a long walk ends, the less likely the list
/// changes under it and it has to start over.
pub const WALK_PAUSE: Duration = Duration::from_secs(2);

/// Whether the subscription list changed in a way its reading rows show:
/// one added or removed, or a changed kind, origin, feed or name. Poll
/// bookkeeping (when it was polled, failures, status), the blogroll flag
/// and the order don't count: the studio's `subscriptions` counter moves
/// on every feed poll.
pub fn subscriptions_differ(old: &[Subscription], new: &[Subscription]) -> bool {
    let shown = |subs: &[Subscription]| {
        let mut v: Vec<_> = subs
            .iter()
            .map(|s| {
                (
                    s.id.clone(),
                    s.kind,
                    s.origin.clone(),
                    s.feed_url.clone(),
                    s.title.clone(),
                )
            })
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    };
    shown(old) != shown(new)
}

/// What the reading list needs this pull.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// Go on with the whole walk an earlier pull began.
    Continue,
    /// Start a whole walk.
    Whole,
    /// Read the head.
    Head,
    /// Leave it.
    Nothing,
}

/// The facts [`start`] decides from.
#[derive(Debug, Clone, Copy, Default)]
pub struct Due {
    /// A whole walk is part-way through.
    pub in_progress: bool,
    /// The safety net is due (or the blyg has no change counters).
    pub full: bool,
    /// The subscription list really changed (not just poll bookkeeping):
    /// one removed, renamed, moved to Background…
    pub subs_changed: bool,
    /// A head read found nothing to explain a moved `reading` counter, and
    /// the last whole walk is old enough to walk again.
    pub deep_due: bool,
    /// `reading` moved since the last accepted read.
    pub moved: bool,
    /// The read isn't held back by `reading_gap`.
    pub gap_open: bool,
    /// A head mark is known (else a head read couldn't tell where to stop).
    pub has_mark: bool,
}

pub fn start(d: Due) -> Start {
    if d.in_progress {
        Start::Continue
    } else if d.full || d.subs_changed || d.deep_due {
        Start::Whole
    } else if d.moved && d.gap_open {
        if d.has_mark {
            Start::Head
        } else {
            Start::Whole
        }
    } else {
        Start::Nothing
    }
}

/// What to do after a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// The list's end (`nextCursor: null`): a complete walk.
    Complete,
    /// A head read reached what the last read covered.
    HeadDone,
    /// This pull's page budget is spent: carry on next pull, as a whole
    /// walk (a head read that ran this long becomes one).
    Yield,
    /// Read the next page.
    Page,
}

/// After one page: `whole` walk or head read, whether the server has more,
/// whether every row on it was already held as it is (`unchanged`), the
/// page's oldest `observedAt` (ms), the head boundary (mark less slack),
/// and pages read this pull against the budget.
///
/// A head read is done at the first page that changed nothing here (what
/// changed since the last read sorts above it), or at one that reaches
/// back past the boundary (nothing that old can have changed since).
pub fn after_page(
    whole: bool,
    has_next: bool,
    unchanged: bool,
    oldest_ms: Option<i64>,
    boundary_ms: Option<i64>,
    pages: u32,
    budget: u32,
) -> Next {
    let past = matches!((oldest_ms, boundary_ms), (Some(o), Some(b)) if o < b);
    if !has_next {
        Next::Complete
    } else if !whole && (unchanged || past) {
        Next::HeadDone
    } else if pages >= budget.max(1) {
        Next::Yield
    } else {
        Next::Page
    }
}

/// Where a head read stops: the mark less [`HEAD_SLACK_MS`].
pub fn boundary(mark_ms: Option<i64>) -> Option<i64> {
    mark_ms.map(|m| m.saturating_sub(HEAD_SLACK_MS))
}

/// A row new to this Mac (or rewritten) that's no newer than what the last
/// read already saw: the studio wrote it late (a slow poll stamps its rows
/// with when it started), so more like it may sit below where a head read
/// stops. The whole list is walked again soon (`deep_pending`).
pub fn late(written_ms: impl IntoIterator<Item = i64>, mark_ms: Option<i64>) -> bool {
    mark_ms.is_some_and(|m| written_ms.into_iter().any(|t| t <= m))
}

/// A read of the reading list in progress, kept across pulls while a whole
/// walk goes on.
#[derive(Debug, Default)]
pub struct Walk {
    /// Read to the end before anything is accepted or pruned.
    pub whole: bool,
    /// The next page's cursor (`None`: the first page).
    pub cursor: Option<String>,
    /// Every row this walk has read (since its last restart): what a
    /// complete walk keeps.
    pub keys: HashSet<(String, String)>,
    /// The revisions captured before its first page (R3): what's accepted
    /// when it ends.
    pub rev: Option<ChangeState>,
    /// Started only because the safety net was due: a change it finds is
    /// one the counters missed.
    pub forced: bool,
    /// The newest `observedAt` (ms) it read: the next head mark.
    pub newest_ms: Option<i64>,
    /// Anything it merged changed a row here.
    pub changed: bool,
}

impl Walk {
    pub fn new(whole: bool, rev: Option<ChangeState>, forced: bool) -> Walk {
        Walk {
            whole,
            rev,
            forced,
            ..Walk::default()
        }
    }

    /// The list changed under the walk (`cursor-expired`): from the first
    /// page again, under the revisions captured for this pull. Rows already
    /// merged stay (merging never removes), but none counts towards the
    /// prune.
    pub fn restart(&mut self, rev: Option<ChangeState>) {
        self.cursor = None;
        self.keys.clear();
        self.rev = rev;
        self.newest_ms = None;
    }
}

#[cfg(test)]
#[path = "walk_tests.rs"]
mod tests;
