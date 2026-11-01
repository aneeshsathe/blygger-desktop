//! --- reading compose --- The compose box on the reading screen: write a
//! post while you read, the way a social app keeps "What's on your mind?"
//! beside the timeline, without leaving the post you're reading.
//!
//! **Where:** a bar along the bottom of the reading screen, under the Stream
//! (and its side pane) and under the Reader's three panes alike. It never
//! covers the post: it's a layout row, so the panes (and the reading pane's
//! WebView, which follows its pane's bounds) end where it starts. It's one
//! line until you type more, then grows to [`MAX_ROWS`] lines and scrolls.
//! The bottom suits both modes: the stream column and the Reader's list
//! are too different in width (the list is ~30%) for a box at the top of
//! either to read the same, and a full-width bar keeps its width in a
//! narrow window.
//!
//! **What it makes:** the quick-post kind, a fragment (a thread when it
//! holds a `![[…]]` quote, or is longer than a fragment can be), through
//! quick capture's own path: **Post** (⌘⏎) keeps the text as a scratch note
//! first and promotes it with `Promote::Publish` (`Backend::promote`, as
//! the capture panel does, so the same floor and safety rules apply and a
//! failed publish leaves a draft); **Draft** (⌘D) is `create_draft`; **⤢**
//! moves the text into the full editor as a draft (`open_new_draft`). A
//! failed post keeps the text and the error, and the next try reuses the
//! draft it left, so there are never two.
//!
//! **Keys:** `c` (the list focused) or ⇧⌘K (Blyg › Compose While Reading,
//! from anywhere in the window) puts the caret in it; esc gives the keys
//! back to the list and keeps the text. While it has the keyboard the
//! reading list's single keys (j/k, r/u, space…) don't fire
//! (`reading_key_down` returns first).
//!
//! **Kept:** what's typed and not yet posted is `state.json`'s
//! `compose_draft`, written on every edit, so it survives switching screens
//! and a restart. `@` mentions, spelling and the `[[` / `![[` pickers work
//! as in quick capture (the shared `composer::Assist`, its query typed in
//! the box). With a post open, **❝ Quote** puts its `![[id]]` in the box.

use std::path::{Path, PathBuf};

