//! --- palette --- ⇧⌘P, the command palette (Fable's proposal 8): every
//! action by name, from one key.
//!
//! It lists the whole keymap table (`keymap::table()`, the one source for
//! menus, buttons and tooltips), the themes, the fonts, your blygs (switch),
//! your hoppers, your posts by title (a quick-open, from any screen), and the
//! extensions' commands as one group. Typing fuzzy-matches each row's label,
//! its longer description and a few aliases ("undo edits" finds Discard
//! Changes). Every row shows its key as this platform spells it, so the
//! palette teaches the keys as the buttons do; a disabled action stays listed
//! with the reason the toolbar gives. With the field empty the last ten
//! commands come first (`state.json`'s `palette_recent`).
//!
//! ⏎ runs the highlighted row, ⌘⏎ runs it and keeps the palette open, ↑/↓
//! move, esc closes. A child module of `app`; `app.rs` keeps small hooks
//! marked `// --- palette ---`.

use std::path::Path;

use super::*;
use crate::app::extensions::RowAction as ExtAction;

gpui_kit::actions!(
    blygger,
    [
        /// ⇧⌘P: the command palette.
        ShowPalette
    ]
);

/// How many recent commands the empty field shows (and `state.json` keeps).
pub const RECENT: usize = 10;

/// What a row does.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Cmd {
    /// A keymap row's action, by name (`keymap::Keybind::action`).
    Action(&'static str),
    Theme(String),
    WritingFont(&'static str),
    UiFont(&'static str),
    /// Switch to this blyg (its URL).
    Blyg(String),
    /// Show this hopper's posts in the Reader.
    Hopper(String),
    /// Open one of your posts in the editor.
    Post(LocalId),
    /// An extension's command, library or Manage extensions.
    Ext(ExtAction),
}

/// One row.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Entry {
    /// Stable across launches, for the recents (`action:Publish`).
    pub id: String,
    pub cmd: Cmd,
    /// "Post", "View", "Themes", "Posts", "Extensions"…
    pub group: &'static str,
    pub label: String,
    /// The longer line under it (the keymap table's description).
    pub detail: String,
    /// Other words that find it.
    pub aliases: &'static [&'static str],
    /// Its key, as this platform writes it (`⌥⌘Z`, `Ctrl+Shift+P`).
    pub key: Option<String>,
    /// Why it can't run now (it stays listed).
    pub disabled: Option<String>,
}

/// The open palette.
pub(crate) struct Open {
    pub input: Entity<InputState>,
    pub entries: Vec<Entry>,
    /// Indexes into `entries`, best first.
    pub shown: Vec<usize>,
    pub sel: usize,
    /// What had the keyboard before, to give it back.
    pub before: Option<FocusHandle>,
    pub seq: usize,
    _sub: Subscription,
}

#[derive(Default)]
pub(crate) struct State {
    pub open: Option<Open>,
    pub recent: Vec<String>,
    recent_loaded: bool,
    seq: usize,
}

/// Other words for the table's actions (proposal 8: "discard", "undo
/// edits", "revert").
pub fn aliases(action: &str) -> &'static [&'static str] {
    match action {
        "DiscardChanges" => &["discard", "undo edits", "revert", "throw away"],
        "DeleteDraft" => &["remove", "trash", "delete"],
        "Withdraw" => &["unpublish", "take down", "retract"],
        "Publish" => &["post", "ship", "send"],
        "NewDraft" => &["new", "create", "write", "compose"],
        "OpenSettings" => &["preferences", "prefs", "options", "config"],
        "ShowReading" => &["feeds", "reader", "inbox", "subscriptions"],
        "ToggleKind" => &["thread", "fragment", "kind"],
        "TogglePreview" => &["render", "html"],
        "ShowVersions" => &["history", "versions", "restore"],
        "QuotePicker" => &["quote", "transclude", "link"],
        "ShowShelf" => &["bookshelf", "favourites", "favorites"],
        "ShowBlygSwitcher" => &["switch", "account", "blogs"],
        "ShowChat" => &["irc", "thicket"],
        "FocusSearch" => &["find", "search", "omnibar"],
        "ToggleFocus" => &["zen", "typewriter", "distraction free"],
        "FontBigger" => &["zoom in", "larger"],
        "FontSmaller" => &["zoom out"],
        "Quit" => &["exit", "close"],
        _ => &[],
    }
}

