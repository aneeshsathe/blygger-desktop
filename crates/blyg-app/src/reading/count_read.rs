//! --- studio 0.71 --- When a reading entry counts as read: the blyg's
//! **Count an entry as read** (`readerMarkRead`, studio 0.70;
//! `blyg_core::mark_read`), chosen in Settings › READER ORDER and applied
//! in the Stream and the Reader.
//!
//! - **when I see it** (`seen`): a Stream card on screen for about a second
//!   counts, short ones included, by the studio's rule (`seen_fully`): the
//!   whole card, or as much of a tall one as most of the screen holds. The
//!   marks wait in a batch and go together every `SEEN_FLUSH` (and when the
//!   Stream stops being watched), as the studio's do; whatever waits is
//!   written when Burrow quits, the window closes or the blyg changes,
//!   always to the blyg it was seen on, and dropped when another rule is
//!   chosen. Opening a post counts too, in either layout.
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

use std::sync::Arc;
use std::time::{Duration, Instant};

use blyg_core::Backend;
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

    /// Every mark waiting, now (Burrow quits, the window closes, the blyg
    /// changes).
    pub fn take_all(&mut self) -> Vec<Key> {
        self.since = None;
        std::mem::take(&mut self.pending)
    }
}

/// The backend a batch's marks were seen on: the blyg behind the app's
/// `SwitchBackend` then, so they reach it after a switch too.
pub struct SeenOn(pub Arc<dyn Backend>);

impl std::fmt::Debug for SeenOn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SeenOn(..)")
    }
}

/// The quit and window-close hooks that write the batch on the way out.
#[derive(Default)]
pub struct OutHooks(Vec<Subscription>);

impl std::fmt::Debug for OutHooks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OutHooks({})", self.0.len())
    }
}

/// The setting as this window knows it (inside `reading::State`).
#[derive(Debug, Default)]
pub struct CountRead {
    /// The blyg's choice; `None`: it doesn't offer one (or it isn't read
    /// yet), and Burrow's earlier behaviour holds.
    pub setting: Option<MarkRead>,
    /// Read from the blyg this session (a read that failed is asked again).
    loaded: bool,
    /// Bumped by each choice made here, so a slower answer can't undo it.
    chosen: u64,
    /// Seen marks waiting to go.
    pub batch: SeenBatch,
    /// The blyg they were seen on (set with the first of them).
    on: Option<SeenOn>,
    hooks: OutHooks,
}

impl CountRead {
    pub fn new(setting: Option<MarkRead>) -> CountRead {
        CountRead {
            setting,
            ..CountRead::default()
        }
    }

