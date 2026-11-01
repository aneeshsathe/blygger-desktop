//! The blyg switcher: several signed-in blygs, one active.
//!
//! The title bar's centre shows the active blyg (its avatar, when cached,
//! and its host) as a button. Clicking it, Blyg › Switch Blyg… (⌃⌘0) or the
//! menu drops the switcher sheet from the title: every signed-in blyg (`1`…`9`
//! or ⌃⌘1…⌃⌘9 switch to one), "Add a blyg…" (the Connect sheet, which keeps
//! the others) and "Manage…" (sign out of one, keeping or deleting its posts
//! on this Mac).
//!
//! Switching is instant: the editor's text is saved first (it autosaves on
//! every keystroke; this catches the last one), `connection::switch_to` swaps
//! the `SwitchBackend`'s inner backend and points `blyg-url` at the new blyg,
//! and the posts list, reading, mentions and subscriptions reload from the new
//! blyg's database. Sheets that belong to the old blyg close.
//!
//! The blyg switched away from is closed: its sync worker finishes the pass
//! it's in and stops. Anything still in its outbox stays in its own database
//! and syncs when it's active again (the toast says so). One sync worker at a
//! time means one set of conflict prompts, events that can't reach the wrong
//! blyg's views, and no network writes to a blyg that isn't on screen.
//!
//! A child module of `app` (it reads `MainView`); `app.rs` keeps small hooks
//! marked `// --- blyg switcher ---`. The list itself lives in the config
//! file (`blyg-core::config::blygs`).

use blyg_core::config::blygs;
use blyg_core::state::{AppState, BlygCard};

use super::*;

gpui_kit::actions!(
    blygger,
    [
        ShowBlygSwitcher,
        AddBlyg,
        ManageBlygs,
        SwitchBlyg1,
        SwitchBlyg2,
        SwitchBlyg3,
        SwitchBlyg4,
        SwitchBlyg5,
        SwitchBlyg6,
        SwitchBlyg7,
        SwitchBlyg8,
        SwitchBlyg9
    ]
);

/// How many blygs have a shortcut (⌃⌘1…⌃⌘9).
pub const SHORTCUTS: usize = 9;

/// One blyg as the switcher lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub url: String,
    pub host: String,
    /// The author's name or site title, from when it was last active.
    pub name: Option<String>,
    pub avatar: Option<String>,
    pub active: bool,
}

impl Entry {
    /// "Field notes", else the host.
    pub fn label(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.host)
    }
}

/// The switcher sheet.
pub struct BlygSheet {
    pub focus: FocusHandle,
    pub entries: Vec<Entry>,
    /// The highlighted row (↑/↓, ⏎).
    pub sel: usize,
    /// "Manage…": each row offers Sign out.
    pub manage: bool,
    /// Signing out of this row: `1` keeps its posts, `2` deletes them.
    pub confirm: Option<usize>,
}

/// Every signed-in blyg, with what was remembered of each.
pub fn entries(cx: &App) -> Vec<Entry> {
    let active = active_url(cx);
    let cards = cards(cx);
    crate::settings::known_blygs(cx)
        .into_iter()
        .map(|url| {
            let card = cards
                .get(&blygs::identity(&url))
                .cloned()
                .unwrap_or_default();
            Entry {
                host: vm::url_host(&url).unwrap_or_else(|| url.clone()),
                active: active.as_deref().is_some_and(|a| blygs::same(a, &url)),
                name: card.name.filter(|n| !n.trim().is_empty()),
                avatar: card.avatar,
                url,
            }
        })
        .collect()
}

/// The active blyg (`blyg-url`); `None` without one, or without the config
/// global (some headless tests).
fn active_url(cx: &App) -> Option<String> {
    cx.try_global::<crate::settings::AppConfig>()
        .and_then(|c| c.store.config().blyg_url().map(str::to_string))
}

/// What the switcher button adds to the plain title's width: the avatar,
/// the chevron, their gaps and the padding.
pub fn title_extra(cx: &App) -> f32 {
    if active_url(cx).is_some() {
        16. + 6. + 6. + 8. + 16.
    } else {
        0.
    }
}

