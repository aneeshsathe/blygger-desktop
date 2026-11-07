//! --- split --- (proposal 15) two posts side by side in the editor
//! column, for the writer who thinks in pairs: a reply and its source, two
//! drafts that should be one, a fork and its origin.
//!
//! - ⌥⌘⏎ (Post › Open Beside) on a list row puts that post on the right.
//!   On Posts it's the selected row; as moving the selection opens a post
//!   on the left, the left then goes back to the post you had open before
//!   it. On the reading screen it's the open (else selected) post, and the
//!   window goes to Posts with it beside the post you were writing.
//! - Your own post on the right is a second editor with its own autosave
//!   and status line. Anyone else's is read-only, drawn with the stream's
//!   native blocks (no WebView), and its header can be dragged into the
//!   left post as a quote (`tray.rs`).
//! - ⌥⌘[ / ⌥⌘] move the keyboard to the left / right post (⌘[ / ⌘] are the
//!   editor's outdent / indent and the reader's Back). The divider drags;
//!   its place is kept in `state.json`. × (or ⌥⌘⏎ on the same row) closes
//!   the right side. Opening the right post on the left closes it too, so
//!   one post is never in two editors.
//! - "Join into left" from the mock is left out for now: deleting the right
//!   draft after merging is irreversible, and the published case needs
//!   care (rule 5).
//!
//! A child module of `app`; hooks marked `// --- split ---`.

use std::path::PathBuf;

use blyg_core::ReadingItem;

use super::reading::{View, vm as rvm};
use super::*;

gpui_kit::actions!(blygger, [OpenBeside, FocusLeftPost, FocusRightPost]);

/// The left post's share of the column: at least this, at most 1 − this.
pub const MIN_SHARE: f32 = 0.25;

/// What the right side shows.
pub enum Beside {
    /// Your own post, in a second editor.
    Own {
        id: LocalId,
        editor: Entity<TextareaState>,
        item: Box<Item>,
        error: Option<String>,
    },
    /// Someone else's post, read-only.
    Held {
        row: Box<ReadingItem>,
        focus: FocusHandle,
    },
}

impl Beside {
    pub fn own_id(&self) -> Option<&LocalId> {
        match self {
            Beside::Own { id, .. } => Some(id),
            Beside::Held { .. } => None,
        }
    }
}

/// Split view's state (a field of `MainView`).
pub struct Split {
    pub beside: Option<Beside>,
    /// The left post's share of the column.
    pub left: f32,
    data_dir: Option<PathBuf>,
    /// The last two posts open on the left, oldest first.
    pub history: Vec<LocalId>,
    /// Set while loading text into the right editor (not an edit to save).
    loading: bool,
}

impl Split {
    pub fn new(data_dir: Option<PathBuf>) -> Split {
        let left = data_dir
            .as_deref()
            .map(blyg_core::state::AppState::load)
            .and_then(|s| s.split_left)
            .map(|v| (v as f32 / 1000.).clamp(MIN_SHARE, 1. - MIN_SHARE))
            .unwrap_or(0.5);
        Split {
            beside: None,
            left,
            data_dir,
            history: Vec::new(),
            loading: false,
        }
    }

    pub fn is_open(&self) -> bool {
        self.beside.is_some()
    }

    fn save_width(&self) {
        let Some(d) = &self.data_dir else { return };
        let v = (self.left * 1000.).round() as u32;
        let _ = blyg_core::state::AppState::update(d, |s| s.split_left = Some(v));
    }

    /// Note the post open on the left.
    pub fn saw(&mut self, id: &LocalId) {
        if self.history.last() != Some(id) {
            self.history.retain(|h| h != id);
            self.history.push(id.clone());
            if self.history.len() > 2 {
                self.history.remove(0);
            }
        }
    }

    /// The post open before `id` on the left, if any.
    pub fn before(&self, id: &LocalId) -> Option<LocalId> {
        self.history.iter().rev().find(|h| *h != id).cloned()
    }
}

