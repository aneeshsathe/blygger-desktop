//! --- browse --- Browsing the thicket from the reader (blygger-studio
//! 0.49.0). Following a quoted layer, a stub's "responds to" line or an
//! ordinary link that names a blyg item from one post to another is a
//! descent; the trail is the way down, so Back (⌘[, "‹ back") retraces it
//! one entry at a time and a breadcrumb in the reading pane's header shows
//! it.
//!
//! Two records, deliberately apart (as upstream's `browse.ts`):
//! - the live trail ([`Trail`]): where you are in this walk, in memory, which
//!   is what Back and the breadcrumb need;
//! - the private log (`Backend::log_browse`, `browse_history`,
//!   `clear_browse`): one row per entry a walk opened, a tree (each row names
//!   the row it was reached from), which the history pane (⇧⌘H) lists across
//!   walks. Logging is best effort: an error is ignored.
//!
//! A walk starts when a post is opened from the stream, the list, a source
//! or anywhere that isn't a link inside the open post. Unlike the web Studio,
//! a walk's first entry is logged only once something is followed from it,
//! so the history lists walks, not every post read (j/k through the stream's
//! pane opens a post each time).
//!
//! Which links name a blyg item is asked of the backend
//! (`Backend::classify_links`) in the background when a post opens, and
//! remembered here for the session; a link not answered yet is asked when
//! it's clicked.

use std::collections::{HashMap, HashSet};

use blyg_core::browse::{BrowseStep, BrowseVia, ItemRef, NewBrowseStep};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{Load, View};
use crate::app::MainView;
use crate::theme::Rule;

gpui_kit::actions!(blygger, [ReaderBack, ToggleBrowseHistory]);

/// At most this many of a post's links are asked about when it opens
/// (upstream asks 40; the backend batches by `CLASSIFY_MAX`).
pub const LINKS_PER_POST: usize = 40;
/// The server's limit on a step's title.
pub const TITLE_MAX: usize = 400;
/// How many log rows the history pane shows.
pub const HISTORY_ROWS: usize = 200;

/// The history pane's words (upstream's).
pub const HISTORY_TITLE: &str = "Browsing history";
pub const HISTORY_PRIVATE: &str = "Where following links has taken you. Private to you.";
pub const HISTORY_EMPTY: &str =
    "Nothing yet. Follow a link or a quoted layer in a post and the path appears here.";
/// The alert on a post of a blyg you don't subscribe to.
pub const NOT_SUBSCRIBED: &str = "You\u{2019}re not subscribed to this blyg";
pub const NOT_SUBSCRIBED_ASK: &str = "subscribe?";
pub const PEEK_NOTE: &str = "Read from its own blyg; nothing is saved.";
pub const PEEK_HELD_ORIGIN: &str = "You subscribe to this blyg, but this post isn\u{2019}t held \
     here. Read from its own blyg; nothing is saved.";

// ================================================================ the trail

/// One entry on the live trail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub origin: String,
    pub id: String,
    pub version: Option<u32>,
    pub title: String,
    /// The log row for this visit, once the backend has answered.
    pub step_id: Option<i64>,
    /// Sent to the log (a walk's first entry waits for a follow).
    pub logged: bool,
}

impl Step {
    pub fn new(origin: &str, id: &str, version: Option<u32>, title: &str) -> Step {
        Step {
            origin: origin.to_string(),
            id: id.to_string(),
            version,
            title: title.to_string(),
            step_id: None,
            logged: false,
        }
    }

    fn same(&self, other: &Step) -> bool {
        self.id.eq_ignore_ascii_case(&other.id)
            && blyg_core::profile::same_origin(&self.origin, &other.origin)
    }

    /// What the breadcrumb and the history pane call it.
    pub fn label(&self) -> String {
        label(&self.title, &self.id)
    }
}

/// A title, else the id's first eight characters (upstream's fallback).
pub fn label(title: &str, id: &str) -> String {
    let t = title.trim();
    if t.is_empty() || t == "…" {
        id.chars().take(8).collect()
    } else {
        t.to_string()
    }
}

/// How the next entry is being reached; used up by the entry that opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    Follow(BrowseVia),
    /// Back from the entry whose log row is `from`.
    Back {
        from: Option<i64>,
    },
}

/// What to log for one visit. When `start` is set (the walk's first entry,
/// logged late), `step.parent_id` is its answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogPlan {
    pub start: Option<NewBrowseStep>,
    pub step: NewBrowseStep,
}

/// The live trail: this walk's id and the way down.
#[derive(Debug, Clone, Default)]
pub struct Trail {
    pub id: Option<String>,
    pub stack: Vec<Step>,
    pending: Option<Pending>,
}