fn state_dir(cx: &App) -> Option<std::path::PathBuf> {
    cx.try_global::<crate::connection::Connection>()
        .map(|c| c.data_dir.clone())
}

fn cards(cx: &App) -> std::collections::BTreeMap<String, BlygCard> {
    state_dir(cx)
        .map(|d| AppState::load(&d).blyg_cards)
        .unwrap_or_default()
}

/// Blyg › Switch To: the first nine blygs (⌃⌘1…⌃⌘9 show as their keys), the
/// active one checked.
pub fn menu_items(entries: &[Entry]) -> Vec<gpui_kit::MenuItem> {
    use gpui_kit::MenuItem;
    if entries.is_empty() {
        return vec![MenuItem::action("No Blygs Yet: Add a Blyg…", AddBlyg)];
    }
    entries
        .iter()
        .take(SHORTCUTS)
        .enumerate()
        .map(|(i, e)| {
            let name = match &e.name {
                Some(n) => format!("{n} ({})", e.host),
                None => e.host.clone(),
            };
            let item = match i {
                0 => MenuItem::action(name, SwitchBlyg1),
                1 => MenuItem::action(name, SwitchBlyg2),
                2 => MenuItem::action(name, SwitchBlyg3),
                3 => MenuItem::action(name, SwitchBlyg4),
                4 => MenuItem::action(name, SwitchBlyg5),
                5 => MenuItem::action(name, SwitchBlyg6),
                6 => MenuItem::action(name, SwitchBlyg7),
                7 => MenuItem::action(name, SwitchBlyg8),
                _ => MenuItem::action(name, SwitchBlyg9),
            };
            item.checked(e.active)
        })
        .collect()
}

/// The `n`th blyg's shortcut, as the menu and the sheet show it.
pub fn shortcut(n: usize) -> Option<String> {
    (n < SHORTCUTS).then(|| crate::keymap::glyphs(&format!("ctrl-cmd-{}", n + 1)))
}

