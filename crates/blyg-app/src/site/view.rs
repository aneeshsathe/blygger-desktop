//! --- site --- The Site sheet's drawing: a section list on the left, the
//! section on the right, Save at the foot.

use blyg_core::site::{self, CustomTheme, SiteConfig, THEME_COLOR_LABELS, THEME_COLORS};
use gpui_kit::base::input::{Escape, Input, Textarea};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{Load, Pick, Section, Sheet};
use crate::app::{MainView, TITLEBAR_H};
use crate::theme::Palette;

/// `#rrggbb` as a colour (grey when it isn't one).
fn hex(s: &str) -> Hsla {
    match site::hex_rgb(s) {
        Some((r, g, b)) => rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32).into(),
        None => rgb(0x888888).into(),
    }
}

/// The face Burrow previews a Studio typeface with.
fn face(id: &str) -> SharedString {
    let f = site::font(id).unwrap_or(&site::FONTS[0]);
    if cfg!(target_os = "windows") {
        f.windows.into()
    } else {
        f.mac.into()
    }
}

impl MainView {
    pub(crate) fn render_site_overlay(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let s = self.site.sheet.as_ref()?;
        let p = self.palette.on_page();
        let h = (window.viewport_size().height - px(TITLEBAR_H) - px(40.)).min(px(640.));
        let body: AnyElement = match &s.load {
            Load::Loading => div()
                .mt(px(12.))
                .text_color(p.muted)
                .child("Loading…")
                .into_any_element(),
            Load::Unavailable => div()
                .mt(px(12.))
                .child("Your blyg doesn't offer its site settings here.")
                .child(
                    div()
                        .mt(px(4.))
                        .text_color(p.muted)
                        .child("Update its studio to set them from Burrow."),
                )
                .into_any_element(),
            Load::Failed(e) => div()
                .mt(px(12.))
                .text_color(p.over)
                .child(format!("Couldn't load them: {e}"))
                .into_any_element(),
            Load::Ready => self.site_body(s, p, h, cx),
        };
        Some(
            div()
                .absolute()
                .top(px(TITLEBAR_H))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .id("site-sheet")
                        .occlude()
                        .track_focus(&s.focus)
                        .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                            let k = &ev.keystroke;
                            if k.key == "escape" {
                                cx.stop_propagation();
                                this.close_site(window, cx);
                            } else if k.key == "enter" && k.modifiers.secondary() {
                                cx.stop_propagation();
                                this.save_site(window, cx);
                            }
                        }))
                        .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                            cx.stop_propagation();
                            this.close_site(window, cx);
                        }))
                        .w(px(780.))
                        .max_w(relative(0.94))
                        .map(|d| self.sheet_style(d))
                        .text_color(p.ink)
                        .child(
                            div()
                                .flex()
                                .items_baseline()
                                .gap(px(8.))
                                .child(div().font_weight(FontWeight::SEMIBOLD).child("Site"))
                                .child(div().text_color(p.muted).text_size(px(12.)).child(
                                    "How your blyg's public pages look and what they list. Saved to the blyg on Save.",
                                )),
                        )
                        .child(body)
                        .with_animation(
                            ("site-sheet-in", self.site.sheet_gen),
                            Animation::new(std::time::Duration::from_millis(220))
                                .with_easing(ease_out_quint()),
                            |d, t| d.mt(px(-240.0 * (1.0 - t))).opacity(t.min(1.0) * 0.4 + 0.6),
                        ),
                )
                .into_any_element(),
        )
    }

    fn site_body(&self, s: &Sheet, p: Palette, h: Pixels, cx: &mut Context<Self>) -> AnyElement {
        let nav = div()
            .flex_none()
            .w(px(128.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .pr(px(10.))
            .border_r_1()
            .border_color(p.line)
            .children(
                Section::ALL
                    .into_iter()
                    .filter(|x| x.shown(&s.form))
                    .map(|x| {
                        let on = s.section == x;
                        div()
                            .id(SharedString::from(format!("site-nav-{}", x.label())))
                            .px(px(8.))
                            .py(px(4.))
                            .rounded(px(self.theme.corner(5.)))
                            .cursor_pointer()
                            .when(on, |d| d.bg(p.sel).font_weight(FontWeight::MEDIUM))
                            .text_color(if on { p.ink } else { p.muted })
                            .hover(|d| d.text_color(p.ink))
                            .child(x.label())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(s) = this.site.sheet.as_mut() {
                                    s.section = x;
                                    s.picking = None;
                                }
                                cx.notify();
                            }))
                    }),
            );
        let section = match s.section {
            Section::Appearance => self.site_appearance(s, p, cx),
            Section::HomePage => self.site_home(s, p, cx),
            Section::Links => self.site_links(s, p, cx),
            Section::Profile => self.site_profile(s, p, cx),
            Section::Network => self.site_network(s, p, cx),
            Section::Updates => self.site_updates(s, p, cx),
            Section::Extensions => self.site_extensions(s, p, cx),
        };
        let dirty = !s.form.patch(&s.base).is_empty() || self.site_fields_changed(s, cx);
        div()
            .mt(px(10.))
            .flex()
            .flex_col()
            .child(
                div().h(h - px(110.)).flex().child(nav).child(
                    div()
                        .id("site-section")
                        .flex_1()
                        .min_w_0()
                        .pl(px(14.))
                        .pr(px(6.))
                        .overflow_y_scroll()
                        .child(section),
                ),
            )
            .when_some(s.error.clone(), |d, e| {
                d.child(div().mt(px(8.)).text_color(p.over).child(e))
            })
            .child(
                div()
                    .mt(px(10.))
                    .pt(px(10.))
                    .border_t_1()
                    .border_color(p.line)
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .text_color(p.muted)
                    .child(
                        self.site_chip("site-save", if s.busy { "Saving…" } else { "Save" }, p)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.save_site(window, cx)),
                            ),
                    )
                    .child(self.site_key("⌘⏎", "save"))
                    .child(self.site_key("esc", "close"))
                    .child(div().flex_1())
                    .child(if s.saved && !dirty {
                        div().text_color(p.green).child("Saved")
                    } else if dirty {
                        div().child("Unsaved changes")
                    } else {
                        div()
                    }),
            )
            .into_any_element()
    }

    /// The text fields differ from what was read.
    fn site_fields_changed(&self, s: &Sheet, cx: &App) -> bool {
        let t =
            |e: &Entity<gpui_kit::base::input::InputState>| e.read(cx).value().trim().to_string();
        let b = |v: &Option<String>| v.clone().unwrap_or_default();
        (s.form.author_url.is_some() && t(&s.author_url) != b(&s.base.author_url))
            || (s.form.site_url.is_some() && t(&s.site_url) != b(&s.base.site_url))
            || (s.form.posts_path.is_some() && t(&s.posts_path) != b(&s.base.posts_path))
            || (s.form.update_feed_url.is_some() && t(&s.feed_url) != b(&s.base.update_feed_url))
    }

    // ------------------------------------------------------------ helpers

    fn site_chip(
        &self,
        id: &'static str,
        label: impl Into<SharedString>,
        p: Palette,
    ) -> Stateful<Div> {
        div()
            .id(id)
            .px(px(10.))
            .py(px(3.))
            .rounded(px(self.theme.chip_radius))
            .map(|d| crate::ornament::border(d, self.theme.chip_border, p.line))
            .cursor_pointer()
            .font_family(self.chrome())
            .text_size(px(11.5))
            .font_weight(FontWeight::MEDIUM)
            .text_color(p.ink)
            .hover(|s| s.border_color(p.accent))
            .child(label.into())
    }

    fn site_key(&self, k: &'static str, label: &'static str) -> Div {
        let p = self.palette.on_page();
        div()
            .flex()
            .items_center()
            .gap(px(5.))
            .child(
                div()
                    .px(px(6.))
                    .py(px(1.))
                    .rounded(px(self.theme.corner(5.)))
                    .border_1()
                    .border_b_2()
                    .border_color(p.line)
                    .text_color(p.ink)
                    .text_size(px(11.5))
                    .child(crate::keymap::hint(k)),
            )
            .child(label)
    }

    fn site_heading(&self, s: &'static str, p: Palette) -> Div {
        div()
            .mt(px(14.))
            .mb(px(4.))
            .text_size(px(10.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(p.muted)
            .child(s.to_uppercase())
    }

    fn site_hint(&self, s: impl Into<SharedString>, p: Palette) -> Div {
        div()
            .mt(px(2.))
            .text_size(px(12.))
            .text_color(p.muted)
            .whitespace_normal()
            .child(s.into())
    }

    fn site_field(&self, el: AnyElement, error: bool, p: Palette) -> Div {
        div()
            .px(px(10.))
            .py(px(6.))
            .rounded(px(self.theme.corner(7.)))
            .border_1()
            .border_color(if error { p.over } else { p.line })
            .text_size(px(13.))
            .child(el)
    }

    /// A checkbox row; `f` flips it in the form.
    fn site_check(
        &self,
        id: &'static str,
        on: bool,
        label: &'static str,
        hint: Option<&'static str>,
        cx: &mut Context<Self>,
        f: fn(&mut SiteConfig),
    ) -> Stateful<Div> {
        let p = self.palette.on_page();
        div()
            .id(id)
            .mt(px(8.))
            .flex()
            .gap(px(8.))
            .cursor_pointer()
            .child(if on { "☑" } else { "☐" })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(label)
                    .when_some(hint, |d, h| d.child(self.site_hint(h, p))),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.site_edit(cx, |s| f(&mut s.form))))
    }

    fn site_radio(
        &self,
        id: SharedString,
        on: bool,
        label: impl Into<SharedString>,
    ) -> Stateful<Div> {
        div()
            .id(id)
            .flex()
            .gap(px(6.))
            .cursor_pointer()
            .child(if on { "◉" } else { "○" })
            .child(label.into())
    }

    // ------------------------------------------------------------ Appearance

    /// A preview of the public page in a palette and face (Studio's
    /// preview: the sheet, a byline, prose, a link, a generated passage).
    fn site_preview(&self, t: &CustomTheme, font: &str, title: &str, label: &str) -> Div {
        let face = face(font);
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(3.))
            .child(div().text_size(px(10.5)).text_color(self.palette.on_page().muted).child(label.to_string()))
            .child(
                div()
                    .p(px(10.))
                    .rounded(px(6.))
                    .bg(hex(&t.page))
                    .child(
                        div()
                            .p(px(12.))
                            .rounded(px(3.))
                            .bg(hex(&t.paper))
                            .border_1()
                            .border_color(hex(&t.rule))
                            .flex()
                            .flex_col()
                            .gap(px(5.))
                            .child(
                                div()
                                    .font_family(face.clone())
                                    .text_size(px(16.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(hex(&t.ink))
                                    .child(if title.is_empty() { "Your blyg".to_string() } else { title.to_string() }),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(hex(&t.ink_soft))
                                    .child("7 November · a thread · v3"),
                            )
                            .child(div().h(px(1.)).bg(hex(&t.rule)))
                            .child(
                                div()
                                    .font_family(face.clone())
                                    .text_size(px(13.5))
                                    .text_color(hex(&t.ink))
                                    .whitespace_normal()
                                    .child("Cutting back is how a garden keeps its shape; what stays is what you chose."),
                            )
                            .child(
                                div()
                                    .font_family(face.clone())
                                    .text_size(px(13.5))
                                    .text_color(hex(&t.pencil))
                                    .underline()
                                    .child("Read the whole thread"),
                            )
                            .child(
                                div()
                                    .px(px(6.))
                                    .py(px(3.))
                                    .rounded(px(3.))
                                    .bg(hex(&t.gen_bg))
                                    .border_1()
                                    .border_color(hex(&t.gen_rule))
                                    .font_family(face)
                                    .text_size(px(13.))
                                    .text_color(hex(&t.ink))
                                    .child("A passage written by [TK] generation."),
                            ),
                    ),
            )
    }

    fn site_swatch(
        &self,
        id: &str,
        label: &str,
        light: CustomTheme,
        dark: Option<CustomTheme>,
        on: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let p = self.palette.on_page();
        let half = |a: &str, b: Option<&str>| match b {
            Some(b) => div()
                .flex()
                .size_full()
                .child(div().flex_1().h_full().bg(hex(a)))
                .child(div().flex_1().h_full().bg(hex(b))),
            None => div().size_full().bg(hex(a)),
        };
        let d = dark.as_ref();
        let id_owned = id.to_string();
        div()
            .id(SharedString::from(format!("site-theme-{id}")))
            .w(px(76.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(3.))
            .cursor_pointer()
            .child(
                div()
                    .w(px(72.))
                    .h(px(48.))
                    .rounded(px(5.))
                    .border_2()
                    .border_color(if on { p.accent } else { p.line })
                    .overflow_hidden()
                    .relative()
                    .child(half(&light.page, d.map(|t| t.page.as_str())))
                    .child(
                        div()
                            .absolute()
                            .top(px(8.))
                            .left(px(12.))
                            .w(px(44.))
                            .h(px(30.))
                            .rounded(px(2.))
                            .overflow_hidden()
                            .child(half(&light.paper, d.map(|t| t.paper.as_str())))
                            .child(
                                div()
                                    .absolute()
                                    .top(px(8.))
                                    .left(px(6.))
                                    .w(px(30.))
                                    .h(px(3.))
                                    .bg(hex(&light.ink)),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top(px(16.))
                                    .left(px(6.))
                                    .w(px(18.))
                                    .h(px(3.))
                                    .bg(hex(&light.pencil)),
                            ),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(if on { p.ink } else { p.muted })
                    .truncate()
                    .max_w(px(76.))
                    .child(label.to_string()),
            )
            .on_click(
                cx.listener(move |this, _, window, cx| {
                    this.site_choose_theme(&id_owned, window, cx)
                }),
            )
    }

    fn site_appearance(&self, s: &Sheet, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let f = &s.form;
        let theme = f.theme.clone().unwrap_or_else(|| "auto".into());
        let custom = f.custom_theme.clone().flatten();
        let custom_dark = f.custom_theme_dark.clone().flatten();
        let font = f.font.clone().unwrap_or_else(|| site::DEFAULT_FONT.into());
        let mut swatches = vec![self.site_swatch(
            "auto",
            "Auto",
            site::auto_palette(false),
            Some(site::auto_palette(true)),
            theme == "auto",
            cx,
        )];
        for pr in site::PRESETS {
            swatches.push(self.site_swatch(
                pr.id,
                pr.label,
                pr.palette(),
                None,
                theme == pr.id,
                cx,
            ));
        }
        if f.custom_theme.is_some() {
            let name = f
                .custom_theme_name
                .clone()
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| "Custom".into());
            swatches.push(self.site_swatch(
                site::CUSTOM_THEME_ID,
                &name,
                custom.clone().unwrap_or_else(|| site::auto_palette(false)),
                custom_dark.clone(),
                theme == site::CUSTOM_THEME_ID,
                cx,
            ));
        }
        // The preview: the palette readers get (both, where it follows
        // their light/dark preference).
        let (one, two) = match theme.as_str() {
            "auto" => (site::auto_palette(false), Some(site::auto_palette(true))),
            site::CUSTOM_THEME_ID => match (&custom, &custom_dark) {
                (Some(l), Some(d)) => (l.clone(), Some(d.clone())),
                (Some(l), None) => (l.clone(), None),
                _ => (site::auto_palette(false), None),
            },
            id => (site::custom_from_preset(id), None),
        };
        let preview = div()
            .mt(px(6.))
            .flex()
            .gap(px(10.))
            .child(self.site_preview(
                &one,
                &font,
                &s.title,
                if two.is_some() { "Light" } else { "Preview" },
            ))
            .when_some(two, |d, t| {
                d.child(self.site_preview(&t, &font, &s.title, "Dark"))
            });
        div()
            .when(f.theme.is_some(), |d| {
                d.child(self.site_heading("Theme", p))
                    .child(self.site_hint("Your public pages (and Studio). Burrow's own look is in Settings.", p))
                    .child(div().mt(px(6.)).flex().flex_wrap().gap(px(8.)).children(swatches))
                    .child(preview)
            })
            .when(theme == site::CUSTOM_THEME_ID && custom.is_some(), |d| {
                d.child(self.site_custom_editor(s, p, cx))
            })
            .when(f.font.is_some(), |d| {
                d.child(self.site_heading("Typeface", p))
                    .child(self.site_hint(
                        "Fonts already on the reader's device, so nothing is downloaded and no one else sees your readers. It looks a little different from one device to the next.",
                        p,
                    ))
                    .child(
                        div().mt(px(6.)).flex().flex_wrap().gap(px(8.)).children(site::FONTS.iter().map(|fo| {
                            let on = font == fo.id;
                            let id = fo.id;
                            div()
                                .id(SharedString::from(format!("site-font-{id}")))
                                .w(px(140.))
                                .px(px(8.))
                                .py(px(6.))
                                .rounded(px(self.theme.corner(6.)))
                                .border_1()
                                .border_color(if on { p.accent } else { p.line })
                                .cursor_pointer()
                                .child(div().font_family(face(id)).text_size(px(14.)).truncate().child("The quick brown fox"))
                                .child(div().text_size(px(11.)).text_color(p.muted).child(fo.label))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.site_edit(cx, |s| s.form.font = Some(id.into()))
                                }))
                        })),
                    )
            })
            .into_any_element()
    }

    fn site_custom_editor(&self, s: &Sheet, p: Palette, cx: &mut Context<Self>) -> Div {
        let f = &s.form;
        let has_dark = f.custom_theme_dark.as_ref().is_some_and(|t| t.is_some());
        let editing = if s.editing_dark && has_dark {
            f.custom_theme_dark.clone().flatten()
        } else {
            f.custom_theme.clone().flatten()
        };
        let Some(t) = editing else {
            return div();
        };
        let fields = if s.editing_dark && has_dark {
            &s.dark
        } else {
            &s.light
        };
        let warnings = site::contrast_warnings(&t);
        let name_err = site::theme_name_error(s.theme_name.read(cx).value().as_ref());
        let tab = |id: &'static str, label: &'static str, on: bool, dark: bool| {
            div()
                .id(id)
                .px(px(8.))
                .py(px(2.))
                .rounded(px(4.))
                .cursor_pointer()
                .when(on, |d| d.bg(p.sel).text_color(p.ink))
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(s) = this.site.sheet.as_mut() {
                        s.editing_dark = dark;
                    }
                    cx.notify();
                }))
        };
        let rows = THEME_COLORS.iter().enumerate().map(|(i, slot)| {
            let v = t.get(slot).to_string();
            let typed = fields[i].read(cx).value().to_string();
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .mt(px(4.))
                .child(
                    div()
                        .size(px(18.))
                        .rounded(px(3.))
                        .border_1()
                        .border_color(p.line)
                        .bg(hex(&v)),
                )
                .child(div().w(px(110.)).child(self.site_field(
                    Input::new(&fields[i]).into_any_element(),
                    !site::is_hex(&typed),
                    p,
                )))
                .child(
                    div()
                        .text_color(p.muted)
                        .text_size(px(12.))
                        .child(THEME_COLOR_LABELS[i]),
                )
        });
        let dark_flag = t.dark;
        div()
            .mt(px(8.))
            .p(px(10.))
            .rounded(px(self.theme.corner(7.)))
            .border_1()
            .border_color(p.line)
            .child(div().font_weight(FontWeight::MEDIUM).child("Custom theme"))
            .child(self.site_heading("Name (shown when it's shared)", p))
            .child(self.site_field(Input::new(&s.theme_name).into_any_element(), name_err.is_some(), p))
            .when_some(name_err, |d, e| d.child(div().text_color(p.over).text_size(px(12.)).child(e)))
            .child(
                div()
                    .mt(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_color(p.muted)
                    .child(tab("site-edit-light", if has_dark { "Light" } else { "Palette" }, !s.editing_dark || !has_dark, false))
                    .when(has_dark, |d| d.child(tab("site-edit-dark", "Dark companion", s.editing_dark, true)))
                    .child(div().flex_1())
                    .child(
                        self.site_chip("site-dark-companion", if has_dark { "Remove dark companion" } else { "Add a dark companion" }, p)
                            .on_click(cx.listener(|this, _, window, cx| this.site_toggle_dark_companion(window, cx))),
                    ),
            )
            .when(has_dark, |d| {
                d.child(self.site_hint(
                    "With a dark companion your pages follow the reader's light or dark preference, as Auto does.",
                    p,
                ))
            })
            .children(rows)
            .when(!has_dark, |d| {
                d.child(
                    div()
                        .id("site-palette-dark")
                        .mt(px(8.))
                        .flex()
                        .gap(px(8.))
                        .cursor_pointer()
                        .child(if dark_flag { "☑" } else { "☐" })
                        .child("A dark palette (pins the page's colour scheme to dark)")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.site_edit(cx, |s| {
                                if let Some(Some(t)) = s.form.custom_theme.as_mut() {
                                    t.dark = !t.dark;
                                }
                            })
                        })),
                )
            })
            .child(
                div()
                    .mt(px(8.))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(6.))
                    .text_color(p.muted)
                    .text_size(px(12.))
                    .child("Start from:")
                    .children(site::PRESETS.iter().map(|pr| {
                        let id = pr.id;
                        div()
                            .id(SharedString::from(format!("site-from-{id}")))
                            .cursor_pointer()
                            .text_color(p.accent)
                            .hover(|d| d.underline())
                            .child(pr.label)
                            .on_click(cx.listener(move |this, _, window, cx| this.site_start_from(id, window, cx)))
                    })),
            )
            .when(!warnings.is_empty(), |d| {
                d.child(
                    div()
                        .mt(px(8.))
                        .text_size(px(12.))
                        .text_color(p.amber)
                        .child("Low contrast (WCAG AA asks 4.5:1 for text):")
                        .children(warnings.into_iter().map(|(l, r)| div().child(format!("· {l}: {r:.1}:1")))),
                )
            })
    }

    // ------------------------------------------------------------ Home page

    fn site_threads(&self) -> Vec<(String, String)> {
        super::published_threads(&self.backend.items())
    }

    fn site_thread_picker(&self, s: &Sheet, p: Palette, cx: &mut Context<Self>) -> Div {
        let q = s.thread_q.read(cx).value().to_lowercase();
        let taken: Vec<String> = match s.picking {
            Some(Pick::MenuThread) => s.form.links_threads.clone().unwrap_or_default(),
            _ => s.form.front_page.clone().into_iter().collect(),
        };
        let threads: Vec<(String, String)> = self
            .site_threads()
            .into_iter()
            .filter(|(id, t)| {
                !taken.contains(id) && (q.is_empty() || t.to_lowercase().contains(&q))
            })
            .take(8)
            .collect();
        let empty = threads.is_empty();
        div()
            .mt(px(6.))
            .p(px(8.))
            .rounded(px(self.theme.corner(7.)))
            .border_1()
            .border_color(p.line)
            .child(self.site_field(Input::new(&s.thread_q).into_any_element(), false, p))
            .children(threads.into_iter().map(|(id, title)| {
                let id2 = id.clone();
                div()
                    .id(SharedString::from(format!("site-pick-{id}")))
                    .mt(px(3.))
                    .px(px(6.))
                    .py(px(2.))
                    .rounded(px(4.))
                    .cursor_pointer()
                    .hover(|d| d.bg(p.sel))
                    .truncate()
                    .child(title)
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.site_pick_thread(id2.clone(), cx)),
                    )
            }))
            .when(empty, |d| {
                d.child(
                    div()
                        .mt(px(4.))
                        .text_color(p.muted)
                        .child("No published threads match."),
                )
            })
            .child(
                div()
                    .id("site-pick-cancel")
                    .mt(px(4.))
                    .cursor_pointer()
                    .text_color(p.muted)
                    .hover(|d| d.text_color(p.ink))
                    .child("Cancel")
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(s) = this.site.sheet.as_mut() {
                            s.picking = None;
                        }
                        cx.notify();
                    })),
            )
    }

    fn site_title_of(&self, id: &str) -> String {
        self.site_threads()
            .into_iter()
            .find(|(t, _)| t == id)
            .map(|(_, title)| title)
            .unwrap_or_else(|| format!("{id} (not a published thread here)"))
    }

    fn site_home(&self, s: &Sheet, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let f = &s.form;
        let orders = f.feed_orders.clone().unwrap_or_else(site::core_feed_orders);
        let now = f
            .feed_order
            .clone()
            .unwrap_or_else(|| site::DEFAULT_FEED_ORDER.into());
        let path = s.posts_path.read(cx).value().trim().to_ascii_lowercase();
        let path_err = f
            .posts_path
            .as_ref()
            .and_then(|_| site::posts_path_error(&path));
        let front = f.front_page.clone().unwrap_or_default();
        div()
            .when(f.feed_order.is_some(), |d| {
                d.child(self.site_heading("Order of your home page", p)).children(orders.into_iter().map(|o| {
                    let on = now == o.id;
                    let id = o.id.clone();
                    let rec = if o.id == site::DEFAULT_FEED_ORDER { " (recommended)" } else { "" };
                    div()
                        .mt(px(6.))
                        .child(
                            self.site_radio(SharedString::from(format!("site-order-{}", o.id)), on, format!("{}{rec}", o.title))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let id = id.clone();
                                    this.site_edit(cx, |s| s.form.feed_order = Some(id))
                                })),
                        )
                        .child(div().pl(px(18.)).child(self.site_hint(o.description, p)))
                }))
                .child(self.site_hint(
                    "Feeds and the archive stay newest first whichever you choose. An order a studio extension adds shows here once it's on and saved.",
                    p,
                ))
            })
            .when(f.front_page.is_some(), |d| {
                d.child(self.site_heading("Front page", p))
                    .child(self.site_hint(
                        "Show one published thread at your blyg's address instead of the latest items, as a static front page.",
                        p,
                    ))
                    .child(
                        div()
                            .mt(px(6.))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(div().flex_1().min_w_0().truncate().child(if front.is_empty() {
                                "The latest items".to_string()
                            } else {
                                self.site_title_of(&front)
                            }))
                            .child(self.site_chip("site-front-choose", "Choose a thread…", p).on_click(cx.listener(|this, _, window, cx| {
                                let q = this.site.sheet.as_ref().map(|s| s.thread_q.clone());
                                if let Some(q) = q {
                                    q.update(cx, |st, cx| {
                                        st.set_value("", window, cx);
                                        st.focus(window, cx);
                                    });
                                }
                                if let Some(s) = this.site.sheet.as_mut() {
                                    s.picking = Some(Pick::FrontPage);
                                }
                                cx.notify();
                            })))
                            .when(!front.is_empty(), |d| {
                                d.child(self.site_chip("site-front-clear", "Use the latest items", p).on_click(
                                    cx.listener(|this, _, _, cx| this.site_edit(cx, |s| s.form.front_page = Some(String::new()))),
                                ))
                            }),
                    )
                    .when(s.picking == Some(Pick::FrontPage), |d| d.child(self.site_thread_picker(s, p, cx)))
            })
            .when(f.posts_path.is_some(), |d| {
                d.child(self.site_heading("Posts path", p))
                    .child(self.site_field(Input::new(&s.posts_path).into_any_element(), path_err.is_some(), p))
                    .child(match path_err {
                        Some(e) => div().mt(px(2.)).text_size(px(12.)).text_color(p.over).child(e),
                        None => self.site_hint(
                            format!(
                                "Your latest items are always at /{}/, front page or not, so a link to them survives a change.",
                                if path.is_empty() { site::DEFAULT_POSTS_PATH } else { &path }
                            ),
                            p,
                        ),
                    })
            })
            .into_any_element()
    }

    // ------------------------------------------------------------ Links

    fn site_links(&self, s: &Sheet, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let f = &s.form;
        let pos = f.links_position.clone().unwrap_or_else(|| "left".into());
        let threads = f.links_threads.clone().unwrap_or_default();
        let n = threads.len();
        div()
            .child(self.site_hint(
                "The main menu: a short list on every public page. The masthead links under your name are in Site Settings.",
                p,
            ))
            .when(f.links_position.is_some(), |d| {
                d.child(self.site_heading("Where it sits", p))
                    .child(div().flex().gap(px(16.)).children(site::LINKS_POSITIONS.iter().map(|(v, label)| {
                        let v = *v;
                        self.site_radio(SharedString::from(format!("site-pos-{v}")), pos == v, *label)
                            .on_click(cx.listener(move |this, _, _, cx| this.site_edit(cx, |s| s.form.links_position = Some(v.into()))))
                    })))
                    .child(self.site_hint("A side column on wide screens; on a phone it folds behind a Menu button under your name.", p))
            })
            .when(f.links_threads.is_some(), |d| {
                d.child(self.site_heading("Threads, listed first", p))
                    .children(threads.into_iter().enumerate().map(|(i, id)| {
                        let id2 = id.clone();
                        div()
                            .mt(px(3.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(div().flex_1().min_w_0().truncate().child(self.site_title_of(&id)))
                            .child(
                                div()
                                    .id(SharedString::from(format!("site-up-{i}")))
                                    .cursor_pointer()
                                    .text_color(if i == 0 { p.line } else { p.muted })
                                    .child("↑")
                                    .on_click(cx.listener(move |this, _, _, cx| this.site_move_thread(i, -1, cx))),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("site-down-{i}")))
                                    .cursor_pointer()
                                    .text_color(if i + 1 == n { p.line } else { p.muted })
                                    .child("↓")
                                    .on_click(cx.listener(move |this, _, _, cx| this.site_move_thread(i, 1, cx))),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("site-remove-{i}")))
                                    .cursor_pointer()
                                    .text_color(p.muted)
                                    .hover(|d| d.text_color(p.over))
                                    .child("×")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        let id = id2.clone();
                                        this.site_edit(cx, |s| {
                                            if let Some(t) = s.form.links_threads.as_mut() {
                                                t.retain(|x| *x != id);
                                            }
                                        })
                                    })),
                            )
                    }))
                    .when(n < site::MAX_LINKS && s.picking != Some(Pick::MenuThread), |d| {
                        d.child(div().mt(px(6.)).flex().child(self.site_chip("site-menu-add", "Add a thread…", p).on_click(cx.listener(
                            |this, _, window, cx| {
                                let q = this.site.sheet.as_ref().map(|s| s.thread_q.clone());
                                if let Some(q) = q {
                                    q.update(cx, |st, cx| {
                                        st.set_value("", window, cx);
                                        st.focus(window, cx);
                                    });
                                }
                                if let Some(s) = this.site.sheet.as_mut() {
                                    s.picking = Some(Pick::MenuThread);
                                }
                                cx.notify();
                            },
                        ))))
                    })
                    .when(s.picking == Some(Pick::MenuThread), |d| d.child(self.site_thread_picker(s, p, cx)))
            })
            .when(f.links_external.is_some(), |d| {
                d.child(self.site_heading("Your links · one per line: Label | https://…", p)).child(
                    div()
                        .h(px(72.))
                        .px(px(10.))
                        .py(px(6.))
                        .rounded(px(self.theme.corner(7.)))
                        .border_1()
                        .border_color(p.line)
                        .text_size(px(13.))
                        .child(Textarea::new(&s.external)),
                )
            })
            .when_some(f.links_collections, |d, on| {
                d.child(self.site_check("site-links-collections", on, "Your public collections", None, cx, |f| {
                    f.links_collections = f.links_collections.map(|b| !b)
                }))
            })
            .when_some(f.links_opml, |d, on| {
                d.child(self.site_check("site-links-opml", on, "Your blogroll as an OPML file, when it lists anyone", None, cx, |f| {
                    f.links_opml = f.links_opml.map(|b| !b)
                }))
            })
            .when_some(f.links_blygger_org, |d, on| {
                d.child(self.site_check("site-links-org", on, "Made with Blygger (blygger.org)", None, cx, |f| {
                    f.links_blygger_org = f.links_blygger_org.map(|b| !b)
                }))
            })
            .when_some(f.links_blygger_com, |d, on| {
                d.child(self.site_check("site-links-com", on, "Find more blygs (blygger.com)", None, cx, |f| {
                    f.links_blygger_com = f.links_blygger_com.map(|b| !b)
                }))
            })
            .into_any_element()
    }

    // ------------------------------------------------------------ Profile

    fn site_profile(&self, s: &Sheet, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let f = &s.form;
        let avatar = f.avatar_media_id.clone().unwrap_or_default();
        div()
            .child(self.site_hint("Your name, bio and masthead links are in Site Settings.", p))
            .when(f.avatar_media_id.is_some(), |d| {
                d.child(self.site_heading("Avatar", p)).child(
                    div()
                        .mt(px(4.))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .when_some(s.avatar.clone(), |d, im| {
                            d.child(img(im).size(px(48.)).rounded(px(6.)).object_fit(ObjectFit::Cover))
                        })
                        .child(div().flex_1().min_w_0().text_color(p.muted).truncate().child(if s.uploading {
                            "Uploading…".to_string()
                        } else if avatar.is_empty() {
                            "No avatar".to_string()
                        } else {
                            format!("Media {avatar}")
                        }))
                        .child(self.site_chip("site-avatar-upload", "Upload…", p).on_click(
                            cx.listener(|this, _, window, cx| this.site_upload_avatar(window, cx)),
                        ))
                        .when(!avatar.is_empty(), |d| {
                            d.child(self.site_chip("site-avatar-clear", "Clear", p).on_click(cx.listener(|this, _, _, cx| {
                                this.site_edit(cx, |s| {
                                    s.form.avatar_media_id = Some(String::new());
                                    s.avatar = None;
                                })
                            })))
                        }),
                )
                .child(self.site_hint("PNG, JPEG, GIF, WebP or SVG. It's public: it's on your pages and in blyg.json.", p))
            })
            .when(f.author_url.is_some(), |d| {
                d.child(self.site_heading("Author URL", p))
                    .child(self.site_field(Input::new(&s.author_url).into_any_element(), false, p))
                    .child(self.site_hint("Where your name links; blank is this blyg's own address.", p))
            })
            .when(f.site_url.is_some(), |d| {
                d.child(self.site_heading("Canonical site URL", p))
                    .child(self.site_field(Input::new(&s.site_url).into_any_element(), false, p))
                    .child(self.site_hint("Blank derives it from each request.", p))
            })
            .into_any_element()
    }

    // ------------------------------------------------------------ Network

    fn site_network(&self, s: &Sheet, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        div()
            .child(self.site_heading("Collection feeds · off by default", p))
            .when_some(s.form.hopper_feeds, |d, on| {
                d.child(self.site_check(
                    "site-hopper-feeds",
                    on,
                    "Give each public collection its own RSS feed",
                    Some("Each public collection (hopper) gets a feed at h/<slug>/feed.xml, linked from its page, and blyg.json lists your public collections. Entries are curation, not reposts: an excerpt, who wrote it, and a link to where it lives. Subscribing to someone else's collection works either way."),
                    cx,
                    |f| f.hopper_feeds = f.hopper_feeds.map(|b| !b),
                ))
            })
            .into_any_element()
    }

    // ------------------------------------------------------------ Updates

    fn site_updates(&self, s: &Sheet, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let f = &s.form;
        let on = f.update_check.unwrap_or(false);
        let state = s.form.update.clone().or_else(|| s.base.update.clone());
        div()
            .child(self.site_heading("Studio update alerts", p))
            .child(self.site_check(
                "site-update-check",
                on,
                "Tell me when a newer release of my blyg's studio exists",
                Some("Once a day your blyg fetches the studio's public release feed and compares the newest version to the one it runs. Nothing about your blyg is sent. Burrow shows it quietly in the status bar."),
                cx,
                |f| f.update_check = f.update_check.map(|b| !b),
            ))
            .when_some(state.filter(|_| on), |d, u| {
                let line = match (&u.current, u.latest.as_str()) {
                    (_, "") => "Not checked yet.".to_string(),
                    (Some(c), l) if u.behind() => format!("A new version is available: {l} (your blyg runs {c})."),
                    (Some(c), _) => format!("Up to date: your blyg runs {c}."),
                    (None, l) => format!("The newest release is {l}."),
                };
                d.child(
                    div()
                        .mt(px(8.))
                        .flex()
                        .gap(px(8.))
                        .child(div().text_color(if u.behind() { p.amber } else { p.muted }).child(line))
                        .when(u.behind(), |d| {
                            d.child(
                                div()
                                    .id("site-release-notes")
                                    .cursor_pointer()
                                    .text_color(p.accent)
                                    .hover(|d| d.underline())
                                    .child("Release notes")
                                    .on_click(|_, _, cx| cx.open_url(site::STUDIO_RELEASES_URL)),
                            )
                        }),
                )
                .when(u.behind(), |d| d.child(self.site_hint("Update it where it runs: npm run upgrade in its directory.", p)))
            })
            .when(f.update_feed_url.is_some(), |d| {
                d.child(self.site_heading("Release feed (blank = the studio's own)", p))
                    .child(self.site_field(Input::new(&s.feed_url).into_any_element(), false, p))
                    .child(self.site_hint(
                        "Only change this if you have modified the studio and track your own versions. Point it at your releases, or turn the check off.",
                        p,
                    ))
            })
            .into_any_element()
    }

    // ------------------------------------------------------------ Extensions

    fn site_extensions(&self, s: &Sheet, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let on: Vec<String> = s.form.extensions.clone().unwrap_or_default();
        div()
            .child(self.site_heading("Studio extensions", p))
            .child(self.site_hint(
                "Experiments compiled into your blyg's studio, by the release or by whoever runs it. They change what Studio shows, never your public pages or what other blygs read. Each starts off. (Burrow's own extensions are under Post › Extensions…)",
                p,
            ))
            .children(s.form.server_extensions.iter().map(|e| {
                let (label, desc) = site::extension_label(&e.name);
                let checked = on.contains(&e.name);
                let name = e.name.clone();
                let compiled = e.compiled;
                div()
                    .id(SharedString::from(format!("site-ext-{}", e.name)))
                    .mt(px(8.))
                    .flex()
                    .gap(px(8.))
                    .when(compiled, |d| d.cursor_pointer())
                    .text_color(if compiled { p.ink } else { p.muted })
                    .child(if checked { "☑" } else { "☐" })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(if compiled { label.to_string() } else { format!("{label} · not in this build") })
                            .when(!desc.is_empty(), |d| d.child(self.site_hint(desc, p))),
                    )
                    .when(compiled, |d| {
                        d.on_click(cx.listener(move |this, _, _, cx| {
                            let name = name.clone();
                            this.site_edit(cx, |s| {
                                let list = s.form.extensions.get_or_insert_with(Vec::new);
                                if let Some(i) = list.iter().position(|n| *n == name) {
                                    list.remove(i);
                                } else {
                                    list.push(name);
                                    list.sort();
                                }
                            })
                        }))
                    })
            }))
            .into_any_element()
    }
}
