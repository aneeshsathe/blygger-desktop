//! --- extensions --- The ⇧⌘P Extensions palette (in the style of the ⌘G
//! AI palette), the consent sheet (the Publish/Delete sheets' chrome) and
//! the Manage extensions sheet.

use std::time::Duration;

use blyg_ext::protocol::CommandResult;
use blyg_ext::{CONSENT_CAVEAT, Capability, ConsentReply, ExtError, ExtState, describe};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{Ask, Notice, ShowExtensions};
use crate::app::{MainView, TITLEBAR_H};

/// What a palette row does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RowAction {
    /// An extension's command.
    Command { ext: String, id: String },
    /// A browser macro (`extensions/macros.rs`).
    Macro { ext: String, id: String },
    /// A library's notes, in the notes drawer.
    Library { ext: String, title: String },
    /// The Manage extensions sheet.
    Manage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    pub action: RowAction,
    pub label: String,
    pub detail: String,
    /// Which extension it comes from (shown on the right).
    pub from: Option<String>,
}

/// The extension sheets.
pub(crate) enum Overlay {
    Palette {
        rows: Vec<Row>,
        selected: usize,
        focus: FocusHandle,
    },
    Consent {
        name: String,
        /// Each capability and whether it's ticked.
        caps: Vec<(Capability, bool)>,
        reply: Option<ConsentReply>,
        focus: FocusHandle,
    },
    Manage {
        focus: FocusHandle,
    },
}

/// The palette's last row.
pub const MANAGE: &str = "Manage extensions…";

impl MainView {
    /// Hook: the extension actions on the root element.
    pub(crate) fn ext_actions(&self, d: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        d.on_action(
            cx.listener(|this, _: &ShowExtensions, window, cx| this.ext_toggle_palette(window, cx)),
        )
    }

    /// The palette's rows on this screen: running extensions' commands,
    /// their libraries, then "Manage extensions…".
    pub(crate) fn ext_palette_rows(&self, window: &Window, cx: &App) -> Vec<Row> {
        let mut rows = vec![];
        if let Some(host) = &self.ext.host {
            let screen = self.ext_screen(window, cx);
            for e in host.palette(screen, self.current.is_some()) {
                rows.push(Row {
                    action: RowAction::Command {
                        ext: e.ext.clone(),
                        id: e.command.id.clone(),
                    },
                    label: e.command.title.clone(),
                    detail: e.command.detail.clone(),
                    from: Some(e.ext),
                });
            }
            rows.extend(self.ext_macro_rows(screen));
            for l in host.libraries() {
                rows.push(Row {
                    action: RowAction::Library {
                        ext: l.ext.clone(),
                        title: l.library.title.clone(),
                    },
                    label: format!("Browse {}", l.library.title),
                    detail: "In the notes drawer: open, edit, copy into a post".into(),
                    from: Some(l.ext),
                });
            }
        }
        rows.push(Row {
            action: RowAction::Manage,
            label: MANAGE.into(),
            detail: "What's installed, on and allowed".into(),
            from: None,
        });
        rows
    }

