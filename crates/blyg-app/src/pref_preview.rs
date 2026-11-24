//! --- preview --- Try a theme, a font or a size before keeping it
//! (docs/SPEC.md § Themes and fonts: trying before keeping).
//!
//! Moving through the THEME, WRITING FONT, INTERFACE FONT and SIZE options in
//! Settings (arrow keys, or resting the pointer on a chip) and through the
//! command palette's theme and font rows applies the option to the whole
//! window at once, **without writing the config file**. ⏎ (or a click) keeps
//! it and writes the key, as before; esc, moving back, or leaving Settings or
//! the palette puts the kept value back.
//!
//! The model: while a preview is up, `MainView::prefs` is the kept prefs
//! (`Active::committed`) with one [`Pick`] laid over them, so everything that
//! reads `prefs` (fonts, size, theme) draws the preview, and the kept values
//! are one field away. Applying a pick is `apply_prefs_live`: a theme lookup
//! in the resolved-once registry (`theme::Themes`, `Arc`s) and a palette
//! copy, nothing per frame and no file I/O.
//!
//! Light and dark: the THEME row chooses for one **slot**, `theme` (light)
//! or `theme-dark` (dark). Trying a theme for the slot that isn't in effect
//! (the dark slot while macOS is light) shows it anyway, by resolving as if
//! the appearance were that slot's, so you always see what you're choosing.
//!
//! A child module of `app`; `app.rs` keeps small hooks marked
//! `// --- preview ---`.

use super::*;

/// How long the pointer rests on a chip before the window takes it on, and
/// how long after it leaves before the kept value comes back: sweeping
/// across the chips toward one doesn't flash every theme on the way, and
/// moving from chip to chip goes straight from one preview to the next.
pub const HOVER_DWELL: Duration = Duration::from_millis(90);

/// One option being tried (or kept).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Pick {
    /// `theme` (`dark: false`) or `theme-dark` (`dark: true`). In the dark
    /// slot `None` is "Same as light" (no `theme-dark` line).
    Theme { dark: bool, id: Option<String> },
    /// `font-family-writing`, by label.
    WritingFont(&'static str),
    /// `font-family-ui`, by label.
    UiFont(&'static str),
    /// `font-size`.
    Size(f32),
    /// A Background rule (Settings › BACKGROUND): never tried, only
    /// turned off or on with space or ⏎ (`save_background_rule`).
    Rule(&'static str),
    /// --- studio 0.71 --- A choice of READ MARKS (`readerMarkRead`):
    /// never tried, only saved with space or ⏎ (`set_count_read`).
    CountRead(blyg_core::mark_read::MarkRead),
}

impl Pick {
    /// Lay this pick over `p`.
    pub fn apply(&self, p: &mut Prefs) {
        match self {
            Pick::Theme { dark: false, id } => {
                p.theme = id.clone().unwrap_or_else(|| "system".into());
            }
            Pick::Theme { dark: true, id } => {
                p.theme_dark = id.clone().filter(|d| d != "system");
            }
            Pick::WritingFont(l) => {
                p.writing_font = l.to_string();
                p.writing_font_set = true;
            }
            Pick::UiFont(l) => {
                p.ui_font = l.to_string();
                p.ui_font_set = true;
            }
            Pick::Size(s) => {
                p.font_size = s.clamp(prefs::MIN_SIZE, prefs::MAX_SIZE);
            }
            Pick::Rule(_) | Pick::CountRead(_) => {}
        }
    }

    /// Put back, in `p`, the fields this pick lays over, from `kept`.
    pub fn restore(&self, kept: &Prefs, p: &mut Prefs) {
        match self {
            Pick::Theme { .. } => {
                p.theme.clone_from(&kept.theme);
                p.theme_dark.clone_from(&kept.theme_dark);
            }
            Pick::WritingFont(_) => {
                p.writing_font.clone_from(&kept.writing_font);
                p.writing_font_set = kept.writing_font_set;
            }
            Pick::UiFont(_) => {
                p.ui_font.clone_from(&kept.ui_font);
                p.ui_font_set = kept.ui_font_set;
            }
            Pick::Size(_) => p.font_size = kept.font_size,
            Pick::Rule(_) | Pick::CountRead(_) => {}
        }
    }
}

/// Where a preview came from: it ends when that surface goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Origin {
    Settings,
    Palette,
}

/// A preview that's up.
pub(crate) struct Active {
    /// The prefs as kept (what the config file says, plus dev overrides).
    pub committed: Prefs,
    pub pick: Pick,
    pub origin: Origin,
    /// Resolve the theme as if macOS were dark (`Some(true)`) or light:
    /// set while trying a theme for the slot that isn't in effect.
    pub force_dark: Option<bool>,
}

/// The rows of Settings that take ⇥ and the arrow keys, in the page's
/// order. The first four preview; READ MARKS' choices (lower on the page,
/// when the blyg offers the setting) are saved and BACKGROUND's rules (when
/// it offers any) turned off and on, with space or ⏎.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Row {
    WritingFont,
    UiFont,
    Size,
    Theme,
    CountRead,
    Background,
}