/// Keymap actions the palette leaves out: itself, the tour's arrows, the
/// numbered blyg keys (the Blygs group lists the blygs by name), and the
/// fake backend's tricks unless it's the fake.
fn skipped(action: &str, fake: bool) -> bool {
    matches!(
        action,
        "ShowPalette"
            | "TutorialNext"
            | "TutorialBack"
            | "SwitchBlyg1"
            | "SwitchBlyg2"
            | "SwitchBlyg3"
            | "SwitchBlyg4"
            | "SwitchBlyg5"
            | "SwitchBlyg6"
            | "SwitchBlyg7"
            | "SwitchBlyg8"
            | "SwitchBlyg9"
    ) || (!fake && matches!(action, "FakeToggleOffline" | "FakeConflict"))
}

/// The table's rows as palette rows: one per action (its menu row first),
/// main-window and global keys only (the browser pane's and the macro
/// sheets' keys act there, not here). `disabled` says why one can't run.
pub(crate) fn action_entries(
    table: &[crate::keymap::Keybind],
    fake: bool,
    disabled: &dyn Fn(&str) -> Option<String>,
) -> Vec<Entry> {
    use crate::keymap::Scope;
    let mut out: Vec<Entry> = Vec::new();
    for k in table {
        if k.new_action.is_none()
            || !matches!(k.scope, Scope::Global | Scope::Main | Scope::MainInput)
            || skipped(k.action, fake)
            || out.iter().any(|e| e.cmd == Cmd::Action(k.action))
        {
            continue;
        }
        // Prefer the action's row that has a menu (its name in the menus).
        let row = table
            .iter()
            .filter(|r| r.action == k.action)
            .find(|r| r.menu.is_some())
            .unwrap_or(k);
        let (group, label) = match row.menu {
            Some(m) => {
                let mut parts = m.split(" › ");
                let group = parts.next().unwrap_or("Commands");
                let leaf = m.rsplit(" › ").next().unwrap_or(m);
                (menu_group(group), leaf.to_string())
            }
            None => ("Commands", row.label.to_string()),
        };
        let key = table
            .iter()
            .find(|r| r.action == k.action && !r.key.is_empty())
            .map(|r| crate::keymap::glyphs(r.key));
        out.push(Entry {
            id: format!("action:{}", k.action),
            cmd: Cmd::Action(k.action),
            group,
            label,
            detail: row.label.to_string(),
            aliases: aliases(k.action),
            key,
            disabled: disabled(k.action),
        });
    }
    out
}

/// A menu's name as a group (`&'static`, as the table's are).
fn menu_group(menu: &str) -> &'static str {
    match menu {
        "Burrow" => "Burrow",
        "Post" => "Post",
        "View" => "View",
        "Blyg" => "Blyg",
        "Edit" => "Edit",
        "Help" => "Help",
        _ => "Commands",
    }
}

/// How well `query` matches `text`: `None` for no match. Every query word
/// must be in the text, as a substring (best at a word's start) or, failing
/// that, as a subsequence of letters (`dscrd` finds "Discard").
pub fn score_text(query: &str, text: &str) -> Option<i64> {
    let text = text.to_lowercase();
    let mut total = 0i64;
    for word in query.split_whitespace() {
        let word = word.to_lowercase();
        total += match text.find(&word) {
            Some(0) => 300,
            Some(i) if !text[..i].ends_with(char::is_alphanumeric) => 200,
            Some(_) => 120,
            None => {
                let mut it = text.chars();
                let mut gaps = 0i64;
                for c in word.chars() {
                    let mut skipped = 0;
                    loop {
                        match it.next() {
                            Some(t) if t == c => break,
                            Some(_) => skipped += 1,
                            None => return None,
                        }
                    }
                    gaps += skipped;
                }
                (60 - gaps).max(1)
            }
        };
    }
    Some(total)
}