    /// ⇧⌘P: open (or close) the palette.
    pub(crate) fn ext_toggle_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.ext.overlay, Some(Overlay::Palette { .. })) {
            return self.ext_close(window, cx);
        }
        if self.sheet.is_some() || self.reading.sheet.is_some() || self.ai.has_overlay() {
            return;
        }
        let rows = self.ext_palette_rows(window, cx);
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.ext.overlay_gen += 1;
        self.ext.overlay = Some(Overlay::Palette {
            rows,
            selected: 0,
            focus,
        });
        cx.notify();
    }

    /// Close whichever extension sheet is up (a consent sheet answers no).
    pub(crate) fn ext_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(Overlay::Consent { reply, name, .. }) = self.ext.overlay.take() {
            match reply {
                Some(r) => r.answer(false),
                None => {
                    self.ext.notices.insert(name, Notice::NeedsPermission);
                }
            }
        }
        self.focus_after_sheet(window, cx);
        cx.notify();
        self.ext_next_ask(window, cx);
    }

    fn ext_palette_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(Overlay::Palette { rows, selected, .. }) = self.ext.overlay.as_mut() else {
            return false;
        };
        let n = rows.len();
        match key {
            "down" => *selected = (*selected + 1) % n,
            "up" => *selected = (*selected + n - 1) % n,
            "enter" => {
                let i = *selected;
                self.ext_run_row(i, window, cx);
            }
            "escape" => self.ext_close(window, cx),
            k => {
                let Some(i) = k.parse::<usize>().ok().filter(|i| (1..=n).contains(i)) else {
                    return false;
                };
                self.ext_run_row(i - 1, window, cx);
            }
        }
        cx.notify();
        true
    }

    /// Run palette row `i`.
    pub(crate) fn ext_run_row(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Overlay::Palette { rows, .. }) = self.ext.overlay.as_ref() else {
            return;
        };
        let Some(row) = rows.get(i).cloned() else {
            return;
        };
        // The selection is read where the user was, before the palette.
        let screen = self.ext_screen_before_palette(window, cx);
        self.ext.overlay = None;
        self.focus_after_sheet(window, cx);
        match row.action {
            RowAction::Manage => self.ext_open_manage(window, cx),
            RowAction::Library { .. } => self.ext_lib_show(window, cx),
            RowAction::Command { ext, id } => self.ext_run_command(ext, id, screen, window, cx),
            RowAction::Macro { ext, id } => self.ext_run_macro(ext, id, window, cx),
        }
        cx.notify();
    }

    /// The screen under the palette (the palette itself has the keyboard).
    fn ext_screen_before_palette(&self, window: &Window, cx: &App) -> blyg_ext::protocol::Screen {
        use blyg_ext::protocol::Screen;
        if self.notes.open {
            Screen::Notes
        } else {
            match self.ext_screen(window, cx) {
                Screen::Notes => Screen::Posts,
                s => s,
            }
        }
    }

    /// Run `ext`'s command `id` off the main thread with the selection,
    /// the open item and the screen; show what it answers.
    pub(crate) fn ext_run_command(
        &mut self,
        ext: String,
        id: String,
        screen: blyg_ext::protocol::Screen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.ext.host.clone() else {
            return;
        };
        let mut context = self.ext_context(screen);
        self.ext_selection(window, cx, move |this, selection, window, cx| {
            context.selection = Some(selection).filter(|s| !s.trim().is_empty());
            let (e, i) = (ext.clone(), id.clone());
            let task = cx.background_spawn(async move { host.command(&e, &i, context) });
            cx.spawn_in(window, async move |this, cx| {
                let result = task.await;
                let _ = this.update_in(cx, |v, window, cx| {
                    v.ext_command_done(&ext, result, window, cx)
                });
            })
            .detach();
            let _ = this;
        });
    }

    fn ext_command_done(
        &mut self,
        ext: &str,
        result: Result<CommandResult, ExtError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(r) => {
                if let Some(t) = r.toast {
                    self.show_toast(t, Some(ext.to_string().into()), cx);
                }
                if let Some(id) = r.open {
                    self.ext_open_item(&blyg_core::LocalId(id), window, cx);
                }
                // A command may have written to a library.
                self.ext_lib_refresh(window, cx);
            }
            Err(e) => self.show_toast(e.message(ext), None, cx),
        }
        cx.notify();
    }

    // ------------------------------------------------------------ consent

    /// Show the consent sheet for `ask` (every capability ticked).
    pub(crate) fn ext_show_consent(
        &mut self,
        ask: Ask,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Replacing another consent sheet answers it.
        if let Some(Overlay::Consent { reply: Some(r), .. }) = self.ext.overlay.take() {
            r.answer(false);
        }
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.ext.overlay_gen += 1;
        self.ext.overlay = Some(Overlay::Consent {
            name: ask.name,
            caps: ask.caps.into_iter().map(|c| (c, true)).collect(),
            reply: ask.reply,
            focus,
        });
        cx.notify();
    }

    /// Tick or untick capability `i`.
    pub(crate) fn ext_toggle_cap(&mut self, i: usize, cx: &mut Context<Self>) {
        if let Some(Overlay::Consent { caps, .. }) = self.ext.overlay.as_mut()
            && let Some(c) = caps.get_mut(i)
        {
            c.1 = !c.1;
            cx.notify();
        }
    }

    /// Allow: write the ticked capabilities to the config and (re)start.
    /// Nothing ticked is "Not now".
    pub(crate) fn ext_allow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Overlay::Consent { name, caps, .. }) = self.ext.overlay.as_ref() else {
            return;
        };
        let ticked: Vec<Capability> = caps.iter().filter(|c| c.1).map(|c| c.0.clone()).collect();
        if ticked.is_empty() {
            return self.ext_close(window, cx);
        }
        let unticked: Vec<Capability> = caps.iter().filter(|c| !c.1).map(|c| c.0.clone()).collect();
        let name = name.clone();
        if !unticked.is_empty() {
            self.ext.declined.push((name.clone(), unticked));
        }
        let Some(Overlay::Consent { reply, .. }) = self.ext.overlay.take() else {
            return;
        };
        if let Some(r) = reply {
            r.answer(true);
        }
        self.ext.notices.remove(&name);
        let changes = super::allow_changes(crate::settings::get(cx).store.config(), &name, &ticked);
        self.focus_after_sheet(window, cx);
        if self.ext_write_config(&changes, window, cx) {
            self.show_toast(
                format!("{name} is allowed"),
                Some("Saved in your config file".into()),
                cx,
            );
        }
        cx.notify();
        self.ext_next_ask(window, cx);
    }

    // ------------------------------------------------------------ manage

    pub(crate) fn ext_open_manage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.ext.overlay_gen += 1;
        self.ext.overlay = Some(Overlay::Manage { focus });
        cx.notify();
    }

    /// Manage › Turn on: enable a disabled extension (consent follows).
    fn ext_turn_on(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let changes = super::allow_changes(crate::settings::get(cx).store.config(), name, &[]);
        self.ext_write_config(&changes, window, cx);
    }

    // ------------------------------------------------------------ render

    /// Hook: whichever extension sheet is up.
    pub(crate) fn render_ext_overlay(
        &self,
        ui_font: &SharedString,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let overlay = self.ext.overlay.as_ref()?;
        let (width, content) = match overlay {
            Overlay::Palette {
                rows,
                selected,
                focus,
            } => (440., self.render_ext_palette(rows, *selected, focus, cx)),
            Overlay::Consent {
                name, caps, focus, ..
            } => (500., self.render_consent(name, caps, focus, cx)),
            Overlay::Manage { focus } => (540., self.render_manage(focus, cx)),
        };
        let gen_ = self.ext.overlay_gen;
        Some(
            div()
                .absolute()
                .top(px(TITLEBAR_H))
                .bottom_0()
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .items_start()
                .child(
                    div()
                        .id("ext-sheet")
                        .debug_selector(|| "ext-sheet".into())
                        .occlude()
                        .w(px(width))
                        .max_w(relative(0.92))
                        .max_h(relative(0.86))
                        .overflow_y_scroll()
                        .map(|d| crate::theme_ext::sheet(d, &self.theme))
                        .px(px(18.))
                        .py(px(16.))
                        .font_family(ui_font.clone())
                        .text_size(px(13.))
                        .text_color(self.palette.ink)
                        .child(content)
                        .with_animation(
                            ("ext-sheet-in", gen_),
                            Animation::new(Duration::from_millis(220))
                                .with_easing(ease_out_quint()),
                            |d, t| d.mt(px(-240.0 * (1.0 - t))).opacity(t.min(1.0) * 0.4 + 0.6),
                        ),
                )
                .into_any_element(),
        )
    }

    fn ext_kbd(&self, k: &'static str) -> Div {
        crate::theme_ext::kbd(div(), &self.theme)
            .px(px(6.))
            .py(px(1.))
            .min_w(px(20.))
            .flex()
            .justify_center()
            .font_weight(FontWeight::MEDIUM)
            .text_size(px(11.5))
            .child(k)
    }

    fn ext_key_hint(&self, k: &'static str, label: &'static str) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(5.))
            .child(self.ext_kbd(k))
            .child(label)
    }

    fn ext_heading(s: impl Into<SharedString>) -> Div {
        div()
            .mb(px(8.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(px(13.))
            .child(s.into())
    }

    fn ext_keys_row(&self) -> Div {
        div()
            .mt(px(12.))
            .flex()
            .flex_wrap()
            .gap(px(14.))
            .text_color(self.palette.muted)
    }

    fn render_ext_palette(
        &self,
        rows: &[Row],
        selected: usize,
        focus: &FocusHandle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let list = rows.iter().enumerate().map(|(i, r)| {
            let on = i == selected;
            let last = r.action == RowAction::Manage;
            div()
                .id(("ext-pal", i))
                .debug_selector(move || format!("ext-pal-{i}"))
                .flex()
                .items_center()
                .gap(px(10.))
                .px(px(8.))
                .py(px(5.))
                .rounded(px(6.))
                .cursor_pointer()
                .when(last && rows.len() > 1, |d| d.mt(px(4.)))
                .when(on, |d| d.bg(p.pick()))
                .when(!on, |d| d.hover(|s| s.bg(p.hover())))
                .on_click(cx.listener(move |this, _, window, cx| this.ext_run_row(i, window, cx)))
                .child(
                    div()
                        .w(px(14.))
                        .text_color(p.muted)
                        .child(format!("{}", i + 1)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(div().child(r.label.clone()))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(p.muted)
                                .truncate()
                                .child(r.detail.clone()),
                        ),
                )
                .children(r.from.clone().map(|f| {
                    div()
                        .flex_none()
                        .text_size(px(11.))
                        .text_color(p.muted)
                        .child(f)
                }))
        });
        let empty = rows.len() == 1;
        div()
            .track_focus(focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                if this.ext_palette_key(ev.keystroke.key.as_str(), window, cx) {
                    cx.stop_propagation();
                }
            }))
            .child(Self::ext_heading("Extensions"))
            .child(div().flex().flex_col().children(list))
            .when(empty, |d| {
                d.child(
                    div()
                        .mt(px(8.))
                        .text_size(px(11.5))
                        .text_color(p.muted)
                        .line_height(relative(1.45))
                        .child(
                            "No extension is running. Manage extensions turns on the bundled \
                             markdown-notes, or add `extension = <name>` to your config file.",
                        ),
                )
            })
            .child(self.ext_keys_row().children([
                self.ext_key_hint("↑↓", "choose"),
                self.ext_key_hint("⏎", "run"),
                self.ext_key_hint("esc", "close"),
            ]))
            .into_any_element()
    }

    fn render_consent(
        &self,
        name: &str,
        caps: &[(Capability, bool)],
        focus: &FocusHandle,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let theme = &*self.theme;
        let rows = caps.iter().enumerate().map(|(i, (c, on))| {
            let on = *on;
            div()
                .id(("ext-cap", i))
                .debug_selector(move || format!("ext-cap-{i}"))
                .flex()
                .items_start()
                .gap(px(8.))
                .py(px(4.))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| this.ext_toggle_cap(i, cx)))
                .child(
                    div()
                        .flex_none()
                        .mt(px(1.))
                        .size(px(15.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .map(|d| crate::theme_ext::chip(d, theme, on))
                        .text_size(px(11.))
                        .when(on, |d| d.bg(p.pick()))
                        .child(if on { "✓" } else { "" }),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .line_height(relative(1.4))
                        .when(!on, |d| d.text_color(p.muted))
                        .child(describe(c)),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(px(10.5))
                        .text_color(p.muted)
                        .child(format!("{}", i + 1)),
                )
        });
        let none = caps.iter().all(|c| !c.1);
        div()
            .track_focus(focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                let k = ev.keystroke.key.as_str();
                match k {
                    "enter" => this.ext_allow(window, cx),
                    "escape" => this.ext_close(window, cx),
                    _ => match k.parse::<usize>() {
                        Ok(n) if n >= 1 => this.ext_toggle_cap(n - 1, cx),
                        _ => return,
                    },
                }
                cx.stop_propagation();
            }))
            .child(Self::ext_heading(format!("{name} wants to:")))
            .child(div().flex().flex_col().children(rows))
            .child(
                div()
                    .mt(px(10.))
                    .text_size(px(11.5))
                    .text_color(p.muted)
                    .line_height(relative(1.45))
                    .child(CONSENT_CAVEAT),
            )
            .child(
                self.ext_keys_row()
                    .child(
                        self.ext_key_hint("⏎", if none { "not now" } else { "allow" })
                            .id("ext-allow")
                            .debug_selector(|| "ext-allow".into())
                            .cursor_pointer()
                            .when(!none, |d| d.text_color(p.accent_text()))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.ext_allow(window, cx)),
                            ),
                    )
                    .child(
                        self.ext_key_hint("esc", "not now")
                            .id("ext-not-now")
                            .debug_selector(|| "ext-not-now".into())
                            .cursor_pointer()
                            .on_click(
                                cx.listener(|this, _, window, cx| this.ext_close(window, cx)),
                            ),
                    )
                    .child(div().child("1–9 tick or untick")),
            )
            .into_any_element()
    }

    fn render_manage(&self, focus: &FocusHandle, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let theme = &*self.theme;
        let (status, diagnostics) = match &self.ext.host {
            Some(h) => (h.status(), h.diagnostics()),
            None => (vec![], vec![]),
        };
        let chip = |id: String, label: &'static str| {
            div()
                .id(SharedString::from(id.clone()))
                .debug_selector(move || id.clone())
                .flex_none()
                .px(px(8.))
                .py(px(1.))
                .map(|d| crate::theme_ext::chip(d, theme, false))
                .text_size(px(11.5))
                .cursor_pointer()
                .hover(|s| s.bg(p.hover()))
                .child(label)
        };
        let caps_line = |label: &'static str, caps: &[Capability]| {
            (!caps.is_empty()).then(|| {
                div()
                    .text_size(px(11.5))
                    .text_color(p.muted)
                    .line_height(relative(1.4))
                    .child(format!(
                        "{label}: {}",
                        caps.iter().map(describe).collect::<Vec<_>>().join(" · ")
                    ))
            })
        };
        let cards = status.into_iter().map(|s| {
            let (word, good) = state_word(&s.state);
            let name = s.name.clone();
            let n2 = s.name.clone();
            let action = match s.state {
                ExtState::Disabled => Some(chip(format!("ext-on-{name}"), "Turn on").on_click(
                    cx.listener(move |this, _, window, cx| {
                        this.ext_turn_on(&name, window, cx);
                    }),
                )),
                _ if s.enabled && !s.missing.is_empty() => Some(
                    chip(format!("ext-allow-{name}"), "Allow…").on_click(cx.listener(
                        move |this, _, window, cx| {
                            this.ext.overlay = None;
                            this.ext_ask_again(&n2, window, cx);
                        },
                    )),
                ),
                _ => None,
            };
            div()
                .py(px(8.))
                .border_b_1()
                .border_color(p.edge())
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(s.name.clone()),
                        )
                        .when(!s.version.is_empty(), |d| {
                            d.child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(p.muted)
                                    .child(s.version.clone()),
                            )
                        })
                        .when(s.bundled, |d| {
                            d.child(
                                div()
                                    .px(px(5.))
                                    .rounded(px(theme.chip_radius.min(4.)))
                                    .bg(p.pick())
                                    .text_size(px(10.5))
                                    .text_color(p.muted)
                                    .child("bundled"),
                            )
                        })
                        .child(div().flex_1())
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(if good { p.green_text() } else { p.muted })
                                .child(word),
                        )
                        .children(action),
                )
                .when(!s.description.is_empty(), |d| {
                    d.child(
                        div()
                            .text_size(px(12.))
                            .text_color(p.muted)
                            .child(s.description.clone()),
                    )
                })
                .children(caps_line("Allowed", &s.granted))
                .children(caps_line("Not allowed", &s.missing))
        });
        div()
            .track_focus(focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                if matches!(ev.keystroke.key.as_str(), "escape" | "enter") {
                    cx.stop_propagation();
                    this.ext_close(window, cx);
                }
            }))
            .child(Self::ext_heading("Extensions"))
            .child(div().flex().flex_col().children(cards))
            .children(diagnostics.into_iter().map(|d| {
                div()
                    .mt(px(6.))
                    .text_size(px(11.5))
                    .text_color(p.warn_text())
                    .child(format!(
                        "{}: {}",
                        blyg_core::config::paths::tilde(&d.path),
                        d.message
                    ))
            }))
            .child(
                div()
                    .mt(px(8.))
                    .text_size(px(11.5))
                    .text_color(p.muted)
                    .line_height(relative(1.45))
                    .child(
                        "Extensions run only when the config file names them (`extension = \
                         <name>`); install one by copying its folder into the extensions folder. \
                         See `blygger +list-extensions`.",
                    ),
            )
            .child(
                self.ext_keys_row().child(
                    self.ext_key_hint("esc", "close")
                        .id("ext-manage-close")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, window, cx| this.ext_close(window, cx))),
                ),
            )
            .into_any_element()
    }
}

