//! --- tray --- (proposal 7) drag-to-quote and the quote tray: the mouse
//! way to quote, and a place for quotes picked up but not placed yet.
//!
//! - **Drag a post** (a stream card, a Reader row, a row of the lineage
//!   line's lists, the held post in split view, a tray chip) and drop it:
//!   - on the editor: a thread gets `![[id | label]]` on its own line after
//!     the line it was dropped on; a fragment gets `[[id | label]]` inline
//!     where it was dropped (quotes go in threads, and the toast says so);
//!   - on the omnibar: a new thread quoting it;
//!   - on the reading screen's compose box: the quote in the box;
//!   - on the tray: it waits there.
//!
//!   What's written is exactly what the picker and Quote write
//!   (`vm::quote_label`, `blyg_core::floor::picker_label`).
//! - **The tray**: ⌥⌘K (or `q` on the reading screen) puts the post in
//!   front of you in a strip above the status bar. ⌘K (and typing `![[` /
//!   `[[` with the picker as a panel) lists tray items first, marked ❝.
//!   Placing one (picked, dropped, or ⏎ on a chip) takes it off the tray;
//!   ⌫ (or ×) on a chip drops it. Kept per blyg in `state.json` across
//!   restarts (not on the sample data).
//!
//! Only blyg posts (theirs or yours, published) can be quoted; a feed post
//! can't. Nothing is fetched: a dragged post is one this Mac holds.
//!
//! A child module of `app`; hooks marked `// --- tray ---`.

use std::path::PathBuf;

use blyg_core::state::TrayEntry;
use blyg_core::{Pickable, ReadingItem};
use gpui_kit::base::input::RopeExt as _;

use super::reading::{View, vm as rvm};
use super::*;
use crate::composer::picker::PickKind;

gpui_kit::actions!(blygger, [AddToTray]);

/// The tray's mark (the strip, the picker's rows).
pub const GLYPH: &str = "❝";

/// The tour's tray: held in memory only.
pub const TOUR: &str = "~tour";

/// The most posts the tray holds (the oldest go first).
pub const MAX: usize = 24;

/// The quote tray (a field of `MainView`).
#[derive(Default)]
pub struct Tray {
    pub items: Vec<TrayEntry>,
    /// The highlighted chip (⏎ places it, ⌫ drops it).
    pub sel: Option<usize>,
    pub focus: Option<FocusHandle>,
    /// Where `state.json` lives (`None`: kept for the session only).
    data_dir: Option<PathBuf>,
    /// The blyg (host) the items belong to.
    host: String,
}

impl Tray {
    pub fn load(data_dir: Option<PathBuf>, host: String) -> Tray {
        let items = data_dir
            .as_deref()
            .map(blyg_core::state::AppState::load)
            .filter(|_| host != TOUR)
            .and_then(|s| s.quote_tray.get(&host).cloned())
            .unwrap_or_default();
        Tray {
            items,
            sel: None,
            focus: None,
            data_dir,
            host,
        }
    }

    fn save(&self) {
        let Some(d) = &self.data_dir else { return };
        if self.host == TOUR {
            return;
        }
        let (host, items) = (self.host.clone(), self.items.clone());
        let _ = blyg_core::state::AppState::update(d, |s| {
            if items.is_empty() {
                s.quote_tray.remove(&host);
            } else {
                s.quote_tray.insert(host, items);
            }
        });
    }

    /// Put `e` on the tray (again: it moves to the end). False when it was
    /// already there.
    pub fn add(&mut self, e: TrayEntry) -> bool {
        let had = self.items.iter().any(|x| x.id == e.id);
        self.items.retain(|x| x.id != e.id);
        self.items.push(e);
        if self.items.len() > MAX {
            self.items.remove(0);
        }
        self.save();
        !had
    }

    /// Take `id` off the tray (placed, or dropped).
    pub fn remove(&mut self, id: &str) {
        let before = self.items.len();
        self.items.retain(|x| x.id != id);
        if self.items.len() != before {
            self.sel = self
                .sel
                .filter(|_| !self.items.is_empty())
                .map(|s| s.min(self.items.len() - 1));
            self.save();
        }
    }

