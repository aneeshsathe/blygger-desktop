//! The quote picker (⌘K): inserts `![[id]]` on its own line in a thread.
//! It offers only what's already held (your own posts plus imported posts
//! from blyg subscriptions) and never fetches anything by URL.

use gpui_kit::base::input::{Escape, InputEvent, InputState, MoveDown, MoveUp};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::vm::{self, Quotable};
use super::{RSheet, View};
use crate::app::MainView;

impl MainView {
    pub(super) fn open_quote_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.reading.sheet, Some(RSheet::Quote { .. })) {
            self.close_reading_sheet(window, cx);
            return;
        }
        if self.reading.view != View::Posts || self.reading.own.is_some() {
            self.show_toast("Open a thread to quote into it", None, cx);
            return;
        }
        let Some(item) = self.current.clone() else {
            self.show_toast("Open a thread to quote into it", None, cx);
            return;
        };
        if !vm::can_quote_into(Some(&item)) {
            self.show_toast(
                "Quotes go in threads",
                Some("⌘T makes this a thread".into()),
                cx,
            );
            return;
        }
        let input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Quote… (your posts and your reading)")
        });
        let sub = cx.subscribe_in(&input, window, |this, _, ev, window, cx| match ev {
            InputEvent::PressEnter { .. } => this.pick_quote(window, cx),
            InputEvent::Change => {
                if let Some(RSheet::Quote { sel, .. }) = this.reading.sheet.as_mut() {
                    *sel = 0;
                }
                cx.notify();
            }
            _ => {}
        });
        self._subs.push(sub);
        input.update(cx, |s, cx| s.focus(window, cx));
        self.open_reading_sheet(
            RSheet::Quote {
                target: item.local_id,
                input,
                sel: 0,
            },
            cx,
        );
    }

    /// What the picker offers right now.
    pub(crate) fn quote_candidates(&self, cx: &App) -> Vec<Quotable> {
        let Some(RSheet::Quote { target, input, .. }) = self.reading.sheet.as_ref() else {
            return vec![];
        };
        let exclude = self
            .backend
            .item(target)
            .and_then(|i| i.server_id)
            .map(|s| s.0);
        vm::quotables(
            &self.backend.items(),
            &self.backend.reading(),
            &self.backend.subscriptions(),
            exclude.as_deref(),
            &input.read(cx).value(),
        )
    }

    fn move_quote(&mut self, delta: isize, cx: &mut Context<Self>) {
        let n = self.quote_candidates(cx).len();
        if let Some(RSheet::Quote { sel, .. }) = self.reading.sheet.as_mut()
            && n > 0
        {
            *sel = (*sel as isize + delta).clamp(0, n as isize - 1) as usize;
            cx.notify();
        }
    }

    pub(crate) fn pick_quote(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cands = self.quote_candidates(cx);
        let Some(RSheet::Quote { target, sel, .. }) = self.reading.sheet.as_ref() else {
            return;
        };
        let Some(pick) = cands.get(*sel).cloned() else {
            return;
        };
        let target = target.clone();
        self.reading.sheet = None;
        if self.current.as_ref().map(|c| &c.local_id) != Some(&target) {
            self.open(&target, window, cx);
        }
        self.mode = super::super::Mode::Edit;
        self.editor.update(cx, |s, cx| s.focus(window, cx));
        let (text, cursor) = {
            let s = self.editor.read(cx);
            (s.value().to_string(), s.cursor())
        };
        let (new_text, caret) = vm::insert_transclusion(&text, cursor, &pick.id);
        self.splice_editor(&text, &new_text, Some(caret), window, cx);
        self.show_toast(
            format!("Quoted “{}”", pick.title),
            Some(pick.source.into()),
            cx,
        );
        cx.notify();
    }

    pub(super) fn render_quote_sheet(&self, sheet: &RSheet, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let RSheet::Quote { input, sel, .. } = sheet else {
            return div().into_any_element();
        };
        let cands = self.quote_candidates(cx);
        let sel = *sel;
        div()
            .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                cx.stop_propagation();
                this.close_reading_sheet(window, cx);
            }))
            .capture_action(cx.listener(|this, _: &MoveUp, _, cx| {
                cx.stop_propagation();
                this.move_quote(-1, cx);
            }))
            .capture_action(cx.listener(|this, _: &MoveDown, _, cx| {
                cx.stop_propagation();
                this.move_quote(1, cx);
            }))
            .child(self.sheet_heading("Quote in this thread"))
            .child(self.input_box(
                gpui_kit::base::input::Input::new(input).into_any_element(),
                false,
            ))
            .child(
                div()
                    .id("quote-list")
                    .mt(px(8.))
                    .max_h(px(260.))
                    .overflow_y_scroll()
                    .when(cands.is_empty(), |d| {
                        d.child(div().p(px(8.)).italic().text_color(p.muted).child(
                            "Nothing held matches. Quotes come from your posts and your reading.",
                        ))
                    })
                    .children(cands.into_iter().enumerate().map(|(i, q)| {
                        div()
                            .id(("quote", i))
                            .px(px(8.))
                            .py(px(5.))
                            .rounded(px(6.))
                            .cursor_pointer()
                            .when(i == sel, |d| d.bg(p.sel))
                            .hover(|s| s.bg(p.sel))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(RSheet::Quote { sel, .. }) = this.reading.sheet.as_mut()
                                {
                                    *sel = i;
                                }
                                this.pick_quote(window, cx);
                            }))
                            .child(div().truncate().child(q.title))
                            .child(div().text_size(px(11.)).text_color(p.muted).child(q.source))
                    })),
            )
            .child(self.keys_row(vec![
                self.key_hint("↑↓", "choose"),
                self.key_hint("⏎", "insert ![[…]] on its own line"),
                self.key_hint("esc", "cancel"),
            ]))
            .into_any_element()
    }
}
