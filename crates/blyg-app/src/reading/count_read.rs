//! --- studio 0.71 --- When a reading entry counts as read: the blyg's
//! **Count an entry as read** (`readerMarkRead`, studio 0.70;
//! `blyg_core::mark_read`), chosen in Settings › READER ORDER and applied
//! in the Stream and the Reader.
//!
//! - **when I see it** (`seen`): a Stream card on screen for about a second
//!   counts, short ones included, by the studio's rule (`seen_fully`): the
//!   whole card, or as much of a tall one as most of the screen holds. The
//!   marks wait in a batch and go together every `SEEN_FLUSH` (and when the
//!   Stream stops being watched), as the studio's do. Opening a post counts
//!   too, in either layout.
//! - **when I open it** (`opened`): only opening a post (the Reader's list,
//!   the Stream's side pane, a card's date) counts.
//! - **only when I mark it** (`never`): nothing counts by itself; `r`, the
//!   post menu and Mark All Read do.
//!
//! A blyg without the setting (before studio 0.70) keeps Burrow's earlier
//! behaviour (`SeenRule::Half`): a Stream card at least half on screen for
//! a second counts at once, and opening a post counts.
//!
//! How the time on screen is measured: the Stream's tick
//! (`stream::TICK`, every 250 ms) asks the list for the bounds of each card
//! from the first one drawn down to the viewport's bottom (never every row)
//! and checks each against the viewport with the rule; a card that passes
//! starts its clock (`stream_vm::ReadTracker`), and a card that fails a
//! tick, or a tick while the window is in the background or something
//! covers the Stream, starts over. After `stream_vm::READ_DWELL` (1 s) of
//! passing ticks it is due, once.

use std::time::{Duration, Instant};

use blyg_core::mark_read::MarkRead;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::vm::Key;
use crate::app::MainView;

/// How long seen marks wait to go together (the studio's `SEEN_FLUSH_MS`).
pub const SEEN_FLUSH: Duration = Duration::from_secs(3);

/// The setting's name, as Studio words it.
pub const COUNT_READ: &str = "Count an entry as read";

/// The rule that says a Stream card is on screen enough to count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeenRule {
    /// Burrow before the setting: half the card, or half the screen.
    Half,
    /// The studio's `seen`: the whole card, or most of what fits.
    Fully,
}

impl SeenRule {
    pub fn counts(self, top: f32, bottom: f32, view_top: f32, view_bottom: f32) -> bool {
        match self {
            SeenRule::Half => super::stream_vm::seen_enough(top, bottom, view_top, view_bottom),
            SeenRule::Fully => seen_fully(top, bottom, view_top, view_bottom),
        }
    }

    /// Whether its marks wait for a batch (the studio's) or go at once.
    pub fn batched(self) -> bool {
        self == SeenRule::Fully
    }
}

/// The studio's `useSeenMarks` test, on one card: at least 99% of it on
/// screen, or an on-screen height of at least 60% of the smaller of the
/// card and the viewport (so a card taller than the screen counts once it
/// fills most of it).
pub fn seen_fully(top: f32, bottom: f32, view_top: f32, view_bottom: f32) -> bool {
    let h = bottom - top;
    let view_h = view_bottom - view_top;
    if h <= 0.0 || view_h <= 0.0 {
        return false;
    }
    let visible = (bottom.min(view_bottom) - top.max(view_top)).max(0.0);
    visible > 0.0 && (visible >= 0.99 * h || visible >= 0.6 * h.min(view_h))
}

/// Seen marks waiting to be sent together.
#[derive(Debug, Default)]
pub struct SeenBatch {
    pending: Vec<Key>,
    since: Option<Instant>,
}

impl SeenBatch {
    pub fn add(&mut self, keys: Vec<Key>, now: Instant) {
        if keys.is_empty() {
            return;
        }
        self.since.get_or_insert(now);
        for k in keys {
            if !self.pending.contains(&k) {
                self.pending.push(k);
            }
        }
    }