/// A state in a word, and whether it's good news.
pub fn state_word(s: &ExtState) -> (String, bool) {
    match s {
        ExtState::Disabled => ("off".into(), false),
        ExtState::Missing => ("not installed".into(), false),
        ExtState::NeedsConsent => ("waiting for permission".into(), false),
        ExtState::Starting => ("starting…".into(), false),
        ExtState::Running => ("running".into(), true),
        ExtState::Restarting { failures } => (format!("restarting ({failures} failed)"), false),
        ExtState::Failed { message } => (message.clone(), false),
        ExtState::Stopped => ("stopped".into(), false),
    }
}

impl MainView {
    /// Hook (Settings): "NOTES FOLDER" with the markdown-notes vault, and
    /// "Markdown notes folder…", which picks a folder, writes
    /// `extension = markdown-notes` and its `vault` setting, and asks for
    /// permission to use it.
    pub(crate) fn render_ext_settings_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let vault = self.ext_vault(cx);
        let running = self.ext_library().is_some();
        let now = match &vault {
            Some(v) if running => format!("{v} · on"),
            Some(v) => format!("{v} · not running yet"),
            None => {
                "Off: a folder of Markdown notes (an Obsidian vault) in the notes drawer".into()
            }
        };
        div()
            .flex()
            .gap(px(12.))
            .py(px(7.))
            .child(
                div()
                    .w(px(92.))
                    .flex_none()
                    .pt(px(4.))
                    .text_size(px(10.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.muted)
                    .child("NOTES FOLDER"),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div().flex().child(
                            div()
                                .id("ext-vault")
                                .debug_selector(|| "ext-vault".into())
                                .px(px(10.))
                                .py(px(3.))
                                .map(|d| crate::theme_ext::chip(d, &self.theme, false))
                                .cursor_pointer()
                                .hover(|s| s.border_color(p.accent).bg(p.hover()))
                                .child("Markdown notes folder…")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.ext_pick_vault(window, cx)
                                })),
                        ),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .text_size(px(11.5))
                            .text_color(p.muted)
                            .child(div().truncate().child(now)),
                    ),
            )
            .into_any_element()
    }

    /// The folder picker for the notes folder.
    fn ext_pick_vault(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Use as notes folder".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(dir) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update_in(cx, |v, window, cx| {
                v.close_sheet(window, cx);
                v.ext_set_vault(&dir, window, cx);
            });
        })
        .detach();
    }
}
