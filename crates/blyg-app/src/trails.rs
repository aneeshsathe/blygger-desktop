//! --- trails --- (proposal 5) a path through posts you chose to keep: an
//! ordered list of stops, yours and others', each with a one-line "why".
//!
//! - On the reading screen, `t` adds the open (else the selected) post to
//!   the current trail and opens an inline "why" field in the trail bar
//!   (⏎ keeps it, esc skips it). `⇧T` starts a new trail (a name, then the
//!   post in front of you as its first stop). With no trail yet, `t` asks
//!   for a name first.
//! - The trail bar sits along the foot of the reading screen, over the
//!   compose box: the trail's name (a click steps to the next trail), its
//!   stops as chips (a click opens one, drag one onto another to reorder,
//!   × takes it off) and "stop 2 of 4" for the post in front of you.
//! - Trails are listed under **Trails** in the Reader's sources pane; a
//!   trail as the source lists its stops in its order, so j/k walk it.
//! - ⌥⌘T (Blyg › Publish Trail…) writes the trail as a thread, `# name`
//!   then `![[id | why]]` per stop with a blank line between, into a new
//!   draft in the editor. Nothing is published until you publish it (⌘⏎).
//!   `h` on that sheet makes a hopper of the stops instead (private, like
//!   every new hopper; its menu makes it public).
//!
//! Local to this Mac (`Backend::trails`, store schema v19), never on the
//! wire. A stop whose post was withdrawn stays as a tombstone: its title is
//! dropped and it's left out of what's published (rule 5). Only posts from
//! blygs (and your own) can be stops, since a trail publishes as quotes.
//!
//! A child module of `app`; `app.rs` and the reading screen keep small hooks
//! marked `// --- trails ---`.

use std::collections::HashMap;

use blyg_core::{ReadingItem, Trail, TrailStop};

use super::reading::sources_vm::{Entry, Source};
use super::reading::{View, vm as rvm};
use super::*;

gpui_kit::actions!(blygger, [PublishTrail]);

/// The trail glyph (the bar, the sources pane).
pub const GLYPH: &str = "⤳";

/// What the trail bar's field is asking for.
#[derive(Debug, Clone, PartialEq)]
pub enum Ask {
    /// Why the stop at `index` of the current trail is there.
    Why { index: usize },
    /// A name for a new trail; then `then_add` becomes its first stop.
    Name { then_add: Option<TrailStop> },
}

pub struct Field {
    pub input: Entity<InputState>,
    pub ask: Ask,
}

/// The ⌥⌘T sheet.
pub struct PublishSheet {
    pub focus: FocusHandle,
    pub busy: bool,
    pub error: Option<String>,
}

/// Trails as the reading screen shows them (a field of `reading::State`).
#[derive(Default)]
pub struct Trails {
    pub list: Vec<Trail>,
    /// Trail id → its stops in order.
    pub stops: HashMap<String, Vec<TrailStop>>,
    /// The trail `t` adds to and the bar shows.
    pub current: Option<String>,
    pub field: Option<Field>,
    pub publish: Option<PublishSheet>,
}

impl Trails {
    pub fn load(backend: &dyn Backend) -> Trails {
        let mut t = Trails::default();
        t.reload(backend);
        t
    }

    /// Read the trails and their stops again.
    pub fn reload(&mut self, backend: &dyn Backend) {
        self.list = backend.trails();
        self.stops = self
            .list
            .iter()
            .map(|t| (t.id.clone(), backend.trail_stops(&t.id)))
            .collect();
        if self
            .current
            .as_ref()
            .is_some_and(|c| !self.list.iter().any(|t| &t.id == c))
        {
            self.current = None;
        }
    }

    pub fn current_trail(&self) -> Option<&Trail> {
        let id = self.current.as_ref()?;
        self.list.iter().find(|t| &t.id == id)
    }

