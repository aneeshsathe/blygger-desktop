//! --- reader order --- The Feed's rules without the UI (`feed.rs` draws
//! them): studio 0.68's tab texts, the Feed's order state, each row's place
//! in the pace-layered order, Raw's order, and which lists follow the Feed.
//! Pure, so Burrow in a browser compiles it unchanged
//! (`crates/blyg-web/src/reading/mod.rs`).

use std::collections::HashMap;

use blyg_core::order::ReaderOrder;
use blyg_core::pace::RowFacts;
use blyg_core::{ReadingItem, RemoteVersion, Subscription, SubscriptionKind};

use super::sources_vm::{Smart, Source};
use super::stream_vm::ReadMode;
use super::vm::{self, Key};

/// What the Agentic feed says before its first edition.
pub const AGENTIC_NOTE: &str = "No edition yet. The Agentic feed is a digest of your reading in \
     sections, written by an assistant: Burrow's own AI (Settings › Agentic feed, or Write an \
     Agentic Feed Edition Now in the Blyg menu), or an assistant on your blyg.";

/// What Raw says when it holds nothing.
pub const RAW_EMPTY: &str = "Nothing has come in yet. Raw lists what came in from the blygs \
     you read, newest first: nothing moved to Background, nothing folded.";

/// The Feed's kinds, its own control (studio's All · Threads · Fragments).
pub const KINDS: [Smart; 3] = [Smart::All, Smart::Threads, Smart::Fragments];

/// A kind's name in the Feed's control.
pub fn kind_label(s: Smart) -> &'static str {
    match s {
        Smart::All => "All",
        other => other.label(),
    }
}

/// An order's name, wherever it's chosen (the reading header, Settings ›
/// READER ORDER): the studio's words, as `ReaderOrder::label` has them,
/// written here so the tour's quotes of it are checked against the app.
pub fn order_label(o: ReaderOrder) -> &'static str {
    match o {
        ReaderOrder::PaceLayered => "Pace-layered",
        ReaderOrder::Newest => "Chronological",
    }
}

/// The Feed's order and each row's place in it.
#[derive(Debug, Default)]
pub struct Feed {
    pub order: ReaderOrder,
    /// Each row's place in the pace-layered order (top first); empty while
    /// the Feed is chronological.
    positions: HashMap<Key, usize>,
    /// The blyg's setting was read this session.
    pub loaded: bool,
    /// Bumped by each choice made here: a read of the blyg's order, or a
    /// save's answer, that started before one doesn't undo it.
    pub chosen: u64,
    /// The order last confirmed (read from the blyg, or a save it
    /// answered): where a failed save goes back to.
    pub(crate) confirmed: ReaderOrder,
    /// Bumped by each rebuild: only the latest one's places land.
    rebuilds: u64,
    /// The places of the current order have landed (or there are none to
    /// wait for). Until they do, the Feed is held: nothing listed, rather
    /// than newest first and then a jump.
    ready: bool,
}

/// The changelogs cached on this computer, by origin and remote id.
pub type Changelogs = HashMap<(String, String), Vec<RemoteVersion>>;

/// Whether `r` is a plain feed's (RSS, Atom): the studio's `l0`.
pub fn is_l0(r: &ReadingItem, subs: &[Subscription]) -> bool {
    subs.iter()
        .any(|s| s.id == r.subscription_id && s.kind == SubscriptionKind::Rss)
}

/// Every row's place from its facts: the pace-layered replay, then the
/// rows it couldn't place (no time of theirs parses) at the bottom, in
/// listing order. Runs off the UI thread.
pub fn places(facts: Vec<RowFacts>, changelogs: &Changelogs) -> HashMap<Key, usize> {
    let keys: Vec<Key> = facts.iter().map(|f| f.key.clone()).collect();
    let mut placed = blyg_core::pace::positions(facts, changelogs);
    for k in keys {
        let next = placed.len();
        placed.entry(k).or_insert(next);
    }
    placed
}

impl Feed {
    pub fn new(order: ReaderOrder) -> Self {
        Feed {
            order,
            confirmed: order,
            ..Default::default()
        }
    }

    /// Whether the Feed waits for its first places (pace-layered, none
    /// landed yet for this order).
    pub fn held(&self) -> bool {
        self.order == ReaderOrder::PaceLayered && !self.ready
    }

    /// A rebuild starts: its ticket.
    pub fn start_rebuild(&mut self) -> u64 {
        self.rebuilds += 1;
        self.rebuilds
    }