/// An entry's best score over its label (weighted), its aliases and its
/// description, and its group's name ("theme paper").
pub fn score(query: &str, e: &Entry) -> Option<i64> {
    let label = score_text(query, &e.label).map(|s| s * 3);
    let alias = e
        .aliases
        .iter()
        .filter_map(|a| score_text(query, a))
        .max()
        .map(|s| s * 2);
    let detail = score_text(query, &e.detail);
    let grouped = score_text(query, &format!("{} {}", e.group, e.label));
    [label, alias, detail, grouped].into_iter().flatten().max()
}

/// What the list shows for `query`: with an empty field, the recent
/// commands first (in order), then the rest (posts only once you type);
/// otherwise the matches, best first, enabled before disabled at a tie.
pub(crate) fn filter(entries: &[Entry], query: &str, recent: &[String]) -> Vec<usize> {
    if query.trim().is_empty() {
        let mut out: Vec<usize> = recent
            .iter()
            .filter_map(|id| entries.iter().position(|e| &e.id == id))
            .collect();
        for (i, e) in entries.iter().enumerate() {
            if !out.contains(&i) && !matches!(e.cmd, Cmd::Post(_)) {
                out.push(i);
            }
        }
        return out;
    }
    let mut hits: Vec<(i64, usize)> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| score(query, e).map(|s| (s, i)))
        .collect();
    hits.sort_by_key(|&(s, i)| {
        (
            std::cmp::Reverse(s),
            entries[i].disabled.is_some(),
            // Your posts after the commands at a tie.
            matches!(entries[i].cmd, Cmd::Post(_)),
            i,
        )
    });
    hits.into_iter().map(|(_, i)| i).collect()
}

/// `id` to the front of the recents, at most [`RECENT`].
pub fn push_recent(recent: &mut Vec<String>, id: &str) {
    recent.retain(|r| r != id);
    recent.insert(0, id.to_string());
    recent.truncate(RECENT);
}

/// The data dir (`state.json`'s home), when connected.
pub(crate) fn data_dir(cx: &App) -> Option<std::path::PathBuf> {
    cx.try_global::<crate::connection::Connection>()
        .map(|c| c.data_dir.clone())
}

fn load_recent(data_dir: Option<&Path>) -> Vec<String> {
    data_dir
        .map(blyg_core::state::AppState::load)
        .map(|s| s.palette_recent)
        .unwrap_or_default()
}

fn save_recent(data_dir: Option<&Path>, recent: &[String]) {
    if let Some(d) = data_dir {
        let _ = blyg_core::state::AppState::update(d, |s| s.palette_recent = recent.to_vec());
    }
}

impl MainView {
    /// Hook: the palette's action on the main window.
    pub(super) fn palette_actions(
        &self,
        d: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        d.on_action(
            cx.listener(|this, _: &ShowPalette, window, cx| this.toggle_palette(window, cx)),
        )
    }