impl Trail {
    /// The next entry is reached by following something inside the current
    /// one (a quoted layer, a stub's source, a link).
    pub fn follow(&mut self, via: BrowseVia) {
        self.pending = Some(Pending::Follow(via));
    }

    /// The open didn't happen (a fetch failed, or it was already open).
    pub fn cancel(&mut self) {
        self.pending = None;
    }

    /// The pane closed: the walk is over (the log keeps it).
    pub fn end(&mut self) {
        *self = Trail::default();
    }

    /// Back one entry: drops the current one and returns the entry to open.
    pub fn back(&mut self) -> Option<Step> {
        if self.stack.len() < 2 {
            return None;
        }
        let left = self.stack.pop()?;
        self.pending = Some(Pending::Back { from: left.step_id });
        self.stack.last().cloned()
    }

    /// The breadcrumb: back to entry `i` at once (not logged, as upstream's
    /// `popTo`). Returns the entry to open.
    pub fn jump(&mut self, i: usize) -> Option<Step> {
        if i + 1 >= self.stack.len() {
            return None;
        }
        self.stack.truncate(i + 1);
        self.pending = None;
        self.stack.last().cloned()
    }

    pub fn can_go_back(&self) -> bool {
        self.stack.len() > 1
    }

    /// An entry opened: a new walk, a step down, a step back, or the same
    /// entry again. Keeps the live path; returns what to log, if anything.
    /// `fresh` makes a new walk's id.
    pub fn visit(&mut self, step: Step, fresh: impl FnOnce() -> String) -> Option<LogPlan> {
        let pending = self.pending.take();
        let same_top = self.stack.last().is_some_and(|t| t.same(&step));
        if same_top {
            if let Some(top) = self.stack.last_mut() {
                if !step.title.trim().is_empty() {
                    top.title = step.title.clone();
                }
                if step.version.is_some() {
                    top.version = step.version;
                }
            }
            return match pending {
                Some(Pending::Back { from }) => Some(LogPlan {
                    start: None,
                    step: self.new_step(from, top_clone(&self.stack), BrowseVia::Back),
                }),
                _ => None,
            };
        }
        match pending {
            Some(Pending::Follow(via)) if !self.stack.is_empty() => {
                let parent = self.stack.last_mut()?;
                let start = (!parent.logged).then(|| {
                    parent.logged = true;
                    parent.clone()
                });
                let parent_id = parent.step_id;
                let start = start.map(|s| self.new_step(None, s, BrowseVia::Start));
                let mut step = step;
                step.logged = true;
                step.step_id = None;
                self.stack.push(step.clone());
                Some(LogPlan {
                    start,
                    step: self.new_step(parent_id, step, via),
                })
            }
            // Opened some other way: a new walk, logged once it goes on.
            _ => {
                self.id = Some(fresh());
                let mut step = step;
                step.logged = false;
                step.step_id = None;
                self.stack = vec![step];
                None
            }
        }
    }

    /// The backend's answer for a logged step: label the entry that's still
    /// there (the reader may have moved on).
    pub fn answered(&mut self, trail: &str, origin: &str, id: &str, step_id: i64) {
        if self.id.as_deref() != Some(trail) {
            return;
        }
        let probe = Step::new(origin, id, None, "");
        if let Some(s) = self
            .stack
            .iter_mut()
            .find(|s| s.same(&probe) && s.step_id.is_none())
        {
            s.step_id = Some(step_id);
        }
    }

    /// The breadcrumb's labels, oldest first; empty unless the walk has gone
    /// somewhere (upstream shows it from two entries).
    pub fn crumbs(&self) -> Vec<String> {
        if self.stack.len() < 2 {
            return Vec::new();
        }
        self.stack.iter().map(Step::label).collect()
    }

    fn new_step(&self, parent_id: Option<i64>, s: Step, via: BrowseVia) -> NewBrowseStep {
        NewBrowseStep {
            trail: self.id.clone().unwrap_or_default(),
            parent_id,
            origin: s.origin,
            remote_id: s.id,
            version: s.version,
            via,
            title: (!s.title.trim().is_empty() && s.title != "…")
                .then(|| s.title.chars().take(TITLE_MAX).collect()),
        }
    }
}

fn top_clone(stack: &[Step]) -> Step {
    stack
        .last()
        .cloned()
        .unwrap_or_else(|| Step::new("", "", None, ""))
}

// ================================================================ the log