impl Row {
    pub const ALL: [Row; 6] = [
        Row::WritingFont,
        Row::UiFont,
        Row::Size,
        Row::Theme,
        Row::CountRead,
        Row::Background,
    ];
}

#[derive(Default)]
pub(crate) struct State {
    pub active: Option<Active>,
    /// Settings' keyboard cursor: the row, and the option on it.
    pub cursor: Option<(Row, Pick)>,
    /// The THEME row edits `theme-dark` (else `theme`).
    pub dark_slot: bool,
    /// The chip under the pointer, once it has rested there.
    pub hover: Option<Pick>,
    hover_gen: usize,
    /// The window was dark at the last frame (for Settings' slot notes).
    pub os_dark: bool,
    /// Scroll the BACKGROUND rule under the cursor into view at the next
    /// frame (it sits below the fold).
    pub reveal: std::rc::Rc<std::cell::Cell<bool>>,
}

impl State {
    /// The kept prefs while a preview is up.
    pub fn committed(&self) -> Option<&Prefs> {
        self.active.as_ref().map(|a| &a.committed)
    }

    pub fn force_dark(&self) -> Option<bool> {
        self.active.as_ref().and_then(|a| a.force_dark)
    }

    /// The pick being tried, if any.
    pub fn trying(&self) -> Option<&Pick> {
        self.active.as_ref().map(|a| &a.pick)
    }
}

/// Whether the dark slot (`theme-dark`) is the one in effect: macOS is dark
/// and `theme-dark` names a theme.
pub fn dark_slot_in_effect(os_dark: bool, p: &Prefs, cx: &App) -> bool {
    os_dark
        && p.theme_dark
            .as_deref()
            .is_some_and(|d| crate::theme::themes(cx).registry.exists(d))
}

