//! Settings (⌘,): a page over the whole window below the title bar, not a
//! pop-up. Every section draws its rows with `row` and its explanations with
//! `note`, so nothing runs past the page's right edge: the content column
//! shrinks with the window (`min_w_0`), chip rows wrap, and prose wraps.

use gpui_kit::base::input::{Escape, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::pref_preview::{Pick, Row};
use super::sheet::{Kit, SheetView, Shown};
use super::{MainView, OMNI_H, ReloadConfig, TITLEBAR_H};
use crate::prefs::{self, LayoutPref};
use crate::theme::{Palette, Rule};

/// Settings' state while it's open (a page, registered as a sheet so only
/// one of them is up).
pub(crate) struct SettingsSheet {
    pub(crate) hotkey: Entity<InputState>,
    pub(crate) hotkey_error: Option<String>,
    pub(crate) focus: FocusHandle,
    /// The page's scroll position.
    pub(crate) scroll: ScrollHandle,
}

impl SheetView for SettingsSheet {
    /// Not a sheet: a page over the whole window.
    fn render(&self, v: &MainView, kit: &Kit, cx: &mut Context<MainView>) -> Shown {
        Shown::Page(v.render_settings_page(
            &self.hotkey,
            self.hotkey_error.as_deref(),
            &self.focus,
            &self.scroll,
            kit.ui_font,
            cx,
        ))
    }

    fn inputs(&self) -> Vec<&Entity<InputState>> {
        vec![&self.hotkey]
    }
}

/// The label column on the left of every row.
pub(crate) const LABEL_W: f32 = 92.;
/// The content column's widest: a readable line, centred in a wide window.
pub(crate) const CONTENT_MAX_W: f32 = 800.;
/// The content column's side padding.
pub(crate) const CONTENT_PAD: f32 = 24.;

/// A settings row: the label on the left, `content` filling the rest. The
/// content column may shrink below its content's natural width
/// (`min_w_0`), so long chips and hints wrap instead of being clipped.
/// Debug selectors: `settings-row-<LABEL>` and `settings-cell-<LABEL>`.
pub(crate) fn row(p: &Palette, label: &'static str, content: impl IntoElement) -> Div {
    div()
        .debug_selector(move || format!("settings-row-{label}"))
        .w_full()
        .flex()
        .gap(px(12.))
        .py(px(7.))
        .child(
            div()
                .w(px(LABEL_W))
                .flex_none()
                .pt(px(4.))
                .text_size(px(10.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.muted)
                .child(label),
        )
        .child(
            div()
                .debug_selector(move || format!("settings-cell-{label}"))
                .flex_1()
                .min_w_0()
                .child(content),
        )
}

/// A muted explanation under a row's chips. It wraps; it is never truncated.
pub(crate) fn note(p: &Palette, text: impl IntoElement) -> Div {
    div()
        .w_full()
        .min_w_0()
        .text_size(px(11.))
        .line_height(relative(1.4))
        .text_color(p.muted)
        .child(text)
}

/// A muted hint beside a chip in a row: takes the rest of the line, wraps.
pub(crate) fn side_note(p: &Palette, text: impl IntoElement) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .text_size(px(11.))
        .line_height(relative(1.4))
        .text_color(p.muted)
        .child(text)
}

/// A wrapping row of chips.
pub(crate) fn chips() -> Div {
    div().flex().flex_wrap().items_center().gap(px(6.))
}

impl MainView {
    /// `BLYGGER_DEMO=settings-full` (fake mode, screenshots): Settings open,
    /// with the blyg's changelog choices shown as a studio 0.58 has them.
    /// `BLYGGER_SNAPSHOT_WIDTH` / `BLYGGER_SNAPSHOT_HEIGHT` resize the window
    /// first, and `BLYGGER_DEMO_SCROLL=bottom` scrolls to the last rows.
    pub(super) fn settings_demo(&mut self, n: usize, window: &mut Window, cx: &mut Context<Self>) {
        match n {
            0 => {
                let snap = &crate::dev_flags::get().snapshot;
                let now = window.viewport_size();
                let w = snap.width.map(px).unwrap_or(now.width);
                let h = snap.height.map(px).unwrap_or(now.height);
                window.resize(size(w, h));
                self.open_settings(&super::OpenSettings, window, cx);
            }
            1 => {
                self.reading.changelogs.settings = super::reading::Load::Ready(Default::default());
                cx.notify();
            }
            // `BLYGGER_DEMO_SCROLL=bottom`: the end of the page.
            2 if crate::dev_flags::get().demo.scroll_bottom => {
                if let Some(SettingsSheet { scroll, .. }) = self.sheet.get() {
                    scroll.scroll_to_bottom();
                }
                cx.notify();
            }
            _ => {}
        }
    }

    /// Settings scrolled to BACKGROUND, just above READ MARKS (scenes): the
    /// end of the page, then back up past it (drawn first: a window that
    /// isn't in front may not redraw).
    pub(super) fn settings_show_background(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(SettingsSheet { scroll, .. }) = self.sheet.get() {
            scroll.scroll_to_bottom();
        }
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(300))
                .await;
            let _ = cx.update(|window, cx| window.draw(cx).clear(cx));
            let _ = this.update(cx, |v, cx| {
                if let Some(SettingsSheet { scroll, .. }) = v.sheet.get() {
                    let o = scroll.offset();
                    scroll.set_offset(point(o.x, (o.y + px(640.)).min(px(0.))));
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// `BLYGGER_DEMO=settings-preview` (fake mode, screenshots): Settings
    /// open with the keyboard on the THEME row, trying
    /// `BLYGGER_DEMO_PREVIEW` (default `kumiko`; `font:<label>` tries a
    /// writing font). `BLYGGER_DEMO_SLOT=dark` tries it for dark mode.
    pub(super) fn settings_preview_demo(
        &mut self,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match n {
            0 => self.settings_demo(0, window, cx),
            1 => {
                let demo = &crate::dev_flags::get().demo;
                let want = demo.preview.clone().unwrap_or_else(|| "kumiko".into());
                let dark = demo.slot_dark;
                if dark {
                    self.set_theme_slot(true, window, cx);
                }
                let (row, pick) = match want.strip_prefix("font:") {
                    Some(f) => (
                        Row::WritingFont,
                        Pick::WritingFont(
                            crate::prefs::find(crate::prefs::WRITING_FONTS, f)
                                .unwrap_or(crate::prefs::WRITING_FONTS[0])
                                .label,
                        ),
                    ),
                    None => (
                        Row::Theme,
                        Pick::Theme {
                            dark,
                            id: Some(want),
                        },
                    ),
                };
                self.preview.cursor = Some((row, pick.clone()));
                self.preview_pick(pick, super::pref_preview::Origin::Settings, window, cx);
            }
            _ => {}
        }
    }

    /// Hook (`render_sheet`): Settings, over the window below the title bar.
    pub(super) fn render_settings_page(
        &self,
        hotkey: &Entity<InputState>,
        hotkey_error: Option<&str>,
        focus: &FocusHandle,
        scroll: &ScrollHandle,
        ui_font: &SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let theme = &*self.theme;
        let kbd = |k: &'static str| {
            crate::theme_ext::kbd(div(), theme)
                .px(px(6.))
                .py(px(1.))
                .min_w(px(20.))
                .flex()
                .justify_center()
                .font_weight(FontWeight::MEDIUM)
                .text_size(px(11.5))
                .child(k)
        };
        let header = div()
            .id("settings-header")
            .h(px(OMNI_H))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(14.))
            .px(px(16.))
            .rule_b(&p)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(14.))
                    .child("Settings"),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .text_color(p.muted)
                    .child(kbd(crate::keymap::hint("⌘,")))
                    .child("toggle"),
            )
            .child(
                div()
                    .id("settings-close")
                    .debug_selector(|| "settings-close".into())
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .text_color(p.muted)
                    .cursor_pointer()
                    .hover(|s| s.text_color(p.ink))
                    .child(kbd("esc"))
                    .child("close")
                    .on_click(cx.listener(|this, _, window, cx| this.close_sheet(window, cx))),
            );
        div()
            .id("settings-page")
            .debug_selector(|| "settings-page".into())
            .occlude()
            .absolute()
            .top(px(TITLEBAR_H))
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .flex_col()
            .bg(p.bg)
            .text_color(p.ink)
            .font_family(ui_font.clone())
            .text_size(px(13.))
            .track_focus(focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                // --- preview --- the page's own keys (not a field's): move
                // through the fonts, size and themes, trying each.
                let k = &ev.keystroke;
                let page_has_keys = this
                    .sheet
                    .get::<SettingsSheet>()
                    .is_some_and(|s| s.focus.is_focused(window));
                if page_has_keys
                    && !k.modifiers.platform
                    && !k.modifiers.control
                    && !k.modifiers.alt
                    && this.settings_preview_key(&k.key, k.modifiers.shift, window, cx)
                {
                    cx.stop_propagation();
                    return;
                }
                if k.key == "escape" {
                    cx.stop_propagation();
                    this.close_sheet(window, cx);
                }
            }))
            .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                cx.stop_propagation();
                this.close_sheet(window, cx);
            }))
            .child(header)
            .child(
                div()
                    // The rows (fonts, buttons, AI, help…) outgrow the window.
                    .id("settings-scroll")
                    .track_scroll(scroll)
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(
                        div()
                            .debug_selector(|| "settings-content".into())
                            .w_full()
                            .max_w(px(CONTENT_MAX_W))
                            .px(px(CONTENT_PAD))
                            .pt(px(12.))
                            .pb(px(32.))
                            .child(self.render_settings_rows(hotkey, hotkey_error, cx)),
                    ),
            )
            .into_any_element()
    }

    fn render_settings_rows(
        &self,
        hotkey: &Entity<InputState>,
        hotkey_error: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.palette;
        let chip =
            |id: SharedString, label: SharedString, on: bool, family: Option<&'static str>| {
                div()
                    .id(id)
                    .px(px(10.))
                    .py(px(3.))
                    .map(|d| crate::theme_ext::chip(d, &self.theme, on))
                    .cursor_pointer()
                    .when_some(family, |d, f| d.font_family(f))
                    .hover(|s| s.border_color(p.accent).bg(p.hover()))
                    .child(label)
            };

        // --- preview --- built first: it needs `cx` mutably.
        let appearance = self.render_appearance(cx);
        let layouts = LayoutPref::ALL.iter().map(|l| {
            let l = *l;
            chip(
                format!("ly{}", l.value()).into(),
                l.label().into(),
                self.prefs.layout == l,
                None,
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.prefs.layout = l;
                this.apply_prefs(window, cx);
            }))
            .into_any_element()
        });
        let config_path = blyg_core::config::paths::tilde(crate::settings::get(cx).store.primary());
        div()
            .flex()
            .flex_col()
            // --- preview --- the fonts, size and theme, with the specimen.
            .child(appearance)
            .child(row(&p, "LAYOUT", chips().children(layouts)))
            .child(self.render_quiet_setting(cx)) // --- quiet ---
            .child(self.render_buttons_setting(cx)) // --- buttons ---
            .child(self.render_flow_settings(cx)) // --- flow ---
            .child(self.render_nearby_setting(cx)) // --- nearby ---
            .child(self.render_agentic_writer_setting(cx)) // --- agentic writer ---
            .child(self.render_reader_view_settings(cx)) // --- reader view / changelog ---
            .child(row(
                &p,
                "QUICK CAPTURE",
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        crate::theme_ext::field_box(div(), &self.theme)
                            .id("hotkey-box")
                            .max_w(px(320.))
                            .px(px(10.))
                            .py(px(5.))
                            .when(hotkey_error.is_some(), |d| d.border_color(p.over))
                            .text_size(px(13.))
                            .child(gpui_kit::base::input::Input::new(hotkey)),
                    )
                    .child(
                        note(
                            &p,
                            match hotkey_error {
                                Some(e) => e.to_string(),
                                None => crate::keymap::hint_owned(format!(
                                    "Now {} · type e.g. ctrl+alt+b or cmd+shift+space, then ⏎",
                                    prefs::hotkey_glyphs(&self.prefs.hotkey)
                                )),
                            },
                        )
                        .text_size(px(11.5))
                        .when(hotkey_error.is_some(), |d| d.text_color(p.over_text())),
                    ),
            ))
            .child(self.render_ai_settings_row(cx)) // --- AI ---
            .child(self.render_ext_settings_row(cx)) // --- extensions ---
            .child(row(
                &p,
                "CONFIG FILE",
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        chips()
                            .child(
                                chip("open-config".into(), "Open config file".into(), false, None)
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.open_config_file_now(cx)),
                                    ),
                            )
                            .child(
                                chip(
                                    "reload-config".into(),
                                    crate::keymap::hint("Reload  ⌘⇧,").into(),
                                    false,
                                    None,
                                )
                                .on_click(cx.listener(
                                    |this, _, window, cx| {
                                        this.reload_config(&ReloadConfig, window, cx)
                                    },
                                )),
                            ),
                    )
                    .child(note(&p, format!("Saved in {config_path}")).text_size(px(11.5)))
                    .child(
                        note(&p, "Every option: blygger +show-config --default --docs")
                            .text_size(px(11.5)),
                    ),
            ))
            // --- help ---
            .child(self.render_help_settings(cx))
    }
}