/// The word for how a logged step was reached (upstream's `viaWord`).
pub fn via_word(via: BrowseVia) -> &'static str {
    match via {
        BrowseVia::Start => "opened",
        BrowseVia::Stub => "stubbed source",
        BrowseVia::Quote => "quoted layer",
        BrowseVia::Link => "link",
        BrowseVia::Back => "back",
    }
}

/// One row of a walk in the history pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkRow {
    pub step: BrowseStep,
    /// Indent: a step sits under the step it was reached from.
    pub depth: usize,
}

/// One walk (trail) of the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Walk {
    pub trail: String,
    /// When it started (its first row's `at`).
    pub at: String,
    /// Entries opened, back steps not counted.
    pub opened: usize,
    pub rows: Vec<WalkRow>,
}

/// The log grouped by walk, newest walk first, each walk's rows in order,
/// indented as the tree they were (upstream's history pane).
pub fn walks(steps: &[BrowseStep]) -> Vec<Walk> {
    let mut by: Vec<(String, Vec<BrowseStep>)> = Vec::new();
    for s in steps {
        match by.iter_mut().find(|(t, _)| *t == s.trail) {
            Some((_, v)) => v.push(s.clone()),
            None => by.push((s.trail.clone(), vec![s.clone()])),
        }
    }
    let mut out: Vec<Walk> = by
        .into_iter()
        .map(|(trail, mut v)| {
            v.sort_by_key(|s| s.id);
            let mut depth: HashMap<i64, usize> = HashMap::new();
            let rows = v
                .into_iter()
                .map(|s| {
                    let d = s.parent_id.and_then(|p| depth.get(&p)).map_or(0, |d| d + 1);
                    let d = if s.via == BrowseVia::Back {
                        d.saturating_sub(1)
                    } else {
                        d
                    };
                    depth.insert(s.id, d);
                    WalkRow { step: s, depth: d }
                })
                .collect::<Vec<_>>();
            Walk {
                trail,
                at: rows.first().map(|r| r.step.at.clone()).unwrap_or_default(),
                opened: rows
                    .iter()
                    .filter(|r| r.step.via != BrowseVia::Back)
                    .count(),
                rows,
            }
        })
        .collect();
    out.sort_by_key(|w| std::cmp::Reverse(w.rows.iter().map(|r| r.step.id).max()));
    out
}

// ================================================================ links

/// A link as the cache keys it: absolute, without its fragment. `None` for
/// anything but http(s).
pub fn norm_url(url: &str) -> Option<String> {
    let mut u = url::Url::parse(url.trim()).ok()?;
    if !matches!(u.scheme(), "http" | "https") {
        return None;
    }
    u.set_fragment(None);
    Some(u.to_string())
}

/// The http(s) links in a post's HTML body, resolved against its origin (the
/// reader's `<base href>`), without duplicates, at most [`LINKS_PER_POST`].
pub fn body_links(html: &str, origin: &str) -> Vec<String> {
    let base =
        crate::app::studio::reader::origin_base(origin).and_then(|b| url::Url::parse(&b).ok());
    let mut out: Vec<String> = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut at = 0;
    while let Some(i) = lower[at..].find("<a") {
        let start = at + i;
        at = start + 2;
        if !lower[at..].starts_with(|c: char| c.is_whitespace()) {
            continue;
        }
        let Some(end) = lower[start..].find('>').map(|e| start + e) else {
            break;
        };
        let tag = &html[start..end];
        let tag_lower = &lower[start..end];
        at = end;
        let Some(h) = tag_lower.find("href=").map(|h| h + 5) else {
            continue;
        };
        let rest = &tag[h..];
        let value = match rest.chars().next() {
            Some(q @ ('"' | '\'')) => rest[1..].split(q).next().unwrap_or(""),
            _ => rest
                .split(|c: char| c.is_whitespace() || c == '>')
                .next()
                .unwrap_or(""),
        };
        let href = value
            .replace("&amp;", "&")
            .replace("&#38;", "&")
            .trim()
            .to_string();
        if href.is_empty() || href.starts_with('#') {
            continue;
        }
        let abs = match url::Url::parse(&href) {
            Ok(u) => Some(u.to_string()),
            Err(url::ParseError::RelativeUrlWithoutBase) => base
                .as_ref()
                .and_then(|b| b.join(&href).ok())
                .map(|u| u.to_string()),
            Err(_) => None,
        };
        if let Some(n) = abs.as_deref().and_then(norm_url)
            && !out.contains(&n)
        {
            out.push(n);
            if out.len() >= LINKS_PER_POST {
                break;
            }
        }
    }
    out
}

// ================================================================ state