/// Whether this window is dark now.
pub fn os_dark(window: &Window) -> bool {
    matches!(
        window.appearance(),
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}

/// The theme options for a slot, in Settings' order: the light slot has
/// System first, the dark slot "Same as light" (`None`).
pub fn theme_options(dark: bool, cx: &App) -> Vec<Pick> {
    let mut out = vec![Pick::Theme {
        dark,
        id: if dark { None } else { Some("system".into()) },
    }];
    for (_, list) in crate::theme::themes(cx).grouped() {
        for t in list {
            out.push(Pick::Theme {
                dark,
                id: Some(t.id.clone()),
            });
        }
    }
    out
}

/// A row's options (`Size` has none: it steps).
pub fn options(row: Row, dark: bool, cx: &App) -> Vec<Pick> {
    match row {
        Row::WritingFont => prefs::WRITING_FONTS
            .iter()
            .map(|f| Pick::WritingFont(f.label))
            .collect(),
        Row::UiFont => prefs::UI_FONTS
            .iter()
            .map(|f| Pick::UiFont(f.label))
            .collect(),
        Row::Theme => theme_options(dark, cx),
        // Steps (Size), or the blyg's rules (`MainView::background_rule_picks`).
        Row::CountRead => blyg_core::mark_read::MarkRead::ALL
            .into_iter()
            .map(Pick::CountRead)
            .collect(),
        Row::Size | Row::Background => Vec::new(),
    }
}

/// The option `delta` steps from `at` among `opts` (wrapping); from an
/// option that isn't listed, the first or last.
pub fn step(opts: &[Pick], at: &Pick, delta: isize) -> Option<Pick> {
    let n = opts.len() as isize;
    if n == 0 {
        return None;
    }
    let i = match opts.iter().position(|o| o == at) {
        Some(i) => (i as isize + delta).rem_euclid(n),
        None if delta < 0 => n - 1,
        None => 0,
    };
    opts.get(i as usize).cloned()
}

/// A theme's display name ("Paper", "Kumiko"; "System", "Same as light").
pub fn theme_label(id: Option<&str>, cx: &App) -> String {
    match id {
        None => "Same as light".into(),
        Some("system") => "System".into(),
        Some("light") => "Paper".into(),
        Some("dark") => "Ink".into(),
        Some(id) => crate::theme::themes(cx).get(id).name.clone(),
    }
}

impl MainView {
    /// The prefs as kept: `prefs` minus whatever is being tried.
    pub(crate) fn committed_prefs(&self) -> &Prefs {
        self.preview.committed().unwrap_or(&self.prefs)
    }

    /// The appearance the theme resolves under: the window's, unless a
    /// theme is being tried for the other slot.
    pub(crate) fn preview_appearance(&self, window: &Window) -> WindowAppearance {
        match self.preview.force_dark() {
            Some(true) => WindowAppearance::Dark,
            Some(false) => WindowAppearance::Light,
            None => window.appearance(),
        }
    }

    /// Try `pick`: lay it over the kept prefs and redraw, writing nothing.
    /// Trying the kept value ends the preview.
    pub(crate) fn preview_pick(
        &mut self,
        pick: Pick,
        origin: Origin,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.preview.trying() == Some(&pick) {
            return;
        }
        let committed = match self.preview.active.take() {
            Some(a) => {
                a.pick.restore(&a.committed, &mut self.prefs);
                a.committed
            }
            None => self.prefs.clone(),
        };
        let mut next = committed.clone();
        pick.apply(&mut next);
        let os_dark = os_dark(window);
        let in_effect_dark = dark_slot_in_effect(os_dark, &committed, cx);
        let force_dark = match &pick {
            // The light slot while the dark one is in effect: show it light.
            Pick::Theme { dark: false, .. } if in_effect_dark => Some(false),
            // The dark slot while macOS is light: show it dark.
            Pick::Theme { dark: true, .. } if !os_dark => Some(true),
            _ => None,
        };
        if next == committed && force_dark.is_none() {
            // Back on the kept value: nothing to try.
            self.apply_prefs_live(window, cx);
            return;
        }
        pick.apply(&mut self.prefs);
        self.preview.active = Some(Active {
            committed,
            pick,
            origin,
            force_dark,
        });
        self.apply_prefs_live(window, cx);
    }

    /// Put the kept value back (esc, leaving): nothing is written.
    pub(crate) fn end_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(a) = self.preview.active.take() {
            a.pick.restore(&a.committed, &mut self.prefs);
            self.apply_prefs_live(window, cx);
        }
    }

    /// End a preview that came from `origin`.
    pub(crate) fn end_preview_from(
        &mut self,
        origin: Origin,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.preview.active.as_ref().map(|a| a.origin) == Some(origin) {
            self.end_preview(window, cx);
        }
    }

    /// Hook (`apply_prefs`): forget a preview's overlay before keeping
    /// another change, so only that change is written.
    pub(crate) fn drop_preview_overlay(&mut self) {
        if let Some(a) = self.preview.active.take() {
            a.pick.restore(&a.committed, &mut self.prefs);
        }
    }

    /// Keep `pick` (⏎, a click): apply it and write its key.
    pub(crate) fn commit_pick(&mut self, pick: Pick, window: &mut Window, cx: &mut Context<Self>) {
        self.drop_preview_overlay();
        pick.apply(&mut self.prefs);
        self.apply_prefs(window, cx);
        // Kept for the slot that isn't showing: say where it went.
        if let Pick::Theme { dark, id } = &pick {
            let os_dark = os_dark(window);
            let in_effect = dark_slot_in_effect(os_dark, &self.prefs, cx);
            let name = theme_label(id.as_deref(), cx);
            if *dark && !os_dark {
                self.show_toast(
                    format!("{name} is your dark-mode theme (theme-dark): macOS is light now"),
                    None,
                    cx,
                );
            } else if !*dark && in_effect {
                self.show_toast(
                    format!("{name} is your light-mode theme (theme): macOS is dark now"),
                    None,
                    cx,
                );
            }
        }
    }

    /// Hook (render): a preview whose surface has gone (Settings closed
    /// some other way, the palette dropped) puts the kept value back.
    pub(crate) fn preview_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.preview.os_dark = os_dark(window);
        let Some(origin) = self.preview.active.as_ref().map(|a| a.origin) else {
            return;
        };
        let gone = match origin {
            Origin::Settings => !self.sheet.is::<super::SettingsSheet>(),
            Origin::Palette => self.cmd_palette.open.is_none(),
        };
        if gone {
            self.end_preview(window, cx);
        }
    }

    // ------------------------------------------------------------ Settings

    /// Hook (`open_settings`): a fresh cursor, and the THEME row on the
    /// slot in effect.
    pub(crate) fn settings_preview_reset(&mut self, window: &Window, cx: &App) {
        self.preview.cursor = None;
        self.preview.hover = None;
        self.preview.hover_gen += 1;
        self.preview.dark_slot = dark_slot_in_effect(os_dark(window), self.committed_prefs(), cx);
    }

    /// The option the row keeps now.
    pub(crate) fn kept_pick(&self, row: Row) -> Pick {
        let p = self.committed_prefs();
        match row {
            Row::WritingFont => Pick::WritingFont(p.writing().label),
            Row::UiFont => Pick::UiFont(p.ui().label),
            Row::Size => Pick::Size(p.font_size),
            Row::Theme if self.preview.dark_slot => Pick::Theme {
                dark: true,
                id: p.theme_dark.clone(),
            },
            Row::Theme => Pick::Theme {
                dark: false,
                id: Some(p.theme.clone()),
            },
            Row::CountRead => Pick::CountRead(self.reading.count_read.setting.unwrap_or_default()),
            Row::Background => self
                .background_rule_picks()
                .into_iter()
                .next()
                .unwrap_or(Pick::Rule(blyg_core::background::BURST)),
        }
    }

    /// The chip under Settings' keyboard cursor, below the fold (READ
    /// MARKS, BACKGROUND): `selector` names it for tests, and the page
    /// scrolls to it after ⇥ or an arrow (`State::reveal`).
    pub(crate) fn settings_focused_chip(
        &self,
        selector: &'static str,
        chip: impl IntoElement,
    ) -> AnyElement {
        let reveal = self.preview.reveal.clone();
        let scroll = self
            .sheet
            .get::<super::SettingsSheet>()
            .map(|s| s.scroll.clone());
        div()
            .flex_none()
            .debug_selector(move || selector.into())
            .on_children_prepainted(move |b, window, _| {
                let (Some(b), Some(scroll)) = (b.first(), scroll.as_ref()) else {
                    return;
                };
                if !reveal.replace(false) {
                    return;
                }
                let view = scroll.bounds();
                let margin = px(48.);
                let off = scroll.offset();
                let dy = if b.bottom() + margin > view.bottom() {
                    view.bottom() - (b.bottom() + margin)
                } else if b.top() - margin < view.top() {
                    view.top() - (b.top() - margin)
                } else {
                    return;
                };
                scroll.set_offset(point(off.x, off.y + dy));
                window.refresh();
            })
            .child(chip)
            .into_any_element()
    }

    /// The rows ⇥ walks: READ MARKS only while the blyg offers the
    /// setting, BACKGROUND only while it offers rules.
    fn tab_rows(&self) -> Vec<Row> {
        let bg = !self.background_rule_picks().is_empty();
        let marks = self.reading.count_read.setting.is_some();
        Row::ALL
            .into_iter()
            .filter(|r| match r {
                Row::Background => bg,
                Row::CountRead => marks,
                _ => true,
            })
            .collect()
    }

    /// Switch the THEME row between the light and dark slots.
    pub(crate) fn set_theme_slot(
        &mut self,
        dark: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(self.preview.trying(), Some(Pick::Theme { .. })) {
            self.end_preview(window, cx);
        }
        self.preview.dark_slot = dark;
        if let Some((Row::Theme, _)) = self.preview.cursor {
            self.preview.cursor = Some((Row::Theme, self.kept_pick(Row::Theme)));
        }
        cx.notify();
    }

    /// Move Settings' keyboard cursor to `row` (on its kept option),
    /// putting back anything being tried on another row.
    fn settings_cursor_to(
        &mut self,
        row: Option<Row>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.end_preview_from(Origin::Settings, window, cx);
        self.preview.cursor = row.map(|r| (r, self.kept_pick(r)));
        if matches!(row, Some(Row::Background | Row::CountRead)) {
            self.preview.reveal.set(true);
        } else if row.is_some()
            // The appearance rows sit at the top of the page.
            && let Some(super::SettingsSheet { scroll, .. }) = self.sheet.get()
        {
            scroll.set_offset(point(px(0.), px(0.)));
        }
        cx.notify();
    }

    /// A key on Settings (the page itself focused, not a field in it).
    /// Returns whether it was taken.
    pub(crate) fn settings_preview_key(
        &mut self,
        key: &str,
        shift: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let rows = self.tab_rows();
        let at = self.preview.cursor.as_ref().map(|(r, _)| *r);
        let row_index = at.and_then(|r| rows.iter().position(|x| *x == r));
        match key {
            "tab" => {
                let next = match (row_index, shift) {
                    (None, false) => Some(rows[0]),
                    (None, true) => Some(rows[rows.len() - 1]),
                    (Some(i), false) => rows.get(i + 1).copied(),
                    (Some(0), true) => None,
                    (Some(i), true) => rows.get(i - 1).copied(),
                };
                self.settings_cursor_to(next, window, cx);
                true
            }
            "up" | "down" | "left" | "right" => {
                let Some((row, pick)) = self.preview.cursor.clone() else {
                    self.settings_cursor_to(Some(rows[0]), window, cx);
                    return true;
                };
                let back = matches!(key, "up" | "left");
                if matches!(row, Row::Background | Row::CountRead) {
                    let opts = match row {
                        Row::Background => self.background_rule_picks(),
                        _ => options(row, false, cx),
                    };
                    if let Some(next) = step(&opts, &pick, if back { -1 } else { 1 }) {
                        self.preview.cursor = Some((row, next));
                        self.preview.reveal.set(true);
                    }
                    cx.notify();
                    return true;
                }
                let next = match (row, &pick) {
                    (Row::Size, Pick::Size(s)) => {
                        let s = if back { s - 1.0 } else { s + 1.0 };
                        Some(Pick::Size(s.clamp(prefs::MIN_SIZE, prefs::MAX_SIZE)))
                    }
                    _ => step(
                        &options(row, self.preview.dark_slot, cx),
                        &pick,
                        if back { -1 } else { 1 },
                    ),
                };
                if let Some(next) = next {
                    self.preview.cursor = Some((row, next.clone()));
                    self.preview_pick(next, Origin::Settings, window, cx);
                }
                cx.notify();
                true
            }
            "enter" | "space" => {
                let Some((row, pick)) = self.preview.cursor.clone() else {
                    return false;
                };
                if let Pick::Rule(rule) = pick {
                    self.toggle_background_rule(rule, cx);
                    return true;
                }
                if let Pick::CountRead(m) = pick {
                    self.set_count_read(m, cx);
                    return true;
                }
                if pick != self.kept_pick(row) {
                    self.commit_pick(pick, window, cx);
                }
                cx.notify();
                true
            }
            "escape" => {
                if self.preview.active.is_some() {
                    self.end_preview(window, cx);
                    if let Some((row, _)) = self.preview.cursor.clone() {
                        self.preview.cursor = Some((row, self.kept_pick(row)));
                    }
                    cx.notify();
                    true
                } else if self.preview.cursor.is_some() {
                    self.preview.cursor = None;
                    cx.notify();
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// The pointer entered (`true`) or left a chip that previews. After
    /// [`HOVER_DWELL`] with no other move, the window takes the chip on, or
    /// (left, and on no other chip) goes back to the keyboard cursor's
    /// option, else the kept value.
    pub(crate) fn settings_hover(
        &mut self,
        pick: Pick,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if hovered {
            self.preview.hover = Some(pick);
        } else if self.preview.hover.as_ref() == Some(&pick) {
            self.preview.hover = None;
        } else {
            return;
        }
        self.preview.hover_gen += 1;
        let gen_ = self.preview.hover_gen;
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(HOVER_DWELL).await;
            let _ = this.update_in(cx, |v, window, cx| {
                if v.preview.hover_gen != gen_ || !v.sheet.is::<super::SettingsSheet>() {
                    return;
                }
                let want = v
                    .preview
                    .hover
                    .clone()
                    .or_else(|| v.preview.cursor.as_ref().map(|(_, p)| p.clone()));
                match want {
                    Some(p) => v.preview_pick(p, Origin::Settings, window, cx),
                    None => v.end_preview_from(Origin::Settings, window, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// A click on a chip that previews: keep it, and put the keyboard
    /// cursor there.
    pub(crate) fn settings_choose(
        &mut self,
        row: Row,
        pick: Pick,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preview.cursor = Some((row, pick.clone()));
        self.preview.hover = None;
        self.preview.hover_gen += 1;
        self.commit_pick(pick, window, cx);
        if let Some(super::SettingsSheet { focus, .. }) = self.sheet.get() {
            let f = focus.clone();
            window.focus(&f, cx);
        }
        cx.notify();
    }

    // ------------------------------------------------------------ palette

    /// The palette's theme rows choose for the slot in effect.
    pub(crate) fn palette_theme_dark(&self, window: &Window, cx: &App) -> bool {
        dark_slot_in_effect(os_dark(window), self.committed_prefs(), cx)
    }

    /// What a palette row tries, if it's a theme or font row.
    pub(crate) fn palette_pick(
        &self,
        cmd: &command_palette::Cmd,
        window: &Window,
        cx: &App,
    ) -> Option<Pick> {
        use command_palette::Cmd;
        match cmd {
            Cmd::Theme(id) => {
                let dark = self.palette_theme_dark(window, cx);
                let id = if dark && id == "system" {
                    None
                } else {
                    Some(id.clone())
                };
                Some(Pick::Theme { dark, id })
            }
            Cmd::WritingFont(l) => Some(Pick::WritingFont(l)),
            Cmd::UiFont(l) => Some(Pick::UiFont(l)),
            _ => None,
        }
    }

    /// Hook (palette): the highlighted row changed. A theme or font row is
    /// tried at once; any other row puts the kept value back.
    pub(crate) fn palette_preview_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pick = self
            .cmd_palette
            .open
            .as_ref()
            .and_then(|o| o.shown.get(o.sel).map(|&i| o.entries[i].cmd.clone()))
            .and_then(|cmd| self.palette_pick(&cmd, window, cx));
        match pick {
            Some(p) => self.preview_pick(p, Origin::Palette, window, cx),
            None => self.end_preview_from(Origin::Palette, window, cx),
        }
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: that brings GPUI's `test` attribute.
    use super::{Pick, step};
    use crate::prefs::{self, LayoutPref, Prefs};

    #[test]
    fn picks_lay_over_and_restore_only_their_fields() {
        let kept = Prefs {
            theme: "cutaway".into(),
            theme_dark: Some("fortress".into()),
            font_size: 19.,
            ..Prefs::default()
        };
        for pick in [
            Pick::Theme {
                dark: false,
                id: Some("kumiko".into()),
            },
            Pick::Theme {
                dark: true,
                id: None,
            },
            Pick::WritingFont("ET Book"),
            Pick::UiFont("Literata"),
            Pick::Size(24.),
        ] {
            let mut p = kept.clone();
            pick.apply(&mut p);
            assert_ne!(p, kept, "{pick:?} changes nothing");
            p.layout = LayoutPref::Stacked;
            pick.restore(&kept, &mut p);
            assert_eq!(p.layout, LayoutPref::Stacked, "{pick:?} restored too much");
            p.layout = kept.layout;
            assert_eq!(p, kept, "{pick:?}");
        }
        // The dark slot's "system" is "Same as light".
        let mut p = kept.clone();
        Pick::Theme {
            dark: true,
            id: Some("system".into()),
        }
        .apply(&mut p);
        assert_eq!(p.theme_dark, None);
        let mut p = kept.clone();
        Pick::Size(99.).apply(&mut p);
        assert_eq!(p.font_size, prefs::MAX_SIZE);
    }

    #[test]
    fn steps_wrap_and_start_at_the_ends() {
        let opts = [Pick::Size(1.), Pick::Size(2.), Pick::Size(3.)];
        assert_eq!(step(&opts, &Pick::Size(3.), 1), Some(Pick::Size(1.)));
        assert_eq!(step(&opts, &Pick::Size(1.), -1), Some(Pick::Size(3.)));
        assert_eq!(step(&opts, &Pick::Size(9.), 1), Some(Pick::Size(1.)));
        assert_eq!(step(&opts, &Pick::Size(9.), -1), Some(Pick::Size(3.)));
        assert_eq!(step(&[], &Pick::Size(1.), 1), None);
    }
}
