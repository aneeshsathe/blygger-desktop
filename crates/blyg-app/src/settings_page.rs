//! Settings (⌘,): a page over the whole window below the title bar, not a
//! pop-up. Every section draws its rows with `row` and its explanations with
//! `note`, so nothing runs past the page's right edge: the content column
//! shrinks with the window (`min_w_0`), chip rows wrap, and prose wraps.

use gpui_kit::base::input::{Escape, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{MainView, OMNI_H, ReloadConfig, TITLEBAR_H};
use crate::prefs::{self, LayoutPref};
use crate::theme::{Palette, Rule};

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
                let env = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<f32>().ok());
                let now = window.viewport_size();
                let w = env("BLYGGER_SNAPSHOT_WIDTH").map(px).unwrap_or(now.width);
                let h = env("BLYGGER_SNAPSHOT_HEIGHT").map(px).unwrap_or(now.height);
                window.resize(size(w, h));
                self.open_settings(&super::OpenSettings, window, cx);
            }
            1 => {
                self.reading.changelogs.settings = super::reading::Load::Ready(Default::default());
                cx.notify();
            }
            // `BLYGGER_DEMO_SCROLL=bottom`: the end of the page.
            2 if std::env::var("BLYGGER_DEMO_SCROLL").as_deref() == Ok("bottom") => {
                if let Some(super::Sheet::Settings { scroll, .. }) = &self.sheet {
                    scroll.scroll_to_bottom();
                }
                cx.notify();
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
                if ev.keystroke.key == "escape" {
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

        let writing = crate::prefs::WRITING_FONTS
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let label = f.label;
                chip(
                    format!("wf{i}").into(),
                    label.into(),
                    self.prefs.writing().label == label,
                    Some(f.family),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.prefs.writing_font = label.to_string();
                    this.prefs.writing_font_set = true;
                    this.apply_prefs(window, cx);
                }))
                .into_any_element()
            });
        let ui = crate::prefs::UI_FONTS.iter().enumerate().map(|(i, f)| {
            let label = f.label;
            chip(
                format!("uf{i}").into(),
                label.into(),
                self.prefs.ui().label == label,
                Some(f.family),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.prefs.ui_font = label.to_string();
                this.prefs.ui_font_set = true;
                this.apply_prefs(window, cx);
            }))
            .into_any_element()
        });
        // --- themes --- every theme, grouped, each with a small swatch.
        let theme_chip = |id: String, label: String, swatch: Option<AnyElement>| {
            let on = self.prefs.theme == id;
            let sel = format!("th-{id}");
            chip(sel.clone().into(), label.into(), on, None)
                .debug_selector(move || sel.clone())
                .flex()
                .items_center()
                .gap(px(6.))
                .children(swatch)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.prefs.theme = id.clone();
                    this.apply_prefs(window, cx);
                }))
                .into_any_element()
        };
        let groups = crate::theme::themes(cx).grouped();
        let themes = div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .children(groups.into_iter().map(|(family, list)| {
                let mut chips: Vec<AnyElement> = Vec::new();
                if family == blyg_core::config::theme::Family::Plain {
                    chips.push(theme_chip("system".into(), "System".into(), None));
                }
                for t in list {
                    let label = match t.id.as_str() {
                        "light" => "Light".to_string(),
                        "dark" => "Dark".to_string(),
                        _ => t.name.clone(),
                    };
                    chips.push(theme_chip(
                        t.id.clone(),
                        label,
                        Some(crate::ornament::swatch_on(&t, self.palette.bg)),
                    ));
                }
                self::chips()
                    .child(
                        div()
                            .w(px(52.))
                            .flex_none()
                            .text_size(px(10.5))
                            .text_color(p.muted)
                            .child(family.label()),
                    )
                    .children(chips)
            }));
        let theme_dark_note = self.prefs.theme_dark.clone().map(|d| {
            note(
                &p,
                crate::keymap::hint_owned(format!("theme-dark = {d} is used while macOS is dark")),
            )
        });
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
        // Only fonts that are in use get registered at launch; the chips show
        // each face in itself, so make sure they're all available here.
        for f in crate::prefs::WRITING_FONTS {
            crate::fonts::ensure(f.bundled, cx);
        }

        div()
            .flex()
            .flex_col()
            .child(row(&p, "WRITING FONT", chips().children(writing)))
            .child(row(&p, "INTERFACE FONT", chips().children(ui)))
            .child(row(
                &p,
                "SIZE",
                chips()
                    .child(
                        chip("sz-".into(), "−".into(), false, None).on_click(cx.listener(
                            |this, _, window, cx| {
                                this.prefs.smaller();
                                this.apply_prefs(window, cx);
                            },
                        )),
                    )
                    .child(
                        div()
                            .w(px(46.))
                            .flex()
                            .justify_center()
                            .child(format!("{} px", self.prefs.font_size)),
                    )
                    .child(
                        chip("sz+".into(), "+".into(), false, None).on_click(cx.listener(
                            |this, _, window, cx| {
                                this.prefs.bigger();
                                this.apply_prefs(window, cx);
                            },
                        )),
                    )
                    .child(
                        div()
                            .ml(px(8.))
                            .text_color(p.muted)
                            .child(crate::keymap::hint("⌘+  ⌘−  ⌘0")),
                    ),
            ))
            .child(row(
                &p,
                "THEME",
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(themes)
                    .children(theme_dark_note),
            ))
            .child(row(&p, "LAYOUT", chips().children(layouts)))
            .child(self.render_buttons_setting(cx)) // --- buttons ---
            .child(self.render_chat_setting(cx)) // --- chat ---
            .child(self.render_flow_settings(cx)) // --- flow ---
            .child(self.render_nearby_setting(cx)) // --- nearby ---
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
                                None => format!(
                                    "Now {} · type e.g. ctrl+alt+b or cmd+shift+space, then ⏎",
                                    prefs::hotkey_glyphs(&self.prefs.hotkey)
                                ),
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