/// Under the specimen: how to try an option, as this platform names keys.
pub(crate) const TRY_HINT: crate::platform_text::Text = crate::platform_text::ui_text_per_platform(
    "Rest the pointer on an option, or use ⇥ and the arrow keys, to try it on the whole \
     window. A click or ⏎ keeps it; esc, or moving away, puts yours back.",
    "Rest the pointer on an option, or use Tab and the arrow keys, to try it on the whole \
     window. A click or Enter keeps it; Esc, or moving away, puts yours back.",
);

/// The specimen's caption while an option is being tried.
const KEEP_HINT: crate::platform_text::Text = crate::platform_text::ui_text_per_platform(
    "⏎ keeps · esc goes back",
    "Enter keeps · Esc goes back",
);

/// The keyboard cursor's mark on a chip or the size: a ring in the accent
/// just outside its edge, apart from the kept option's accent edge.
pub(crate) fn focus_ring(p: &Palette) -> Vec<BoxShadow> {
    vec![
        BoxShadow {
            color: p.bg,
            offset: point(px(0.), px(0.)),
            blur_radius: px(0.),
            spread_radius: px(1.),
            inset: false,
        },
        BoxShadow {
            color: p.accent,
            offset: point(px(0.), px(0.)),
            blur_radius: px(0.),
            spread_radius: px(3.),
            inset: false,
        },
    ]
}