impl MainView {
    /// Hook: the switcher's actions on the main window.
    pub(super) fn switcher_actions(
        &self,
        d: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        macro_rules! nth {
            ($d:expr, $($action:ident => $n:expr),*) => {
                $d$(.on_action(cx.listener(|this, _: &$action, window, cx| {
                    this.switch_nth($n, window, cx)
                })))*
            };
        }
        let d = d
            .on_action(cx.listener(|this, _: &ShowBlygSwitcher, window, cx| {
                this.toggle_blyg_switcher(false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ManageBlygs, window, cx| {
                this.toggle_blyg_switcher(true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &AddBlyg, window, cx| this.add_blyg(window, cx)));
        nth!(d,
            SwitchBlyg1 => 0, SwitchBlyg2 => 1, SwitchBlyg3 => 2,
            SwitchBlyg4 => 3, SwitchBlyg5 => 4, SwitchBlyg6 => 5,
            SwitchBlyg7 => 6, SwitchBlyg8 => 7, SwitchBlyg9 => 8)
    }

    /// Open (or close) the switcher sheet; `manage` opens it on "Manage…".
    pub(crate) fn toggle_blyg_switcher(
        &mut self,
        manage: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(Sheet::Blygs(s)) = &mut self.sheet {
            if s.manage == manage {
                return self.close_sheet(window, cx);
            }
            s.manage = manage;
            s.confirm = None;
            return cx.notify();
        }
        self.remember_blyg_card(cx);
        let entries = entries(cx);
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.sheet_gen += 1;
        self.sheet = Some(Sheet::Blygs(BlygSheet {
            sel: entries.iter().position(|e| e.active).unwrap_or(0),
            focus,
            entries,
            manage,
            confirm: None,
        }));
        cx.notify();
    }

    /// "Add a blyg…": the Connect sheet with an empty address. Signing in
    /// adds the blyg to the list and switches to it; the others stay.
    pub(crate) fn add_blyg(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tutorial_keeps_blyg(cx) {
            return; // --- onboarding --- the tour's sample blyg only
        }
        self.open_connect(window, cx);
        if let Some(Sheet::Connect { url, .. }) = &self.sheet {
            url.update(cx, |s, cx| s.set_value("", window, cx));
        }
    }

    pub(crate) fn switch_nth(&mut self, n: usize, window: &mut Window, cx: &mut Context<Self>) {
        match crate::settings::known_blygs(cx).get(n) {
            Some(url) => self.switch_blyg(url.clone(), window, cx),
            None if n == 0 => self.show_toast("Connect a blyg first", None, cx),
            None => self.show_toast(
                format!("There's no blyg {} in the switcher", n + 1),
                Some("Add one from the switcher: click the blyg's name in the title bar".into()),
                cx,
            ),
        }
    }

    /// Make `url` the active blyg.
    pub(crate) fn switch_blyg(&mut self, url: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.tutorial_keeps_blyg(cx) {
            return; // --- onboarding --- the tour's sample blyg only
        }
        // Never lose text: the editor autosaves on every change, but save
        // whatever it holds now in case the last edit hasn't landed.
        self.flush_editor(cx);
        self.remember_blyg_card(cx);
        let old_host = active_url(cx).and_then(|u| vm::url_host(&u));
        let pending = match self.backend.sync_status() {
            SyncStatus::Offline { pending } => pending,
            SyncStatus::Saving | SyncStatus::Syncing | SyncStatus::Error => 1,
            SyncStatus::Synced => 0,
        };
        let host = vm::url_host(&url).unwrap_or_else(|| url.clone());
        match crate::connection::switch_to(&url, cx) {
            Ok(crate::connection::Switched::Same) => {
                if matches!(self.sheet, Some(Sheet::Blygs(_))) {
                    self.close_sheet(window, cx);
                }
            }
            Ok(crate::connection::Switched::Changed) => {
                self.blyg_switched(window, cx);
                let sub = match (pending, old_host) {
                    (0, _) | (_, None) => None,
                    (_, Some(old)) => Some(
                        format!("Changes waiting on {old} sync when you switch back to it").into(),
                    ),
                };
                self.show_toast(format!("Switched to {host}"), sub, cx);
            }
            Ok(crate::connection::Switched::NeedsSignIn) => {
                self.open_connect(window, cx);
                if let Some(Sheet::Connect {
                    url: field, error, ..
                }) = &mut self.sheet
                {
                    field.update(cx, |s, cx| s.set_value(url.clone(), window, cx));
                    *error = Some(format!(
                        "Burrow has no sign-in for {host} any more. Sign in again to switch to it."
                    ));
                }
                cx.notify();
            }
            Err(e) => self.show_toast(e, None, cx),
        }
    }

    /// Save the editor's text if the backend doesn't have it yet.
    fn flush_editor(&mut self, cx: &mut Context<Self>) {
        let Some(cur) = self.current.clone() else {
            return;
        };
        let text = self.editor.read(cx).value().to_string();
        let stored = self.backend.item(&cur.local_id).map(|i| i.content_md);
        if stored.as_deref() != Some(text.as_str()) {
            self.after_edit(cx);
        }
    }

    /// The backend changed under the window (switching blygs, or signing out
    /// of the active one): drop what belonged to the old blyg and load
    /// everything from the new one.
    pub(crate) fn blyg_switched(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sheet = None;
        self.leave_new_post(window, cx); // --- new post ---
        self.current = None;
        self.restoring = None;
        self.mode = Mode::Search;
        self.set_editor_text("", window, cx);
        self.profiles = Default::default(); // --- profiles ---
        self.base_url = self.backend.base_url();
        self.unread = self
            .backend
            .reading()
            .iter()
            .filter(|r| r.is_unread())
            .count();
        // --- reading & versions --- a fresh reading state from the new
        // blyg, on the same screen.
        let view = self.reading.view;
        self.reading = reading::State::new(&*self.backend, cx);
        self.refresh_problems(cx);
        self.remember_blyg_card(cx);
        if self.backend.sync_status() != SyncStatus::Synced || self.backend.items().is_empty() {
            self.initial_sync(None, window, cx);
        }
        if view == reading::View::Posts {
            self.focus_after_sheet(window, cx);
            self.requery(window, cx);
        } else {
            self.requery(window, cx);
            self.show_view(view, window, cx);
        }
        cx.notify();
    }

    /// Remember the active blyg's name and avatar (its cached profile) for
    /// the switcher, which can't read the other blygs' databases.
    fn remember_blyg_card(&self, cx: &App) {
        let (Some(url), Some(dir)) = (active_url(cx), state_dir(cx)) else {
            return;
        };
        let Some(p) = self
            .backend
            .cached_profile(&url)
            .or_else(|| self.backend.cached_profile(&format!("{url}/")))
        else {
            return;
        };
        let card = BlygCard {
            name: p.name.or(p.title).filter(|n| !n.trim().is_empty()),
            avatar: p.avatar,
        };
        let key = blygs::identity(&url);
        if AppState::load(&dir).blyg_cards.get(&key) == Some(&card) {
            return;
        }
        let _ = AppState::update(&dir, |s| {
            s.blyg_cards.insert(key, card);
        });
    }

    // ------------------------------------------------------------ title bar

    /// Hook: the active blyg in the title bar's centre, as the switcher's
    /// button. `None` with no blyg (the plain title shows).
    pub(super) fn render_blyg_title(
        &self,
        ui_font: &SharedString,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let url = active_url(cx)?;
        let host = vm::url_host(&url)?;
        let card = cards(cx).remove(&blygs::identity(&url)).unwrap_or_default();
        let p = self.palette.on_bar();
        let plate = crate::ornament::title_plate(&self.theme);
        let open = matches!(self.sheet, Some(Sheet::Blygs(_)));
        let many = crate::settings::known_blygs(cx).len() > 1;
        let tip = crate::keymap::glyphs("ctrl-cmd-0");
        Some(
            div()
                .id("blyg-switcher")
                .debug_selector(|| "blyg-switcher".into())
                .flex()
                .items_center()
                .gap(px(6.))
                .px(px(8.))
                .py(px(2.))
                .rounded(px(5.))
                .cursor_pointer()
                .font_family(ui_font.clone())
                .font_weight(FontWeight::MEDIUM)
                .text_size(px(12.))
                .text_color(self.palette.bar_ink)
                .when(plate, |d| d.bg(self.palette.bar))
                .when(open, |d| d.bg(p.sel))
                .hover(|s| s.bg(p.sel))
                .child(avatar(card.avatar.as_deref(), &host, 16., &self.palette))
                .child(format!("Burrow — {host}"))
                .child(div().text_size(px(9.)).text_color(p.muted).child(if many {
                    "▾"
                } else {
                    "▿"
                }))
                .tooltip(move |_, cx| {
                    cx.new(|_| reading::Tip(format!("Switch blyg · {tip}")))
                        .into()
                })
                .on_click(cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    this.toggle_blyg_switcher(false, window, cx)
                }))
                .into_any_element(),
        )
    }

    // ------------------------------------------------------------ the sheet

    fn blyg_sheet_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Sheet::Blygs(s)) = &mut self.sheet else {
            return;
        };
        let key = ev.keystroke.key.as_str();
        let n = s.entries.len();
        if let Some(i) = s.confirm {
            let url = s.entries.get(i).map(|e| e.url.clone());
            match (key, url) {
                ("1", Some(u)) => self.sign_out_blyg(u, false, window, cx),
                ("2", Some(u)) => self.sign_out_blyg(u, true, window, cx),
                ("escape", _) => {
                    s.confirm = None;
                    cx.notify();
                }
                _ => return,
            }
            return cx.stop_propagation();
        }
        match key {
            "escape" => self.close_sheet(window, cx),
            "up" => {
                s.sel = s.sel.saturating_sub(1);
                cx.notify();
            }
            "down" => {
                s.sel = (s.sel + 1).min(n.saturating_sub(1));
                cx.notify();
            }
            "enter" if s.manage => {
                s.confirm = (s.sel < n).then_some(s.sel);
                cx.notify();
            }
            "enter" => {
                if let Some(e) = s.entries.get(s.sel) {
                    let url = e.url.clone();
                    self.switch_blyg(url, window, cx);
                }
            }
            "a" => self.add_blyg(window, cx),
            "m" => {
                s.manage = !s.manage;
                cx.notify();
            }
            k if !s.manage && k.len() == 1 && ("1"..="9").contains(&k) => {
                let i = k.parse::<usize>().unwrap_or(1) - 1;
                if let Some(e) = s.entries.get(i) {
                    let url = e.url.clone();
                    self.switch_blyg(url, window, cx);
                }
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    /// Sign out of one blyg from "Manage…".
    fn sign_out_blyg(
        &mut self,
        url: String,
        delete_local: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tutorial_keeps_blyg(cx) {
            return; // --- onboarding --- the tour's sample blyg only
        }
        match crate::connection::disconnect_blyg(&url, delete_local, cx) {
            Ok(out) => {
                let sub = Some(
                    if delete_local {
                        "Its sign-in and its posts on this Mac are gone"
                    } else {
                        crate::keymap::hint("Its sign-in is gone; its posts stay on this Mac")
                    }
                    .into(),
                );
                if out.was_active {
                    self.blyg_switched(window, cx);
                    if out.active.is_none()
                        || crate::connection::mode(cx)
                            == Some(crate::connection::Mode::Disconnected)
                    {
                        self.open_connect(window, cx);
                    }
                } else if let Some(Sheet::Blygs(s)) = &mut self.sheet {
                    s.entries = entries(cx);
                    s.confirm = None;
                    s.sel = s.sel.min(s.entries.len().saturating_sub(1));
                    if s.entries.is_empty() {
                        self.sheet = None;
                    }
                }
                self.show_toast(format!("Signed out of {}", out.host), sub, cx);
            }
            Err(e) => self.show_toast(e, None, cx),
        }
        cx.notify();
    }

    /// Hook: the switcher sheet's width and body (`render_sheet`).
    pub(super) fn render_blyg_sheet(
        &self,
        s: &BlygSheet,
        cx: &mut Context<Self>,
    ) -> (f32, AnyElement) {
        let p = self.palette;
        let theme = &*self.theme;
        let kbd = |k: String| {
            crate::theme_ext::kbd(div(), theme)
                .px(px(5.))
                .py(px(1.))
                .min_w(px(18.))
                .flex()
                .justify_center()
                .text_size(px(10.5))
                .child(k)
        };
        let mut body = div()
            .track_focus(&s.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                this.blyg_sheet_key(ev, window, cx)
            }))
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .mb(px(6.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(13.))
                            .child(if s.manage {
                                "Manage blygs"
                            } else {
                                "Switch blyg"
                            }),
                    )
                    .child(div().text_size(px(11.)).text_color(p.muted).child(
                        match s.entries.len() {
                            1 => "1 signed in".to_string(),
                            n => format!("{n} signed in"),
                        },
                    )),
            );
        if let Some(e) = s.confirm.and_then(|i| s.entries.get(i)) {
            let url = e.url.clone();
            let u2 = url.clone();
            body = body
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .child(format!("Sign out of {}?", e.host)),
                )
                .child(
                    div()
                        .mt(px(4.))
                        .text_color(p.muted)
                        .line_height(relative(1.45))
                        .child(crate::keymap::hint(
                            "Its password or token is removed from your Keychain and it leaves the \
                             switcher. Nothing changes on the blyg itself. Unsynced changes in a \
                             deleted local copy are lost.",
                        )),
                )
                .child(
                    div()
                        .mt(px(12.))
                        .flex()
                        .flex_wrap()
                        .gap(px(14.))
                        .text_color(p.muted)
                        .child(
                            div()
                                .id("blyg-signout-keep")
                                .cursor_pointer()
                                .flex()
                                .gap(px(5.))
                                .child(kbd("1".into()))
                                .child(crate::keymap::hint("sign out, keep its posts on this Mac"))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.sign_out_blyg(url.clone(), false, window, cx)
                                })),
                        )
                        .child(
                            div()
                                .id("blyg-signout-delete")
                                .cursor_pointer()
                                .flex()
                                .gap(px(5.))
                                .text_color(p.over_text())
                                .child(kbd("2".into()))
                                .child("also delete the local copy")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.sign_out_blyg(u2.clone(), true, window, cx)
                                })),
                        )
                        .child(
                            div()
                                .id("blyg-signout-cancel")
                                .cursor_pointer()
                                .flex()
                                .gap(px(5.))
                                .child(kbd("esc".into()))
                                .child("cancel")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(Sheet::Blygs(s)) = &mut this.sheet {
                                        s.confirm = None;
                                    }
                                    cx.notify();
                                })),
                        ),
                );
            return (420., body.into_any_element());
        }
        for (i, e) in s.entries.iter().enumerate() {
            let url = e.url.clone();
            let manage = s.manage;
            let selected = i == s.sel;
            body = body.child(
                div()
                    .id(SharedString::from(format!("blyg-row-{i}")))
                    .debug_selector(move || format!("blyg-row-{i}"))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(8.))
                    .py(px(6.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .when(selected, |d| d.bg(p.sel))
                    .hover(|d| d.bg(p.sel))
                    .child(avatar(e.avatar.as_deref(), &e.host, 26., &p))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .truncate()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(if selected { p.sel_ink } else { p.ink })
                                    .child(e.label().to_string()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(11.))
                                    .text_color(if selected { p.sel_muted } else { p.muted })
                                    .child(if e.active {
                                        format!("{} · writing here now", e.host)
                                    } else {
                                        e.host.clone()
                                    }),
                            ),
                    )
                    .when(e.active && !manage, |d| {
                        d.child(div().text_color(p.accent).text_size(px(12.)).child("✓"))
                    })
                    .when(!manage, |d| d.children(shortcut(i).map(kbd)))
                    .when(manage, |d| {
                        d.child(
                            div()
                                .text_size(px(11.5))
                                .text_color(p.over_text())
                                .child("Sign out…"),
                        )
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if manage {
                            if let Some(Sheet::Blygs(s)) = &mut this.sheet {
                                s.sel = i;
                                s.confirm = Some(i);
                            }
                            cx.notify();
                        } else {
                            this.switch_blyg(url.clone(), window, cx)
                        }
                    })),
            );
        }
        let action = |id: &'static str, k: &'static str, label: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .flex()
                .items_center()
                .gap(px(6.))
                .px(px(8.))
                .py(px(5.))
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|d| d.bg(p.sel))
                .child(kbd(k.into()))
                .child(label)
        };
        body = body.child(div().my(px(6.)).h(px(1.)).bg(p.line)).child(
            div()
                .flex()
                .gap(px(4.))
                .text_color(p.muted)
                .child(
                    action("blyg-add", "A", "Add a blyg…")
                        .on_click(cx.listener(|this, _, window, cx| this.add_blyg(window, cx))),
                )
                .child(
                    action(
                        "blyg-manage",
                        "M",
                        if s.manage { "Done" } else { "Manage…" },
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(Sheet::Blygs(s)) = &mut this.sheet {
                            s.manage = !s.manage;
                            s.confirm = None;
                        }
                        cx.notify();
                    })),
                )
                .child(div().flex_1())
                .child(
                    action("blyg-close", "esc", "close")
                        .on_click(cx.listener(|this, _, window, cx| this.close_sheet(window, cx))),
                ),
        );
        (420., body.into_any_element())
    }
}

/// The blyg's avatar from the image cache, else its initial in a circle.
fn avatar(url: Option<&str>, host: &str, size: f32, p: &crate::theme::Palette) -> AnyElement {
    if let Some(path) = url.and_then(crate::images::cached) {
        return img(ImageSource::Resource(Resource::Path(path.into())))
            .size(px(size))
            .rounded_full()
            .into_any_element();
    }
    if let Some(u) = url {
        // Not cached yet: ask for it (it repaints when it lands).
        let _ = crate::images::state(u);
    }
    let initial: String = host
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "·".into());
    div()
        .size(px(size))
        .flex_none()
        .rounded_full()
        .bg(p.accent)
        .text_color(p.bg)
        .flex()
        .items_center()
        .justify_center()
        .text_size(px((size * 0.55).round()))
        .font_weight(FontWeight::SEMIBOLD)
        .child(initial)
        .into_any_element()
}

#[cfg(test)]
#[path = "switcher_tests.rs"]
mod tests;
