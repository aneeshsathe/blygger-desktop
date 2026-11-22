//! Drawing the Posts screen's omnibar, list and rows. The search and the
//! selection behind them live in `posts.rs`.

use super::*;

impl MainView {
    pub(super) fn render_omnibar(
        &self,
        ui_font: &SharedString,
        size: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let p = self.palette;
        let hint = if self.mode == Mode::Search {
            self.list.hint()
        } else {
            String::new()
        };
        div()
            .id("omnibar")
            .map(|d| self.omnibar_quote_drop(d, cx)) // --- tray ---
            .h(px(OMNI_H))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(14.))
            .rule_b(&p)
            .child(div().text_size(px(16.)).text_color(p.muted).child("⌕"))
            .child(
                div()
                    .flex_1()
                    .font_family(ui_font.clone())
                    .text_size(px(size - 1.))
                    .capture_action(cx.listener(|this, _: &MoveUp, window, cx| {
                        cx.stop_propagation();
                        this.move_selection(-1, window, cx);
                    }))
                    .capture_action(cx.listener(|this, _: &MoveDown, window, cx| {
                        cx.stop_propagation();
                        this.move_selection(1, window, cx);
                    }))
                    .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                        cx.stop_propagation();
                        this.set_query_text("", window, cx);
                    }))
                    .child(gpui_kit::base::input::Input::new(&self.omni)),
            )
            .when(!hint.is_empty(), |d| {
                d.child(
                    div()
                        .flex_none()
                        .font_family(self.theme.chrome_font(&self.prefs))
                        .text_size(px(12.))
                        .text_color(p.muted)
                        .child(hint),
                )
            })
    }

    pub(super) fn render_list(
        &self,
        len: ListLength,
        ui_font: &SharedString,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let p = self.palette.on_side();
        let lsize = self.prefs.list_size();
        let count = self.list.results().len();
        // --- themes --- the sidebar's ground, then the rows, then the mist.
        let base = div()
            .flex_none()
            .relative()
            .overflow_hidden()
            .bg(p.bg)
            .text_color(p.ink)
            .border_color(self.palette.side_line)
            .font_family(ui_font.clone())
            .children(crate::ornament::sidebar_ground(&self.theme));
        let base = match len {
            ListLength::Width(w) => base.w(w).h_full().border_r_1(),
            ListLength::Height(h) => base.h(h).w_full().border_b_1(),
        };
        if count == 0 && self.offers_connect(cx) {
            // Setup was skipped: say how to connect, or try sample data.
            return base.child(
                div()
                    .relative()
                    .p(px(14.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .children(crate::ornament::empty_art(&self.theme))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(lsize))
                            .text_color(p.muted)
                            .child(
                                div()
                                    .id("empty-connect")
                                    .debug_selector(|| "empty-connect".into())
                                    .cursor_pointer()
                                    .text_color(p.ink)
                                    .hover(|s| s.underline())
                                    .child(crate::keymap::hint(EMPTY_CONNECT))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_connect(window, cx)
                                    })),
                            )
                            .child("·")
                            .child(
                                div()
                                    .id("empty-sample")
                                    .debug_selector(|| "empty-sample".into())
                                    .cursor_pointer()
                                    .hover(|s| s.underline())
                                    .child(EMPTY_SAMPLE)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.onboarding_sample_data(window, cx)
                                    })),
                            ),
                    ),
            );
        }
        if count == 0 {
            let q = self.list.trimmed_query().to_string();
            // --- loose ends --- / --- capture --- operators and addresses.
            let msg = self
                .list
                .empty_message()
                .unwrap_or_else(|| "Nothing here yet. Type to start writing.".to_string());
            return base.child(
                div()
                    .relative()
                    .p(px(14.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .when(q.is_empty(), |d| {
                        d.children(crate::ornament::empty_art(&self.theme))
                    })
                    .child(
                        div()
                            .italic()
                            .text_size(px(lsize))
                            .text_color(p.muted)
                            .child(msg),
                    ),
            );
        }
        // --- loose ends --- the summary in words, over the rows.
        base.flex()
            .flex_col()
            .children(self.render_loose_ends(cx))
            .child(
                div().w_full().flex_1().min_h_0().child(
                    // --- delight --- the list crossfades on a blyg switch.
                    self.delight_list(
                        uniform_list(
                            "items",
                            count,
                            cx.processor(
                                move |this, range: std::ops::Range<usize>, _window, cx| {
                                    range
                                        .map(|ix| this.render_row(ix, lsize, cx))
                                        .collect::<Vec<_>>()
                                },
                            ),
                        )
                        .track_scroll(&self.list_scroll)
                        .size_full()
                        .into_any_element(),
                    ),
                ),
            )
            .children(crate::ornament::scroll_edge(&self.theme))
    }

    pub(super) fn render_row(&self, ix: usize, lsize: f32, cx: &mut Context<Self>) -> AnyElement {
        let item = &self.list.results()[ix];
        let selected = self.list.selected() == Some(&item.local_id);
        // --- themes --- sidebar colours; the selected row's own.
        let p = if selected {
            self.palette.on_sel()
        } else {
            self.palette.on_side()
        };
        let chrome = self.theme.chrome_font(&self.prefs);
        let title: SharedString = vm::item_title(item).into();
        let highlights: Vec<_> = vm::highlight_ranges(&title, self.list.query())
            .into_iter()
            .map(|r| {
                (
                    r,
                    HighlightStyle {
                        color: Some(p.accent),
                        font_weight: Some(FontWeight::SEMIBOLD),
                        ..Default::default()
                    },
                )
            })
            .collect();
        let (pill_text, pill_pub) = vm::pill(item);
        // Once per row: the label scan reads the whole post.
        let held = self.labels_held(&item.content_md);
        let pub_tip: Option<SharedString> = pill_pub.then(|| vm::published_tip(item).into());
        let id = item.local_id.clone();
        let row = div().id(("row", ix)).py(px(6.)).text_color(p.ink);
        // --- shelf --- a hairline under the last shelved row (absolute, so
        // every row keeps the one height `uniform_list` needs).
        let shelf_end = ix + 1 == self.list.shelf_len() && ix + 1 < self.list.results().len();
        let row = row.when(shelf_end, |d| {
            d.relative().child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .h(px(1.))
                    .bg(self.palette.side_line),
            )
        });
        let settle_id = item.local_id.clone(); // --- delight ---
        let el = crate::ornament::row(&self.theme, row, selected)
            .on_click(cx.listener(move |this, _, window, cx| this.open(&id, window, cx)))
            .child(
                div()
                    .text_size(px(lsize))
                    .line_height(relative(1.35))
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    // --- shelf --- a post kept to hand.
                    .when(self.list.is_on_shelf(&item.local_id), |d| {
                        d.child(
                            div()
                                .id(("row-shelf", ix))
                                .flex_none()
                                .text_size(px(lsize * 0.8))
                                .tooltip(|_, cx| {
                                    cx.new(|_| {
                                        reading::Tip(
                                            crate::keymap::hint(
                                                "Saved · ⌘B opens Saved · ⌥⌘B removes it",
                                            )
                                            .into(),
                                        )
                                    })
                                    .into()
                                })
                                .child(shelf::GLYPH),
                        )
                    })
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .child(StyledText::new(title).with_highlights(highlights)),
                    ),
            )
            .child(
                div()
                    .mt(px(2.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .font_family(chrome)
                    .text_size(px(11.))
                    .text_color(p.muted)
                    // Unpublished edits in their own colour (`color-edited`)
                    // and slot (`marker.edited`, Fortress's Δ), so they don't
                    // read as unread (`marker.new`, the accent dot).
                    .when_some(vm::edits_tip(item), |d, tip| {
                        let tip = format!("Unpublished edits: {tip}");
                        d.child(
                            div()
                                .id(("row-edited", ix))
                                .tooltip(move |_, cx| cx.new(|_| reading::Tip(tip.clone())).into())
                                .child(
                                    crate::ornament::marker(
                                        &self.theme,
                                        blyg_core::config::theme::Slot::MarkerEdited,
                                        p.edited,
                                        chrome,
                                    )
                                    .unwrap_or_else(|| {
                                        div()
                                            .size(px(6.))
                                            .rounded_full()
                                            .bg(p.edited)
                                            .into_any_element()
                                    }),
                                ),
                        )
                    })
                    .child(vm::kind_label(item.kind))
                    .children(self.upkeep_row_marker(item, ix, p.muted)) // --- upkeep ---
                    .child(
                        div()
                            .id(("row-pill", ix))
                            .px(px(6.))
                            .rounded(px(self.theme.chip_radius))
                            .border_1()
                            .line_height(px(15.))
                            .border_color(if pill_pub { p.accent } else { p.line })
                            .when_some(pub_tip.clone().filter(|_| !held), |d, tip| {
                                d.text_color(p.accent).tooltip(move |_, cx| {
                                    cx.new(|_| reading::Tip(tip.to_string())).into()
                                })
                            })
                            .when(pill_pub, |d| d.text_color(p.accent))
                            .child(pill_text)
                            // Below studio 0.47: this post waits for its labels.
                            .when(held, |d| {
                                let (bg, ink) = self.palette.tip();
                                d.tooltip(move |_, cx| {
                                    cx.new(|_| toolbar::ButtonTip {
                                        text: crate::keymap::hint(server_notice::HELD_TIP).into(),
                                        reason: None,
                                        bg,
                                        ink,
                                        font: chrome,
                                    })
                                    .into()
                                })
                            }),
                    )
                    .when_some(pub_tip, |d, tip| {
                        d.children(
                            crate::ornament::marker(
                                &self.theme,
                                blyg_core::config::theme::Slot::MarkerPinned,
                                p.accent,
                                chrome,
                            )
                            .map(|m| {
                                div()
                                    .id(("row-published", ix))
                                    .tooltip(move |_, cx| {
                                        cx.new(|_| reading::Tip(tip.to_string())).into()
                                    })
                                    .child(m)
                            }),
                        )
                    })
                    .child(vm::relative_time(&item.updated, self.now)),
            )
            .map(|cell| crate::ornament::row_wrap(&self.theme, cell));
        self.delight_row(el, &settle_id) // --- delight ---
    }
}