    /// The blyg changed (the switcher): its own tray.
    fn follow(&mut self, host: &str) {
        if self.host != host {
            *self = Tray::load(self.data_dir.clone(), host.to_string());
        }
    }
}

/// The tray's items as picker rows, first, matching `query` (every word,
/// as the picker's search), marked ❝; then `found` without them.
pub fn tray_first(items: &[TrayEntry], found: Vec<Pickable>, query: &str) -> Vec<Pickable> {
    let words = blyg_core::pick::words(query);
    let mut out: Vec<Pickable> = items
        .iter()
        .rev()
        .filter(|e| blyg_core::pick::matches(&words, &format!("{} {}", e.title, e.label), &e.id))
        .map(|e| {
            let held = found.iter().find(|p| p.id == e.id);
            Pickable {
                id: e.id.clone(),
                title: format!("{GLYPH} {}", e.title),
                excerpt: held.map(|p| p.excerpt.clone()).unwrap_or_default(),
                label: e.label.clone(),
                source: held.map_or(blyg_core::PickSource::Imported, |p| p.source),
                kind: held.map_or(Kind::Thread, |p| p.kind),
                version: held.map_or(0, |p| p.version),
                at: held.map(|p| p.at.clone()).unwrap_or_default(),
                subscription_id: Some(e.subscription_id.clone()),
                source_title: Some("quote tray".into()),
            }
        })
        .collect();
    let ids: Vec<String> = out.iter().map(|p| p.id.clone()).collect();
    out.extend(found.into_iter().filter(|p| !ids.contains(&p.id)));
    out
}

/// A reading row as a tray entry (a blyg post's id and its quote label).
pub fn entry_for_row(r: &ReadingItem) -> TrayEntry {
    TrayEntry {
        id: r.remote_id.clone(),
        label: rvm::quote_label(r),
        title: rvm::post_title(r),
        subscription_id: r.subscription_id.clone(),
    }
}

/// Your published post as a tray entry; `None` for a draft, a scratch
/// note or a withdrawn post (nothing to quote).
pub fn entry_for_item(item: &Item) -> Option<TrayEntry> {
    if item.status != blyg_core::Status::Public {
        return None;
    }
    let id = item.server_id.as_ref()?.0.clone();
    let title = item.title();
    Some(TrayEntry {
        label: blyg_core::pick::pick_label(
            Some(title.as_str()),
            blyg_core::pick::link_label_md(&item.content_md, item.kind),
        ),
        id,
        title,
        subscription_id: blyg_core::OWN_SUBSCRIPTION.into(),
    })
}

/// Where a quote of `id` goes into `text` when dropped at byte `at`.
/// In a thread: `![[id | label]]` on its own line after the line `at` is
/// on. In a fragment: `[[id | label]]` inline at `at`. The new text and
/// the caret after it.
pub fn drop_into(
    text: &str,
    at: usize,
    thread: bool,
    id: &str,
    label: Option<&str>,
) -> (String, usize) {
    let mut at = at.min(text.len());
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    if thread {
        let line_end = text[at..].find('\n').map_or(text.len(), |n| at + n);
        rvm::insert_directive(
            text,
            line_end,
            &blyg_core::grammar::directive_source(id, label),
        )
    } else {
        crate::composer::picker::insert(text, at, at, PickKind::Link, id, label)
    }
}

/// A post being dragged to be quoted.
#[derive(Debug, Clone)]
pub struct DraggedQuote {
    /// What goes between the brackets.
    pub id: String,
    pub title: String,
    /// The label, when known; else it's worked out from the reading row
    /// (`key`) on drop, so a row doesn't render Markdown just to be
    /// draggable.
    pub label: Option<String>,
    pub key: Option<rvm::Key>,
    /// Dragged from the tray (placing it takes it off).
    pub from_tray: bool,
}

impl DraggedQuote {
    pub fn row(r: &ReadingItem, title: String) -> DraggedQuote {
        DraggedQuote {
            id: r.remote_id.clone(),
            title,
            label: None,
            key: Some(rvm::key(r)),
            from_tray: false,
        }
    }