/// The reader's browsing state (inside `reading::State`).
pub struct Browse {
    pub trail: Trail,
    /// Which links name a blyg item, by [`norm_url`]: `None` = not one.
    pub classified: HashMap<String, Option<ItemRef>>,
    /// Asked and not answered yet.
    asking: HashSet<String>,
    /// The history pane is shown, and what it lists.
    pub history_open: bool,
    pub history: Load<Vec<BrowseStep>>,
    /// "clear history" was clicked once (click again).
    pub clear_confirm: bool,
}

impl Default for Browse {
    fn default() -> Self {
        Browse {
            trail: Trail::default(),
            classified: HashMap::new(),
            asking: HashSet::new(),
            history_open: false,
            history: Load::Idle,
            clear_confirm: false,
        }
    }
}

impl Browse {
    /// The item a link names, when that's known.
    #[cfg(test)]
    pub fn target(&self, url: &str) -> Option<Option<&ItemRef>> {
        let n = norm_url(url)?;
        self.classified.get(&n).map(Option::as_ref)
    }
}

// ================================================================ MainView

impl MainView {
    /// Hook (`reading_actions`): ⌘[ and ⇧⌘H.
    pub(super) fn browse_actions(&self, d: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        d.on_action(cx.listener(|this, _: &ReaderBack, window, cx| this.reader_back(window, cx)))
            .on_action(
                cx.listener(|this, _: &ToggleBrowseHistory, _, cx| this.toggle_browse_history(cx)),
            )
    }

    /// Hook: the post in `reading.opened` has just opened. Keeps the trail,
    /// logs the step in the background, and asks which of its links name
    /// blyg items.
    pub(crate) fn browse_visited(&mut self, cx: &mut Context<Self>) {
        let Some(o) = self.reading.opened.as_ref() else {
            return;
        };
        let item = &o.item;
        let step = Step::new(
            &item.origin,
            &item.remote_id,
            Some(item.version),
            &super::vm::post_title(item),
        );
        let html = if item.content_html.trim().is_empty() {
            blyg_render::render_markdown(&item.content_md)
        } else {
            item.content_html.clone()
        };
        let links = body_links(&html, &item.origin);
        if let Some(plan) = self
            .reading
            .browse
            .trail
            .visit(step, blyg_core::browse::new_trail)
        {
            self.browse_log(plan, cx);
        }
        self.browse_classify(links, cx);
    }

    /// The pane is about to show a post while none was open: a new walk.
    pub(crate) fn browse_pane_opening(&mut self) {
        if self.reading.opened.is_none() {
            self.reading.browse.trail.end();
        }
    }

