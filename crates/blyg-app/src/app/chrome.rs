//! The window's chrome: the title bar, the config-problems banner, the
//! status bar and toasts.

use super::*;

impl MainView {
    pub(super) fn show_toast(
        &mut self,
        text: impl Into<SharedString>,
        sub: Option<SharedString>,
        cx: &mut Context<Self>,
    ) {
        self.toast_seq += 1;
        let id = self.toast_seq;
        let ms = if sub.is_some() { 3200 } else { 2600 };
        self.toast = Some(Toast {
            id,
            text: text.into(),
            sub,
            leaving: false,
        });
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(ms))
                .await;
            let _ = this.update(cx, |v, cx| {
                if v.studio.guards.sticky_toast == Some(id) {
                    return; // until it's clicked
                }
                if let Some(t) = v.toast.as_mut().filter(|t| t.id == id) {
                    t.leaving = true;
                    cx.notify();
                }
            });
            cx.background_executor()
                .timer(Duration::from_millis(220))
                .await;
            let _ = this.update(cx, |v, cx| {
                if v.toast.as_ref().is_some_and(|t| t.id == id)
                    && v.studio.guards.sticky_toast != Some(id)
                {
                    v.toast = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// A toast that stays until it's clicked (or another replaces it).
    pub(super) fn show_toast_until_dismissed(
        &mut self,
        text: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        self.show_toast(text, Some("Click to dismiss".into()), cx);
        self.studio.guards.sticky_toast = Some(self.toast_seq);
    }

    pub(super) fn render_titlebar(
        &self,
        ui_font: &SharedString,
        show_title: bool,
        blyg_title: Option<AnyElement>, // --- blyg switcher ---
    ) -> impl IntoElement {
        let p = self.palette;
        let plate = crate::ornament::title_plate(&self.theme);
        let connected = blyg_title.is_some(); // --- blyg switcher ---
        div()
            .id("titlebar")
            // On Windows the OS title bar moves the window, and a drag area
            // here would answer HTCAPTION for the toolbar and Menu buttons
            // drawn over it, so clicks on them would drag the window instead.
            .when(!cfg!(target_os = "windows"), |d| {
                d.window_control_area(WindowControlArea::Drag)
            })
            .h(px(TITLEBAR_H))
            .flex_none()
            .relative()
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .bg(p.bar)
            .border_b_1()
            .border_color(p.bar_line)
            .children(crate::ornament::titlebar_band(&self.theme))
            // --- buttons --- the toolbar hides the title when it needs the room
            // --- blyg switcher --- with a blyg, the title is the switcher.
            .when_some(blyg_title, |d, t| d.child(t))
            .when(show_title && !connected, |d| {
                d.child(
                    div()
                        .font_family(ui_font.clone())
                        .font_weight(FontWeight::MEDIUM)
                        .text_size(px(12.))
                        .text_color(p.bar_ink)
                        .when(plate, |d| d.px(px(8.)).rounded(px(3.)).bg(p.bar))
                        .child(self.title.clone()),
                )
            })
    }

    /// Config file problems, with line numbers, in a dismissible banner.
    pub(super) fn render_problems(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.config.problems.is_empty() || self.config.problems_dismissed {
            return None;
        }
        let p = self.palette;
        let n = self.config.problems.len();
        let errors = self.config.problems.iter().any(|d| d.is_error());
        const SHOWN: usize = 4;
        let lines = self
            .config
            .problems
            .iter()
            .take(SHOWN)
            .map(|d| div().truncate().child(d.short()));
        Some(
            div()
                .id("config-problems")
                .flex_none()
                .flex()
                .gap(px(10.))
                .px(px(14.))
                .py(px(7.))
                .rule_b(&p)
                .bg(p.panel())
                .font_family(self.theme.chrome_font(&self.prefs))
                .text_size(px(11.5))
                .text_color(p.muted)
                .child(
                    div()
                        .flex_none()
                        .size(px(7.))
                        .mt(px(5.))
                        .rounded_full()
                        .bg(if errors { p.over } else { p.amber }),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(1.))
                        .child(
                            div()
                                .text_color(p.ink)
                                .font_weight(FontWeight::MEDIUM)
                                .child(crate::keymap::hint_owned(format!(
                                    "The config file has {n} problem{} · ⌘⇧, reloads it after you fix {}",
                                    if n == 1 { "" } else { "s" },
                                    if n == 1 { "it" } else { "them" }
                                ))),
                        )
                        .children(lines)
                        .when(n > SHOWN, |d| {
                            d.child(format!(
                                "…and {} more (blygger +validate-config lists them all)",
                                n - SHOWN
                            ))
                        }),
                )
                .child(
                    div()
                        .id("open-config")
                        .flex_none()
                        .cursor_pointer()
                        .hover(|s| s.text_color(p.ink))
                        .on_click(cx.listener(|this, _, _, cx| this.open_config_file_now(cx)))
                        .child("Open"),
                )
                .child(
                    div()
                        .id("dismiss-problems")
                        .flex_none()
                        .cursor_pointer()
                        .hover(|s| s.text_color(p.ink))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.config.problems_dismissed = true;
                            cx.notify();
                        }))
                        .child("✕"),
                )
                .into_any_element(),
        )
    }

    pub(super) fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette.on_status();
        let chrome = self.theme.chrome_font(&self.prefs);
        let disconnected =
            crate::connection::mode(cx) == Some(crate::connection::Mode::Disconnected);
        let (sync_text, dot) = if disconnected {
            ("not connected".to_string(), vm::SyncDot::Grey)
        } else if self.loading && !self.publishing {
            ("loading your posts…".to_string(), vm::SyncDot::Busy)
        } else {
            vm::sync_label(self.sync, self.publishing)
        };
        let dot_color = match dot {
            vm::SyncDot::Green => p.green,
            vm::SyncDot::Amber => p.amber,
            vm::SyncDot::Busy => p.accent,
            vm::SyncDot::Grey => p.grey,
            vm::SyncDot::Red => p.over,
        };
        // --- themes --- the dot is a mark on the status bar's own ground.
        let dot_color =
            crate::theme_ext::readable(dot_color, self.palette.status, crate::theme_ext::MARK);
        // --- follow-ups --- Reading / Mentions / Subscriptions show their
        // own status, not the current post's.
        let screen = reading::vm::screen_status(self.reading.view, self.unread);
        let item = self.current.as_ref().filter(|_| screen.post);
        let chars = item.map(|i| i.char_count()).unwrap_or(0);
        let kind = item.map(|i| i.kind).unwrap_or(Kind::Fragment);
        let (count, level) = vm::counter(kind, chars);
        let banner = vm::banner(kind, chars);

        let dot_el = div().size(px(7.)).rounded_full().bg(dot_color);
        let dot_el = if dot == vm::SyncDot::Busy {
            dot_el
                .with_animation(
                    "pulse",
                    Animation::new(Duration::from_millis(1400)).repeat(),
                    |d, t| {
                        let tri = 1.0 - (2.0 * t - 1.0).abs();
                        d.opacity(1.0 - 0.75 * tri)
                    },
                )
                .into_any_element()
        } else {
            self.delight_dot(dot_el, dot_color) // --- delight ---
        };

        let bar = div()
            .id("status")
            .h(px(STATUS_H))
            .flex_none()
            .relative()
            .flex()
            .items_center()
            .gap(px(12.))
            .px(px(14.))
            .border_t_1()
            .border_color(self.palette.status_line)
            .bg(self.palette.status)
            .font_family(chrome)
            .text_size(px(11.5))
            .text_color(p.muted)
            .when_some(item, |d, item| {
                d.child(
                    div()
                        .id("kind")
                        .cursor_pointer()
                        .hover(|s| s.text_color(p.ink))
                        .on_click(
                            cx.listener(|this, _, window, cx| this.toggle_kind_now(window, cx)),
                        )
                        .child(vm::kind_label(item.kind)),
                )
                // --- delight --- the counter's colour eases.
                .child(
                    self.delight_counter(
                        div()
                            .when(level == vm::Level::Over, |d| {
                                d.font_weight(FontWeight::SEMIBOLD)
                            })
                            .child(count),
                        match level {
                            vm::Level::Warn => self.palette.on_status_text(p.warn),
                            vm::Level::Over => self.palette.on_status_text(p.over),
                            _ => p.muted,
                        },
                    ),
                )
            })
            // --- full editor ---
            .children(self.studio_status(&p).filter(|_| screen.post))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .justify_center()
                    .text_color(self.palette.on_status_text(p.over))
                    .children(banner),
            )
            .children(screen.to_read.map(|t| div().id("to-read").child(t)))
            .children(self.render_server_notice())
            .children(self.render_ext_notice(cx)) // --- extensions ---
            .children(self.render_macro_status(cx)) // --- browser macros ---
            .children(self.render_update_notice(cx)) // --- auto-update ---
            .children(self.render_ai_status()) // --- AI ---
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(dot_el)
                    .child(sync_text)
                    // Not connected: a click opens Connect.
                    .id("sync-status")
                    .when(disconnected, |d| {
                        d.debug_selector(|| "sync-status-connect".into())
                            .cursor_pointer()
                            .hover(|s| s.text_color(p.ink).underline())
                            .tooltip(|_, cx| {
                                cx.new(|_| reading::Tip("Connect your blyg…".into())).into()
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                if !this.sheet.is_open() {
                                    this.open_connect(window, cx)
                                }
                            }))
                    }),
            )
            // --- conflicts --- an item still in conflict says so, and a
            // click shows the resolve sheet again (issue #52).
            .children(item.filter(|i| i.conflict).map(|i| {
                let id = i.local_id.clone();
                div()
                    .id("conflict-marker")
                    .text_color(p.over)
                    .cursor_pointer()
                    .hover(|d| d.underline())
                    .child("sync conflict · resolve…")
                    .tooltip(|_, cx| {
                        cx.new(|_| {
                            reading::Tip(
                                "The server's copy changed while you edited. Click to choose \
                                 which to keep; publishing waits until then."
                                    .into(),
                            )
                        })
                        .into()
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.reopen_conflict(&id, window, cx);
                    }))
                    .into_any_element()
            }))
            // --- new post --- "New note · saved locally · …" while in it.
            .children(match self.new_post_label() {
                Some(l) => Some(div().child(l.to_string()).into_any_element()),
                // "public vN · unpublished edits" says how to publish or
                // discard them on hover.
                None => item.map(|item| {
                    let tip = vm::edits_tip(item);
                    div()
                        .id("version")
                        .child(vm::version_label(item))
                        .when_some(tip, |d, tip| {
                            d.tooltip(move |_, cx| cx.new(|_| reading::Tip(tip.clone())).into())
                        })
                        .into_any_element()
                }),
            })
            // --- themes ---
            .children(crate::ornament::status(
                &self.theme,
                self.status_data(),
                chrome,
            ));

        if self.shake_gen > 0 {
            bar.with_animation(
                ("shake", self.shake_gen),
                Animation::new(Duration::from_millis(350)),
                |d, t| d.left(px(shake_offset(t))),
            )
            .into_any_element()
        } else {
            bar.into_any_element()
        }
    }

    /// --- themes --- What the status ornaments show: posts touched per day
    /// over the last two weeks (the tide and block sparklines).
    pub(super) fn status_data(&self) -> crate::ornament::StatusData {
        if !self
            .theme
            .has(blyg_core::config::theme::Slot::StatusOrnament)
        {
            return crate::ornament::StatusData {
                activity: Vec::new(),
            };
        }
        let mut activity = vec![0u32; 14];
        for item in self.list.results() {
            if let Ok(t) = chrono::DateTime::parse_from_rfc3339(&item.updated) {
                let days = (self.now - t.with_timezone(&chrono::Utc)).num_days();
                if (0..14).contains(&days) {
                    activity[13 - days as usize] += 1;
                }
            }
        }
        crate::ornament::StatusData { activity }
    }

    pub(super) fn render_toast(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let t = self.toast.as_ref()?;
        let p = self.palette;
        let leaving = t.leaving;
        let sticky = self.studio.guards.sticky_toast == Some(t.id);
        let el = div()
            .id("toast")
            .absolute()
            .when(sticky, |d| {
                d.cursor_pointer().on_click(cx.listener(|this, _, _, cx| {
                    this.studio.guards.sticky_toast = None;
                    this.toast = None;
                    cx.notify();
                }))
            })
            // --- full editor: clear of the native preview ---
            .right(self.studio_toast_right())
            .bottom(px(STATUS_H + 12.))
            .max_w(px(420.))
            .px(px(12.))
            .py(px(9.))
            .rounded(px(self.theme.toast_radius))
            .bg(p.toast)
            .text_color(p.toast_ink)
            .map(|d| crate::ornament::border(d, self.theme.toast_border, p.toast_ink.opacity(0.4)))
            .font_family(self.theme.chrome_font(&self.prefs))
            .text_size(px(12.5))
            .shadow_md()
            .child(t.text.clone())
            .when_some(t.sub.clone(), |d, sub| {
                d.child(div().opacity(0.7).child(sub))
            });
        Some(
            el.with_animation(
                ElementId::NamedInteger("toast".into(), (t.id * 2 + leaving as usize) as u64),
                Animation::new(Duration::from_millis(200)).with_easing(ease_out_quint()),
                move |d, t| {
                    let v = if leaving { 1.0 - t } else { t };
                    d.opacity(v).mb(px(-8.0 * (1.0 - v)))
                },
            )
            .into_any_element(),
        )
    }
}