/// The divider, being dragged.
#[derive(Debug, Clone)]
struct DraggedDivider;

impl Render for DraggedDivider {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// The right editor's status line: kind, count, version, saved.
pub fn own_status(item: &Item, chars: usize, error: Option<&str>) -> String {
    let (count, _) = crate::vm::counter(item.kind, chars);
    let saved = error.map_or_else(|| "saved".to_string(), |e| format!("not saved: {e}"));
    format!(
        "{} · {count} · {} · {saved}",
        crate::vm::kind_label(item.kind),
        crate::vm::version_label(item)
    )
}

impl MainView {
    /// Hook: split view's actions on the main window.
    pub(super) fn split_actions(&self, d: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        d.on_action(cx.listener(|this, _: &OpenBeside, window, cx| this.open_beside(window, cx)))
            .on_action(cx.listener(|this, _: &FocusLeftPost, window, cx| {
                this.focus_split_side(false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &FocusRightPost, window, cx| {
                this.focus_split_side(true, window, cx)
            }))
    }

    /// Hook (top of render): note the left post; one post never sits in
    /// both editors; the right editor wears the theme.
    pub(super) fn split_frame(&mut self, cx: &mut Context<Self>) {
        let Some(cur) = self.current.as_ref().map(|c| c.local_id.clone()) else {
            return;
        };
        self.split.saw(&cur);
        if self.split.beside.as_ref().and_then(Beside::own_id) == Some(&cur) {
            self.split.beside = None;
        }
        if let Some(Beside::Own { editor, .. }) = &self.split.beside {
            let p = self.palette;
            editor.update(cx, |s, _| {
                s.set_editor_style(gpui_kit::base::input::InputEditorStyle {
                    foreground: p.ink,
                    muted_foreground: p.muted,
                    background: p.editor,
                    border: p.line,
                    selection: p.text_selection,
                    caret: p.accent,
                    ..Default::default()
                });
                s.set_editor_paddings(Edges {
                    top: px(26.),
                    bottom: px(40.),
                    left: px(28.),
                    right: px(28.),
                });
            });
        }
    }

    /// Hook: the width the left editor lays its text out in.
    pub(super) fn split_pane_w(&self, pane_w: Pixels) -> Pixels {
        if self.split.is_open() {
            (pane_w * self.split.left).floor()
        } else {
            pane_w
        }
    }

    /// ⌥⌘⏎: the row in front of you beside the post you're writing.
    pub(crate) fn open_beside(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.reading.view == View::Reading {
            let row = self
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
                });
            let Some(row) = row else {
                return self.show_toast("Select a post to open it beside", None, cx);
            };
            self.show_view(View::Posts, window, cx);
            return self.beside_row(row, window, cx);
        }
        if self.reading.view != View::Posts {
            return;
        }
        let Some(target) = self.list.selected().cloned() else {
            return self.show_toast("Select a post in the list to open it beside", None, cx);
        };
        if self.split.beside.as_ref().and_then(Beside::own_id) == Some(&target) {
            return self.close_split(window, cx);
        }
        let cur = self.current.as_ref().map(|c| c.local_id.clone());
        if cur.as_ref() == Some(&target) {
            let Some(prev) = self.split.before(&target) else {
                return self.show_toast(
                    "Open the post to write beside it first",
                    Some(crate::keymap::hint("then select this one and press ⌥⌘⏎").into()),
                    cx,
                );
            };
            self.beside_own(&target, window, cx);
            self.open(&prev, window, cx);
        } else {
            self.beside_own(&target, window, cx);
        }
        cx.notify();
    }