    /// Send a plan to the log (best effort), then label the trail's entries
    /// with the rows' ids.
    fn browse_log(&mut self, plan: LogPlan, cx: &mut Context<Self>) {
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move {
            let start_id = plan
                .start
                .as_ref()
                .and_then(|s| crate::guarded(|| backend.log_browse(s)).ok());
            let mut step = plan.step.clone();
            if plan.start.is_some() {
                step.parent_id = start_id;
            }
            let id = crate::guarded(|| backend.log_browse(&step)).ok();
            (plan, start_id, id)
        });
        cx.spawn(async move |this, cx| {
            let (plan, start_id, id) = task.await;
            let _ = this.update(cx, |v, cx| {
                let t = &mut v.reading.browse.trail;
                if let (Some(s), Some(sid)) = (&plan.start, start_id) {
                    t.answered(&s.trail, &s.origin, &s.remote_id, sid);
                }
                if let Some(id) = id
                    && plan.step.via != BrowseVia::Back
                {
                    let s = &plan.step;
                    t.answered(&s.trail, &s.origin, &s.remote_id, id);
                }
                if v.reading.browse.history_open {
                    v.load_browse_history(cx);
                }
            });
        })
        .detach();
    }

    /// Ask which of `links` name blyg items (those not known or asked yet).
    fn browse_classify(&mut self, links: Vec<String>, cx: &mut Context<Self>) {
        let b = &mut self.reading.browse;
        let unknown: Vec<String> = links
            .into_iter()
            .filter(|u| !b.classified.contains_key(u) && !b.asking.contains(u))
            .collect();
        if unknown.is_empty() {
            return;
        }
        b.asking.extend(unknown.iter().cloned());
        let backend = self.backend.clone();
        let asked = unknown.clone();
        let task =
            cx.background_spawn(async move { crate::guarded(|| backend.classify_links(&asked)) });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, _| {
                let b = &mut v.reading.browse;
                for u in &unknown {
                    b.asking.remove(u);
                }
                // On an error the links stay plain; asked again next time.
                if let Ok(found) = r {
                    b.remember(&unknown, found);
                }
            });
        })
        .detach();
    }

    /// A link in a post was clicked. A blyg item opens in the reader (a
    /// step of the walk when `in_reader`); anything else goes where links
    /// go (the browser pane or the default browser), so your place in the
    /// reader is kept. `modified`: ⌘/⌥ held, which always means "a link".
    pub(crate) fn browse_link(
        &mut self,
        url: String,
        modified: bool,
        in_reader: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(n) = norm_url(&url).filter(|_| !modified) else {
            self.open_link(url, window, cx);
            return;
        };
        match self.reading.browse.classified.get(&n).cloned() {
            Some(Some(t)) => self.browse_open_target(t, in_reader, window, cx),
            Some(None) => self.open_link(url, window, cx),
            None => {
                // Not answered yet: ask about this one now.
                let backend = self.backend.clone();
                let asked = vec![n.clone()];
                let task = cx.background_spawn(async move {
                    crate::guarded(|| backend.classify_links(&asked))
                });
                cx.spawn_in(window, async move |this, cx| {
                    let r = task.await;
                    let _ = this.update_in(cx, |v, window, cx| {
                        let target = match r {
                            Ok(found) => {
                                v.reading.browse.remember(std::slice::from_ref(&n), found);
                                v.reading.browse.classified.get(&n).cloned().flatten()
                            }
                            Err(_) => None,
                        };
                        match target {
                            Some(t) => v.browse_open_target(t, in_reader, window, cx),
                            None => v.open_link(url, window, cx),
                        }
                    });
                })
                .detach();
            }
        }
    }

    fn browse_open_target(
        &mut self,
        t: ItemRef,
        in_reader: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if in_reader {
            self.reading.browse.trail.follow(BrowseVia::Link);
        }
        self.open_original(t.origin, t.id, None, window, cx);
    }

    /// A quoted layer or a stub's source inside the open post was clicked.
    pub(crate) fn browse_follow(
        &mut self,
        via: BrowseVia,
        origin: String,
        id: String,
        version: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.reading.view == View::Reading && self.reading.opened.is_some() {
            self.reading.browse.trail.follow(via);
        }
        self.open_original(origin, id, version, window, cx);
    }

    /// ⌘[ / "‹ back": the previous entry of the walk.
    pub(crate) fn reader_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.reading.view != View::Reading || self.reading.opened.is_none() {
            return;
        }
        if let Some(s) = self.reading.browse.trail.back() {
            self.open_original(s.origin, s.id, None, window, cx);
        }
    }

    /// A breadcrumb (or the history pane's "this walk") entry.
    pub(crate) fn browse_jump(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.reading.browse.trail.jump(i) {
            self.open_original(s.origin, s.id, None, window, cx);
        }
    }

    /// ⇧⌘H / the header's "history": show or hide the pane beside the post.
    pub(crate) fn toggle_browse_history(&mut self, cx: &mut Context<Self>) {
        if self.reading.view != View::Reading {
            return;
        }
        let b = &mut self.reading.browse;
        b.history_open = !b.history_open;
        b.clear_confirm = false;
        if b.history_open {
            if self.reading.opened.is_none() {
                self.show_toast(
                    "Open a post to see your browsing history beside it",
                    None,
                    cx,
                );
            }
            self.load_browse_history(cx);
        }
        cx.notify();
    }

    pub(crate) fn load_browse_history(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.reading.browse.history, Load::Ready(_)) {
            self.reading.browse.history = Load::Loading;
        }
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move {
            crate::guarded(|| backend.browse_history(None, 0, HISTORY_ROWS))
        });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                v.reading.browse.history = match r {
                    Ok(rows) => Load::Ready(rows),
                    Err(e) => Load::Failed(e.to_string()),
                };
                cx.notify();
            });
        })
        .detach();
    }

    #[cfg(test)]
    pub(crate) fn clear_browse_history_for_test(&mut self, cx: &mut Context<Self>) {
        self.clear_browse_history(cx)
    }

    /// "clear history" (twice): the whole private log.
    fn clear_browse_history(&mut self, cx: &mut Context<Self>) {
        if !self.reading.browse.clear_confirm {
            self.reading.browse.clear_confirm = true;
            cx.notify();
            return;
        }
        self.reading.browse.clear_confirm = false;
        let backend = self.backend.clone();
        let task =
            cx.background_spawn(async move { crate::guarded(|| backend.clear_browse(None)) });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                match r {
                    Ok(()) => {
                        // The walk in progress is no longer in the log: what
                        // follows starts a fresh one.
                        let t = &mut v.reading.browse.trail;
                        let keep = t.stack.last().cloned();
                        t.end();
                        if let Some(mut s) = keep {
                            s.logged = false;
                            s.step_id = None;
                            t.id = Some(blyg_core::browse::new_trail());
                            t.stack = vec![s];
                        }
                        v.show_toast("Browsing history cleared", None, cx);
                    }
                    Err(e) => v.show_toast(format!("Couldn't clear the history: {e}"), None, cx),
                }
                v.load_browse_history(cx);
            });
        })
        .detach();
    }

    /// The not-subscribed alert's "subscribe?": the Subscribe sheet with the
    /// blyg's address in it, previewing at once.
    pub(crate) fn subscribe_prefilled(
        &mut self,
        origin: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_subscribe(window, cx);
        if let Some(super::RSheet::Subscribe { input, .. }) = self.reading.sheet.as_ref() {
            let input = input.clone();
            input.update(cx, |s, cx| s.set_value(origin, window, cx));
            self.subscribe_enter(window, cx);
        }
    }

    // ------------------------------------------------------------ drawing

    /// The reading pane's top line: "‹ back", the breadcrumb, "history".
    pub(crate) fn render_trail_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette.on_page();
        let t = &self.reading.browse.trail;
        let crumbs = t.crumbs();
        let last = crumbs.len().saturating_sub(1);
        let back = t.can_go_back();
        let link = |d: Stateful<Div>| {
            d.cursor_pointer()
                .text_color(p.ink)
                .hover(|s| s.text_color(p.accent))
        };
        let button = |d: Stateful<Div>| {
            d.cursor_pointer()
                .text_color(p.ink)
                .hover(|s| s.text_color(p.accent).bg(p.sel))
        };
        div()
            .id("trail-bar")
            .debug_selector(|| "trail-bar".into())
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.))
            .mb(px(8.))
            .pr(px(48.)) // the stream pane's "✕ esc"
            .font_family(self.chrome())
            .text_size(px(11.5))
            .text_color(p.muted)
            .child(
                div()
                    .id("trail-back")
                    .debug_selector(|| "trail-back".into())
                    .px(px(6.))
                    .rounded(px(self.theme.corner(5.)))
                    .when(back, |d| {
                        button(d).on_click(
                            cx.listener(|this, _, window, cx| this.reader_back(window, cx)),
                        )
                    })
                    .when(!back, |d| d.opacity(0.45))
                    .child("‹ back")
                    .tooltip(|_, cx| {
                        cx.new(|_| super::Tip("Back along the links you followed · ⌘[".into()))
                            .into()
                    }),
            )
            .children(crumbs.into_iter().enumerate().map(|(i, c)| {
                let el = div()
                    .id(("trail-crumb", i))
                    .max_w(px(180.))
                    .truncate()
                    .child(c);
                let el = if i == last {
                    el.text_color(p.ink).font_weight(FontWeight::MEDIUM)
                } else {
                    link(el).on_click(
                        cx.listener(move |this, _, window, cx| this.browse_jump(i, window, cx)),
                    )
                };
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .min_w_0()
                    .when(i > 0, |d| d.child("›"))
                    .child(el)
            }))
            .child(div().flex_1())
            .child(
                div()
                    .id("trail-history")
                    .debug_selector(|| "trail-history".into())
                    .px(px(6.))
                    .rounded(px(self.theme.corner(5.)))
                    .map(button)
                    .when(self.reading.browse.history_open, |d| d.text_color(p.accent))
                    .child("history")
                    .tooltip(|_, cx| {
                        cx.new(|_| super::Tip("Browsing history · ⇧⌘H".into()))
                            .into()
                    })
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_browse_history(cx))),
            )
            .into_any_element()
    }

    /// The note on a post that isn't held (a peek): the not-subscribed alert
    /// with "subscribe?", or that the blyg is followed but the post isn't
    /// held.
    pub(crate) fn render_peek_note(
        &self,
        o: &super::Opened,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !super::original::is_external(&o.key) {
            return None;
        }
        let p = self.palette.on_page();
        let origin = o.item.origin.clone();
        let followed = self
            .reading
            .subs
            .iter()
            .any(|s| blyg_core::profile::same_origin(&s.origin, &origin));
        let base = div()
            .id("peek-note")
            .debug_selector(|| "peek-note".into())
            .mb(px(10.))
            .px(px(10.))
            .py(px(6.))
            .rounded(px(self.theme.corner(7.)))
            .font_family(self.chrome())
            .text_size(px(12.5))
            .bg(p.notice_bg)
            .text_color(p.notice);
        if followed {
            return Some(base.child(PEEK_HELD_ORIGIN).into_any_element());
        }
        Some(
            base.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(5.))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(NOT_SUBSCRIBED),
                    )
                    .child("—")
                    .child(
                        div()
                            .id("peek-subscribe")
                            .debug_selector(|| "peek-subscribe".into())
                            .cursor_pointer()
                            .text_color(p.accent)
                            .border_b_1()
                            .border_color(p.accent.opacity(0.5))
                            .child(NOT_SUBSCRIBED_ASK)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.subscribe_prefilled(origin.clone(), window, cx)
                            })),
                    ),
            )
            .child(div().text_size(px(11.5)).opacity(0.85).child(PEEK_NOTE))
            .into_any_element(),
        )
    }

    /// The detail with the history pane beside it, when that's open.
    pub(crate) fn with_history_pane(
        &self,
        detail: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !self.reading.browse.history_open || self.reading.opened.is_none() {
            return detail;
        }
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .child(detail)
            .child(self.render_history_pane(cx))
            .into_any_element()
    }

    fn render_history_pane(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette.on_page();
        let b = &self.reading.browse;
        let row = |id: ElementId, depth: usize, lead: &'static str, text: String| {
            div()
                .id(id)
                .flex()
                .gap(px(5.))
                .py(px(2.))
                .pl(px(depth.min(8) as f32 * 12.))
                .cursor_pointer()
                .text_color(p.ink)
                .hover(|s| s.text_color(p.accent))
                .child(div().flex_none().text_color(p.muted).child(lead))
                .child(div().min_w_0().truncate().child(text))
        };
        let heading = |s: &'static str| {
            div()
                .mt(px(12.))
                .mb(px(4.))
                .text_size(px(11.))
                .text_color(p.muted)
                .font_weight(FontWeight::SEMIBOLD)
                .child(s)
        };
        let stack = &b.trail.stack;
        let here = stack.len().saturating_sub(1);
        let this_walk = (!stack.is_empty()).then(|| {
            div()
                .child(heading("This walk"))
                .children(stack.iter().enumerate().map(|(i, s)| {
                    let el = row(ElementId::from(("browse-here", i)), i, "↳", s.label());
                    if i == here {
                        el.font_weight(FontWeight::MEDIUM)
                    } else {
                        el.on_click(
                            cx.listener(move |this, _, window, cx| this.browse_jump(i, window, cx)),
                        )
                    }
                }))
        });
        let earlier: AnyElement = match &b.history {
            Load::Ready(steps) if steps.is_empty() => div()
                .italic()
                .text_color(p.muted)
                .child(HISTORY_EMPTY)
                .into_any_element(),
            Load::Ready(steps) => div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .children(walks(steps).into_iter().map(|w| {
                    let now = b.trail.id.as_deref() == Some(w.trail.as_str());
                    div()
                        .child(div().text_size(px(11.)).text_color(p.muted).child(format!(
                            "{} · {} opened{}",
                            crate::vm::relative_time(&w.at, self.now),
                            w.opened,
                            if now { " · now" } else { "" }
                        )))
                        .children(w.rows.into_iter().map(|r| {
                            let s = r.step;
                            let lead = if s.via == BrowseVia::Back {
                                "↩"
                            } else {
                                "↳"
                            };
                            let (o, i) = (s.origin.clone(), s.remote_id.clone());
                            row(
                                ElementId::from(("browse-step", s.id as usize)),
                                r.depth,
                                lead,
                                label(s.title.as_deref().unwrap_or(""), &s.remote_id),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(px(10.5))
                                    .text_color(p.muted)
                                    .child(via_word(s.via)),
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.browse_follow(
                                        BrowseVia::Link,
                                        o.clone(),
                                        i.clone(),
                                        None,
                                        window,
                                        cx,
                                    )
                                },
                            ))
                        }))
                }))
                .into_any_element(),
            Load::Failed(e) => div()
                .text_color(p.muted)
                .child(format!("Couldn't load the history: {e}"))
                .into_any_element(),
            _ => div()
                .text_color(p.muted)
                .child("loading…")
                .into_any_element(),
        };
        let has_rows = matches!(&b.history, Load::Ready(s) if !s.is_empty());
        div()
            .id("browse-history")
            .debug_selector(|| "browse-history".into())
            .w(px(270.))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .rule_l(&p)
            .font_family(self.chrome())
            .text_size(px(12.))
            .child(
                div()
                    .flex_none()
                    .px(px(14.))
                    .pt(px(22.))
                    .pb(px(8.))
                    .rule_b(&p)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .child(
                                div()
                                    .flex_1()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_size(px(13.))
                                    .child(HISTORY_TITLE),
                            )
                            .child(
                                div()
                                    .id("browse-history-close")
                                    .cursor_pointer()
                                    .text_color(p.muted)
                                    .hover(|s| s.text_color(p.ink))
                                    .child("✕")
                                    .on_click(
                                        cx.listener(|this, _, _, cx| {
                                            this.toggle_browse_history(cx)
                                        }),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(2.))
                            .text_size(px(11.))
                            .text_color(p.muted)
                            .child(HISTORY_PRIVATE),
                    ),
            )
            .child(
                div()
                    .id("browse-history-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(14.))
                    .pb(px(12.))
                    .children(this_walk)
                    .child(heading("Earlier walks"))
                    .child(earlier),
            )
            .when(has_rows, |d| {
                let confirm = b.clear_confirm;
                d.child(
                    div().flex_none().px(px(14.)).py(px(10.)).rule_t(&p).child(
                        self.chip(
                            "browse-clear",
                            if confirm {
                                "click again to clear"
                            } else {
                                "clear history"
                            },
                        )
                        .when(confirm, |d| d.border_color(p.over).text_color(p.over))
                        .on_click(cx.listener(|this, _, _, cx| this.clear_browse_history(cx))),
                    ),
                )
            })
            .into_any_element()
    }
}