// --- preview --- the fonts, size and theme rows, and the specimen.
impl MainView {
    /// WRITING FONT, INTERFACE FONT, SIZE and THEME, with the specimen
    /// beside them (under them when the window is narrow). Resting on a
    /// chip, or the arrow keys, tries it (`pref_preview.rs`); a click or ⏎
    /// keeps it. Chips show the kept option in the accent, and the keyboard
    /// cursor with a ring.
    fn render_appearance(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let kept = self.committed_prefs().clone();
        let cursor = self.preview.cursor.clone();
        let specimen = self.render_specimen(cx);
        // Only fonts that are in use get registered at launch; the chips
        // show each face in itself, so make sure they're all available here.
        for f in crate::prefs::WRITING_FONTS {
            crate::fonts::ensure(f.bundled, cx);
        }
        let pick_chip = |row: Row,
                         pick: Pick,
                         id: String,
                         label: SharedString,
                         on: bool,
                         family: Option<&'static str>,
                         swatch: Option<AnyElement>| {
            let focused = cursor
                .as_ref()
                .is_some_and(|(r, c)| *r == row && *c == pick);
            let sel = id.clone();
            let hover_pick = pick.clone();
            div()
                .id(SharedString::from(id))
                .debug_selector(move || sel.clone())
                .px(px(10.))
                .py(px(3.))
                .map(|d| crate::theme_ext::chip(d, &self.theme, on))
                .when(focused, |d| d.bg(p.bg).shadow(focus_ring(&p)))
                .cursor_pointer()
                .when_some(family, |d, f| d.font_family(f))
                .hover(|s| s.border_color(p.accent).bg(p.hover()))
                .flex()
                .items_center()
                .gap(px(6.))
                .children(swatch)
                .child(label)
                .on_hover(cx.listener(move |this, h: &bool, window, cx| {
                    this.settings_hover(hover_pick.clone(), *h, window, cx)
                }))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.settings_choose(row, pick.clone(), window, cx)
                }))
                .into_any_element()
        };

        // --- fonts ---
        let writing = crate::prefs::WRITING_FONTS
            .iter()
            .enumerate()
            .map(|(i, f)| {
                pick_chip(
                    Row::WritingFont,
                    Pick::WritingFont(f.label),
                    format!("wf{i}"),
                    f.label.into(),
                    kept.writing().label == f.label,
                    Some(f.family),
                    None,
                )
            });
        let ui = crate::prefs::UI_FONTS.iter().enumerate().map(|(i, f)| {
            pick_chip(
                Row::UiFont,
                Pick::UiFont(f.label),
                format!("uf{i}"),
                f.label.into(),
                kept.ui().label == f.label,
                Some(f.family),
                None,
            )
        });

        // --- size --- ± keep a step from what's shown; ←/→ try one.
        let shown = self.prefs.font_size;
        let size_focused = matches!(cursor, Some((Row::Size, _)));
        let step_chip = |id: &'static str, label: &'static str, to: f32| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .px(px(10.))
                .py(px(3.))
                .map(|d| crate::theme_ext::chip(d, &self.theme, false))
                .cursor_pointer()
                .hover(|s| s.border_color(p.accent).bg(p.hover()))
                .child(label)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.settings_choose(Row::Size, Pick::Size(to), window, cx)
                }))
        };
        let size = chips()
            .child(step_chip("sz-", "−", shown - 1.))
            .child(
                div()
                    .debug_selector(|| "sz-value".into())
                    .w(px(46.))
                    .py(px(2.))
                    .rounded(px(4.))
                    .flex()
                    .justify_center()
                    .when(size_focused, |d| d.bg(p.bg).shadow(focus_ring(&p)))
                    .when(shown != kept.font_size, |d| d.text_color(p.accent_text()))
                    .child(format!("{} px", shown)),
            )
            .child(step_chip("sz+", "+", shown + 1.))
            .child(
                div()
                    .ml(px(8.))
                    .text_color(p.muted)
                    .child(crate::keymap::hint("⌘+  ⌘−  ⌘0")),
            );

        // --- themes --- one slot at a time: `theme` (light) or `theme-dark`.
        let dark = self.preview.dark_slot;
        let os_dark = self.preview.os_dark;
        let dark_in_effect = super::pref_preview::dark_slot_in_effect(os_dark, &kept, cx);
        let slot_chip = |slot_dark: bool| {
            let id = if slot_dark {
                "theme-slot-dark"
            } else {
                "theme-slot-light"
            };
            let now = slot_dark == dark_in_effect;
            let label = match (slot_dark, now) {
                (false, true) => "☀ Light · in use",
                (false, false) => "☀ Light",
                (true, true) => "☾ Dark · in use",
                (true, false) => "☾ Dark",
            };
            div()
                .id(id)
                .debug_selector(move || id.into())
                .px(px(10.))
                .py(px(3.))
                .map(|d| crate::theme_ext::chip(d, &self.theme, dark == slot_dark))
                .cursor_pointer()
                .hover(|s| s.border_color(p.accent).bg(p.hover()))
                .child(label)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.set_theme_slot(slot_dark, window, cx)
                }))
        };
        let slot_note = if dark {
            format!(
                "Writes theme-dark: the theme while macOS is dark{}",
                if os_dark {
                    ", as it is now."
                } else {
                    ". macOS is light now, so a theme you try here shows dark anyway."
                }
            )
        } else {
            match kept.theme_dark.as_deref() {
                Some(d) => format!(
                    "Writes theme: the theme while macOS is light. {} is your dark-mode theme.",
                    super::pref_preview::theme_label(Some(d), cx)
                ),
                None => "Writes theme. With no dark-mode theme it stays in dark mode too \
                         (System switches to Ink)."
                    .to_string(),
            }
        };
        let slot_note = if cfg!(target_os = "windows") {
            slot_note.replace("macOS", "Windows")
        } else {
            slot_note
        };
        let kept_theme: Option<String> = if dark {
            kept.theme_dark.clone()
        } else {
            Some(kept.theme.clone())
        };
        let theme_chip = |id: Option<String>, label: String, swatch: Option<AnyElement>| {
            let on = kept_theme == id;
            let sel = match (&id, dark) {
                (None, _) => "thd-same".to_string(),
                (Some(i), false) => format!("th-{i}"),
                (Some(i), true) => format!("thd-{i}"),
            };
            pick_chip(
                Row::Theme,
                Pick::Theme { dark, id },
                sel,
                label.into(),
                on,
                None,
                swatch,
            )
        };
        let groups = crate::theme::themes(cx).grouped();
        let themes = div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .children(groups.into_iter().map(|(family, list)| {
                let mut row_chips: Vec<AnyElement> = Vec::new();
                if family == blyg_core::config::theme::Family::Plain {
                    row_chips.push(if dark {
                        theme_chip(None, "Same as light".into(), None)
                    } else {
                        theme_chip(Some("system".into()), "System".into(), None)
                    });
                }
                for t in list {
                    let label = match t.id.as_str() {
                        "light" => "Light".to_string(),
                        "dark" => "Dark".to_string(),
                        _ => t.name.clone(),
                    };
                    row_chips.push(theme_chip(
                        Some(t.id.clone()),
                        label,
                        Some(crate::ornament::swatch_on(&t, self.palette.bg)),
                    ));
                }
                chips()
                    .child(
                        div()
                            .w(px(52.))
                            .flex_none()
                            .text_size(px(10.5))
                            .text_color(p.muted)
                            .child(family.label()),
                    )
                    .children(row_chips)
            }));
        let theme = div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                chips()
                    .child(
                        div()
                            .w(px(52.))
                            .flex_none()
                            .text_size(px(10.5))
                            .text_color(p.muted)
                            .child("For"),
                    )
                    .child(slot_chip(false))
                    .child(slot_chip(true)),
            )
            .child(note(&p, slot_note))
            .child(themes);

        let keys = note(&p, TRY_HINT.get()).debug_selector(|| "settings-try-hint".into());

        // The specimen leads, at the content column's width, so the chip
        // rows under it keep theirs (a column beside them squeezed every
        // row to two chips a line).
        div()
            .debug_selector(|| "settings-appearance".into())
            .w_full()
            .flex()
            .flex_col()
            .child(row(
                &p,
                "PREVIEW",
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(specimen)
                    .child(keys),
            ))
            .child(row(&p, "WRITING FONT", chips().children(writing)))
            .child(row(&p, "INTERFACE FONT", chips().children(ui)))
            .child(row(&p, "SIZE", size))
            .child(row(&p, "THEME", theme))
            .into_any_element()
    }

    /// A small, real sample of the window in the theme, fonts and size
    /// shown now (the one being tried, else yours): the title bar, a posts
    /// list with the selected row and an unread mark, a post's title, a
    /// paragraph with a link and a code span, a quote box, a row of chips
    /// and the status bar. Drawn with the app's own pieces (the row and
    /// quote ornaments, chips, the unread marker, the status palette), so
    /// a theme's shapes show as well as its colours.
    fn render_specimen(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = &*self.theme;
        let p = self.palette;
        let chrome = t.chrome_font(&self.prefs);
        let writing = self.prefs.writing().family;
        let ui = self.prefs.ui().family;
        // The editor's size, at three quarters: a model of the window.
        let body = (self.prefs.font_size * 0.75).round().max(9.);
        let list = (self.prefs.list_size() * 0.8).round().max(9.);
        let side = p.on_side();
        let sel = p.on_sel();
        let status = p.on_status();
        let bar = p.on_bar();

        let titlebar = div()
            .h(px(22.))
            .flex_none()
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(5.))
            .bg(p.bar)
            .border_b_1()
            .border_color(p.bar_line)
            .font_family(chrome)
            .text_size(px(10.))
            .text_color(p.bar_ink)
            .children((0..3).map(|_| div().size(px(6.)).rounded_full().bg(bar.muted.opacity(0.5))))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .justify_center()
                    .child("Burrow — blyg.example.com"),
            );

        let list_row = |id: &'static str,
                        title: &'static str,
                        meta: &'static str,
                        selected: bool,
                        unread: bool| {
            let rp = if selected { sel } else { side };
            let cell = crate::ornament::row(
                t,
                div().id(id).py(px(4.)).flex().flex_col().text_color(rp.ink),
                selected,
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .font_family(ui)
                    .text_size(px(list))
                    .child(div().truncate().child(title))
                    .when(unread, |d| d.child(self.unread_marker(rp.accent))),
            )
            .child(
                div()
                    .font_family(chrome)
                    .text_size(px(9.5))
                    .text_color(rp.muted)
                    .child(meta),
            );
            crate::ornament::row_wrap(t, cell)
        };
        let sidebar = div()
            .w(relative(0.3))
            .flex_none()
            .flex()
            .flex_col()
            .py(px(4.))
            .bg(p.side)
            .rule_r(&p)
            .child(list_row(
                "spec-row-1",
                "Field notes",
                "thread · v3",
                true,
                false,
            ))
            .child(list_row(
                "spec-row-2",
                "Two doors",
                "fragment · new",
                false,
                true,
            ))
            .child(list_row("spec-row-3", "Harbour log", "draft", false, false));

        let para = "The tide came in early. Ada's harbour log says the same, and her post \
                    ![[01J9K7T]] quotes it.";
        let link = para.find("Ada's harbour log").unwrap_or(0);
        let code = para.find("![[01J9K7T]]").unwrap_or(0);
        let hl = vec![
            (
                link..link + "Ada's harbour log".len(),
                HighlightStyle {
                    color: Some(p.accent),
                    underline: Some(UnderlineStyle {
                        thickness: px(1.),
                        color: Some(p.accent.opacity(0.5)),
                        wavy: false,
                    }),
                    ..Default::default()
                },
            ),
            (
                code..code + "![[01J9K7T]]".len(),
                HighlightStyle {
                    background_color: Some(crate::theme::mix(p.editor, p.ink, 0.08)),
                    ..Default::default()
                },
            ),
        ];
        let quote = crate::ornament::quote_frame(
            t,
            div()
                .id("spec-quote")
                .text_size(px((body * 0.9).round()))
                .child("Every burrow has two doors.")
                .child(
                    div()
                        .pt(px(2.))
                        .font_family(chrome)
                        .text_size(px(9.5))
                        .text_color(p.muted)
                        .child("— rue.example.org"),
                ),
        );
        let editor = div()
            .flex_1()
            .min_w_0()
            .px(px(12.))
            .py(px(10.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .bg(p.editor)
            .text_color(p.ink)
            .font_family(writing)
            .text_size(px(body))
            .line_height(relative(1.35))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px((body * 1.2).round()))
                    .child("Field notes"),
            )
            .child(div().child(StyledText::new(para).with_highlights(hl)))
            .child(quote);

        let chip = |label: &'static str, on: bool| {
            div()
                .px(px(8.))
                .py(px(1.))
                .map(|d| crate::theme_ext::chip(d, t, on))
                .child(label)
        };
        let chip_row = div()
            .px(px(10.))
            .py(px(6.))
            .flex()
            .flex_wrap()
            .gap(px(5.))
            .bg(p.bg)
            .border_t_1()
            .border_color(p.divider)
            .font_family(chrome)
            .text_size(px(10.))
            .text_color(p.ink)
            .child(chip("Reply", true))
            .child(chip("Quote into a thread", false))
            .child(chip("Link post", false));

        let status_bar = div()
            .h(px(20.))
            .flex_none()
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .bg(p.status)
            .border_t_1()
            .border_color(p.status_line)
            .font_family(chrome)
            .text_size(px(9.5))
            .text_color(status.muted)
            .child("Thread")
            .child("1,204")
            .child(div().flex_1())
            .child(
                div()
                    .size(px(6.))
                    .rounded_full()
                    .bg(crate::theme_ext::readable(
                        p.green,
                        p.status,
                        crate::theme_ext::MARK,
                    )),
            )
            .child(div().text_color(status.ink).child("synced"));

        let trying = self.preview.trying().map(|pick| match pick {
            Pick::Theme { dark, id } => format!(
                "Trying {}{}",
                super::pref_preview::theme_label(id.as_deref(), cx),
                if *dark { " for dark mode" } else { "" }
            ),
            Pick::WritingFont(l) => format!("Trying {l} for writing"),
            Pick::UiFont(l) => format!("Trying {l} for the interface"),
            Pick::Size(s) => format!("Trying {s} px"),
            Pick::Rule(_) | Pick::CountRead(_) => String::new(), // never tried
        });
        let caption = div()
            .debug_selector(|| "settings-specimen-caption".into())
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x(px(6.))
            .text_size(px(10.5))
            .child(match trying {
                Some(s) => div()
                    .text_color(p.accent_text())
                    .child(format!("{s} · {}", KEEP_HINT.get()))
                    .into_any_element(),
                None => div()
                    .text_color(p.muted)
                    .child(format!("{} · yours", t.name))
                    .into_any_element(),
            });

        div()
            .debug_selector(|| "settings-specimen".into())
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(caption)
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .rounded(px(t.corner(8.)))
                    .border_1()
                    .border_color(p.edge())
                    .child(titlebar)
                    .child(div().flex().child(sidebar).child(editor))
                    .child(chip_row)
                    .child(status_bar),
            )
            .into_any_element()
    }
}