    pub fn current_stops(&self) -> &[TrailStop] {
        self.current
            .as_ref()
            .and_then(|c| self.stops.get(c))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// The keys of a trail's stops, in order.
    pub fn keys(&self, trail: &str) -> Vec<rvm::Key> {
        self.stops
            .get(trail)
            .map(|s| s.iter().map(TrailStop::key).collect())
            .unwrap_or_default()
    }
}

/// "stop 2 of 4": where `key` is on `stops` (1-based), if it's one.
pub fn stop_label(stops: &[TrailStop], key: &rvm::Key) -> Option<String> {
    let at = stops.iter().position(|s| &s.key() == key)?;
    Some(format!("stop {} of {}", at + 1, stops.len()))
}

/// A trail as the Reader's source: its held stops, in its order (indices
/// into `rows`, among `searched`).
pub fn in_trail_order(rows: &[ReadingItem], searched: &[usize], keys: &[rvm::Key]) -> Vec<usize> {
    let ok: std::collections::HashSet<usize> = searched.iter().copied().collect();
    let at: HashMap<(&str, &str), usize> = rows
        .iter()
        .enumerate()
        .filter(|(i, _)| ok.contains(i))
        .map(|(i, r)| ((r.subscription_id.as_str(), r.remote_id.as_str()), i))
        .collect();
    keys.iter()
        .filter_map(|(s, r)| at.get(&(s.as_str(), r.as_str())).copied())
        .collect()
}

/// The sources pane with a "Trails" section after the hoppers (before
/// the folders and subscriptions).
pub fn with_trails(mut entries: Vec<Entry>, trails: &[Trail]) -> Vec<Entry> {
    if trails.is_empty() {
        return entries;
    }
    let at = entries
        .iter()
        .position(|e| matches!(e, Entry::Heading("Folders" | "Subscriptions")))
        .unwrap_or(entries.len());
    let add =
        std::iter::once(Entry::Heading("Trails")).chain(trails.iter().cloned().map(Entry::Trail));
    entries.splice(at..at, add);
    entries
}

/// A stop for `r`, with why it's there.
pub fn stop_for(r: &ReadingItem, why: &str) -> TrailStop {
    TrailStop {
        subscription_id: r.subscription_id.clone(),
        remote_id: r.remote_id.clone(),
        origin: r.origin.clone(),
        why: why.to_string(),
        title: Some(rvm::post_title(r)),
        withdrawn: false,
    }
}

/// What the publish sheet writes.
pub fn thread_text(trail: &Trail, stops: &[TrailStop], below: bool) -> String {
    blyg_core::trails::publish_text(&trail.name, stops, below)
}

/// A chip being dragged to a new place on the trail.
#[derive(Debug, Clone)]
pub struct DraggedStop {
    pub index: usize,
    pub label: SharedString,
}

impl Render for DraggedStop {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.))
            .py(px(3.))
            .rounded(px(6.))
            .bg(gpui_kit::black().opacity(0.78))
            .text_color(gpui_kit::white())
            .text_size(px(12.))
            .child(self.label.clone())
    }
}

impl MainView {
    /// Hook: the trails' actions on the main window.
    pub(super) fn trails_actions(&self, d: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        d.on_action(
            cx.listener(|this, _: &PublishTrail, window, cx| this.open_trail_publish(window, cx)),
        )
    }

    /// Hook (the reading screen's keys): `t` adds the post in front of you,
    /// `⇧T` starts a new trail. True when handled.
    pub(crate) fn trail_key(
        &mut self,
        shift: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.reading.view != View::Reading {
            return false;
        }
        if shift {
            self.new_trail(window, cx);
        } else {
            self.add_to_trail(window, cx);
        }
        true
    }

    /// Hook: the bar's field has the keyboard (the reading keys stand back).
    pub(crate) fn trail_field_focused(&self, window: &Window, cx: &App) -> bool {
        self.reading
            .trails
            .field
            .as_ref()
            .is_some_and(|f| f.input.read(cx).focus_handle(cx).is_focused(window))
            || self
                .reading
                .trails
                .publish
                .as_ref()
                .is_some_and(|p| p.focus.is_focused(window))
    }

    /// The post `t` would add: the open one, else the selected one.
    fn trail_candidate(&self) -> Option<ReadingItem> {
        if let Some(o) = &self.reading.opened {
            return Some(o.item.clone());
        }
        let key = self.reading.sel.as_ref()?;
        self.reading
            .rows
            .iter()
            .find(|r| &rvm::key(r) == key)
            .cloned()
    }