impl Browse {
    /// Note the backend's answers for `asked` (any it didn't mention: not a
    /// blyg item).
    fn remember(&mut self, asked: &[String], found: Vec<blyg_core::browse::LinkTarget>) {
        for l in found {
            if let Some(n) = norm_url(&l.url) {
                self.classified.insert(n, l.target);
            }
        }
        for u in asked {
            self.classified.entry(u.clone()).or_insert(None);
        }
    }
}

// ================================================================ demo

impl MainView {
    /// `BLYGGER_DEMO=rd-browse…` / `rd-stream-nested` (fake mode): the
    /// walk after two hops, the history pane, a peek of a blyg nobody here
    /// follows, and a stream post whose quote quotes another.
    pub(crate) fn browse_demo(
        &mut self,
        scenario: &str,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::fake::reading_seed::{
            ADA, ADA_REPLY, ADA_TIDES, KIT, KIT_TIDES, LIN, LIN_BENCH,
        };
        match (scenario, n) {
            (_, 0) => self.show_view(View::Reading, window, cx),
            ("rd-stream-nested", 1) => {
                // A post (on screen only) quoting Ada's reply, which quotes
                // Lin's bench note.
                let Some(mut q) = self
                    .reading
                    .rows
                    .iter()
                    .find(|r| r.remote_id == ADA_REPLY)
                    .cloned()
                else {
                    return;
                };
                q.subscription_id = "sub-rue".into();
                q.subscription_title = "Rue's Notebook".into();
                q.origin = "https://rue.blyg.example.com/".into();
                q.remote_id = "01k2rve0nested000000000001".into();
                q.author = q.author.map(|mut a| {
                    a.name = Some("Rue".into());
                    a
                });
                q.content_md = format!(
                    "Quotes all the way down\n\nAda's reply, and the note it answers:\n\n![[{ADA_REPLY}]]\n\nEach layer says whose it is."
                );
                q.content_html.clear();
                q.stub_of = None;
                q.transclusions = vec![blyg_core::TransclusionRef {
                    id: ADA_REPLY.into(),
                    version: Some(1),
                    origin: Some(ADA.into()),
                    cited: None,
                    selector: None,
                }];
                q.created = Some(chrono::Utc::now().to_rfc3339());
                q.updated = q.created.clone();
                self.reading.rows.insert(0, q);
                self.reading.refilter();
                self.stream_move(1, window, cx);
            }
            (_, 1) => {
                if let Some(k) = self
                    .reading
                    .rows
                    .iter()
                    .find(|r| r.remote_id == ADA_REPLY)
                    .map(super::vm::key)
                {
                    self.open_reading(k, window, cx);
                }
                self.browse_follow(
                    BrowseVia::Quote,
                    LIN.into(),
                    LIN_BENCH.into(),
                    Some(1),
                    window,
                    cx,
                );
            }
            ("rd-browse-peek", 2) => {
                let link = format!("{KIT}f/{KIT_TIDES}/");
                self.reading.browse.classified.insert(
                    link.clone(),
                    Some(ItemRef {
                        origin: KIT.into(),
                        id: KIT_TIDES.into(),
                    }),
                );
                self.browse_link(link, false, true, window, cx);
            }
            ("rd-browse" | "rd-browse-history", 2) => {
                self.browse_link(format!("{ADA}f/{ADA_TIDES}/"), false, true, window, cx);
                if scenario == "rd-browse-history" {
                    self.reading.browse.history_open = true;
                    self.load_browse_history(cx);
                }
            }
            _ => {}
        }
    }
}
