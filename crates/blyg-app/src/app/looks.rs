//! Config, prefs, theme and fonts: writing prefs back to the config file,
//! reloading it when it changes on disk, the problems it has, the theme
//! and font size, and the Settings sheet's hotkey.

use super::*;

/// The config file as last read: what it says, its problems, and its state
/// on disk.
pub(super) struct ConfigFile {
    /// What the config file says (`prefs` minus unsaved dev overrides);
    /// changes are written back as the difference from this.
    pub(super) persisted: Prefs,
    /// Problems in the config file, shown as a dismissible banner.
    pub(super) problems: Vec<blyg_core::config::Diagnostic>,
    pub(super) problems_dismissed: bool,
    /// The config files' and themes folder's state when last read, so an
    /// edit on disk reloads (see `watch_config`).
    watch_stamp: u64,
}

impl ConfigFile {
    pub(super) fn new(cx: &App) -> Self {
        Self {
            persisted: crate::settings::prefs(cx),
            problems: crate::settings::current_diagnostics(cx),
            problems_dismissed: false,
            watch_stamp: 0,
        }
    }

    /// Take the files' state on disk as read, so our own write isn't an
    /// edit to reload.
    pub(super) fn restamp(&mut self, cx: &App) {
        self.watch_stamp = crate::settings::watch_stamp(&crate::settings::watched(cx));
    }
}

impl MainView {
    // ------------------------------------------------------------ config

    /// Write whatever changed in `prefs` back to the config file.
    pub(super) fn save_prefs(&mut self, cx: &mut Context<Self>) {
        let changes = self.prefs.changes_from(&self.config.persisted);
        match crate::settings::write(&changes, cx) {
            Ok(()) => {
                // Only the keys that changed were written, so a dev override
                // of another key (BLYGGER_THEME) stays unsaved.
                let mut p = self.config.persisted.clone();
                for (key, _) in &changes {
                    match *key {
                        "font-family-writing" => {
                            p.writing_font = self.prefs.writing_font.clone();
                            p.writing_font_set = self.prefs.writing_font_set;
                        }
                        "font-family-ui" => {
                            p.ui_font = self.prefs.ui_font.clone();
                            p.ui_font_set = self.prefs.ui_font_set;
                        }
                        "font-size" => p.font_size = self.prefs.font_size,
                        "theme" => p.theme = self.prefs.theme.clone(),
                        "theme-dark" => p.theme_dark = self.prefs.theme_dark.clone(),
                        "layout" => p.layout = self.prefs.layout,
                        "capture-hotkey" => p.hotkey = self.prefs.hotkey.clone(),
                        "show-buttons" => p.show_buttons = self.prefs.show_buttons, // --- buttons ---
                        // --- reader view / changelog ---
                        "open-posts-on-site" => p.author_site = self.prefs.author_site,
                        _ => {}
                    }
                }
                self.config.persisted = p;
                self.refresh_problems(cx);
                self.config.restamp(cx);
            }
            Err(e) => self.show_toast(e, None, cx),
        }
    }

    pub(super) fn refresh_problems(&mut self, cx: &mut Context<Self>) {
        let problems = crate::settings::current_diagnostics(cx);
        if problems != self.config.problems {
            self.config.problems_dismissed = false;
        }
        self.config.problems = problems;
        self.title = vm::window_title(crate::settings::blyg_url(cx).as_deref());
    }

    /// ⌘⇧, — re-read the config file and apply fonts, theme, layout and the
    /// hotkey without a restart.
    pub(super) fn reload_config(
        &mut self,
        _: &ReloadConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reload_config_now(window, cx);
    }

