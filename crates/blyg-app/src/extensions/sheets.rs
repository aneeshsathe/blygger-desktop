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
    /// A granted macro site's `home`, in the browser pane (to sign in).
    OpenSite { ext: String, site: String },
    /// A library's notes, in the notes drawer.
    Library {
        ext: String,
        id: String,
        title: String,
    },
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
            rows.extend(self.ext_site_rows());
            for l in host.libraries() {
                rows.push(Row {
                    action: RowAction::Library {
                        ext: l.ext.clone(),
                        id: l.library.id.clone(),
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
                // (The tour's sample question leaves no notice.)
                None if self.ext_tour_on() => {}
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
            RowAction::Library { id, .. } => {
                self.ext_lib_show(window, cx);
                self.ext_lib_choose(Some(id), window, cx);
            }
            RowAction::Command { ext, id } => self.ext_run_command(ext, id, screen, window, cx),
            RowAction::Macro { ext, id } => self.ext_run_macro(ext, id, window, cx),
            RowAction::OpenSite { ext, site } => self.ext_open_site(&ext, &site, window, cx),
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
        // --- onboarding --- the tour's sample sheet: say so, write nothing.
        if self.ext_tour_on() {
            if let Some(Overlay::Consent { reply: Some(r), .. }) = self.ext.overlay.take() {
                r.answer(false);
            }
            self.ext.overlay = None;
            self.focus_after_sheet(window, cx);
            self.ext_tour_refuses(format!("{name} would be allowed"), cx);
            return;
        }
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
        if self.ext_tour_refuses(format!("{name} would be turned on"), cx) {
            return;
        }
        let changes = super::allow_changes(crate::settings::get(cx).store.config(), name, &[]);
        self.ext_write_config(&changes, window, cx);
    }

    /// Turn off (Manage, or the consent sheet's ⌘⌫): remove `name`'s
    /// `extension` lines; the reload stops it. Its grants and settings
    /// stay, so Turn on brings it back as it was, unless `forget` (Forget
    /// permissions), which removes its `extension-allow` lines too. Any
    /// question or notice about it goes away.
    pub(crate) fn ext_turn_off(
        &mut self,
        name: &str,
        forget: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // --- onboarding --- in the tour: the sheet goes, nothing is written.
        if self.ext_tour_on() {
            if matches!(&self.ext.overlay, Some(Overlay::Consent { name: n, .. }) if n == name) {
                if let Some(Overlay::Consent { reply: Some(r), .. }) = self.ext.overlay.take() {
                    r.answer(false);
                }
                self.ext.overlay = None;
                self.focus_after_sheet(window, cx);
            }
            let what = if forget {
                format!("{name} would be turned off, its permissions forgotten")
            } else {
                format!("{name} would be turned off")
            };
            self.ext_tour_refuses(what, cx);
            return;
        }
        let cfg = crate::settings::get(cx).store.config();
        let was_on = cfg.extensions_enabled().iter().any(|n| n == name);
        let changes = super::disable_changes(cfg, name, forget);
        // A consent sheet for it (on screen or queued) is answered no.
        let consent =
            matches!(&self.ext.overlay, Some(Overlay::Consent { name: n, .. }) if n == name);
        if consent {
            if let Some(Overlay::Consent { reply: Some(r), .. }) = self.ext.overlay.take() {
                r.answer(false);
            }
            self.focus_after_sheet(window, cx);
        }
        for a in std::mem::take(&mut self.ext.asks) {
            if a.name == name {
                if let Some(r) = a.reply {
                    r.answer(false);
                }
            } else {
                self.ext.asks.push_back(a);
            }
        }
        self.ext.notices.remove(name);
        // Turning it on again is a fresh start: ask again.
        self.ext.asked.retain(|(n, _)| n != name);
        self.ext.declined.retain(|(n, _)| n != name);
        if !changes.is_empty() && self.ext_write_config(&changes, window, cx) {
            let text = match (was_on, forget) {
                (true, false) => format!("{name} turned off"),
                (true, true) => format!("{name} turned off, permissions forgotten"),
                (false, _) => format!("{name}'s permissions forgotten"),
            };
            self.show_toast(text, Some("Saved in your config file".into()), cx);
        }
        cx.notify();
        if consent {
            self.ext_next_ask(window, cx);
        }
    }

    /// The consent sheet's Turn off: the extension it asks for.
    fn ext_consent_turn_off(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(Overlay::Consent { name, .. }) = self.ext.overlay.as_ref() {
            let name = name.clone();
            self.ext_turn_off(&name, false, window, cx);
        }
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
                    "backspace" if ev.keystroke.modifiers.secondary() => {
                        this.ext_consent_turn_off(window, cx)
                    }
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
                    .child(
                        self.ext_key_hint(crate::keymap::hint("⌘⌫"), "turn off")
                            .id("ext-turn-off")
                            .debug_selector(|| "ext-turn-off".into())
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.ext_consent_turn_off(window, cx)
                            })),
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
            let actions = self.ext_manage_actions(&s, &chip, cx);
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
                        ),
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
                .when(!actions.is_empty(), |d| {
                    d.child(
                        div()
                            .mt(px(3.))
                            .flex()
                            .flex_wrap()
                            .gap(px(6.))
                            .children(actions),
                    )
                })
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

    /// A Manage card's buttons: Turn on (off), or Allow… (something not
    /// allowed yet) and Turn off (on, in any state); Forget permissions
    /// (anything allowed).
    fn ext_manage_actions(
        &self,
        s: &blyg_ext::ExtensionStatus,
        chip: &dyn Fn(String, &'static str) -> Stateful<Div>,
        cx: &Context<Self>,
    ) -> Vec<Stateful<Div>> {
        let mut actions = vec![];
        let n = s.name.clone();
        if !s.enabled {
            actions.push(chip(format!("ext-on-{n}"), "Turn on").on_click(
                cx.listener(move |this, _, window, cx| this.ext_turn_on(&n, window, cx)),
            ));
        } else {
            if !s.missing.is_empty() {
                let n = n.clone();
                actions.push(
                    chip(format!("ext-allow-{n}"), "Allow…").on_click(cx.listener(
                        move |this, _, window, cx| {
                            this.ext.overlay = None;
                            this.ext_ask_again(&n, window, cx);
                        },
                    )),
                );
            }
            actions.push(chip(format!("ext-off-{n}"), "Turn off").on_click(
                cx.listener(move |this, _, window, cx| this.ext_turn_off(&n, false, window, cx)),
            ));
        }
        if !s.granted.is_empty() {
            let n = s.name.clone();
            actions.push(
                chip(format!("ext-forget-{n}"), "Forget permissions").on_click(
                    cx.listener(move |this, _, window, cx| this.ext_turn_off(&n, true, window, cx)),
                ),
            );
        }
        actions
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
    /// Hook (Settings): "NOTES FOLDERS", the markdown-notes vaults, each
    /// with Remove (the config line only; the folder is never touched),
    /// and "Add folder…", which picks folders (several at once), writes
    /// `extension = markdown-notes` and a `vault` / `vault-<label>` setting
    /// for each, and asks for permission to use them.
    pub(crate) fn render_ext_settings_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let (on, vaults) = self.ext_vaults(cx);
        let running = self.ext_libraries().iter().any(|l| l.ext == super::NOTES);
        let now = match (on, vaults.is_empty()) {
            (false, true) => {
                "Off: folders of Markdown notes (Obsidian vaults) in the notes drawer".into()
            }
            (false, false) => "Off: turn on markdown-notes to use them".to_string(),
            (true, true) => "On, with no folder yet: add one".into(),
            (true, false) if running => "On · in the notes drawer's Notes tab".into(),
            (true, false) => "Not running yet: waiting for permission".into(),
        };
        let rows: Vec<AnyElement> = vaults
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let key = v.key.clone();
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .min_w_0()
                    .text_size(px(12.))
                    .child(div().flex_none().child(v.title.clone()))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(11.5))
                            .text_color(p.muted)
                            .child(v.path.clone()),
                    )
                    .child(
                        div()
                            .id(("ext-vault-remove", i))
                            .debug_selector(move || format!("ext-vault-remove-{i}"))
                            .flex_none()
                            .text_size(px(11.5))
                            .text_color(p.muted)
                            .cursor_pointer()
                            .hover(|s| s.underline().text_color(p.ink))
                            .child("Remove")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.ext_remove_vault(&key, window, cx)
                            })),
                    )
                    .into_any_element()
            })
            .collect();
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
                    .child("NOTES FOLDERS"),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .children(rows)
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
                                .child("Add folder…")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.ext_pick_vaults(window, cx)
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
}