    /// Every row, as things are now.
    pub(crate) fn palette_entries(&self, window: &Window, cx: &App) -> Vec<Entry> {
        let facts = self.toolbar_facts(cx);
        let disabled = |a: &str| crate::app::toolbar::rule(a, &facts).0;
        let table = crate::keymap::table();
        let mut out = action_entries(&table, self.fake.is_some(), &disabled);
        // Themes.
        for (_, list) in crate::theme::themes(cx).grouped() {
            for t in list {
                let name = match t.id.as_str() {
                    "light" => "Light (Paper)".to_string(),
                    "dark" => "Dark (Ink)".to_string(),
                    _ => t.name.clone(),
                };
                let on = self.prefs.theme == t.id;
                out.push(Entry {
                    id: format!("theme:{}", t.id),
                    cmd: Cmd::Theme(t.id.clone()),
                    group: "Themes",
                    label: format!("Theme: {name}"),
                    detail: if on {
                        "The theme you're using".into()
                    } else {
                        "Colour theme (the theme config key)".into()
                    },
                    aliases: &["colour", "color", "appearance"],
                    key: None,
                    disabled: on.then(|| "Already your theme".into()),
                });
            }
        }
        if self.prefs.theme != "system" {
            out.push(Entry {
                id: "theme:system".into(),
                cmd: Cmd::Theme("system".into()),
                group: "Themes",
                label: "Theme: System".into(),
                detail: "Follow macOS light and dark".into(),
                aliases: &["colour", "color", "appearance", "auto"],
                key: None,
                disabled: None,
            });
        }
        // Fonts.
        for f in crate::prefs::WRITING_FONTS {
            let on = self.prefs.writing().label == f.label;
            out.push(Entry {
                id: format!("wfont:{}", f.label),
                cmd: Cmd::WritingFont(f.label),
                group: "Fonts",
                label: format!("Writing font: {}", f.label),
                detail: "The editor and preview's font".into(),
                aliases: &["typeface", "font"],
                key: None,
                disabled: on.then(|| "Already your writing font".into()),
            });
        }
        for f in crate::prefs::UI_FONTS {
            let on = self.prefs.ui().label == f.label;
            out.push(Entry {
                id: format!("ufont:{}", f.label),
                cmd: Cmd::UiFont(f.label),
                group: "Fonts",
                label: format!("Interface font: {}", f.label),
                detail: "The list, the omnibar and sheets".into(),
                aliases: &["typeface", "font", "ui font"],
                key: None,
                disabled: on.then(|| "Already your interface font".into()),
            });
        }
        // Blygs.
        for (n, b) in crate::app::switcher::entries(cx).into_iter().enumerate() {
            out.push(Entry {
                id: format!("blyg:{}", b.host),
                cmd: Cmd::Blyg(b.url.clone()),
                group: "Blygs",
                label: format!("Switch to {}", b.label()),
                detail: b.host.clone(),
                aliases: &["blyg", "switch"],
                key: crate::app::switcher::shortcut(n),
                disabled: b.active.then(|| "The blyg you're on".into()),
            });
        }
        // Hoppers.
        for h in self.backend.hoppers() {
            out.push(Entry {
                id: format!("hopper:{}", h.id),
                cmd: Cmd::Hopper(h.id.clone()),
                group: "Hoppers",
                label: format!("Hopper: {}", h.name),
                detail: h
                    .description
                    .clone()
                    .unwrap_or_else(|| "Its posts in the Reader".into()),
                aliases: &["collection", "hopper"],
                key: None,
                disabled: None,
            });
        }
        // Extensions.
        for r in self.ext_palette_rows(window, cx) {
            let id = match &r.action {
                ExtAction::Command { ext, id } | ExtAction::Macro { ext, id } => {
                    format!("ext:{ext}:{id}")
                }
                ExtAction::OpenSite { ext, site } => format!("ext:{ext}:site:{site}"),
                ExtAction::Library { ext, id, .. } => format!("ext:{ext}:lib:{id}"),
                ExtAction::Manage => "ext:manage".into(),
            };
            out.push(Entry {
                id,
                cmd: Cmd::Ext(r.action),
                group: "Extensions",
                label: r.label,
                detail: match r.from {
                    Some(f) if r.detail.is_empty() => f,
                    Some(f) => format!("{} · {f}", r.detail),
                    None => r.detail,
                },
                aliases: &["extension"],
                key: None,
                disabled: None,
            });
        }
        // Your posts, by title (shown once you type).
        for i in self.backend.items() {
            if i.status == blyg_core::Status::Withdrawn {
                continue;
            }
            out.push(Entry {
                id: format!("post:{}", i.local_id.0),
                group: "Posts",
                label: vm::item_title(&i),
                detail: format!("{} · {}", vm::kind_label(i.kind), vm::pill(&i).0),
                cmd: Cmd::Post(i.local_id),
                aliases: &[],
                key: None,
                disabled: None,
            });
        }
        out
    }

