//! --- hoppers --- The blyg's hoppers in the Reader (blygger-studio's
//! hoppers: named collections of posts, theirs and yours).
//!
//! - **View**: a "Hoppers" section in the sources pane, under Background
//!   (and Your blyg), each hopper with its count and a mark when it's
//!   public. Picking one lists its posts: the held rows whose `hoppers`
//!   name it, your own posts in it (`~own`), and any post only the hopper
//!   read returned (`State::hopper_only`). Withdrawn posts stay under
//!   Background, as the studio's hopper timelines leave them out.
//! - **Add**: Hopper › on a post (the row menu, the reading pane's chip,
//!   `h` on the reading screen, Post › Put in Hopper… ⌥⌘P) lists every
//!   hopper with a check; choosing one puts the post in it or takes it out,
//!   and New Hopper… makes one and puts the post in it. With posts picked
//!   in the list (⌘-click, ⇧-click, ⌘A), `h`, ⌥⌘P and a picked row's menu
//!   act on all of them: a hopper holding every one is checked (some: a
//!   dash), and choosing it takes them all out, else puts the rest in.
//!   Membership is local and instant, and queued for the blyg
//!   (`Backend::set_in_hopper`): offline, it waits in the outbox.
//! - **Order**: a hopper lists its posts in its own order, newest added
//!   first (`Backend::hopper_order`), as the studio does.
//! - **Manage**: a hopper's menu in the pane has Rename…, Description…,
//!   Make Public… / Make Private and Delete Hopper…. These go to the blyg
//!   first (`Backend::update_hopper`, `delete_hopper`) and say so when it
//!   can't be reached. Making a hopper public publishes a page, so it asks
//!   first; deleting asks too, and never touches the posts.

use std::collections::HashSet;

use blyg_core::{Backend, HopperEdit, ReadingItem};
use gpui_kit::base::input::{Escape, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::sources::MenuCmd;
use super::vm::{self, Key};
use super::{RSheet, View};
use crate::app::MainView;

gpui_kit::actions!(blygger, [NewHopper, HopperPicker]);

/// A hopper: a funnel over a tray.
pub const ICON_HOPPER: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linejoin="round" stroke-linecap="round"><path d="M4 4.5h16l-5.5 7v4.5l-5 2.5v-7z"/><path d="M3.5 20.5h17"/></svg>"#;
/// Public: a small globe.
pub const ICON_PUBLIC: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><circle cx="12" cy="12" r="8.5"/><path d="M3.5 12h17M12 3.5c2.6 2.4 3.8 5.2 3.8 8.5s-1.2 6.1-3.8 8.5c-2.6-2.4-3.8-5.2-3.8-8.5s1.2-6.1 3.8-8.5z"/></svg>"#;

/// What making a hopper public means (the studio's own words, near enough).
pub const PUBLIC_NOTE: &str = "A public hopper gets its own page on your blyg and is listed under \
     Collections on your homepage and archive. It never appears in your feed. The page shows \
     every post in it (yours once published), and its address stays put if you rename it.";

/// What the name sheet is for.
#[derive(Debug, Clone, PartialEq)]
pub enum NameFor {
    /// New Hopper…; then put these posts in it (none: just make it).
    New {
        then_add: Vec<Key>,
    },
    Rename(String),
    Describe(String),
}

/// What the confirm sheet asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirm {
    /// Make Public… (it publishes a page).
    Public,
    /// Delete Hopper… (the posts stay).
    Delete,
}

/// The posts only a hopper holds (this Mac doesn't hold them as reading
/// rows), as rows: read at their version, so they never count as unread.
pub fn unheld_rows(backend: &dyn Backend) -> Vec<ReadingItem> {
    backend
        .hopper_unheld()
        .into_iter()
        .map(|mut r| {
            r.read_version = Some(r.version);
            r
        })
        .collect()
}

/// The keys of `unheld_rows`.
pub fn unheld_keys(backend: &dyn Backend) -> HashSet<Key> {
    backend.hopper_unheld().iter().map(vm::key).collect()
}