    pub fn entry(e: &TrayEntry, from_tray: bool) -> DraggedQuote {
        DraggedQuote {
            id: e.id.clone(),
            title: e.title.clone(),
            label: Some(e.label.clone()),
            key: Some((e.subscription_id.clone(), e.id.clone())),
            from_tray,
        }
    }
}

impl Render for DraggedQuote {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.))
            .py(px(3.))
            .rounded(px(6.))
            .bg(gpui_kit::black().opacity(0.78))
            .text_color(gpui_kit::white())
            .text_size(px(12.))
            .child(format!(
                "{GLYPH} {}",
                crate::vm::truncate_chars(&self.title, 40)
            ))
    }
}

/// `drag` on `d`: the element can be picked up as a quote of that post.
pub fn draggable(d: Stateful<Div>, drag: DraggedQuote) -> Stateful<Div> {
    d.on_drag(drag, |q, _, _, cx| cx.new(|_| q.clone()))
}

impl MainView {
    /// Hook: the tray's actions on the main window.
    pub(super) fn tray_actions(&self, d: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        d.on_action(cx.listener(|this, _: &AddToTray, window, cx| this.add_to_tray(window, cx)))
    }

    /// Hook (top of render): the tray follows the active blyg.
    pub(super) fn tray_frame(&mut self, cx: &mut Context<Self>) {
        // The tour works on sample posts: its tray is its own and is never
        // written to state.json, so it can't leak into the blyg's tray.
        let host = if self.onboarding.tutorial.is_some() {
            TOUR.to_string()
        } else {
            crate::settings::blyg_url(cx)
                .and_then(|u| crate::vm::url_host(&u))
                .unwrap_or_default()
        };
        self.tray.follow(&host);
        if self.tray.focus.is_none() {
            self.tray.focus = Some(cx.focus_handle());
        }
    }

    /// The tray's place in `state.json` (none on the sample data or in a
    /// headless test).
    pub(super) fn tray_data_dir(cx: &App) -> Option<PathBuf> {
        let c = cx.try_global::<crate::connection::Connection>()?;
        (crate::connection::mode(cx) != Some(crate::connection::Mode::Fake))
            .then(|| c.data_dir.clone())
    }

