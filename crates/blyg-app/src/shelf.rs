//! --- shelf --- your own posts kept to hand: the living threads you keep
//! updating (a /now page, a reading log), at the top of the Write list in
//! your order, and one key away.
//!
//! - ⌥⌘B (Post › Put on Shelf / Take off Shelf) shelves the open post or
//!   takes it off. Any post of yours: thread, fragment, draft or scratch note.
//! - ⌘B (Post › Shelf…) opens the shelf: 1–9 or ⏎ opens a post, ↑/↓ move,
//!   ⌥↑/⌥↓ reorder, ⌫ takes the highlighted post off, esc closes.
//! - Shelved posts that match the omnibar come first in the list, each with
//!   📚 before its title (`vm::ListModel`).
//!
//! Local to this Mac for now (`Backend::shelf`, store schema v17);
//! blygger-studio#135 adds a synced `/shelf`. Not the version pin (📌 in
//! ⌘Y), which is public and permanent.
//!
//! A child module of `app` (it reads `MainView`); `app.rs` keeps small hooks
//! marked `// --- shelf ---`.

use super::*;

gpui_kit::actions!(blygger, [ShowShelf, ToggleShelf]);

/// The glyph a shelved post shows in the list and on the shelf.
pub const GLYPH: &str = "📚";

/// How many shelved posts have a number key on the shelf.
pub const NUMBERED: usize = 9;

/// The shelf sheet.
pub struct ShelfSheet {
    pub focus: FocusHandle,
    /// The posts, in shelf order, as they were when it opened (or last
    /// changed here).
    pub posts: Vec<Item>,
    /// The highlighted row (↑/↓, ⏎).
    pub sel: usize,
}

/// The toast after ⌥⌘B.
pub fn toggled_toast(on: bool) -> &'static str {
    if on {
        "On your shelf · ⌘B"
    } else {
        "Taken off your shelf"
    }
}

/// The shelf's rows: each shelved post this backend still holds.
pub fn posts(backend: &dyn Backend) -> Vec<Item> {
    backend
        .shelf()
        .iter()
        .filter_map(|id| backend.item(id))
        .collect()
}

impl MainView {
    /// Hook: the shelf's actions on the main window.
    pub(super) fn shelf_actions(&self, d: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        d.on_action(
            cx.listener(|this, _: &ShowShelf, window, cx| this.toggle_shelf_sheet(window, cx)),
        )
        .on_action(
            cx.listener(|this, _: &ToggleShelf, window, cx| this.toggle_on_shelf(window, cx)),
        )
    }

    /// Hook: the list's order follows the shelf.
    pub(super) fn reload_shelf(&mut self) {
        self.list.set_shelf(self.backend.shelf());
    }