/// Order `shown` (indices into `rows`) as the hopper orders its posts
/// (`order`, newest added first). A row the hopper doesn't name (one kept
/// listed after it left) goes last; ties keep the list's order.
pub fn in_hopper_order(rows: &[ReadingItem], shown: &mut [usize], order: &[Key]) {
    let rank: std::collections::HashMap<&Key, usize> =
        order.iter().enumerate().map(|(i, k)| (k, i)).collect();
    shown.sort_by_cached_key(|&i| {
        rows.get(i)
            .and_then(|r| rank.get(&vm::key(r)).copied())
            .unwrap_or(usize::MAX)
    });
}

/// A hopper's public page: `{blyg}/h/{slug}/`.
pub fn public_url(blyg: &str, slug: &str) -> String {
    format!("{}/h/{slug}/", blyg.trim_end_matches('/'))
}

/// Can `key` go in a hopper from the reader? A held post (theirs or
/// yours) that isn't withdrawn, and not one shown without a subscription.
pub fn can_hopper(view: &MainView, key: &Key) -> bool {
    !super::original::is_external(key)
        && view
            .reading
            .rows
            .iter()
            .find(|r| &vm::key(r) == key)
            .is_some_and(|r| !super::sources_vm::is_background(r))
}

impl MainView {
    /// Hook: the actions on the root element.
    pub(super) fn hopper_actions(&self, d: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        d.on_action(cx.listener(|this, _: &NewHopper, window, cx| {
            this.open_hopper_name_sheet(NameFor::New { then_add: vec![] }, window, cx)
        }))
        .on_action(
            cx.listener(|this, _: &HopperPicker, window, cx| this.open_hopper_picker(window, cx)),
        )
    }

    /// What the picker acts on in the reading list: the picked posts that
    /// can go in a hopper, in the list's order, with how the sheet names
    /// them. `None` with nothing picked.
    fn hopper_picks(&self) -> Option<(Vec<Key>, String)> {
        if self.reading.view != View::Reading || self.reading.picked.is_empty() {
            return None;
        }
        let keys: Vec<Key> = self
            .reading
            .picked
            .targets(&self.shown_keys(), None)
            .into_iter()
            .filter(|k| can_hopper(self, k))
            .collect();
        let title = match keys.as_slice() {
            [one] => self
                .reading
                .rows
                .iter()
                .find(|r| &vm::key(r) == one)
                .map(|r| vm::title(&r.content_md))
                .unwrap_or_default(),
            many => format!("{} posts", many.len()),
        };
        Some((keys, title))
    }

    /// The post in front of you: the open one, else the selected one; on
    /// Posts, your published post. With its title.
    fn hopper_target(&self) -> Option<(Key, String)> {
        match self.reading.view {
            View::Reading => {
                let key = self
                    .reading
                    .opened
                    .as_ref()
                    .map(|o| o.key.clone())
                    .or_else(|| self.reading.sel.clone())?;
                let row = self.reading.rows.iter().find(|r| vm::key(r) == key)?;
                Some((key, vm::title(&row.content_md)))
            }
            View::Posts => {
                let row = self.current_own_row()?;
                Some((vm::key(&row), vm::title(&row.content_md)))
            }
            _ => None,
        }
    }