    /// What ⌥⌘K would put on the tray: the open (else selected) post in
    /// Reading, your open published post in Posts.
    fn tray_candidate(&self) -> Result<TrayEntry, &'static str> {
        if self.reading.view == View::Reading {
            let r = self
                .reading
                .opened
                .as_ref()
                .map(|o| o.item.clone())
                .or_else(|| {
                    let k = self.reading.sel.as_ref()?;
                    self.reading
                        .rows
                        .iter()
                        .find(|r| &rvm::key(r) == k)
                        .cloned()
                })
                .ok_or("Select a post to put it on the quote tray")?;
            return self.quotable(&r).map(|_| entry_for_row(&r));
        }
        let item = self
            .current
            .as_ref()
            .ok_or("Open a published post to put it on the quote tray")?;
        entry_for_item(item).ok_or("Only a published post can be quoted")
    }

    /// Can `r` be quoted (`![[…]]`)? Not a withdrawn post, not a feed's.
    fn quotable(&self, r: &ReadingItem) -> Result<(), &'static str> {
        if r.state == "tombstone" {
            return Err("That post was withdrawn");
        }
        let feed = self
            .reading
            .subs
            .iter()
            .find(|s| s.id == r.subscription_id)
            .is_some_and(|s| s.kind != blyg_core::SubscriptionKind::Blyg);
        if feed {
            return Err("A feed's post can't be quoted, only a blyg's");
        }
        Ok(())
    }

    /// ⌥⌘K / `q`: the post in front of you onto the tray.
    pub(crate) fn add_to_tray(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.tray_candidate() {
            Ok(e) => self.put_on_tray(e, cx),
            Err(why) => self.show_toast(why, None, cx),
        }
    }

    fn put_on_tray(&mut self, e: TrayEntry, cx: &mut Context<Self>) {
        let title = e.title.clone();
        let new = self.tray.add(e);
        self.show_toast(
            if new {
                format!("{GLYPH} On the quote tray: “{title}”")
            } else {
                format!("{GLYPH} Already on the tray: “{title}”")
            },
            Some(crate::keymap::hint("⌘K lists it first · drag it into a post").into()),
            cx,
        );
        cx.notify();
    }

    /// Hook (the quote picker's search): tray items first.
    pub(crate) fn tray_first(&self, found: Vec<Pickable>, query: &str) -> Vec<Pickable> {
        tray_first(&self.tray.items, found, query)
    }

    /// Hook (a pick from the picker, a drop): placed, so off the tray.
    pub(crate) fn tray_placed(&mut self, id: &str) {
        self.tray.remove(id);
    }

    /// The label a dropped quote is written with, and whether it can be
    /// quoted at all.
    fn resolve_quote(&self, q: &DraggedQuote) -> Result<Option<String>, &'static str> {
        if let Some(l) = q.label.clone().filter(|l| !l.trim().is_empty()) {
            return Ok(Some(l));
        }
        // The row by its key, else by its id (a lineage row knows no
        // subscription); held rows first, then the store.
        let hit = |r: &ReadingItem| match &q.key {
            Some(k) => &rvm::key(r) == k,
            None => r.remote_id == q.id,
        };
        let row = self
            .reading
            .rows
            .iter()
            .find(|r| hit(r))
            .cloned()
            .or_else(|| self.backend.reading().into_iter().find(|r| hit(r)));
        if let Some(r) = &row {
            self.quotable(r)?;
            return Ok(Some(rvm::quote_label(r)).filter(|l| !l.trim().is_empty()));
        }
        // One of your own published posts.
        let own = self
            .backend
            .items()
            .into_iter()
            .find(|i| i.server_id.as_ref().is_some_and(|s| s.0 == q.id));
        match own {
            Some(i) if i.status == blyg_core::Status::Withdrawn => Err("That post was withdrawn"),
            Some(i) => Ok(entry_for_item(&i).map(|e| e.label)),
            None => Ok(None),
        }
    }

    /// A quote dropped on the editor, at window position `pos` (`None`: at
    /// the caret).
    pub(crate) fn drop_quote_in_editor(
        &mut self,
        q: &DraggedQuote,
        pos: Option<Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let label = match self.resolve_quote(q) {
            Ok(l) => l,
            Err(why) => return self.show_toast(why, None, cx),
        };
        let Some(item) = self.current.clone() else {
            return self.quote_in_new_thread(q, label, window, cx);
        };
        let thread = item.kind == Kind::Thread;
        let (text, cursor) = {
            let s = self.editor.read(cx);
            (s.value().to_string(), s.cursor())
        };
        let at = pos
            .and_then(|p| self.editor_offset_at(p, cx))
            .unwrap_or(cursor);
        let below = self.backend.server_outdated();
        let l = label
            .as_deref()
            .and_then(|l| blyg_core::floor::picker_label(below, l));
        let (new_text, caret) = drop_into(&text, at, thread, &q.id, l);
        self.mode = Mode::Edit;
        self.editor.update(cx, |s, cx| s.focus(window, cx));
        self.splice_editor(&text, &new_text, Some(caret), window, cx);
        self.tray_placed(&q.id);
        if thread {
            self.show_toast(format!("Quoted “{}”", q.title), None, cx);
        } else {
            self.show_toast(
                format!("Linked “{}”", q.title),
                Some(crate::keymap::hint("Quotes go in threads · ⌘T makes this a thread").into()),
                cx,
            );
        }
    }

    /// Dropped on the omnibar (or the editor with nothing open): a new
    /// thread quoting it.
    pub(crate) fn quote_in_new_thread(
        &mut self,
        q: &DraggedQuote,
        label: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let below = self.backend.server_outdated();
        let l = label
            .as_deref()
            .and_then(|l| blyg_core::floor::picker_label(below, l));
        let body = format!("{}\n\n", blyg_core::grammar::directive_source(&q.id, l));
        match self.backend.create_draft(Kind::Thread, &body) {
            Ok(id) => {
                self.open_new_draft(&id, window, cx);
                self.tray_placed(&q.id);
                self.show_toast(format!("New thread quoting “{}”", q.title), None, cx);
            }
            Err(e) => self.show_toast(format!("Couldn't start a thread: {e}"), None, cx),
        }
    }

    /// Dropped on the omnibar.
    pub(crate) fn drop_quote_on_omnibar(
        &mut self,
        q: &DraggedQuote,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.resolve_quote(q) {
            Ok(label) => self.quote_in_new_thread(q, label, window, cx),
            Err(why) => self.show_toast(why, None, cx),
        }
    }

    /// Dropped on the reading screen's compose box.
    pub(crate) fn drop_quote_in_compose(
        &mut self,
        q: &DraggedQuote,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let label = match self.resolve_quote(q) {
            Ok(l) => l,
            Err(why) => return self.show_toast(why, None, cx),
        };
        let below = self.backend.server_outdated();
        let l = label
            .as_deref()
            .and_then(|l| blyg_core::floor::picker_label(below, l));
        let directive = blyg_core::grammar::directive_source(&q.id, l);
        self.compose_insert_directive(&directive, window, cx);
        self.tray_placed(&q.id);
    }

    /// Dropped on the tray: it waits there.
    fn drop_quote_on_tray(&mut self, q: &DraggedQuote, cx: &mut Context<Self>) {
        if q.from_tray {
            return;
        }
        let label = match self.resolve_quote(q) {
            Ok(l) => l,
            Err(why) => return self.show_toast(why, None, cx),
        };
        self.put_on_tray(
            TrayEntry {
                id: q.id.clone(),
                label: label.unwrap_or_else(|| q.title.clone()),
                title: q.title.clone(),
                subscription_id: q.key.clone().map(|k| k.0).unwrap_or_default(),
            },
            cx,
        );
    }

    /// The text offset under window position `pos` in the editor: on the
    /// line under it, at the character under it (else the line's end).
    /// `None` when it isn't over the text.
    fn editor_offset_at(&self, pos: Point<Pixels>, cx: &App) -> Option<usize> {
        let s = self.editor.read(cx);
        let rope = s.text();
        let text = s.value();
        let rows = s.visible_row_range()?;
        let lines = rope.lines_len();
        if lines == 0 {
            return None;
        }
        let first = rope.line_start_offset(rows.start.min(lines - 1));
        let last = if rows.end >= lines {
            rope.len()
        } else {
            rope.line_start_offset(rows.end)
        };
        let mut at = first;
        while at <= last && at <= text.len() {
            let end = text[at..].find('\n').map_or(text.len(), |n| at + n);
            let b = s.range_to_bounds(&(at..end.max(at)))?;
            let lh = s.line_height()?;
            let bottom = b.top() + b.size.height.max(lh);
            if pos.y >= b.top() && pos.y < bottom {
                let mut off = end;
                for (i, c) in text[at..end].char_indices() {
                    let k = at + i;
                    if let Some(cb) = s.range_to_bounds(&(k..k + c.len_utf8()))
                        && pos.y >= cb.top()
                        && pos.y < cb.bottom()
                        && pos.x < cb.right()
                    {
                        off = if pos.x < cb.left() + cb.size.width / 2. {
                            k
                        } else {
                            k + c.len_utf8()
                        };
                        break;
                    }
                }
                return Some(off);
            }
            if end >= text.len() {
                break;
            }
            at = end + 1;
        }
        None
    }

    /// Hook (the editor pane): a quote can be dropped on it.
    pub(super) fn editor_quote_drop(
        &self,
        d: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let hl = self.palette.accent.opacity(0.08);
        d.drag_over::<DraggedQuote>(move |s, _, _, _| s.bg(hl))
            .on_drop(cx.listener(|this, q: &DraggedQuote, window, cx| {
                let pos = window.mouse_position();
                this.drop_quote_in_editor(q, Some(pos), window, cx)
            }))
    }

    /// Hook (the omnibar): a quote dropped there starts a thread.
    pub(super) fn omnibar_quote_drop(
        &self,
        d: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let hl = self.palette.accent.opacity(0.12);
        d.drag_over::<DraggedQuote>(move |s, _, _, _| s.bg(hl))
            .on_drop(cx.listener(|this, q: &DraggedQuote, window, cx| {
                this.drop_quote_on_omnibar(q, window, cx)
            }))
    }

    // ------------------------------------------------------------ the strip

    fn place_tray_item(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(e) = self.tray.items.get(ix).cloned() else {
            return;
        };
        let q = DraggedQuote::entry(&e, true);
        if self.reading.view == View::Reading {
            self.drop_quote_in_compose(&q, window, cx);
        } else {
            self.drop_quote_in_editor(&q, None, window, cx);
        }
    }

    fn tray_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let n = self.tray.items.len();
        let Some(sel) = self.tray.sel.filter(|s| *s < n) else {
            return;
        };
        match ev.keystroke.key.as_str() {
            "backspace" | "delete" => {
                let id = self.tray.items[sel].id.clone();
                self.tray.remove(&id);
            }
            "enter" => self.place_tray_item(sel, window, cx),
            "left" => self.tray.sel = Some(sel.saturating_sub(1)),
            "right" => self.tray.sel = Some((sel + 1).min(n - 1)),
            "escape" => {
                self.tray.sel = None;
                self.focus_after_sheet(window, cx);
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// Hook: the strip above the status bar (while the tray holds
    /// something, or a post is being dragged).
    pub(super) fn render_tray(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.tray.items.is_empty() && !cx.has_active_drag() {
            return None;
        }
        let focus = self.tray.focus.clone()?;
        let p = self.palette.on_status();
        let sel = self.tray.sel;
        let chips = self.tray.items.iter().enumerate().map(|(i, e)| {
            let on = sel == Some(i);
            let id = e.id.clone();
            let f = focus.clone();
            let tip = format!(
                "{} · ⏎ places it · ⌫ drops it · or drag it into a post",
                e.label
            );
            draggable(
                div()
                    .id(("tray-chip", i))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .px(px(7.))
                    .py(px(1.))
                    .rounded(px(self.theme.corner(6.)))
                    .border_1()
                    .border_color(if on { p.accent } else { p.line })
                    .cursor_pointer()
                    .child(crate::vm::truncate_chars(&e.title, 30))
                    .child(
                        div()
                            .id(("tray-x", i))
                            .text_color(p.muted)
                            .hover(|h| h.text_color(p.ink))
                            .child("×")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.tray.remove(&id);
                                cx.notify();
                            })),
                    )
                    .tooltip(move |_, cx| cx.new(|_| super::reading::Tip(tip.clone())).into())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.tray.sel = Some(i);
                        window.focus(&f, cx);
                        cx.notify();
                    })),
                DraggedQuote::entry(e, true),
            )
        });
        let hl = p.accent.opacity(0.12);
        Some(
            div()
                .id("quote-tray")
                .track_focus(&focus)
                .on_key_down(cx.listener(Self::tray_key))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(6.))
                .px(px(12.))
                .py(px(4.))
                .rule_t(&p)
                .bg(p.bg)
                .font_family(self.chrome())
                .text_size(px(11.5))
                .text_color(p.ink)
                .drag_over::<DraggedQuote>(move |s, _, _, _| s.bg(hl))
                .on_drop(
                    cx.listener(|this, q: &DraggedQuote, _, cx| this.drop_quote_on_tray(q, cx)),
                )
                .child(
                    div()
                        .flex_none()
                        .text_color(p.accent)
                        .child(format!("{GLYPH} tray")),
                )
                .child(
                    div()
                        .id("tray-chips")
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .gap(px(5.))
                        .overflow_x_scroll()
                        .children(chips)
                        .when(self.tray.items.is_empty(), |d| {
                            d.child(
                                div()
                                    .text_color(p.muted)
                                    .child("Drop a post here to quote it later"),
                            )
                        }),
                )
                .child(
                    div()
                        .flex_none()
                        .text_color(p.muted)
                        .child(crate::keymap::hint("⌘K lists these first · ⌫ drops one")),
                )
                .into_any_element(),
        )
    }
}