use blyg_core::{Kind, LocalId, Promote, Status};
use gpui_kit::base::input::{
    Enter, Escape, IndentInline, InputEditorStyle, InputEvent, MoveDown, MoveUp, Paste, Textarea,
    TextareaState,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{View, vm};
use crate::app::scratch::MakeDraft;
use crate::app::{MainView, Publish};
use crate::composer::picker::PickKind;
use crate::composer::{Assist, AssistKey};
use crate::theme::Rule;

gpui_kit::actions!(
    blygger,
    [
        /// ⇧⌘K: the caret in the reading screen's compose box (the reading
        /// screen first, from elsewhere).
        FocusCompose
    ]
);

/// The empty box's prompt.
pub const PLACEHOLDER: &str = "What's on your mind?  (c or ⇧⌘K)";

/// The box grows to this many lines, then scrolls.
pub const MAX_ROWS: usize = 8;

/// The compose box's state (a field of `reading::State`). The editor is
/// made on the first frame of the reading screen (it needs a window).
pub struct Compose {
    pub(crate) editor: Option<Entity<TextareaState>>,
    assist: Option<Entity<Assist>>,
    /// Where `state.json` lives (`None` in headless tests: nothing kept).
    pub(crate) data_dir: Option<PathBuf>,
    /// A post is on its way.
    pub(crate) busy: bool,
    /// The last post or save that failed, shown under the box.
    pub(crate) error: Option<String>,
    /// The draft a failed post left behind: the next try reuses it.
    pub(crate) pending: Option<LocalId>,
    /// The box had the keyboard at the last frame (the footer shows then).
    focused: bool,
    /// Setting the text programmatically (not an edit to keep).
    loading: bool,
    /// The text before the last edit (to see typed `[[` / `![[`).
    last: String,
    /// Where the bar was last drawn (the tour rings it and keeps its card
    /// clear of it).
    pub(crate) bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
}

impl Compose {
    pub fn new(data_dir: Option<PathBuf>) -> Compose {
        Compose {
            editor: None,
            assist: None,
            data_dir,
            busy: false,
            error: None,
            pending: None,
            focused: false,
            loading: false,
            last: String::new(),
            bounds: Default::default(),
        }
    }

    /// The box's text ("" before it's made).
    pub fn text(&self, cx: &App) -> String {
        self.editor
            .as_ref()
            .map(|e| e.read(cx).value().to_string())
            .unwrap_or_default()
    }

    /// The box has the keyboard.
    pub fn has_keyboard(&self, window: &Window, cx: &App) -> bool {
        self.editor
            .as_ref()
            .is_some_and(|e| e.read(cx).focus_handle(cx).is_focused(window))
    }
}

/// The kind the box posts as: a thread when it holds a `![[…]]` quote
/// (fragments can't transclude) or is too long for a fragment
/// (`blyg_core::promotion_kind`), else a fragment.
pub fn kind_for(text: &str) -> Kind {
    if blyg_core::grammar::source_ids(text).is_empty() {
        blyg_core::promotion_kind(Kind::Fragment, text)
    } else {
        Kind::Thread
    }
}

/// The footer's left side: the kind and the count.
pub fn counter(text: &str) -> (String, crate::vm::Level) {
    let chars = text.chars().count();
    if blyg_core::grammar::source_ids(text).is_empty() {
        let (c, level) = crate::vm::counter(Kind::Fragment, chars);
        if chars > blyg_core::FRAGMENT_LIMIT {
            (format!("posts as a thread · {c}"), crate::vm::Level::Warn)
        } else {
            (format!("fragment · {c}"), level)
        }
    } else {
        ("thread · it quotes a post".into(), crate::vm::Level::Normal)
    }
}

/// The text kept from last time (`state.json`'s `compose_draft`).
pub fn load_draft(data_dir: Option<&Path>) -> String {
    data_dir
        .map(blyg_core::state::AppState::load)
        .and_then(|s| s.compose_draft)
        .unwrap_or_default()
}

/// Keep the text (none when it's blank).
pub fn save_draft(data_dir: Option<&Path>, text: &str) {
    let Some(d) = data_dir else { return };
    let keep = (!text.trim().is_empty()).then(|| text.to_string());
    if blyg_core::state::AppState::load(d).compose_draft == keep {
        return;
    }
    let _ = blyg_core::state::AppState::update(d, |s| s.compose_draft = keep);
}

/// What the quote action would insert: the open post's `![[id]]` (with its
/// label, as the picker writes it), for a post from a blyg. Feed posts
/// can't be quoted that way.
fn quote_for(this: &MainView) -> Option<String> {
    let o = this.reading.opened.as_ref()?;
    let item = &o.item;
    let blyg = this
        .reading
        .subs
        .iter()
        .find(|s| s.id == item.subscription_id)
        .is_none_or(|s| s.kind == blyg_core::SubscriptionKind::Blyg);
    blyg.then(|| vm::quote_directive(item, this.backend.server_outdated()))
}

impl MainView {
    /// Hook: the compose box's actions on the root element.
    pub(super) fn compose_actions(
        &self,
        d: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        d.on_action(
            cx.listener(|this, _: &FocusCompose, window, cx| this.focus_compose(window, cx)),
        )
    }

    /// Hook (top of render): make the box once the reading screen shows,
    /// and keep its style with the theme.
    pub(crate) fn compose_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.reading.view != View::Reading {
            return;
        }
        self.ensure_compose(window, cx);
        let focused = self.reading.compose.has_keyboard(window, cx);
        self.reading.compose.focused = focused;
        let p = self.palette.on_page();
        let font = self.prefs.ui().family;
        if let Some(a) = &self.reading.compose.assist {
            a.update(cx, |a, _| {
                a.set_palette(p);
                a.set_ui_font(font);
            });
        }
        if let Some(e) = &self.reading.compose.editor {
            e.update(cx, |s, _| {
                s.set_editor_style(InputEditorStyle {
                    foreground: p.ink,
                    muted_foreground: p.placeholder(),
                    background: p.bg,
                    border: p.edge(),
                    selection: p.text_selection,
                    caret: p.caret(),
                    ..Default::default()
                });
                s.set_editor_paddings(Edges {
                    top: px(6.),
                    bottom: px(6.),
                    left: px(10.),
                    right: px(10.),
                });
            });
        }
    }

    /// Make the editor (once), with the text kept from last time.
    pub(crate) fn ensure_compose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.reading.compose.editor.is_some() {
            return;
        }
        let editor = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, MAX_ROWS)
                .soft_wrap(true)
                .placeholder(crate::keymap::hint(PLACEHOLDER))
        });
        let kept = load_draft(self.reading.compose.data_dir.as_deref());
        if !kept.is_empty() {
            editor.update(cx, |s, cx| s.set_value(kept.clone(), window, cx));
        }
        let assist = {
            let (e, backend) = (editor.clone(), self.backend.clone());
            let p = self.palette;
            cx.new(|cx| Assist::new(e, backend, p, window, cx))
        };
        self._subs.push(cx.subscribe_in(
            &editor,
            window,
            |this, _, ev: &InputEvent, window, cx| {
                if matches!(ev, InputEvent::Change) {
                    this.compose_changed(window, cx);
                }
            },
        ));
        let c = &mut self.reading.compose;
        c.last = kept;
        c.editor = Some(editor);
        c.assist = Some(assist);
    }

    /// Every edit: keep the text, clear an old error, and open the `[[` /
    /// `![[` picker when the brackets were just typed.
    fn compose_changed(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.reading.compose.loading {
            return;
        }
        let Some(editor) = self.reading.compose.editor.clone() else {
            return;
        };
        let (new, cursor) = {
            let s = editor.read(cx);
            (s.value().to_string(), s.cursor())
        };
        let old = std::mem::replace(&mut self.reading.compose.last, new.clone());
        let typed = super::quote_picker::transclusion_trigger(&old, &new, cursor)
            .map(|r| (PickKind::Quote, r.end - PickKind::Quote.open().len()))
            .or_else(|| {
                crate::composer::picker::link_trigger(&old, &new, cursor)
                    .map(|at| (PickKind::Link, at))
            });
        if let (Some((kind, at)), Some(assist)) = (typed, self.reading.compose.assist.clone()) {
            assist.update(cx, |a, cx| a.open_picker(kind, at, None, cx));
        }
        self.reading.compose.error = None;
        save_draft(self.reading.compose.data_dir.as_deref(), &new);
        cx.notify();
    }

    /// `c` / ⇧⌘K: the caret in the box, at the end (the reading screen
    /// first, from elsewhere).
    pub(crate) fn focus_compose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.reading.view != View::Reading {
            self.show_view(View::Reading, window, cx);
        }
        self.ensure_compose(window, cx);
        if let Some(editor) = self.reading.compose.editor.clone() {
            editor.update(cx, |s, cx| {
                s.focus(window, cx);
                let end = s.text().len();
                s.set_selected_range(end..end, cx);
            });
        }
        cx.notify();
    }

    /// esc: the keys back to the list; the text stays.
    fn compose_leave(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.reading.focus, cx);
        cx.notify();
    }

    /// Set the box's text (a clear, a quote); kept like an edit.
    fn compose_set_text(
        &mut self,
        text: &str,
        caret: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.reading.compose.editor.clone() else {
            return;
        };
        self.reading.compose.loading = true;
        editor.update(cx, |s, cx| {
            s.set_value(text.to_string(), window, cx);
            let c = caret.min(s.text().len());
            s.set_selected_range(c..c, cx);
        });
        self.reading.compose.loading = false;
        self.reading.compose.last = text.to_string();
        if let Some(a) = &self.reading.compose.assist {
            a.update(cx, |a, cx| a.reset(cx));
        }
        save_draft(self.reading.compose.data_dir.as_deref(), text);
        cx.notify();
    }

    /// The item the text goes into: the draft a failed post left (saved
    /// with the text as it is now), else a new scratch note (to post, as
    /// quick capture does) or a new draft.
    fn compose_item(&mut self, text: &str, draft: bool) -> blyg_core::Result<LocalId> {
        let kind = kind_for(text);
        let pending = self.reading.compose.pending.clone().and_then(|id| {
            self.backend
                .item(&id)
                .filter(|i| matches!(i.status, Status::Draft | Status::Scratch))
        });
        if let Some(item) = pending {
            self.backend.save(&item.local_id, text)?;
            if kind == Kind::Thread && item.kind == Kind::Fragment {
                self.backend.set_kind(&item.local_id, Kind::Thread)?;
            }
            return Ok(item.local_id);
        }
        self.reading.compose.pending = None;
        if draft {
            self.backend.create_draft(kind, text)
        } else {
            // A long note is promoted as a thread anyway; a quote makes one.
            self.backend.create_scratch(kind, text)
        }
    }

    /// The box's text, or a toast saying there's none.
    fn compose_text_or_say(&mut self, what: &str, cx: &mut Context<Self>) -> Option<String> {
        if self.reading.compose.busy {
            return None;
        }
        let text = self.reading.compose.text(cx).trim_end().to_string();
        if text.trim().is_empty() {
            self.show_toast(format!("Nothing to {what} yet"), None, cx);
            return None;
        }
        Some(text)
    }

    /// Post (⌘⏎): publish now, through quick capture's path (a scratch note,
    /// promoted with `Promote::Publish`). Cleared once it's published; on a
    /// failure the text stays with the error.
    pub(crate) fn compose_post(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.compose_text_or_say("post", cx) else {
            return;
        };
        let id = match self.compose_item(&text, false) {
            Ok(id) => id,
            Err(e) => {
                self.reading.compose.error = Some(format!("Couldn't keep it: {e}"));
                cx.notify();
                return;
            }
        };
        self.reading.compose.busy = true;
        self.reading.compose.error = None;
        cx.notify();
        let backend = self.backend.clone();
        let pid = id.clone();
        let task =
            cx.background_spawn(
                async move { backend.promote(&pid, Promote::Publish { note: None }) },
            );
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |v, window, cx| {
                v.reading.compose.busy = false;
                match result {
                    Ok(p) => {
                        v.reading.compose.pending = None;
                        let had_keys = v.reading.compose.has_keyboard(window, cx);
                        // What was typed while it was on its way stays.
                        if v.reading.compose.text(cx).trim_end() == text {
                            v.compose_set_text("", 0, window, cx);
                        }
                        if had_keys {
                            window.focus(&v.reading.focus, cx);
                        }
                        let what = match p.kind {
                            Kind::Thread => "Posted as a thread",
                            Kind::Fragment => "Posted",
                        };
                        let sub = p.published.map(|o| {
                            let link = o.permalink.trim_start_matches("https://");
                            SharedString::from(format!("v{} · {link}", o.version))
                        });
                        v.show_toast(what, sub, cx);
                        v.requery(window, cx);
                    }
                    Err(e) => {
                        v.reading.compose.pending = Some(id);
                        v.reading.compose.error =
                            Some(format!("Couldn't post: {e} · kept as a draft in Posts"));
                        v.requery(window, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Save as draft (⌘D): a draft on the blyg (synced, unpublished).
    pub(crate) fn compose_save_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.compose_text_or_say("save", cx) else {
            return;
        };
        match self.compose_item(&text, true) {
            Ok(_) => {
                self.reading.compose.pending = None;
                self.reading.compose.error = None;
                self.compose_set_text("", 0, window, cx);
                window.focus(&self.reading.focus, cx);
                self.show_toast("Saved as a draft", Some("It's in Posts".into()), cx);
                self.requery(window, cx);
            }
            Err(e) => {
                self.reading.compose.error = Some(format!("Couldn't save the draft: {e}"));
                cx.notify();
            }
        }
    }

    /// ⤢: the text into the full editor as a draft, for longer writing.
    pub(crate) fn compose_expand(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.compose_text_or_say("open in the editor", cx) else {
            return;
        };
        match self.compose_item(&text, true) {
            Ok(id) => {
                self.reading.compose.pending = None;
                self.reading.compose.error = None;
                self.compose_set_text("", 0, window, cx);
                self.open_new_draft(&id, window, cx);
                self.show_toast("In the editor, as a draft", None, cx);
            }
            Err(e) => {
                self.reading.compose.error = Some(format!("Couldn't make a draft: {e}"));
                cx.notify();
            }
        }
    }

    /// ❝ Quote: the open post's `![[id]]` on its own line at the caret.
    pub(crate) fn compose_quote_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(directive) = quote_for(self) else {
            return;
        };
        self.ensure_compose(window, cx);
        let Some(editor) = self.reading.compose.editor.clone() else {
            return;
        };
        let (text, cursor) = {
            let s = editor.read(cx);
            (s.value().to_string(), s.cursor())
        };
        let cursor = if self.reading.compose.has_keyboard(window, cx) {
            cursor
        } else {
            text.len()
        };
        let (new, caret) = vm::insert_directive(&text, cursor, &directive);
        self.compose_set_text(&new, caret, window, cx);
        editor.update(cx, |s, cx| s.focus(window, cx));
    }

    /// Give a popup key to the mention popup, the picker or the spelling
    /// menu if one is up; `true` when it took the key.
    fn compose_route_key(
        &mut self,
        key: AssistKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(assist) = self.reading.compose.assist.clone() else {
            return false;
        };
        let took = assist.read(cx).wants_keys()
            && assist.update(cx, |a, cx| a.handle_key(key, window, cx));
        if took {
            cx.stop_propagation();
        }
        took
    }

    // ------------------------------------------------------------ render

    /// The bar along the bottom of the reading screen.
    pub(super) fn render_compose_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let c = &self.reading.compose;
        let editor = c.editor.clone()?;
        let assist = c.assist.clone()?;
        let p = self.palette.on_page();
        let theme = &*self.theme;
        let text = editor.read(cx).value().to_string();
        let empty = text.trim().is_empty();
        let quote = quote_for(self);
        let show_footer = c.focused || !empty || c.error.is_some() || c.busy;
        let chrome = self.chrome();
        let button = |id: &'static str, label: &'static str, tip: String, on: bool, off: bool| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .flex_none()
                .h(px(26.))
                .px(px(9.))
                .flex()
                .items_center()
                .map(|d| crate::theme_ext::chip(d, theme, on && !off))
                .font_family(chrome)
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .when(off, |d| d.text_color(p.muted).border_dashed())
                .when(!off, |d| d.cursor_pointer().hover(|s| s.bg(p.hover())))
                .when(!off && !on, |d| d.text_color(p.ink))
                .tooltip(move |_, cx| cx.new(|_| super::Tip(tip.clone())).into())
                .child(label)
        };
        let off = empty || c.busy;
        let post_label = if c.busy { "Posting…" } else { "Post" };
        let buttons = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                button(
                    "compose-expand",
                    "⤢",
                    crate::keymap::hint_owned(
                        "Open in the editor, as a draft, for longer writing".into(),
                    ),
                    false,
                    off,
                )
                .when(!off, |d| {
                    d.on_click(cx.listener(|this, _, window, cx| this.compose_expand(window, cx)))
                }),
            )
            .child(
                button(
                    "compose-draft",
                    "Draft",
                    crate::keymap::hint_owned("Save as a draft on your blyg  ⌘D".into()),
                    false,
                    off,
                )
                .when(!off, |d| {
                    d.on_click(
                        cx.listener(|this, _, window, cx| this.compose_save_draft(window, cx)),
                    )
                }),
            )
            .child(
                button(
                    "compose-post",
                    post_label,
                    crate::keymap::hint_owned("Publish now  ⌘⏎".into()),
                    true,
                    off,
                )
                .when(!off, |d| {
                    d.on_click(cx.listener(|this, _, window, cx| this.compose_post(window, cx)))
                }),
            );
        let field = div()
            .id("compose-field")
            .flex_1()
            .min_w_0()
            .relative()
            .map(|d| crate::theme_ext::field_box(d, theme))
            .when(c.focused, |d| d.border_color(p.accent))
            .font_family(SharedString::from(self.prefs.writing().family))
            .text_size(px((self.prefs.font_size - 2.).max(13.)))
            .line_height(relative(1.45))
            .child(Textarea::new(&editor))
            .child(assist);
        let footer = show_footer.then(|| {
            let (count, level) = counter(&text);
            let left = match &c.error {
                Some(e) => div()
                    .id("compose-error")
                    .debug_selector(|| "compose-error".into())
                    .text_color(p.over_text())
                    .child(e.clone()),
                None => div()
                    .id("compose-count")
                    .when(level == crate::vm::Level::Warn, |d| {
                        d.text_color(p.warn_text())
                    })
                    .when(level == crate::vm::Level::Over, |d| {
                        d.text_color(p.over_text())
                    })
                    .child(count),
            };
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .px(px(2.))
                .font_family(chrome)
                .text_size(px(11.))
                .text_color(p.muted)
                .child(left.flex_1().min_w_0().truncate())
                .child(crate::keymap::hint(
                    "⌘⏎ post · ⌘D draft · esc back to reading",
                ))
        });
        // ❝ Quote: the open post into the box (a blyg post only).
        let quote_chip = quote.map(|_| {
            button(
                "compose-quote",
                "❝ Quote",
                "Quote the open post here (![[…]]; the post becomes a thread)".into(),
                false,
                c.busy,
            )
            .when(!c.busy, |d| {
                d.on_click(cx.listener(|this, _, window, cx| this.compose_quote_open(window, cx)))
            })
        });
        let at = c.bounds.clone();
        // Records where the bar is drawn (for the tour's ring).
        let marker = canvas(move |b, _, _| at.set(Some(b)), |_, _, _, _| {})
            .absolute()
            .inset_0();
        Some(
            div()
                .id("reading-compose")
                .debug_selector(|| "reading-compose".into())
                .relative()
                .child(marker)
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(4.))
                .px(px(14.))
                .pt(px(8.))
                .pb(px(if show_footer { 6. } else { 8. }))
                .rule_t(&p)
                .bg(p.bg)
                // The mention popup / picker / spelling menu keys first.
                .capture_action(cx.listener(|this, _: &MoveUp, window, cx| {
                    this.compose_route_key(AssistKey::Up, window, cx);
                }))
                .capture_action(cx.listener(|this, _: &MoveDown, window, cx| {
                    this.compose_route_key(AssistKey::Down, window, cx);
                }))
                .capture_action(cx.listener(|this, _: &IndentInline, window, cx| {
                    this.compose_route_key(AssistKey::Tab, window, cx);
                }))
                .capture_action(cx.listener(|this, _: &Paste, _, cx| {
                    if let Some(a) = &this.reading.compose.assist {
                        a.update(cx, |a, _| a.note_paste());
                    }
                }))
                .capture_action(cx.listener(|this, a: &Enter, window, cx| {
                    if a.secondary {
                        cx.stop_propagation();
                        this.compose_post(window, cx);
                    } else {
                        this.compose_route_key(AssistKey::Enter, window, cx);
                    }
                }))
                .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                    if this.compose_route_key(AssistKey::Escape, window, cx) {
                        return;
                    }
                    cx.stop_propagation();
                    this.compose_leave(window, cx);
                }))
                // ⌘⏎ and ⌘D post and save the box, not the Posts selection.
                .on_action(
                    cx.listener(|this, _: &Publish, window, cx| this.compose_post(window, cx)),
                )
                .on_action(cx.listener(|this, _: &MakeDraft, window, cx| {
                    this.compose_save_draft(window, cx)
                }))
                .child(
                    div()
                        .flex()
                        .items_end()
                        .gap(px(8.))
                        .child(field)
                        .children(quote_chip)
                        .child(buttons),
                )
                .children(footer)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{counter, kind_for, load_draft, save_draft};
    use blyg_core::Kind;

    #[test]
    fn a_quote_or_a_long_text_posts_as_a_thread() {
        assert_eq!(kind_for("a quick thought"), Kind::Fragment);
        assert_eq!(
            kind_for("see\n![[01k2rve0trvst0000000000001]]"),
            Kind::Thread
        );
        assert_eq!(
            kind_for("a link [[01k2rve0trvst0000000000001]] stays one"),
            Kind::Fragment
        );
        assert_eq!(kind_for(&"x".repeat(1001)), Kind::Thread);
        assert_eq!(counter("hello").0, "fragment · 5 / 1000");
        assert!(
            counter(&"y".repeat(1200))
                .0
                .starts_with("posts as a thread")
        );
        assert_eq!(
            counter("![[01k2rve0trvst0000000000001]]").0,
            "thread · it quotes a post"
        );
    }

    #[test]
    fn the_draft_in_progress_is_kept_in_state_json() {
        let dir = tempfile::tempdir().unwrap();
        let d = Some(dir.path());
        assert_eq!(load_draft(d), "");
        save_draft(d, "half a thought");
        assert_eq!(load_draft(d), "half a thought");
        // Other state survives.
        let _ = blyg_core::state::AppState::update(dir.path(), |s| s.onboarded = true);
        save_draft(d, "half a thought, more");
        let s = blyg_core::state::AppState::load(dir.path());
        assert!(s.onboarded);
        assert_eq!(s.compose_draft.as_deref(), Some("half a thought, more"));
        // Blank is nothing kept.
        save_draft(d, "  \n");
        assert_eq!(
            blyg_core::state::AppState::load(dir.path()).compose_draft,
            None
        );
        assert_eq!(load_draft(None), "");
    }
}