    /// Take the blyg's (or this window's) choice. Away from "when I see
    /// it", the marks that rule collected are dropped: under the new one a
    /// card only seen isn't read.
    fn set(&mut self, m: Option<MarkRead>) {
        if m != Some(MarkRead::Seen) {
            self.batch.take_all();
            self.on = None;
        }
        self.setting = m;
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
    /// again when Settings opens and when the window comes to the front
    /// (`force`, `count_read_on_front`), so a change made in Studio shows. A blyg without it keeps the choice hidden.
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
            let got = task.await;
            let _ = this.update(cx, |v, cx| {
                let c = &mut v.reading.count_read;
                match got {
                    // Offline at launch, say: ask again on the next refresh.
                    Err(_) => c.loaded = false,
                    Ok(got) if c.chosen == chosen && c.setting != got => {
                        c.set(got);
                        cx.notify();
                    }
                    Ok(_) => {}
                }
            });
        })
        .detach();
    }

    /// The window came to the front: the rule may have been chosen
    /// elsewhere meanwhile (Studio on the web), so read it again, once it's
    /// in use (Reading or Settings has asked for it this session).
    pub(crate) fn count_read_on_front(&mut self, cx: &mut Context<Self>) {
        if self.reading.count_read.loaded {
            self.count_read_refresh(true, cx);
        }
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
        c.set(Some(m));
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

    /// The backend a read mark goes to: the blyg active now, itself, not
    /// the app's `SwitchBackend` (which forwards to whichever blyg is
    /// active when the mark is written), so a mark held for a while still
    /// reaches the blyg it was made on.
    pub(crate) fn mark_target(&self, cx: &App) -> Arc<dyn Backend> {
        match cx.try_global::<crate::connection::Connection>() {
            Some(c) if std::ptr::addr_eq(Arc::as_ptr(&self.backend), Arc::as_ptr(&c.switch)) => {
                c.switch.current()
            }
            _ => self.backend.clone(),
        }
    }

    /// Hold seen marks for the batch, with the blyg they were seen on. A
    /// batch from another blyg (the backend was swapped under the window)
    /// goes to its own blyg first. The first one also sets the hooks that
    /// write the batch when Burrow quits or the window closes.
    pub(crate) fn add_seen(&mut self, keys: Vec<Key>, now: Instant, cx: &mut Context<Self>) {
        if keys.is_empty() {
            return;
        }
        let target = self.mark_target(cx);
        let same = |on: &SeenOn| std::ptr::addr_eq(Arc::as_ptr(&on.0), Arc::as_ptr(&target));
        if self
            .reading
            .count_read
            .on
            .as_ref()
            .is_some_and(|on| !same(on))
        {
            self.flush_seen_now();
        }
        let c = &mut self.reading.count_read;
        c.on.get_or_insert(SeenOn(target));
        c.batch.add(keys, now);
        self.seen_hooks(cx);
    }

    /// Send the seen marks that are due (all of them with `force`) to the
    /// blyg they were seen on, off the UI thread, together. Your own posts
    /// are never marked.
    pub(crate) fn flush_seen(&mut self, now: Instant, force: bool, cx: &mut Context<Self>) {
        let c = &mut self.reading.count_read;
        let due = c.batch.due(now, force);
        if due.is_empty() {
            return;
        }
        let Some(SeenOn(on)) = c.on.take() else {
            return self.send_marks(due, cx);
        };
        send_to(on, due, cx);
    }

    /// The backend under the window changed (a blyg switched to, connected
    /// or signed out of, the sample data, the tour's swaps): the one place
    /// the reading state is rebuilt for it. The seen marks waiting go to the
    /// blyg they were seen on, the rows, folders, order and rule start over
    /// from the new backend, and its "Count an entry as read" is read now,
    /// so the old backend's rule never marks entries on this one.
    pub(crate) fn reading_for_new_backend(&mut self, cx: &mut Context<Self>) {
        self.flush_seen_now();
        self.reading = crate::app::reading::State::new(&*self.backend, cx);
        self.count_read_refresh(false, cx);
    }

    /// Write every seen mark waiting, now, on this thread (a mark is a
    /// local write: the blyg gets it with the next sync): Burrow quits, the
    /// window closes, or the blyg changes. They go to the blyg they were
    /// seen on.
    pub(crate) fn flush_seen_now(&mut self) {
        let c = &mut self.reading.count_read;
        let keys = c.batch.take_all();
        let Some(SeenOn(on)) = c.on.take() else {
            return;
        };
        for (sub, rid) in keys.iter().filter(|k| !super::own::is_own(k)) {
            let _ = on.mark_read(sub, rid);
        }
        // The last hold on a blyg switched away from: dropping a live
        // backend waits for its sync thread, which the UI thread mustn't.
        std::thread::spawn(move || drop(on));
    }

    /// Write the batch when Burrow quits or this window closes (set once
    /// per reading state).
    fn seen_hooks(&mut self, cx: &mut Context<Self>) {
        let hooks = &mut self.reading.count_read.hooks;
        if !hooks.0.is_empty() {
            return;
        }
        hooks.0.push(cx.on_app_quit(|v, _| {
            v.flush_seen_now();
            async {}
        }));
        hooks.0.push(cx.on_release(|v, _| v.flush_seen_now()));
    }

    /// Mark these posts read in one background task, on the blyg active
    /// now.
    pub(crate) fn send_marks(&mut self, keys: Vec<Key>, cx: &mut Context<Self>) {
        let target = self.mark_target(cx);
        send_to(target, keys, cx);
    }
}

/// Mark `keys` read on `backend` in one background task. Your own posts are
/// never marked.
fn send_to(backend: Arc<dyn Backend>, keys: Vec<Key>, cx: &mut Context<MainView>) {
    let keys: Vec<Key> = keys
        .into_iter()
        .filter(|k| !super::own::is_own(k))
        .collect();
    if keys.is_empty() {
        return;
    }
    cx.background_spawn(async move {
        for (sub, rid) in keys {
            let _ = backend.mark_read(&sub, &rid);
        }
    })
    .detach();
}

impl MainView {
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