    /// Poll the config files and the themes folder (cheap `stat`s, once a
    /// second, off the main thread) and reload when one changes on disk.
    pub(super) fn watch_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = crate::settings::watched(cx);
        self.config.watch_stamp = crate::settings::watch_stamp(&paths);
        if cfg!(test) {
            return;
        }
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(1000))
                    .await;
                let Ok(paths) = this.update(cx, |_, cx| crate::settings::watched(cx)) else {
                    break;
                };
                let stamp = cx
                    .background_spawn(async move { crate::settings::watch_stamp(&paths) })
                    .await;
                let alive = this.update_in(cx, |v, window, cx| {
                    if stamp != v.config.watch_stamp {
                        v.config.watch_stamp = stamp;
                        v.reload_config_now(window, cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn reload_config_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        crate::settings::reload(cx);
        crate::theme::reload(cx);
        self.config.restamp(cx);
        let fresh = crate::settings::prefs(cx);
        let hotkey_changed = fresh.hotkey != self.prefs.hotkey;
        self.config.persisted = fresh.clone();
        self.preview.active = None; // --- preview --- the file wins
        self.prefs = fresh;
        let mut msg = "Config reloaded".to_string();
        if hotkey_changed && let Err(e) = crate::capture::set_hotkey(&self.prefs.hotkey, cx) {
            msg = format!("Config reloaded, but the hotkey didn't change: {e}");
        }
        self.config.problems_dismissed = false;
        self.refresh_problems(cx);
        // --- composer ---
        crate::composer::set_spellcheck(self.prefs.spellcheck, cx);
        crate::refresh_menus(cx);
        if !self.config.problems.is_empty() {
            let n = self.config.problems.len();
            msg = format!("{msg} · {n} problem{}", if n == 1 { "" } else { "s" });
        }
        self.apply_prefs_live(window, cx);
        self.ext_reload(window, cx); // --- extensions ---
        self.nearby_reload(cx); // --- nearby ---
        self.quiet_reload(cx); // --- quiet ---
        self.show_toast(msg, None, cx);
    }

    pub(super) fn open_config_file(
        &mut self,
        _: &OpenConfigFile,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_config_file_now(cx);
    }

    pub(super) fn open_config_file_now(&mut self, cx: &mut Context<Self>) {
        match crate::settings::open_config_file(cx) {
            Ok(path) => self.show_toast(
                format!("Opening {}", blyg_core::config::paths::tilde(&path)),
                Some(crate::keymap::hint("Save it, then ⌘⇧, reloads").into()),
                cx,
            ),
            Err(e) => self.show_toast(e, None, cx),
        }
    }

    /// Apply `prefs` to the UI and write the changed keys to the config file.
    pub(super) fn apply_prefs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.drop_preview_overlay(); // --- preview ---
        self.apply_prefs_live(window, cx);
        self.save_prefs(cx);
    }

    pub(super) fn apply_prefs_live(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_theme(window, cx);
        crate::capture::prefs_changed(&self.prefs, cx);
        cx.notify();
    }

    /// Resolve the theme for the prefs and this window's appearance, and
    /// take its palette and suggested fonts.
    pub(super) fn apply_theme(&mut self, window: &Window, cx: &mut Context<Self>) {
        // --- preview --- the window's appearance, or the slot being tried.
        let appearance = self.preview_appearance(window);
        let theme = crate::theme::resolve(&self.prefs, appearance, cx);
        self.prefs.adopt_theme_fonts(&theme);
        crate::fonts::ensure(self.prefs.writing().bundled, cx);
        crate::fonts::ensure(self.prefs.ui().bundled, cx);
        crate::fonts::ensure(theme.font_chrome.and_then(|f| f.bundled), cx);
        self.palette = theme.palette;
        crate::theme::set_reader_theme(&theme, self.prefs.writing());
        self.theme = theme;
    }

    pub(super) fn open_settings(
        &mut self,
        _: &OpenSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.sheet.is::<SettingsSheet>() {
            self.close_sheet(window, cx);
            return;
        }
        let current = self.prefs.hotkey.clone();
        let hotkey = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(current)
                .placeholder("ctrl+alt+B")
        });
        let sub = cx.subscribe_in(&hotkey, window, |this, input, ev, window, cx| {
            if let InputEvent::PressEnter { .. } = ev {
                let text = input.read(cx).value().to_string();
                this.apply_hotkey(text, window, cx);
            }
        });
        self._subs.push(sub);
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.settings_preview_reset(window, cx); // --- preview ---
        self.sheet.open(SettingsSheet {
            hotkey,
            hotkey_error: None,
            focus,
            scroll: ScrollHandle::new(),
        });
        self.changelog_settings_refresh(cx); // --- reader view / changelog ---
        self.background_rules_refresh(cx); // --- background ---
        cx.notify();
    }

    pub(super) fn apply_hotkey(
        &mut self,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let clash = crate::keymap::clash_with_hotkey(&text).map(|k| {
            format!(
                "{} is already {} in Burrow",
                crate::keymap::glyphs(k.key),
                k.label
            )
        });
        let set = match clash {
            Some(e) => Err(e),
            None => crate::capture::set_hotkey(&text, cx),
        };
        let err = match set {
            Ok(()) => {
                self.prefs.hotkey = text.trim().to_string();
                self.apply_prefs(window, cx);
                self.show_toast(
                    format!(
                        "Quick capture is now {}",
                        prefs::hotkey_glyphs(&self.prefs.hotkey)
                    ),
                    None,
                    cx,
                );
                None
            }
            Err(e) => Some(e),
        };
        if let Some(SettingsSheet {
            hotkey_error,
            focus,
            ..
        }) = self.sheet.get_mut()
        {
            *hotkey_error = err;
            if hotkey_error.is_none() {
                window.focus(focus, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn font_bigger(
        &mut self,
        _: &FontBigger,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.drop_preview_overlay(); // --- preview ---
        self.prefs.bigger();
        self.apply_prefs(window, cx);
    }
    pub(super) fn font_smaller(
        &mut self,
        _: &FontSmaller,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.drop_preview_overlay(); // --- preview ---
        self.prefs.smaller();
        self.apply_prefs(window, cx);
    }
    pub(super) fn font_reset(
        &mut self,
        _: &FontReset,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.drop_preview_overlay(); // --- preview ---
        self.prefs.reset_size();
        self.apply_prefs(window, cx);
    }
}