    /// ⇧⌘P: open the palette, or close it.
    pub(crate) fn toggle_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.cmd_palette.open.is_some() {
            return self.close_palette(window, cx);
        }
        if self.sheet.is_some()
            || self.reading.sheet.is_some()
            || self.ai.has_overlay()
            || self.ext.has_overlay()
        {
            return;
        }
        if !self.cmd_palette.recent_loaded {
            self.cmd_palette.recent = load_recent(data_dir(cx).as_deref());
            self.cmd_palette.recent_loaded = true;
        }
        let entries = self.palette_entries(window, cx);
        let shown = filter(&entries, "", &self.cmd_palette.recent);
        let before = window.focused(cx);
        let input = cx
            .new(|cx| InputState::new(window, cx).placeholder("Type a command, a theme, a post…"));
        let sub = cx.subscribe_in(
            &input,
            window,
            |this, _, ev: &InputEvent, window, cx| match ev {
                InputEvent::Change => this.palette_query_changed(cx),
                InputEvent::PressEnter { secondary, .. } => {
                    this.palette_run_selected(*secondary, window, cx)
                }
                _ => {}
            },
        );
        input.update(cx, |s, cx| s.focus(window, cx));
        self.cmd_palette.seq += 1;
        self.cmd_palette.open = Some(Open {
            input,
            entries,
            shown,
            sel: 0,
            before,
            seq: self.cmd_palette.seq,
            _sub: sub,
        });
        cx.notify();
    }

    /// Close it and give the keyboard back.
    pub(crate) fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.cmd_palette.open.take() else {
            return;
        };
        match open.before {
            Some(f) => window.focus(&f, cx),
            None => self.focus_after_sheet(window, cx),
        }
        cx.notify();
    }

    fn palette_query_changed(&mut self, cx: &mut Context<Self>) {
        let recent = self.cmd_palette.recent.clone();
        if let Some(o) = self.cmd_palette.open.as_mut() {
            let q = o.input.read(cx).value().to_string();
            o.shown = filter(&o.entries, &q, &recent);
            o.sel = 0;
            cx.notify();
        }
    }

    pub(crate) fn palette_move(&mut self, delta: isize, cx: &mut Context<Self>) {
        if let Some(o) = self.cmd_palette.open.as_mut() {
            let n = o.shown.len() as isize;
            if n > 0 {
                o.sel = (o.sel as isize + delta).rem_euclid(n) as usize;
            }
            cx.notify();
        }
    }

    /// ⏎ (or ⌘⏎, `keep`): run the highlighted row.
    pub(crate) fn palette_run_selected(
        &mut self,
        keep: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(e) = self
            .cmd_palette
            .open
            .as_ref()
            .and_then(|o| o.shown.get(o.sel).map(|&i| o.entries[i].clone()))
        else {
            return;
        };
        self.palette_run(e, keep, window, cx);
    }

    /// Run `e`. A disabled row says why, in a toast, and does nothing.
    pub(crate) fn palette_run(
        &mut self,
        e: Entry,
        keep: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(why) = &e.disabled {
            self.show_toast(format!("{}: {why}", e.label), None, cx);
            return;
        }
        push_recent(&mut self.cmd_palette.recent, &e.id);
        save_recent(data_dir(cx).as_deref(), &self.cmd_palette.recent);
        if !keep {
            self.close_palette(window, cx);
        }
        match e.cmd {
            Cmd::Action(name) => {
                if let Some(make) = crate::keymap::table()
                    .into_iter()
                    .find(|k| k.action == name)
                    .and_then(|k| k.new_action)
                {
                    if !self.focus.contains_focused(window, cx) {
                        window.focus(&self.focus, cx);
                    }
                    window.dispatch_action(make(), cx);
                }
            }
            Cmd::Theme(id) => {
                self.prefs.theme = id;
                self.apply_prefs(window, cx);
            }
            Cmd::WritingFont(label) => {
                self.prefs.writing_font = label.to_string();
                self.prefs.writing_font_set = true;
                self.apply_prefs(window, cx);
            }
            Cmd::UiFont(label) => {
                self.prefs.ui_font = label.to_string();
                self.prefs.ui_font_set = true;
                self.apply_prefs(window, cx);
            }
            Cmd::Blyg(url) => self.switch_blyg(url, window, cx),
            Cmd::Hopper(id) => {
                self.set_read_mode(reading::stream_vm::ReadMode::Reader, window, cx);
                self.select_source(reading::sources_vm::Source::Hopper(id), cx);
            }
            Cmd::Post(id) => {
                self.leave_reading();
                self.open(&id, window, cx);
            }
            Cmd::Ext(action) => self.ext_run(action, window, cx),
        }
        // Kept open: the rows' states (a theme now in use) follow.
        if keep && self.cmd_palette.open.is_some() {
            let entries = self.palette_entries(window, cx);
            let recent = self.cmd_palette.recent.clone();
            if let Some(o) = self.cmd_palette.open.as_mut() {
                let q = o.input.read(cx).value().to_string();
                o.shown = filter(&entries, &q, &recent);
                o.entries = entries;
                o.sel = o.sel.min(o.shown.len().saturating_sub(1));
                o.input.update(cx, |s, cx| s.focus(window, cx));
            }
        }
        cx.notify();
    }

    /// Hook: the palette over everything, under the title bar.
    pub(super) fn render_cmd_palette(
        &self,
        ui_font: &SharedString,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let o = self.cmd_palette.open.as_ref()?;
        let p = self.palette;
        let theme = &*self.theme;
        o.input.update(cx, |s, _| s.set_editor_style(p.field()));
        let mut last_group = "";
        let recent_n = if o.input.read(cx).value().trim().is_empty() {
            self.cmd_palette
                .recent
                .iter()
                .filter(|id| o.entries.iter().any(|e| &e.id == *id))
                .count()
        } else {
            0
        };
        let rows: Vec<AnyElement> = o
            .shown
            .iter()
            .take(80)
            .enumerate()
            .flat_map(|(n, &i)| {
                let e = &o.entries[i];
                let on = n == o.sel;
                let group = if n < recent_n { "Recent" } else { e.group };
                let heading = (group != last_group).then(|| {
                    div()
                        .pt(px(if n == 0 { 0. } else { 6. }))
                        .pb(px(2.))
                        .px(px(8.))
                        .text_size(px(10.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.muted)
                        .child(group.to_uppercase())
                        .into_any_element()
                });
                last_group = group;
                let dim = e.disabled.is_some();
                let tip = e.disabled.clone();
                let row = div()
                    .id(("palette-row", n))
                    .debug_selector(move || format!("palette-row-{n}"))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .when(on, |d| d.bg(p.pick()))
                    .when(!on, |d| d.hover(|s| s.bg(p.hover())))
                    .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                        let keep = ev.modifiers().secondary();
                        if let Some(e) = this
                            .cmd_palette
                            .open
                            .as_ref()
                            .and_then(|o| o.shown.get(n).map(|&i| o.entries[i].clone()))
                        {
                            this.palette_run(e, keep, window, cx);
                        }
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .when(dim, |d| d.text_color(p.muted))
                                    .child(e.label.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(p.muted)
                                    .truncate()
                                    .child(match &e.disabled {
                                        Some(why) => why.clone(),
                                        None => e.detail.clone(),
                                    }),
                            ),
                    )
                    .children(e.key.clone().map(|k| {
                        crate::theme_ext::kbd(div(), theme)
                            .flex_none()
                            .px(px(6.))
                            .py(px(1.))
                            .text_size(px(11.))
                            .when(dim, |d| d.opacity(0.6))
                            .child(k)
                    }))
                    .when_some(tip, |d, tip| {
                        d.tooltip(move |_, cx| cx.new(|_| reading::Tip(tip.clone())).into())
                    })
                    .into_any_element();
                heading.into_iter().chain([row])
            })
            .collect();
        let empty = o.shown.is_empty();
        let kbd = |k: &'static str| {
            crate::theme_ext::kbd(div(), theme)
                .px(px(5.))
                .text_size(px(10.5))
                .child(crate::keymap::hint(k))
        };
        let hint = |k: &'static str, l: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .child(kbd(k))
                .child(l)
        };
        let card = div()
            .id("palette")
            .debug_selector(|| "palette".into())
            .occlude()
            .w(px(560.))
            .max_w(relative(0.92))
            .map(|d| crate::theme_ext::sheet(d, theme))
            .px(px(10.))
            .py(px(10.))
            .font_family(ui_font.clone())
            .text_size(px(13.))
            .text_color(p.ink)
            .flex()
            .flex_col()
            .gap(px(6.))
            .capture_action(cx.listener(|this, _: &MoveUp, _, cx| {
                cx.stop_propagation();
                this.palette_move(-1, cx);
            }))
            .capture_action(cx.listener(|this, _: &MoveDown, _, cx| {
                cx.stop_propagation();
                this.palette_move(1, cx);
            }))
            .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                cx.stop_propagation();
                this.close_palette(window, cx);
            }))
            // ⌘⏎ in a text field is Publish (`MainInput`): here it runs the
            // row and keeps the palette.
            .capture_action(cx.listener(|this, _: &Publish, window, cx| {
                cx.stop_propagation();
                this.palette_run_selected(true, window, cx);
            }))
            .child(
                div()
                    .px(px(6.))
                    .py(px(4.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(p.line)
                    .child(gpui_kit::base::input::Input::new(&o.input)),
            )
            .child(
                div()
                    .id("palette-rows")
                    .max_h(px(380.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .children(rows)
                    .when(empty, |d| {
                        d.child(
                            div()
                                .px(px(8.))
                                .py(px(6.))
                                .text_color(p.muted)
                                .child("Nothing matches."),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(12.))
                    .px(px(4.))
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .child(hint("↑↓", "choose"))
                    .child(hint("⏎", "run"))
                    .child(hint("⌘⏎", "run, keep open"))
                    .child(hint("esc", "close")),
            );
        let gen_ = o.seq;
        let card = if crate::app::delight::reduce_motion() {
            card.into_any_element()
        } else {
            card.with_animation(
                ("palette-in", gen_),
                Animation::new(Duration::from_millis(160)).with_easing(ease_out_quint()),
                |d, t| d.mt(px(-12.0 * (1.0 - t))).opacity(0.5 + 0.5 * t),
            )
            .into_any_element()
        };
        Some(
            div()
                .absolute()
                .top(px(TITLEBAR_H + 8.))
                .bottom_0()
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .items_start()
                .child(card)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Cmd, Entry, LocalId, RECENT, action_entries, aliases, filter, push_recent, score_text,
        skipped,
    };

    fn entry(id: &str, label: &str, aliases: &'static [&'static str]) -> Entry {
        Entry {
            id: id.into(),
            cmd: Cmd::Theme(id.into()),
            group: "Themes",
            label: label.into(),
            detail: String::new(),
            aliases,
            key: None,
            disabled: None,
        }
    }

    #[test]
    fn fuzzy_matches_words_starts_and_letters() {
        assert!(score_text("dis", "Discard Changes…").is_some());
        assert!(score_text("dscrd", "Discard Changes…").is_some());
        assert!(score_text("zzz", "Discard Changes…").is_none());
        // A word's start beats the middle of a word, which beats letters.
        let start = score_text("pub", "Publish…").unwrap();
        let mid = score_text("ish", "Publish…").unwrap();
        let loose = score_text("pbl", "Publish…").unwrap();
        assert!(start > mid && mid > loose, "{start} {mid} {loose}");
        // Every word must match.
        assert!(score_text("discard post", "Discard Changes…").is_none());
    }

    #[test]
    fn aliases_find_actions_and_labels_rank_first() {
        let es = vec![
            entry("a", "Discard Changes…", aliases("DiscardChanges")),
            entry("b", "Delete Draft…", aliases("DeleteDraft")),
        ];
        assert_eq!(filter(&es, "undo edits", &[]), [0]);
        assert_eq!(filter(&es, "revert", &[]), [0]);
        assert_eq!(filter(&es, "del", &[]).first(), Some(&1));
    }

    #[test]
    fn the_empty_field_shows_recent_first_and_no_posts() {
        let mut es = vec![entry("a", "One", &[]), entry("b", "Two", &[])];
        es.push(Entry {
            cmd: Cmd::Post(LocalId("p".into())),
            ..entry("post:p", "A post", &[])
        });
        assert_eq!(filter(&es, "", &["b".into()]), [1, 0]);
        assert_eq!(filter(&es, "post", &[]), [2]);
    }

    #[test]
    fn recents_are_ten_newest_first() {
        let mut r = Vec::new();
        for i in 0..12 {
            push_recent(&mut r, &format!("x{i}"));
        }
        push_recent(&mut r, "x5");
        assert_eq!(r.len(), RECENT);
        assert_eq!(r[0], "x5");
        assert_eq!(r[1], "x11");
        assert_eq!(r.iter().filter(|x| *x == "x5").count(), 1);
    }

    #[test]
    fn the_table_becomes_one_row_per_action_with_keys() {
        let table = crate::keymap::table();
        let es = action_entries(&table, false, &|a| {
            (a == "Publish").then(|| "Select a post first".into())
        });
        let publish: Vec<_> = es
            .iter()
            .filter(|e| e.cmd == Cmd::Action("Publish"))
            .collect();
        assert_eq!(publish.len(), 1, "one row per action");
        assert_eq!(publish[0].label, "Publish…");
        assert_eq!(publish[0].group, "Post");
        assert_eq!(
            publish[0].key.as_deref(),
            Some(crate::keymap::glyphs(if cfg!(target_os = "windows") {
                "ctrl-enter"
            } else {
                "cmd-enter"
            }))
            .as_deref()
        );
        assert_eq!(publish[0].disabled.as_deref(), Some("Select a post first"));
        // Menu-only rows are listed too, keyless.
        let withdraw = es
            .iter()
            .find(|e| e.cmd == Cmd::Action("Withdraw"))
            .unwrap();
        assert_eq!(withdraw.key, None);
        // Not the palette itself, the tour's arrows, the browser pane's keys
        // or the fake's tricks.
        for a in [
            "ShowPalette",
            "TutorialNext",
            "BrowserBack",
            "FakeConflict",
            "SwitchBlyg1",
        ] {
            assert!(!es.iter().any(|e| e.cmd == Cmd::Action(a)), "{a}");
        }
        // Every main-window action with a menu is in it.
        for k in &table {
            if k.menu.is_some()
                && matches!(
                    k.scope,
                    crate::keymap::Scope::Main | crate::keymap::Scope::Global
                )
                && !skipped(k.action, false)
            {
                assert!(
                    es.iter().any(|e| e.cmd == Cmd::Action(k.action)),
                    "{} missing",
                    k.action
                );
            }
        }
    }
}