    /// A reading row on the right: your own post in an editor, anyone
    /// else's read-only.
    fn beside_row(&mut self, row: ReadingItem, window: &mut Window, cx: &mut Context<Self>) {
        if row.subscription_id == blyg_core::OWN_SUBSCRIPTION {
            let own = self
                .backend
                .items()
                .into_iter()
                .find(|i| i.server_id.as_ref().is_some_and(|s| s.0 == row.remote_id));
            if let Some(i) = own {
                if self.current.as_ref().map(|c| &c.local_id) == Some(&i.local_id) {
                    return self.show_toast("That post is the one open on the left", None, cx);
                }
                return self.beside_own(&i.local_id, window, cx);
            }
        }
        if row.state == "tombstone" && row.pinned_version_retained.is_none() {
            return self.show_toast("That post was withdrawn", None, cx);
        }
        self.split.beside = Some(Beside::Held {
            row: Box::new(row),
            focus: cx.focus_handle(),
        });
        cx.notify();
    }

    /// Your post `id` in the right editor.
    fn beside_own(&mut self, id: &LocalId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.backend.item(id) else {
            return;
        };
        let editor = cx.new(|cx| TextareaState::new(window, cx).soft_wrap(true));
        self.split.loading = true;
        let text = item.content_md.clone();
        editor.update(cx, |s, cx| {
            s.set_value(text, window, cx);
            let end = s.text().len();
            s.set_selected_range(end..end, cx);
        });
        self.split.loading = false;
        let sub = cx.subscribe_in(&editor, window, |this, _, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Change) {
                this.split_saved(cx);
            }
        });
        self._subs.push(sub);
        self.split.beside = Some(Beside::Own {
            id: id.clone(),
            editor,
            item: Box::new(item),
            error: None,
        });
        cx.notify();
    }

    /// The right editor changed: save it (its own autosave; the sync
    /// pushes it like any edit).
    pub(crate) fn split_saved(&mut self, cx: &mut Context<Self>) {
        if self.split.loading {
            return;
        }
        let Some(Beside::Own {
            id, editor, error, ..
        }) = &mut self.split.beside
        else {
            return;
        };
        let text = editor.read(cx).value().to_string();
        let id = id.clone();
        *error = self.backend.save(&id, &text).err().map(|e| e.to_string());
        if let Some(fresh) = self.backend.item(&id) {
            self.list.update_item(fresh.clone());
            if let Some(Beside::Own { item, .. }) = &mut self.split.beside {
                **item = fresh;
            }
        }
        cx.notify();
    }

    pub(crate) fn close_split(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let right_focused = match &self.split.beside {
            Some(Beside::Own { editor, .. }) => editor.read(cx).focus_handle(cx).is_focused(window),
            Some(Beside::Held { focus, .. }) => focus.is_focused(window),
            None => false,
        };
        self.split.beside = None;
        if right_focused {
            self.focus_split_side(false, window, cx);
        }
        cx.notify();
    }

    /// ⌥⌘[ / ⌥⌘]: the keyboard to the left or the right post.
    pub(crate) fn focus_split_side(
        &mut self,
        right: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !right {
            if self.current.is_some() {
                self.mode = Mode::Edit;
                self.editor.update(cx, |s, cx| s.focus(window, cx));
            }
            return cx.notify();
        }
        match &self.split.beside {
            Some(Beside::Own { editor, .. }) => editor.update(cx, |s, cx| s.focus(window, cx)),
            Some(Beside::Held { focus, .. }) => window.focus(focus, cx),
            None => self.show_toast(
                "Nothing beside this post",
                Some(crate::keymap::hint("⌥⌘⏎ on a list row opens it beside").into()),
                cx,
            ),
        }
        cx.notify();
    }

    /// Hook: the editor column, with the right post beside the left one
    /// while split view is open.
    pub(super) fn split_wrap(&self, left: AnyElement, cx: &mut Context<Self>) -> AnyElement {
        let Some(beside) = &self.split.beside else {
            return left;
        };
        let p = self.palette;
        let share = self.split.left;
        let right = match beside {
            Beside::Own {
                editor,
                item,
                error,
                ..
            } => {
                let chars = editor.read(cx).value().chars().count();
                let status = own_status(item, chars, error.as_deref());
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .bg(p.editor)
                    .child(self.split_header(crate::vm::item_title(item), None, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .font_family(SharedString::from(self.prefs.writing().family))
                            .text_size(px(self.prefs.font_size))
                            .line_height(relative(1.6))
                            .child(gpui_kit::base::input::Textarea::new(editor)),
                    )
                    .child(
                        div()
                            .id("split-status")
                            .flex_none()
                            .px(px(12.))
                            .py(px(3.))
                            .rule_t(&p)
                            .font_family(self.chrome())
                            .text_size(px(11.))
                            .text_color(if error.is_some() { p.over } else { p.muted })
                            .child(status),
                    )
                    .into_any_element()
            }
            Beside::Held { row, focus } => {
                let blocks = self.reading.stream.blocks_of(row);
                let mut n = 0usize;
                let body = self.render_blocks(&blocks, row, &mut n, cx);
                let who = row
                    .author
                    .as_ref()
                    .and_then(|a| a.name.clone())
                    .unwrap_or_else(|| row.subscription_title.clone());
                let drag = super::tray::DraggedQuote::row(row, rvm::post_title(row));
                div()
                    .id("split-held")
                    .track_focus(focus)
                    .size_full()
                    .flex()
                    .flex_col()
                    .bg(p.editor)
                    .child(self.split_header(
                        format!("{who} · {} · read-only", rvm::host(&row.origin)),
                        Some(drag),
                        cx,
                    ))
                    .child(
                        div()
                            .id("split-held-body")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .px(px(24.))
                            .py(px(18.))
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .font_family(SharedString::from(self.prefs.writing().family))
                            .text_size(px((self.prefs.font_size * 16.0 / 19.0).round()))
                            .line_height(relative(1.5))
                            .text_color(p.ink)
                            .children(body),
                    )
                    .into_any_element()
            }
        };
        div()
            .id("split-view")
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_row()
            .on_drag_move::<DraggedDivider>(cx.listener(
                |this, e: &DragMoveEvent<DraggedDivider>, _, cx| {
                    let w = e.bounds.size.width.as_f32().max(1.);
                    let x = (e.event.position.x - e.bounds.left()).as_f32();
                    this.split.left = (x / w).clamp(MIN_SHARE, 1. - MIN_SHARE);
                    cx.notify();
                },
            ))
            .on_drop(cx.listener(|this, _: &DraggedDivider, _, _| this.split.save_width()))
            .child(
                div()
                    .flex_none()
                    .w(relative(share))
                    .h_full()
                    .flex()
                    .child(left),
            )
            .child(
                div()
                    .id("split-divider")
                    .flex_none()
                    .w(px(5.))
                    .h_full()
                    .cursor_col_resize()
                    .bg(p.line)
                    // --- side --- a double-click centres it.
                    .on_click(cx.listener(|this, e: &ClickEvent, _, cx| {
                        if e.click_count() == 2 {
                            this.split.left = 0.5;
                            this.split.save_width();
                            cx.notify();
                        }
                    }))
                    .on_drag(DraggedDivider, |_, _, _, cx| cx.new(|_| DraggedDivider)),
            )
            .child(div().flex_1().min_w_0().h_full().child(right))
            .into_any_element()
    }

    fn split_header(
        &self,
        title: String,
        drag: Option<super::tray::DraggedQuote>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let head = div()
            .id("split-header")
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(12.))
            .py(px(4.))
            .rule_b(&p)
            .font_family(self.chrome())
            .text_size(px(11.5))
            .text_color(p.muted)
            .child(div().flex_1().min_w_0().truncate().child(title))
            .child(
                div()
                    .flex_none()
                    .child(crate::keymap::hint("⌥⌘[ ⌥⌘] switch sides")),
            )
            .child(
                div()
                    .id("split-close")
                    .flex_none()
                    .cursor_pointer()
                    .hover(|h| h.text_color(p.ink))
                    .child("×")
                    .on_click(cx.listener(|this, _, window, cx| this.close_split(window, cx))),
            );
        match drag {
            Some(q) => super::tray::draggable(head.cursor_grab(), q).into_any_element(),
            None => head.into_any_element(),
        }
    }
}