    /// The marks to send now: all of them once the oldest has waited
    /// `SEEN_FLUSH`, or at once when `force` (the Stream stopped being
    /// watched).
    pub fn due(&mut self, now: Instant, force: bool) -> Vec<Key> {
        match self.since {
            Some(t0) if force || now.duration_since(t0) >= SEEN_FLUSH => {
                self.since = None;
                std::mem::take(&mut self.pending)
            }
            _ => Vec::new(),
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

/// The setting as this window knows it (inside `reading::State`).
#[derive(Debug, Default)]
pub struct CountRead {
    /// The blyg's choice; `None`: it doesn't offer one (or it isn't read
    /// yet), and Burrow's earlier behaviour holds.
    pub setting: Option<MarkRead>,
    /// Read from the blyg this session.
    loaded: bool,
    /// Bumped by each choice made here, so a slower answer can't undo it.
    chosen: u64,
    /// Seen marks waiting to go.
    pub batch: SeenBatch,
}

impl CountRead {
    pub fn new(setting: Option<MarkRead>) -> CountRead {
        CountRead {
            setting,
            ..CountRead::default()
        }
    }

    /// Opening a post counts (all but `never`).
    pub fn on_open(&self) -> bool {
        self.setting != Some(MarkRead::Never)
    }

    /// How a Stream card counts by being on screen, if it does.
    pub fn seen_rule(&self) -> Option<SeenRule> {
        match self.setting {
            None => Some(SeenRule::Half),
            Some(MarkRead::Seen) => Some(SeenRule::Fully),
            Some(MarkRead::Opened | MarkRead::Never) => None,
        }
    }
}

impl MainView {
    /// Read the blyg's choice once a session (Reading opening asks), and
    /// again when Settings opens (`force`), so a change made in Studio
    /// shows. A blyg without it keeps the choice hidden.
    pub(crate) fn count_read_refresh(&mut self, force: bool, cx: &mut Context<Self>) {
        let c = &mut self.reading.count_read;
        if c.loaded && !force {
            return;
        }
        c.loaded = true;
        let chosen = c.chosen;
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.load_reader_mark_read() });
        cx.spawn(async move |this, cx| {
            let Ok(got) = task.await else {
                return;
            };
            let _ = this.update(cx, |v, cx| {
                let c = &mut v.reading.count_read;
                if c.chosen == chosen && c.setting != got {
                    c.setting = got;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Choose: at once here, then saved on the blyg (that key alone). A
    /// refusal puts back what it had and says why.
    pub(crate) fn set_count_read(&mut self, m: MarkRead, cx: &mut Context<Self>) {
        let c = &mut self.reading.count_read;
        let Some(before) = c.setting else {
            return;
        };
        if before == m {
            return;
        }
        c.setting = Some(m);
        c.chosen += 1;
        let chosen = c.chosen;
        cx.notify();
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.save_reader_mark_read(m) });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                if v.reading.count_read.chosen != chosen {
                    return;
                }
                match r {
                    Ok(kept) => v.reading.count_read.setting = Some(kept),
                    Err(e) => {
                        v.reading.count_read.setting = Some(before);
                        v.show_toast(format!("Couldn't save “{COUNT_READ}”: {e}"), None, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Send the seen marks that are due (all of them with `force`), off the
    /// UI thread, together. Your own posts are never marked.
    pub(crate) fn flush_seen(&mut self, now: Instant, force: bool, cx: &mut Context<Self>) {
        let due = self.reading.count_read.batch.due(now, force);
        self.send_marks(due, cx);
    }

    /// Mark these posts read in one background task.
    pub(crate) fn send_marks(&mut self, keys: Vec<Key>, cx: &mut Context<Self>) {
        let keys: Vec<Key> = keys
            .into_iter()
            .filter(|k| !super::own::is_own(k))
            .collect();
        if keys.is_empty() {
            return;
        }
        let backend = self.backend.clone();
        cx.background_spawn(async move {
            for (sub, rid) in keys {
                let _ = backend.mark_read(&sub, &rid);
            }
        })
        .detach();
    }

    /// Settings › READER ORDER's "Count an entry as read": the three
    /// choices, when the blyg offers the setting.
    pub(crate) fn render_count_read_settings(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let now = self.reading.count_read.setting?;
        let p = self.palette;
        let theme = &*self.theme;
        let chips = MarkRead::ALL.into_iter().map(|m| {
            let on = now == m;
            let id = format!("count-read-{}", m.as_str());
            div()
                .id(SharedString::from(id.clone()))
                .debug_selector(move || id.clone())
                .flex_none()
                .px(px(10.))
                .py(px(3.))
                .flex()
                .items_center()
                .gap(px(5.))
                .map(|d| crate::theme_ext::chip(d, theme, on))
                .cursor_pointer()
                .hover(|s| s.border_color(p.accent).bg(p.hover()))
                .child(if on { "●" } else { "○" })
                .child(m.label())
                .on_click(cx.listener(move |this, _, _, cx| this.set_count_read(m, cx)))
        });
        Some(
            div()
                .id("count-read")
                .debug_selector(|| "count-read".into())
                .mt(px(8.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(p.muted)
                        .child(COUNT_READ),
                )
                .child(div().flex().flex_wrap().gap(px(6.)).children(chips))
                .child(crate::app::settings_page::note(&p, hint(now)))
                .into_any_element(),
        )
    }
}

/// What the chosen rule does, under the chips.
pub fn hint(m: MarkRead) -> &'static str {
    match m {
        MarkRead::Seen => {
            "A post in the Stream counts once it has been on screen for about a second, \
             short ones included: all of the card in view, or at least 60% of the card or \
             of the window, whichever is smaller, while Burrow is in front. Marks go to \
             your blyg a few seconds later, together. Opening a post counts too. Saved on \
             your blyg."
        }
        MarkRead::Opened => {
            "A post counts when you open it: in the Reader, in the Stream's side pane, or \
             from a card's date. Saved on your blyg."
        }
        MarkRead::Never => {
            "Nothing counts by itself: mark posts with r, the post menu or Mark All Read. \
             Saved on your blyg."
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{SEEN_FLUSH, SeenBatch, SeenRule, seen_fully};

    #[test]
    fn seen_fully_is_the_studios_rule() {
        // A short card wholly on screen counts; half of it doesn't.
        assert!(seen_fully(100.0, 160.0, 0.0, 500.0));
        assert!(!seen_fully(470.0, 530.0, 0.0, 500.0), "half");
        // 60% of a short card is enough (the studio's second test).
        assert!(seen_fully(464.0, 524.0, 0.0, 500.0), "36 of 60 px");
        assert!(!seen_fully(490.0, 550.0, 0.0, 500.0));
        // A card taller than the screen: once it fills 60% of it.
        assert!(seen_fully(-1000.0, 2000.0, 0.0, 500.0));
        assert!(seen_fully(200.0, 2000.0, 0.0, 500.0), "300 of 500");
        assert!(!seen_fully(250.0, 2000.0, 0.0, 500.0), "250 of 500");
        assert!(!seen_fully(600.0, 700.0, 0.0, 500.0), "below");
        assert!(!seen_fully(0.0, 0.0, 0.0, 500.0));
        // Burrow's earlier rule takes half of a short card.
        assert!(SeenRule::Half.counts(450.0, 550.0, 0.0, 500.0));
        assert!(!SeenRule::Fully.counts(450.0, 550.0, 0.0, 500.0));
    }

    #[test]
    fn a_batch_goes_once_its_oldest_mark_has_waited() {
        let k = |s: &str| (s.into(), s.into());
        let t0 = Instant::now();
        let mut b = SeenBatch::default();
        assert!(b.due(t0, true).is_empty(), "nothing to send");
        b.add(vec![k("a")], t0);
        b.add(vec![k("b"), k("a")], t0 + Duration::from_secs(1));
        assert_eq!(b.len(), 2, "each post once");
        assert!(b.due(t0 + Duration::from_secs(2), false).is_empty());
        assert_eq!(b.due(t0 + SEEN_FLUSH, false), vec![k("a"), k("b")]);
        assert!(b.is_empty());
        b.add(vec![k("c")], t0 + SEEN_FLUSH);
        assert_eq!(b.due(t0 + SEEN_FLUSH, true), vec![k("c")], "forced");
    }
}