    /// Why `r` can't be a stop, if it can't.
    fn trail_refusal(&self, r: &ReadingItem) -> Option<&'static str> {
        if r.state == "tombstone" {
            return Some("That post was withdrawn");
        }
        let feed = self
            .reading
            .subs
            .iter()
            .find(|s| s.id == r.subscription_id)
            .is_some_and(|s| s.kind != blyg_core::SubscriptionKind::Blyg);
        feed.then_some("Only blyg posts can be stops: a trail publishes as quotes")
    }

    /// `t`: the post in front of you onto the current trail, then its why.
    pub(crate) fn add_to_trail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(r) = self.trail_candidate() else {
            return self.show_toast("Select a post to add it to a trail", None, cx);
        };
        if let Some(why) = self.trail_refusal(&r) {
            return self.show_toast(why, None, cx);
        }
        let stop = stop_for(&r, "");
        let Some(trail) = self.reading.trails.current.clone() else {
            return self.ask_trail(
                Ask::Name {
                    then_add: Some(stop),
                },
                window,
                cx,
            );
        };
        self.put_stop(&trail, stop, window, cx);
    }

    /// `⇧T`: name a new trail; the post in front of you is its first stop.
    pub(crate) fn new_trail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let then_add = self
            .trail_candidate()
            .filter(|r| self.trail_refusal(r).is_none())
            .map(|r| stop_for(&r, ""));
        self.ask_trail(Ask::Name { then_add }, window, cx);
    }

    fn put_stop(
        &mut self,
        trail: &str,
        stop: TrailStop,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.backend.add_trail_stop(trail, &stop) {
            Ok(index) => {
                self.reload_trails(cx);
                self.ask_trail(Ask::Why { index }, window, cx);
            }
            Err(e) => self.show_toast(format!("Couldn't add it to the trail: {e}"), None, cx),
        }
    }

    /// Read the trails again (and the Reader's list, when a trail is its
    /// source).
    pub(crate) fn reload_trails(&mut self, cx: &mut Context<Self>) {
        self.reading.trails.reload(self.backend.as_ref());
        if matches!(self.reading.source, Source::Trail(_)) {
            if let Source::Trail(id) = &self.reading.source
                && !self.reading.trails.list.iter().any(|t| &t.id == id)
            {
                self.reading.source = Source::default();
            }
            self.reading.refilter();
        }
        cx.notify();
    }

    /// Open the bar's field for `ask`.
    fn ask_trail(&mut self, ask: Ask, window: &mut Window, cx: &mut Context<Self>) {
        let placeholder = match &ask {
            Ask::Why { .. } => "Why is it on the trail?  ⏎ keeps it · esc skips",
            Ask::Name { .. } => "Name the new trail  ⏎ starts it · esc cancels",
        };
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        if let Ask::Why { index } = &ask
            && let Some(s) = self.reading.trails.current_stops().get(*index)
            && !s.why.is_empty()
        {
            let why = s.why.clone();
            input.update(cx, |s, cx| s.set_value(why, window, cx));
        }
        let sub = cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| {
            if matches!(ev, InputEvent::PressEnter { .. }) {
                this.answer_trail(window, cx);
            }
        });
        self._subs.push(sub);
        input.update(cx, |s, cx| s.focus(window, cx));
        self.reading.trails.field = Some(Field { input, ask });
        cx.notify();
    }

    /// ⏎ in the bar's field.
    pub(crate) fn answer_trail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(f) = self.reading.trails.field.take() else {
            return;
        };
        let text = f.input.read(cx).value().trim().to_string();
        window.focus(&self.reading.focus, cx);
        match f.ask {
            Ask::Why { index } => {
                if let Some(t) = self.reading.trails.current.clone()
                    && let Err(e) = self.backend.set_trail_why(&t, index, &text)
                {
                    self.show_toast(format!("Couldn't keep the why: {e}"), None, cx);
                }
                self.reload_trails(cx);
            }
            Ask::Name { then_add } => match self.backend.create_trail(&text) {
                Ok(t) => {
                    self.reading.trails.current = Some(t.id.clone());
                    self.reload_trails(cx);
                    match then_add {
                        Some(stop) => self.put_stop(&t.id, stop, window, cx),
                        None => self.show_toast(
                            format!("New trail “{}”", t.name),
                            Some("t adds the post in front of you".into()),
                            cx,
                        ),
                    }
                }
                Err(e) => self.show_toast(format!("Couldn't start the trail: {e}"), None, cx),
            },
        }
        cx.notify();
    }

    /// esc in the bar's field: skip the why, or don't start the trail.
    pub(crate) fn cancel_trail_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reading.trails.field = None;
        window.focus(&self.reading.focus, cx);
        cx.notify();
    }

    /// Make `id` the current trail (and the Reader's source, in the Reader).
    pub(crate) fn select_trail(&mut self, id: &str, cx: &mut Context<Self>) {
        self.reading.trails.current = Some(id.to_string());
        self.select_source(Source::Trail(id.to_string()), cx);
        cx.notify();
    }

    /// A click on the trail's name: the next trail.
    fn next_trail(&mut self, cx: &mut Context<Self>) {
        let list = &self.reading.trails.list;
        if list.is_empty() {
            return;
        }
        let at = self
            .reading
            .trails
            .current
            .as_ref()
            .and_then(|c| list.iter().position(|t| &t.id == c))
            .map_or(0, |i| (i + 1) % list.len());
        self.reading.trails.current = Some(list[at].id.clone());
        if matches!(self.reading.source, Source::Trail(_)) {
            let id = list[at].id.clone();
            self.select_source(Source::Trail(id), cx);
        }
        cx.notify();
    }

    /// A chip's click: open that stop (when it's held here).
    fn open_stop(&mut self, key: rvm::Key, window: &mut Window, cx: &mut Context<Self>) {
        if !self.reading.rows.iter().any(|r| rvm::key(r) == key) {
            return self.show_toast("That post isn't held on this Mac", None, cx);
        }
        self.reading.sel = Some(key.clone());
        self.open_reading(key, window, cx);
    }

    fn remove_stop(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(t) = self.reading.trails.current.clone() else {
            return;
        };
        if let Err(e) = self.backend.remove_trail_stop(&t, index) {
            self.show_toast(format!("Couldn't take it off: {e}"), None, cx);
        }
        self.reload_trails(cx);
    }

    fn move_stop(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        let Some(t) = self.reading.trails.current.clone() else {
            return;
        };
        if let Err(e) = self.backend.move_trail_stop(&t, from, to) {
            self.show_toast(format!("Couldn't move it: {e}"), None, cx);
        }
        self.reload_trails(cx);
    }

    // ------------------------------------------------------------ publish

    /// ⌥⌘T: the publish sheet for the current trail.
    pub(crate) fn open_trail_publish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.reading.trails.publish.is_some() {
            return self.close_trail_publish(window, cx);
        }
        self.reading.trails.reload(self.backend.as_ref());
        if self.reading.trails.current.is_none() {
            self.reading.trails.current = self.reading.trails.list.first().map(|t| t.id.clone());
        }
        let Some(t) = self.reading.trails.current_trail() else {
            return self.show_toast(
                "No trail yet",
                Some("In Reading, t adds a post to a trail".into()),
                cx,
            );
        };
        if self
            .reading
            .trails
            .current_stops()
            .iter()
            .all(|s| s.withdrawn)
        {
            let name = t.name.clone();
            return self.show_toast(format!("“{name}” has no stops to publish"), None, cx);
        }
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.reading.trails.publish = Some(PublishSheet {
            focus,
            busy: false,
            error: None,
        });
        cx.notify();
    }

    fn close_trail_publish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reading.trails.publish = None;
        if self.reading.view == View::Reading {
            window.focus(&self.reading.focus, cx);
        } else {
            self.focus_after_sheet(window, cx);
        }
        cx.notify();
    }

    /// ⏎ on the sheet: the trail as a thread, in a new draft in the editor.
    /// Nothing is published.
    pub(crate) fn write_trail_thread(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = self.reading.trails.current_trail().cloned() else {
            return;
        };
        let text = thread_text(
            &t,
            self.reading.trails.current_stops(),
            self.backend.server_outdated(),
        );
        self.reading.trails.publish = None;
        match self.backend.create_draft(Kind::Thread, &text) {
            Ok(id) => {
                self.open_new_draft(&id, window, cx);
                self.show_toast(
                    format!("“{}” is a thread draft", t.name),
                    Some(
                        crate::keymap::hint(
                            "Write around the quotes · nothing is published until ⌘⏎",
                        )
                        .into(),
                    ),
                    cx,
                );
            }
            Err(e) => self.show_toast(format!("Couldn't write the thread: {e}"), None, cx),
        }
    }

    /// `h` on the sheet: a hopper with the trail's stops (on the blyg,
    /// private like every new hopper).
    fn trail_to_hopper(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = self.reading.trails.current_trail().cloned() else {
            return;
        };
        let keys: Vec<rvm::Key> = self
            .reading
            .trails
            .current_stops()
            .iter()
            .filter(|s| !s.withdrawn)
            .map(TrailStop::key)
            .collect();
        if let Some(s) = &mut self.reading.trails.publish {
            s.busy = true;
            s.error = None;
        }
        let backend = self.backend.clone();
        let name = t.name.clone();
        let n = keys.len();
        let task = cx.background_spawn(async move {
            let h = backend.create_hopper(&name)?;
            for (sub, rid) in &keys {
                backend.set_in_hopper(&h.id, sub, rid, true)?;
            }
            Ok::<_, blyg_core::CoreError>(h.name)
        });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |v, window, cx| match r {
                Ok(name) => {
                    v.close_trail_publish(window, cx);
                    v.reading_changed(cx);
                    v.show_toast(
                        format!("Hopper “{name}” made with {n} posts"),
                        Some("Private until you make it public (its menu)".into()),
                        cx,
                    );
                }
                Err(e) => {
                    if let Some(s) = &mut v.reading.trails.publish {
                        s.busy = false;
                        s.error = Some(format!("Couldn't make the hopper: {e}"));
                    }
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn trail_publish_key(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let busy = self.reading.trails.publish.as_ref().is_some_and(|s| s.busy);
        let m = &ev.keystroke.modifiers;
        if m.platform || m.control || m.alt {
            return;
        }
        match ev.keystroke.key.as_str() {
            "escape" => self.close_trail_publish(window, cx),
            "enter" if !busy => self.write_trail_thread(window, cx),
            "h" if !busy => self.trail_to_hopper(window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    /// Hook: the publish sheet over everything.
    pub(super) fn render_trail_publish(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let s = self.reading.trails.publish.as_ref()?;
        let t = self.reading.trails.current_trail()?;
        let p = self.palette.on_page();
        let text = thread_text(
            t,
            self.reading.trails.current_stops(),
            self.backend.server_outdated(),
        );
        let preview: String = text.lines().take(9).collect::<Vec<_>>().join("\n");
        let key = |k: &'static str, label: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(px(5.))
                .child(
                    div()
                        .px(px(6.))
                        .rounded(px(4.))
                        .border_1()
                        .border_color(p.line)
                        .text_color(p.ink)
                        .child(k),
                )
                .child(label)
        };
        Some(
            div()
                .absolute()
                .top(px(TITLEBAR_H))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .id("trail-publish")
                        .occlude()
                        .track_focus(&s.focus)
                        .on_key_down(cx.listener(Self::trail_publish_key))
                        .w(px(500.))
                        .max_w(relative(0.92))
                        .map(|d| self.sheet_style(d))
                        .text_color(p.ink)
                        .child(
                            div()
                                .mb(px(8.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format!("Publish trail “{}” as a thread", t.name)),
                        )
                        .child(
                            div()
                                .p(px(8.))
                                .rounded(px(6.))
                                .bg(p.sel)
                                .text_size(px(12.))
                                .font_family(self.chrome())
                                .text_color(p.muted)
                                .whitespace_normal()
                                .children(
                                    preview
                                        .lines()
                                        .map(|l| div().min_h(px(8.)).child(l.to_string())),
                                ),
                        )
                        .child(
                            div()
                                .mt(px(8.))
                                .text_size(px(12.))
                                .text_color(p.muted)
                                .child(
                                    "A new thread draft opens in the editor with the quotes in \
                                     order, to write around. Nothing is published until you \
                                     publish it.",
                                ),
                        )
                        .when_some(s.error.clone(), |d, e| {
                            d.child(div().mt(px(6.)).text_color(p.over).child(e))
                        })
                        .child(
                            div()
                                .mt(px(12.))
                                .flex()
                                .flex_wrap()
                                .gap(px(14.))
                                .text_color(p.muted)
                                .child(key("⏎", "write it as a draft"))
                                .child(key("h", "make a hopper instead"))
                                .child(key("esc", "cancel")),
                        ),
                )
                .into_any_element(),
        )
    }

    // ------------------------------------------------------------ the bar

    /// Hook: the trail bar along the foot of the reading screen.
    pub(super) fn render_trails_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.reading.view != View::Reading {
            return None;
        }
        let trails = &self.reading.trails;
        let naming = matches!(
            trails.field.as_ref().map(|f| &f.ask),
            Some(Ask::Name { .. })
        );
        let t = trails.current_trail();
        if t.is_none() && !naming {
            return None;
        }
        let p = self.palette.on_page();
        let stops = trails.current_stops();
        let here = self
            .reading
            .opened
            .as_ref()
            .map(|o| o.key.clone())
            .or_else(|| self.reading.sel.clone());
        let at = here.as_ref().and_then(|k| stop_label(stops, k));
        let chips = stops.iter().enumerate().map(|(i, s)| {
            let label: SharedString = if s.withdrawn {
                "withdrawn".into()
            } else {
                let why = s.why.trim();
                let t = if why.is_empty() {
                    s.title.clone().unwrap_or_default()
                } else {
                    why.to_string()
                };
                crate::vm::truncate_chars(&t, 28).into()
            };
            let on = here.as_ref() == Some(&s.key());
            let key = s.key();
            let drag = DraggedStop {
                index: i,
                label: label.clone(),
            };
            let tip: SharedString = if s.withdrawn {
                "Withdrawn by its author: its text is gone, and it's left out when the trail is published".into()
            } else {
                s.title.clone().unwrap_or_default().into()
            };
            div()
                .id(("trail-stop", i))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(4.))
                .px(px(7.))
                .py(px(2.))
                .rounded(px(self.theme.corner(6.)))
                .border_1()
                .border_color(if on { p.accent } else { p.line })
                .when(s.withdrawn, |d| d.italic().text_color(p.muted))
                .when(!s.withdrawn, |d| d.cursor_pointer().hover(|h| h.bg(p.sel)))
                .child(div().text_color(p.muted).child(format!("{}", i + 1)))
                .child(label)
                .child(
                    div()
                        .id(("trail-stop-x", i))
                        .text_color(p.muted)
                        .hover(|h| h.text_color(p.ink))
                        .child("×")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.remove_stop(i, cx)
                        })),
                )
                .tooltip(move |_, cx| cx.new(|_| super::reading::Tip(tip.to_string())).into())
                .when(!s.withdrawn, |d| {
                    d.on_click(cx.listener(move |this, _, window, cx| {
                        this.open_stop(key.clone(), window, cx)
                    }))
                })
                .on_drag(drag, |d, _, _, cx| cx.new(|_| d.clone()))
                .drag_over::<DraggedStop>(move |s, _, _, _| s.border_color(p.accent))
                .on_drop(cx.listener(move |this, d: &DraggedStop, _, cx| {
                    if d.index != i {
                        this.move_stop(d.index, i, cx)
                    }
                }))
        });
        let hint = crate::keymap::hint("t add · ⇧T new · ⌥⌘T publish…");
        let field = trails.field.as_ref().map(|f| {
            div()
                .flex_1()
                .min_w(px(160.))
                .px(px(8.))
                .py(px(2.))
                .rounded(px(self.theme.corner(6.)))
                .border_1()
                .border_color(p.accent)
                .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                    cx.stop_propagation();
                    this.cancel_trail_field(window, cx);
                }))
                .child(gpui_kit::base::input::Input::new(&f.input))
        });
        Some(
            div()
                .id("trail-bar")
                .flex_none()
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(14.))
                .py(px(6.))
                .rule_t(&p)
                .font_family(self.chrome())
                .text_size(px(12.))
                .text_color(p.ink)
                .child(
                    div()
                        .id("trail-name")
                        .flex_none()
                        .cursor_pointer()
                        .font_weight(FontWeight::SEMIBOLD)
                        .hover(|h| h.text_color(p.accent))
                        .child(match t {
                            Some(t) => format!("{GLYPH} trail {}", t.name),
                            None => format!("{GLYPH} new trail"),
                        })
                        .tooltip(|_, cx| {
                            cx.new(|_| super::reading::Tip("Click: the next trail".into()))
                                .into()
                        })
                        .on_click(cx.listener(|this, _, _, cx| this.next_trail(cx))),
                )
                .child(
                    div()
                        .id("trail-stops")
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .gap(px(5.))
                        .overflow_x_scroll()
                        .children(chips)
                        .when(stops.is_empty() && !naming, |d| {
                            d.child(
                                div()
                                    .text_color(p.muted)
                                    .child("No stops yet: t adds the post in front of you"),
                            )
                        }),
                )
                .children(field)
                .when_some(at, |d, at| {
                    d.child(div().flex_none().text_color(p.accent).child(at))
                })
                .child(div().flex_none().text_color(p.muted).child(hint))
                .into_any_element(),
        )
    }
}