    /// `h` / ⌥⌘P: the picker for the posts picked in the list, else for
    /// the post in front of you.
    pub(crate) fn open_hopper_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((keys, title)) = self.hopper_picks() {
            if keys.is_empty() {
                self.show_toast("These posts can't go in a hopper", None, cx);
                return;
            }
            return self.open_hopper_picker_on(keys, title, window, cx);
        }
        let Some((key, title)) = self.hopper_target() else {
            let hint = if self.reading.view == View::Posts {
                "Open one of your published posts first"
            } else {
                "Open or select a post first"
            };
            self.show_toast(hint, None, cx);
            return;
        };
        if self.reading.view == View::Reading && !can_hopper(self, &key) {
            self.show_toast("This post can't go in a hopper", None, cx);
            return;
        }
        self.open_hopper_picker_for(key, title, window, cx);
    }

    /// The picker for one post (the reading pane's and the stream card's
    /// Hopper › chip).
    pub(crate) fn open_hopper_picker_for(
        &mut self,
        key: Key,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_hopper_picker_on(vec![key], title, window, cx)
    }

    fn open_hopper_picker_on(
        &mut self,
        keys: Vec<Key>,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reading.hoppers = self.backend.hoppers();
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.open_reading_sheet(
            RSheet::HopperPick {
                keys,
                title,
                sel: 0,
                focus,
            },
            cx,
        );
    }

    /// Put the posts in the hopper, or take them all out when every one is
    /// there already.
    pub(crate) fn toggle_hopper(&mut self, keys: &[Key], id: &str, cx: &mut Context<Self>) {
        let members = self.backend.hopper_members();
        let held = |k: &Key| members.get(k).is_some_and(|h| h.iter().any(|x| x == id));
        let on = !keys.iter().all(held);
        let name = self
            .reading
            .hoppers
            .iter()
            .find(|h| h.id == id)
            .map(|h| h.name.clone())
            .unwrap_or_else(|| "the hopper".into());
        let mut changed = 0;
        let mut failed = None;
        for k in keys.iter().filter(|k| held(k) != on) {
            match self.backend.set_in_hopper(id, &k.0, &k.1, on) {
                Ok(()) => changed += 1,
                Err(e) => {
                    failed = Some(e);
                    break;
                }
            }
        }
        self.reading.hoppers = self.backend.hoppers();
        match failed {
            None => {
                let what = match changed {
                    1 if keys.len() == 1 => String::new(),
                    1 => " 1 post".into(),
                    n => format!(" {n} posts"),
                };
                self.show_toast(
                    if on {
                        format!("Added{what} to {name}")
                    } else {
                        format!("Removed{what} from {name}")
                    },
                    None,
                    cx,
                );
            }
            Some(e) => self.show_toast(format!("Couldn't change the hopper: {e}"), None, cx),
        }
        cx.notify();
    }

    /// What a row's menu acts on: the picks when the row is among them,
    /// else the row alone.
    fn hopper_menu_keys(&self, key: &Key) -> Vec<Key> {
        if self.reading.picked.contains(key) {
            self.reading
                .picked
                .targets(&self.shown_keys(), None)
                .into_iter()
                .filter(|k| can_hopper(self, k))
                .collect()
        } else {
            vec![key.clone()]
        }
    }

    /// A post's "Hopper ›" list in the row menu (the picks', when the row
    /// is picked). Checked: every one of them is in it.
    pub(crate) fn hopper_menu_items(&self, key: &Key) -> Vec<(String, Option<MenuCmd>, bool)> {
        let keys = self.hopper_menu_keys(key);
        let members = self.backend.hopper_members();
        let mut v: Vec<_> = self
            .reading
            .hoppers
            .iter()
            .map(|h| {
                (
                    h.name.clone(),
                    Some(MenuCmd::ToggleHopper(keys.clone(), h.id.clone())),
                    held_by(&members, &keys, &h.id) == Held::All,
                )
            })
            .collect();
        if !v.is_empty() {
            v.push((String::new(), None, false));
        }
        v.push((
            "New Hopper…".into(),
            Some(MenuCmd::NewHopperFor(keys)),
            false,
        ));
        v
    }

    /// A hopper's menu in the sources pane.
    pub(crate) fn hopper_items_for(&self, id: &str) -> Vec<(String, Option<MenuCmd>, bool)> {
        let public = self
            .reading
            .hoppers
            .iter()
            .find(|h| h.id == id)
            .is_some_and(|h| h.public);
        let id = id.to_string();
        vec![
            (
                "Rename…".into(),
                Some(MenuCmd::RenameHopper(id.clone())),
                false,
            ),
            (
                "Description…".into(),
                Some(MenuCmd::DescribeHopper(id.clone())),
                false,
            ),
            if public {
                (
                    "Make Private".into(),
                    Some(MenuCmd::SetHopperPublic(id.clone(), false)),
                    false,
                )
            } else {
                (
                    "Make Public…".into(),
                    Some(MenuCmd::SetHopperPublic(id.clone(), true)),
                    false,
                )
            },
            (String::new(), None, false),
            (
                "Delete Hopper…".into(),
                Some(MenuCmd::DeleteHopper(id)),
                false,
            ),
            (String::new(), None, false),
            ("New Hopper…".into(), Some(MenuCmd::NewHopper), false),
        ]
    }

    fn hopper_name(&self, id: &str) -> String {
        self.reading
            .hoppers
            .iter()
            .find(|h| h.id == id)
            .map(|h| h.name.clone())
            .unwrap_or_default()
    }

    /// After a hopper changed: re-read the list and leave a deleted
    /// hopper's posts.
    fn reload_hoppers(&mut self, cx: &mut Context<Self>) {
        self.reading.hoppers = self.backend.hoppers();
        self.reading.hopper_order = self.backend.hopper_order();
        if let super::sources_vm::Source::Hopper(id) = &self.reading.source
            && !self.reading.hoppers.iter().any(|h| &h.id == id)
        {
            self.reading.source = super::sources_vm::Source::default();
        }
        self.reading.refilter();
        cx.notify();
    }

    // ------------------------------------------------------------ picker

    /// The picker's rows: each hopper, then "New Hopper…".
    fn pick_len(&self) -> usize {
        self.reading.hoppers.len() + 1
    }

    /// Choose row `i` of the picker: toggle that hopper, or New Hopper….
    pub(crate) fn pick_hopper_row(
        &mut self,
        i: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(RSheet::HopperPick { keys, .. }) = self.reading.sheet.as_ref() else {
            return;
        };
        let keys = keys.clone();
        match self.reading.hoppers.get(i).map(|h| h.id.clone()) {
            Some(id) => {
                self.toggle_hopper(&keys, &id, cx);
                if let Some(RSheet::HopperPick { sel, .. }) = self.reading.sheet.as_mut() {
                    *sel = i;
                }
            }
            None => self.open_hopper_name_sheet(NameFor::New { then_add: keys }, window, cx),
        }
    }

    fn hopper_pick_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let n = self.pick_len();
        let Some(RSheet::HopperPick { sel, .. }) = self.reading.sheet.as_mut() else {
            return;
        };
        let k = ev.keystroke.key.as_str();
        match k {
            "down" | "j" => *sel = (*sel + 1).min(n - 1),
            "up" | "k" => *sel = sel.saturating_sub(1),
            "enter" | "space" => {
                let i = *sel;
                self.pick_hopper_row(i, window, cx);
            }
            "n" => {
                let i = n - 1;
                self.pick_hopper_row(i, window, cx);
            }
            "escape" => self.close_reading_sheet(window, cx),
            d if d.len() == 1 && d.chars().all(|c| ('1'..='9').contains(&c)) => {
                let i = d.parse::<usize>().unwrap_or(1) - 1;
                if i < n - 1 {
                    self.pick_hopper_row(i, window, cx);
                }
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn render_hopper_pick(&self, sheet: &RSheet, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette.on_page();
        let RSheet::HopperPick {
            keys,
            title,
            sel,
            focus,
        } = sheet
        else {
            return div().into_any_element();
        };
        let members = self.backend.hopper_members();
        let row = |i: usize, sel: bool| {
            div()
                .id(("hopper-pick", i))
                .debug_selector(move || format!("hopper-pick-{i}"))
                .px(px(8.))
                .py(px(5.))
                .rounded(px(self.theme.corner(5.)))
                .flex()
                .items_center()
                .gap(px(8.))
                .cursor_pointer()
                .when(sel, |d| d.bg(p.accent.opacity(0.14)))
                .hover(|s| s.bg(p.accent.opacity(0.08)))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.pick_hopper_row(i, window, cx);
                }))
        };
        let mut rows: Vec<AnyElement> =
            self.reading
                .hoppers
                .iter()
                .enumerate()
                .map(|(i, h)| {
                    let mark = match held_by(&members, keys, &h.id) {
                        Held::All => "✓",
                        Held::Some => "–",
                        Held::None => "",
                    };
                    row(i, i == *sel)
                        .child(
                            div()
                                .w(px(14.))
                                .flex_none()
                                .text_color(p.accent)
                                .child(mark),
                        )
                        .child(
                            svg()
                                .data(ICON_HOPPER)
                                .size(px(14.))
                                .flex_none()
                                .text_color(p.muted),
                        )
                        .child(div().flex_1().min_w_0().truncate().child(h.name.clone()))
                        .when(h.public, |d| {
                            d.child(div().text_size(px(11.)).text_color(p.muted).child("public"))
                        })
                        .child(
                            div()
                                .flex_none()
                                .text_size(px(11.))
                                .text_color(p.muted)
                                .child(match h.count {
                                    1 => "1 post".to_string(),
                                    n => format!("{n} posts"),
                                }),
                        )
                        // Its number key, as a key cap.
                        .child(div().w(px(18.)).flex_none().flex().justify_center().when(
                            i < 9,
                            |d| {
                                d.child(
                                    div()
                                        .px(px(4.))
                                        .rounded(px(self.theme.corner(3.)))
                                        .border_1()
                                        .border_color(p.muted.opacity(0.4))
                                        .text_size(px(10.))
                                        .text_color(p.muted)
                                        .child((i + 1).to_string()),
                                )
                            },
                        ))
                        .into_any_element()
                })
                .collect();
        let new_ix = self.reading.hoppers.len();
        rows.push(
            row(new_ix, new_ix == *sel)
                .child(div().w(px(14.)).flex_none())
                .child(div().flex_1().text_color(p.accent).child("New Hopper…"))
                .child(
                    div().w(px(18.)).flex_none().flex().justify_center().child(
                        div()
                            .px(px(4.))
                            .rounded(px(self.theme.corner(3.)))
                            .border_1()
                            .border_color(p.muted.opacity(0.4))
                            .text_size(px(10.))
                            .text_color(p.muted)
                            .child("n"),
                    ),
                )
                .into_any_element(),
        );
        div()
            .track_focus(focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                this.hopper_pick_key(ev, window, cx)
            }))
            .child(self.sheet_heading("Hopper"))
            .child(
                div()
                    .mb(px(8.))
                    .text_color(p.muted)
                    .truncate()
                    .child(if keys.len() > 1 {
                        title.clone()
                    } else {
                        format!("“{title}”")
                    }),
            )
            .when(self.reading.hoppers.is_empty(), |d| {
                d.child(
                    div()
                        .mb(px(6.))
                        .text_color(p.muted)
                        .child("No hoppers yet. A hopper collects posts, yours and others'."),
                )
            })
            .child(
                div()
                    .id("hopper-pick-rows")
                    .flex()
                    .flex_col()
                    .children(rows),
            )
            .child(self.keys_row(vec![
                self.key_hint("1–9", "in / out"),
                self.key_hint("⏎", "in / out"),
                self.key_hint("n", "new hopper"),
                self.key_hint("esc", "done"),
            ]))
            .into_any_element()
    }

    // ------------------------------------------------------------ name sheet

    /// New Hopper…, Rename… or Description….
    pub(crate) fn open_hopper_name_sheet(
        &mut self,
        what: NameFor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (current, placeholder) = match &what {
            NameFor::New { .. } => (String::new(), "Hopper name"),
            NameFor::Rename(id) => (self.hopper_name(id), "Hopper name"),
            NameFor::Describe(id) => (
                self.reading
                    .hoppers
                    .iter()
                    .find(|h| &h.id == id)
                    .and_then(|h| h.description.clone())
                    .unwrap_or_default(),
                "One line about it (empty clears it)",
            ),
        };
        let input = cx.new(|cx| {
            let mut s = InputState::new(window, cx).placeholder(placeholder);
            s.set_value(current, window, cx);
            s
        });
        let ev = cx.subscribe_in(&input, window, |this, _, ev, window, cx| match ev {
            InputEvent::PressEnter { .. } => this.commit_hopper_name_sheet(window, cx),
            InputEvent::Change => {
                if let Some(RSheet::HopperName {
                    input,
                    error,
                    tried,
                    ..
                }) = this.reading.sheet.as_mut()
                    && error.is_some()
                    && input.read(cx).value().trim() != tried.as_str()
                {
                    *error = None;
                    cx.notify();
                }
            }
            _ => {}
        });
        self._subs.push(ev);
        input.update(cx, |s, cx| {
            s.focus(window, cx);
            s.select_all(window, cx);
        });
        self.open_reading_sheet(
            RSheet::HopperName {
                input,
                what,
                error: None,
                tried: String::new(),
                busy: false,
            },
            cx,
        );
    }

    /// ⏎ in the name sheet: on the blyg first; the sheet stays with the
    /// reason when it can't.
    pub(crate) fn commit_hopper_name_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(RSheet::HopperName {
            input, what, busy, ..
        }) = self.reading.sheet.as_mut()
        else {
            return;
        };
        if *busy {
            return;
        }
        let text = input.read(cx).value().trim().to_string();
        let what = what.clone();
        let refuse = if !matches!(what, NameFor::Describe(_)) && text.is_empty() {
            Some("A hopper needs a name".to_string())
        } else if matches!(what, NameFor::Describe(_))
            && text.chars().count() > blyg_core::HOPPER_DESCRIPTION_MAX
        {
            Some(format!(
                "A description is at most {} characters",
                blyg_core::HOPPER_DESCRIPTION_MAX
            ))
        } else {
            None
        };
        if let Some(msg) = refuse {
            if let Some(RSheet::HopperName { error, tried, .. }) = self.reading.sheet.as_mut() {
                *error = Some(msg);
                *tried = text;
            }
            return cx.notify();
        }
        *busy = true;
        cx.notify();
        let backend = self.backend.clone();
        let w = what.clone();
        let sent = text.clone();
        let task = cx.background_spawn(async move {
            match &w {
                NameFor::New { then_add } => {
                    let h = backend.create_hopper(&text)?;
                    for (sub, rid) in then_add {
                        backend.set_in_hopper(&h.id, sub, rid, true)?;
                    }
                    Ok(h.name)
                }
                NameFor::Rename(id) => backend
                    .update_hopper(
                        id,
                        &HopperEdit {
                            name: Some(text),
                            ..Default::default()
                        },
                    )
                    .map(|h| h.name),
                NameFor::Describe(id) => backend
                    .update_hopper(
                        id,
                        &HopperEdit {
                            description: Some(text),
                            ..Default::default()
                        },
                    )
                    .map(|h| h.name),
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let r: blyg_core::Result<String> = task.await;
            let _ = this.update_in(cx, |v, window, cx| match r {
                Ok(name) => {
                    v.close_reading_sheet(window, cx);
                    v.reload_hoppers(cx);
                    let msg = match &what {
                        NameFor::New { then_add } => match then_add.len() {
                            0 => format!("New hopper “{name}”"),
                            1 => format!("Added to {name}"),
                            n => format!("Added {n} posts to {name}"),
                        },
                        NameFor::Rename(_) => format!("Renamed to {name}"),
                        NameFor::Describe(_) => format!("Described {name}"),
                    };
                    let sub = matches!(&what, NameFor::New { then_add } if then_add.is_empty())
                        .then(|| "Press h on a post to put it in".into());
                    v.show_toast(msg, sub, cx);
                }
                Err(e) => {
                    if let Some(RSheet::HopperName {
                        error, busy, tried, ..
                    }) = v.reading.sheet.as_mut()
                    {
                        *busy = false;
                        *error = Some(hopper_error(e));
                        *tried = sent;
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn render_hopper_name_sheet(
        &self,
        sheet: &RSheet,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette.on_page();
        let RSheet::HopperName {
            input,
            what,
            error,
            busy,
            ..
        } = sheet
        else {
            return div().into_any_element();
        };
        let (heading, note, verb): (String, &str, &'static str) = match what {
            NameFor::New { then_add } => (
                "New Hopper".into(),
                if then_add.len() > 1 {
                    "A hopper collects posts, yours and others'. It's private until you make it \
                     public; the picked posts go in it."
                } else if then_add.len() == 1 {
                    "A hopper collects posts, yours and others'. It's private until you make it \
                     public; this post goes in it."
                } else {
                    "A hopper collects posts, yours and others'. It's private until you make it \
                     public."
                },
                "create",
            ),
            NameFor::Rename(id) => (
                "Rename Hopper".into(),
                if self
                    .reading
                    .hoppers
                    .iter()
                    .any(|h| &h.id == id && h.slug_frozen)
                {
                    "Its public page keeps its address."
                } else {
                    "Renamed on your blyg."
                },
                "rename",
            ),
            NameFor::Describe(id) => (
                format!("Describe “{}”", self.hopper_name(id)),
                "One line, at most 280 characters, shown on its public page and in Collections.",
                "save",
            ),
        };
        div()
            .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                cx.stop_propagation();
                this.close_reading_sheet(window, cx);
            }))
            .child(self.sheet_heading(heading))
            .child(div().mb(px(8.)).text_color(p.muted).child(note))
            .child(self.input_box(
                gpui_kit::base::input::Input::new(input).into_any_element(),
                error.is_some(),
            ))
            .when_some(error.clone(), |d, e| {
                d.child(div().mt(px(6.)).text_color(p.over).child(e))
            })
            .when(*busy, |d| {
                d.child(
                    div()
                        .mt(px(6.))
                        .text_color(p.muted)
                        .child("Saving on your blyg…"),
                )
            })
            .child(self.keys_row(vec![
                self.key_hint("⏎", verb),
                self.key_hint("esc", "cancel"),
            ]))
            .into_any_element()
    }

    // ------------------------------------------------------------ public & delete

    /// Make Public… or Delete Hopper…: asked first.
    pub(crate) fn open_hopper_confirm(
        &mut self,
        id: String,
        what: Confirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.open_reading_sheet(
            RSheet::HopperConfirm {
                id,
                what,
                error: None,
                busy: false,
                focus,
            },
            cx,
        );
    }

    /// ⏎ in the confirm sheet.
    pub(crate) fn confirm_hopper(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(RSheet::HopperConfirm { id, what, busy, .. }) = self.reading.sheet.as_mut() else {
            return;
        };
        if *busy {
            return;
        }
        *busy = true;
        let (id, what) = (id.clone(), *what);
        let name = self.hopper_name(&id);
        let backend = self.backend.clone();
        let i2 = id.clone();
        let task = cx.background_spawn(async move {
            match what {
                Confirm::Public => backend
                    .update_hopper(
                        &i2,
                        &HopperEdit {
                            public: Some(true),
                            ..Default::default()
                        },
                    )
                    .map(Some),
                Confirm::Delete => backend.delete_hopper(&i2).map(|()| None),
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |v, window, cx| match r {
                Ok(h) => {
                    v.close_reading_sheet(window, cx);
                    v.reload_hoppers(cx);
                    match what {
                        Confirm::Public => {
                            let url = h
                                .and_then(|h| h.slug)
                                .zip(v.backend.base_url())
                                .map(|(slug, base)| public_url(&base, &slug));
                            v.show_toast(format!("“{name}” is public"), url.map(Into::into), cx);
                        }
                        Confirm::Delete => v.show_toast(
                            format!("Deleted the hopper “{name}”"),
                            Some("Its posts are where they were".into()),
                            cx,
                        ),
                    }
                }
                Err(e) => {
                    if let Some(RSheet::HopperConfirm { error, busy, .. }) =
                        v.reading.sheet.as_mut()
                    {
                        *busy = false;
                        *error = Some(hopper_error(e));
                    }
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Make Private: no page to publish, so no question.
    pub(crate) fn set_hopper_public(
        &mut self,
        id: String,
        public: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = self.hopper_name(&id);
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move {
            backend.update_hopper(
                &id,
                &HopperEdit {
                    public: Some(public),
                    ..Default::default()
                },
            )
        });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |v, _, cx| {
                v.reload_hoppers(cx);
                match r {
                    Ok(_) => v.show_toast(
                        format!("“{name}” is private"),
                        Some("Its public page is gone".into()),
                        cx,
                    ),
                    Err(e) => {
                        v.show_toast(format!("Couldn't change it: {}", hopper_error(e)), None, cx)
                    }
                }
            });
        })
        .detach();
    }

    pub(super) fn render_hopper_confirm(
        &self,
        sheet: &RSheet,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette.on_page();
        let RSheet::HopperConfirm {
            id,
            what,
            error,
            busy,
            focus,
        } = sheet
        else {
            return div().into_any_element();
        };
        let h = self.reading.hoppers.iter().find(|h| &h.id == id);
        let name = h.map(|h| h.name.clone()).unwrap_or_default();
        let (heading, body, verb): (String, String, &'static str) = match what {
            Confirm::Public => {
                let at = h
                    .and_then(|h| h.slug.clone())
                    .zip(self.backend.base_url())
                    .map(|(slug, base)| {
                        format!(" Its page will be at {}.", public_url(&base, &slug))
                    })
                    .unwrap_or_default();
                (
                    format!("Make “{name}” public?"),
                    format!("{PUBLIC_NOTE}{at}"),
                    "make public",
                )
            }
            Confirm::Delete => {
                let n = h.map(|h| h.count).unwrap_or(0);
                let page = if h.is_some_and(|h| h.public) {
                    " Its public page goes too."
                } else {
                    ""
                };
                (
                    format!("Delete the hopper “{name}”?"),
                    format!(
                        "The {n} post{} in it stay where they are: only the collection goes.{page} \
                         This can't be undone.",
                        if n == 1 { "" } else { "s" }
                    ),
                    "delete",
                )
            }
        };
        div()
            .track_focus(focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                match ev.keystroke.key.as_str() {
                    "enter" => this.confirm_hopper(window, cx),
                    "escape" => this.close_reading_sheet(window, cx),
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .child(self.sheet_heading(heading))
            .child(
                div()
                    .line_height(relative(1.45))
                    .text_color(p.muted)
                    .child(body),
            )
            .when_some(error.clone(), |d, e| {
                d.child(div().mt(px(8.)).text_color(p.over).child(e))
            })
            .when(*busy, |d| {
                d.child(
                    div()
                        .mt(px(6.))
                        .text_color(p.muted)
                        .child("Asking your blyg…"),
                )
            })
            .child(self.keys_row(vec![
                self.key_hint("⏎", verb),
                self.key_hint("esc", "cancel"),
            ]))
            .into_any_element()
    }
}

/// How many of some posts a hopper holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Held {
    All,
    Some,
    None,
}

/// How many of `keys` hopper `id` holds (`members`: `hopper_members`).
pub fn held_by(
    members: &std::collections::HashMap<Key, Vec<String>>,
    keys: &[Key],
    id: &str,
) -> Held {
    let n = keys
        .iter()
        .filter(|k| members.get(*k).is_some_and(|h| h.iter().any(|x| x == id)))
        .count();
    match n {
        0 => Held::None,
        n if n == keys.len() => Held::All,
        _ => Held::Some,
    }
}

/// A refused hopper change in words: the blyg's own reason (a 403 names
/// the permission the sign-in lacks), or why it couldn't be asked.
pub fn hopper_error(e: blyg_core::CoreError) -> String {
    match e {
        blyg_core::CoreError::Rejected { message, .. } => message,
        blyg_core::CoreError::Offline => {
            "Your blyg can't be reached right now. Hoppers are changed there; try again when \
             you're online."
                .into()
        }
        e => e.to_string(),
    }
}