    /// ⌘B: open the shelf, or close it.
    pub(crate) fn toggle_shelf_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.sheet, Some(Sheet::Shelf(_))) {
            return self.close_sheet(window, cx);
        }
        let posts = posts(&*self.backend);
        let cur = self.current.as_ref().map(|c| c.local_id.clone());
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.sheet_gen += 1;
        self.sheet = Some(Sheet::Shelf(ShelfSheet {
            sel: posts
                .iter()
                .position(|p| Some(&p.local_id) == cur.as_ref())
                .unwrap_or(0),
            focus,
            posts,
        }));
        cx.notify();
    }

    /// ⌥⌘B: shelve the open post, or take it off.
    pub(crate) fn toggle_on_shelf(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.current.as_ref().map(|c| c.local_id.clone()) else {
            return self.show_toast("Open a post to put it on your shelf", None, cx);
        };
        let on = !self.list.is_on_shelf(&id);
        self.set_on_shelf(&id, on, window, cx);
    }

    fn set_on_shelf(
        &mut self,
        id: &LocalId,
        on: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(e) = self.backend.set_on_shelf(id, on) {
            return self.show_toast(format!("Couldn't change your shelf: {e}"), None, cx);
        }
        self.after_shelf_change(window, cx);
        self.show_toast(toggled_toast(on), None, cx);
    }

    /// The shelf changed: the list re-queries (a post taken off goes back
    /// to its place; the selection stays in view) and an open shelf sheet
    /// re-reads it.
    fn after_shelf_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.requery(window, cx);
        if let Some(ix) = self.list.selected_index() {
            self.list_scroll.scroll_to_item(ix, ScrollStrategy::Nearest);
        }
        let fresh = posts(&*self.backend);
        if let Some(Sheet::Shelf(s)) = &mut self.sheet {
            s.posts = fresh;
            s.sel = s.sel.min(s.posts.len().saturating_sub(1));
        }
        cx.notify();
    }

    /// Open a shelved post. If the omnibar's search hides it, the search is
    /// cleared so the post shows in the list.
    fn open_from_shelf(&mut self, id: LocalId, window: &mut Window, cx: &mut Context<Self>) {
        self.sheet = None;
        if !self.list.results().iter().any(|i| i.local_id == id) {
            self.set_query_text("", window, cx);
        }
        self.open(&id, window, cx);
        if let Some(ix) = self.list.selected_index() {
            self.list_scroll.scroll_to_item(ix, ScrollStrategy::Nearest);
        }
    }

    fn shelf_sheet_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Sheet::Shelf(s)) = &mut self.sheet else {
            return;
        };
        let key = ev.keystroke.key.as_str();
        let alt = ev.keystroke.modifiers.alt;
        let n = s.posts.len();
        let picked = s.posts.get(s.sel).map(|p| p.local_id.clone());
        match key {
            "escape" => self.close_sheet(window, cx),
            "up" | "down" if alt => {
                let Some(id) = picked else { return };
                let to = if key == "up" {
                    s.sel.saturating_sub(1)
                } else {
                    (s.sel + 1).min(n.saturating_sub(1))
                };
                if to == s.sel {
                    return cx.stop_propagation();
                }
                s.sel = to;
                if let Err(e) = self.backend.move_on_shelf(&id, to) {
                    self.show_toast(format!("Couldn't move it: {e}"), None, cx);
                }
                self.after_shelf_change(window, cx);
            }
            "up" => {
                s.sel = s.sel.saturating_sub(1);
                cx.notify();
            }
            "down" => {
                s.sel = (s.sel + 1).min(n.saturating_sub(1));
                cx.notify();
            }
            "enter" => {
                if let Some(id) = picked {
                    self.open_from_shelf(id, window, cx);
                }
            }
            "backspace" | "delete" => {
                if let Some(id) = picked {
                    self.set_on_shelf(&id, false, window, cx);
                }
            }
            k if k.len() == 1 && ("1"..="9").contains(&k) => {
                let i = k.parse::<usize>().unwrap_or(1) - 1;
                if let Some(id) = s.posts.get(i).map(|p| p.local_id.clone()) {
                    self.open_from_shelf(id, window, cx);
                }
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    pub(super) fn render_shelf_sheet(
        &self,
        s: &ShelfSheet,
        cx: &mut Context<Self>,
    ) -> (f32, AnyElement) {
        let p = self.palette;
        let theme = &*self.theme;
        let kbd = |k: String| {
            crate::theme_ext::kbd(div(), theme)
                .px(px(5.))
                .py(px(1.))
                .min_w(px(18.))
                .flex()
                .justify_center()
                .text_size(px(10.5))
                .child(k)
        };
        let mut body = div()
            .track_focus(&s.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                this.shelf_sheet_key(ev, window, cx)
            }))
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .mb(px(6.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(13.))
                            .child(format!("{GLYPH} Shelf")),
                    )
                    .child(div().text_size(px(11.)).text_color(p.muted).child(
                        match s.posts.len() {
                            0 => String::new(),
                            1 => "1 post".to_string(),
                            n => format!("{n} posts"),
                        },
                    )),
            );
        if s.posts.is_empty() {
            body = body.child(
                div()
                    .py(px(6.))
                    .text_color(p.muted)
                    .line_height(relative(1.45))
                    .child(crate::keymap::hint(
                        "Nothing on your shelf yet. Keep the posts you keep updating here: \
                         open one and press ⌥⌘B.",
                    )),
            );
        }
        for (i, item) in s.posts.iter().enumerate() {
            let selected = i == s.sel;
            let id = item.local_id.clone();
            body = body.child(
                div()
                    .id(SharedString::from(format!("shelf-row-{i}")))
                    .debug_selector(move || format!("shelf-row-{i}"))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(8.))
                    .py(px(6.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .when(selected, |d| d.bg(p.sel))
                    .hover(|d| d.bg(p.sel))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .truncate()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(if selected { p.sel_ink } else { p.ink })
                                    .child(vm::item_title(item)),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(11.))
                                    .text_color(if selected { p.sel_muted } else { p.muted })
                                    .child(format!(
                                        "{} · {} · {}",
                                        vm::kind_label(item.kind),
                                        vm::pill(item).0,
                                        vm::relative_time(&item.updated, self.now)
                                    )),
                            ),
                    )
                    .when(i < NUMBERED, |d| d.child(kbd((i + 1).to_string())))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_from_shelf(id.clone(), window, cx)
                    })),
            );
        }
        let hints = if s.posts.is_empty() {
            "⌥⌘B shelves the open post · esc closes"
        } else {
            "1–9 or ⏎ opens · ⌥↑/⌥↓ reorder · ⌫ takes off · ⌥⌘B shelves the open post · esc"
        };
        body = body.child(div().my(px(6.)).h(px(1.)).bg(p.line)).child(
            div()
                .text_size(px(11.))
                .text_color(p.muted)
                .child(crate::keymap::hint(hints)),
        );
        (440., body.into_any_element())
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: the app's glob imports bring in GPUI's own `test`.
    use super::toggled_toast;

    #[test]
    fn the_toast_says_where_it_went() {
        assert_eq!(toggled_toast(true), "On your shelf · ⌘B");
        assert_eq!(toggled_toast(false), "Taken off your shelf");
    }
}

#[cfg(test)]
#[path = "shelf_tests.rs"]
mod ui_tests;