    /// A rebuild's places, landing only if it's the latest rebuild and the
    /// Feed is still pace-layered.
    pub fn land(&mut self, ticket: u64, placed: HashMap<Key, usize>) -> bool {
        if ticket != self.rebuilds || self.order != ReaderOrder::PaceLayered {
            return false;
        }
        self.positions = placed;
        self.ready = true;
        true
    }

    /// The order turned chronological: no places to keep or wait for.
    pub(crate) fn clear_places(&mut self) {
        self.positions.clear();
        self.ready = false;
    }

    /// The user chose `order` here: shown at once; the ticket its save's
    /// answer carries.
    pub fn choose(&mut self, order: ReaderOrder) -> u64 {
        self.chosen += 1;
        self.order = order;
        self.chosen
    }

    /// A save's answer (`kept`: what the blyg or this computer kept; `None`
    /// when it failed) for the choice `at`: the order to show, or `None`
    /// when a later choice was made since (its own answer decides). A
    /// failed save goes back to the order last confirmed.
    pub fn answer(&mut self, at: u64, kept: Option<ReaderOrder>) -> Option<ReaderOrder> {
        if at != self.chosen {
            return None;
        }
        match kept {
            Some(k) => {
                self.confirmed = k;
                Some(k)
            }
            None => Some(self.confirmed),
        }
    }

    /// The rows (or the order) changed: work out each row's place again,
    /// with the pins of the changelogs this computer has cached, here and
    /// now (the app does it off the UI thread: `MainView::feed_rebuild`).
    #[cfg(test)]
    pub fn rebuild(&mut self, rows: &[ReadingItem], changelogs: &Changelogs) {
        self.positions = match self.order {
            ReaderOrder::Newest => HashMap::new(),
            ReaderOrder::PaceLayered => places(
                rows.iter().map(|r| RowFacts::of(r, false)).collect(),
                changelogs,
            ),
        };
        self.ready = true;
    }

    /// Where `key` is in the pace-layered Feed.
    pub fn place(&self, key: &Key) -> Option<usize> {
        self.positions.get(key).copied()
    }
}

/// Whether what's shown follows the Feed's order: the Stream, and every
/// Reader source but those that keep their own: hoppers, Saved and Raw, as
/// the studio's (`reading-data.ts`), Burrow's trails, and the Agentic feed.
pub fn follows_feed(mode: ReadMode, source: &Source) -> bool {
    mode == ReadMode::Stream
        || !matches!(
            source,
            Source::Hopper(_)
                | Source::Trail(_)
                | Source::Smart(Smart::Shelf | Smart::Raw | Smart::Agentic)
        )
}

/// Whether the Feed's kinds control shows: in Reader, on the Feed or one
/// of its kinds.
pub fn shows_kinds(mode: ReadMode, source: &Source) -> bool {
    mode == ReadMode::Reader && matches!(source, Source::Smart(s) if KINDS.contains(s))
}

/// Put `shown` (indices into `rows`) in the Feed's order, or Raw's.
pub fn arrange(
    rows: &[ReadingItem],
    shown: &mut [usize],
    mode: ReadMode,
    source: &Source,
    feed: &Feed,
) {
    if mode == ReadMode::Reader && *source == Source::Smart(Smart::Raw) {
        raw_order(rows, shown);
    } else if follows_feed(mode, source) && feed.order == ReaderOrder::PaceLayered {
        // A row with no place yet arrived since: above every place, as the
        // studio's does. The sort is stable, so those keep their order.
        shown.sort_by_key(|&i| {
            rows.get(i)
                .and_then(|r| feed.place(&vm::key(r)))
                .map_or(0, |p| p + 1)
        });
    }
}

/// Raw's order (studio's `rawIdentitySql`): newest first by the post's
/// date, clamped to when it came in; then subscription and remote id,
/// descending.
pub fn raw_order(rows: &[ReadingItem], shown: &mut [usize]) {
    let at = |r: &ReadingItem| {
        blyg_core::pace::clamped(
            r.updated.as_deref().or(r.created.as_deref()),
            &r.observed_at,
        )
        .unwrap_or(i64::MIN)
    };
    shown.sort_by(|&a, &b| {
        let (ra, rb) = (&rows[a], &rows[b]);
        at(rb)
            .cmp(&at(ra))
            .then_with(|| rb.subscription_id.cmp(&ra.subscription_id))
            .then_with(|| rb.remote_id.cmp(&ra.remote_id))
    });
}
