//! --- background --- Background and Foreground in the reader (studio 0.61,
//! contract 5.10.0; `blyg_core::background`). The Background smart feed
//! lists what the studio's Background lens lists; every other source, the
//! counts and the stream leave it out (`sources_vm::is_background`).
//!
//! Under an open post, when the blyg serves it (`setReadingPlacement`):
//! **↓ Background** / **↑ Foreground** (`b`), **↺ Clear my choice** when
//! there is one, and, for someone's post, **⇊ Blyg to Background** / **⇈ Blyg
//! to Foreground**. Settings › Reading has the rules the blyg offers
//! (studio 0.64: parts published with a thread, posts with no words of
//! their own, republished unchanged, your own minor changes; studio 0.68
//! adds quotes refreshed, including rewritten quotes, small edits and
//! followed within 15 minutes). "Including rewritten quotes" acts only
//! while "Quotes refreshed" is on, and is greyed out without it. None of it
//! marks anything read or changes anything published. An older studio gets
//! none of it: its Background lens holds withdrawn posts, as before.
//!
//! In the Background list each post says why it's there ("published with
//! ‘<thread>’", …; `MainView::background_why`). A thread published with its parts
//! (studio 0.64) says "+N parts": on its row, as a fold under its card in
//! the stream, and listed under it when open (`render_burst_parts`); a
//! part opens like any post. By keys: `p` folds or unfolds the parts of the
//! open thread (else the selected card), and 1–9 open a listed part; the
//! hint beside "+N parts" says so.
//!
//! Settings › BACKGROUND: ⇥ past THEME reaches the rules (a focus ring),
//! the arrows move between them, and space or ⏎ turns one off or on, with a
//! toast saying where its posts go.

use std::collections::{HashMap, HashSet};

use blyg_core::ReadingItem;
use blyg_core::background::{
    BackgroundRules, Placement, RULES, burst_parts, parts_label, rule_label,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::Load;
use super::vm::{self, ActionChip, Key};
use crate::app::MainView;

/// The blyg's Background, as the reading screen and Settings need it.
#[derive(Default)]
pub struct Background {
    /// The blyg serves Background (as of the last reading refresh).
    pub served: bool,
    /// This sign-in may place posts in Background (a read-only one sees
    /// the lens and no chips).
    pub placing: bool,
    /// Its rules, read when Settings opens.
    pub rules: Load<BackgroundRules>,
    /// A rule is being saved.
    pub saving: bool,
    /// Each thread's parts (`blyg_core::background::burst_parts`), from
    /// the rows as last listed ([`Background::set_rows`]).
    parts: HashMap<Key, Vec<Key>>,
    /// Threads whose parts are unfolded under their stream card.
    unfolded: HashSet<Key>,
    /// Open threads whose parts were folded away (listed by default).
    folded: HashSet<Key>,
}

impl Background {
    /// The rows changed: work out each thread's parts again.
    pub fn set_rows(&mut self, rows: &[ReadingItem]) {
        self.parts = burst_parts(rows);
    }

    /// The parts of the thread `key` (none for anything else).
    pub fn parts_of(&self, key: &Key) -> &[Key] {
        self.parts.get(key).map_or(&[], Vec::as_slice)
    }

    /// Whether the thread's parts are listed: under an open post unless
    /// folded away, under a stream card once unfolded.
    pub fn parts_listed(&self, key: &Key, open: bool) -> bool {
        if open {
            !self.folded.contains(key)
        } else {
            self.unfolded.contains(key)
        }
    }

    fn toggle_parts(&mut self, key: &Key, open: bool) {
        let set = if open {
            &mut self.folded
        } else {
            &mut self.unfolded
        };
        if !set.remove(key) {
            set.insert(key.clone());
        }
    }
}

/// Why a Background post is there, for its row and its open view; `None`
/// for a post in the foreground (or one that says so itself: withdrawn).
pub fn why(r: &ReadingItem) -> Option<String> {
    if !r.is_background() {
        return None;
    }
    r.background.as_ref()?.why()
}

/// Move the post to Background, or back to Foreground (`b`).
pub const PLACE: &str = "Background";
/// Let the rules decide again.
pub const CLEAR: &str = "Clear placement";
/// Move the post's whole blyg.
pub const BLYG: &str = "Blyg background";

/// The Background chips for `r` when the blyg serves Background; none
/// otherwise. Your own posts have no blyg chip (yours is never moved whole).
pub fn chips(r: &ReadingItem, served: bool) -> Vec<ActionChip> {
    if !served {
        return vec![];
    }
    let chip = |id: &'static str, label: String, tip: String| ActionChip {
        id,
        label,
        tip,
        enabled: true,
    };
    let b = r.background.clone().unwrap_or_default();
    let mut out = vec![if r.is_background() {
        chip(
            PLACE,
            "↑ Foreground".into(),
            "Back to the main lists, whatever the rules say (b). Nothing is marked read.".into(),
        )
    } else {
        chip(
            PLACE,
            "↓ Background".into(),
            "Moves this post to the Background list and out of every other (b). \
             Nothing is marked read, and nothing you publish changes."
                .into(),
        )
    }];
    if b.placement.is_some() {
        out.push(chip(
            CLEAR,
            "↺ Clear my choice".into(),
            "Background and Foreground follow the rules again.".into(),
        ));
    }
    if !blyg_core::own::is_own_row(r) {
        let name = if r.subscription_title.is_empty() {
            vm::host(&r.origin)
        } else {
            r.subscription_title.clone()
        };
        out.push(if b.blyg {
            chip(
                BLYG,
                "⇈ Blyg to Foreground".into(),
                format!("Brings {name}'s posts back to the main lists."),
            )
        } else {
            chip(
                BLYG,
                "⇊ Blyg to Background".into(),
                format!(
                    "Moves every post of {name} to Background, except any you moved to \
                     Foreground yourself."
                ),
            )
        });
    }
    out
}

/// What turning a rule off (`on: false`) or back on says: where its posts
/// go now.
pub fn rule_toast(rule: &str, on: bool) -> String {
    let posts = match rule {
        blyg_core::background::BURST => "Parts published with a thread",
        "wordless" => "Posts with no words of their own",
        "maintenance" => "Posts republished unchanged",
        "minor" => "Your own minor changes",
        blyg_core::background::QUOTES_REFRESHED => "Posts whose quotes were refreshed",
        blyg_core::background::QUOTES_MATERIAL => "Refreshes with rewritten quotes",
        "near-same" => "Small edits",
        blyg_core::background::SUPERSEDED => "Posts followed within 15 minutes",
        _ => "Those posts",
    };
    if on {
        format!("{posts} go to Background again")
    } else {
        format!("{posts} are back in your lists")
    }
}

/// What a placement says when it's done.
pub fn placed_toast(placement: Option<Placement>) -> &'static str {
    match placement {
        Some(Placement::Background) => "Moved to Background",
        Some(Placement::Foreground) => "Moved to Foreground",
        None => "The rules decide again",
    }
}

impl MainView {
    /// Why `r` is in Background, as its badge says it. A thread's part
    /// names the thread ("published with ‘A morning on the harbour wall’")
    /// when its title is known.
    pub(crate) fn background_why(&self, r: &ReadingItem) -> Option<String> {
        let why = why(r)?;
        if why != blyg_core::background::BURST_WHY {
            return Some(why);
        }
        let thread = r.background.as_ref().and_then(|b| b.burst_thread());
        let title = thread
            .and_then(|t| {
                self.reading
                    .rows
                    .iter()
                    .find(|x| x.subscription_id == r.subscription_id && x.remote_id.as_ref() == t)
            })
            .map(vm::post_title)
            .filter(|t| t != "…" && !t.trim().is_empty());
        Some(match title {
            Some(t) => format!("published with ‘{t}’"),
            None => why,
        })
    }

    /// The reading pane's Background chips for the open post.
    pub(crate) fn background_chips(&self, r: &ReadingItem) -> Vec<ActionChip> {
        chips(r, self.reading.background.placing)
    }

    /// A Background chip under the open post was clicked.
    pub(crate) fn background_action(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(row) = self.reading.opened.as_ref().map(|o| o.item.clone()) else {
            return;
        };
        let b = row.background.clone().unwrap_or_default();
        match id {
            PLACE => {
                let to = if row.is_background() {
                    Placement::Foreground
                } else {
                    Placement::Background
                };
                self.place_row(row, Some(to), cx)
            }
            CLEAR => self.place_row(row, None, cx),
            BLYG => self.place_blyg(row, !b.blyg, cx),
            _ => {}
        }
    }

    /// `b`: the post in front of you to Background, or back to Foreground.
    pub(crate) fn background_key(&mut self, cx: &mut Context<Self>) {
        if !self.reading.background.placing {
            let why = blyg_core::background::not_placing(self.reading.background.served);
            self.show_toast(why, None, cx);
            return;
        }
        let key: Option<Key> = self
            .reading
            .opened
            .as_ref()
            .map(|o| o.key.clone())
            .or_else(|| self.reading.sel.clone());
        let Some(row) =
            key.and_then(|k| self.reading.rows.iter().find(|r| vm::key(r) == k).cloned())
        else {
            self.show_toast("Open or select a post first", None, cx);
            return;
        };
        if super::original::is_external(&vm::key(&row)) {
            self.show_toast(
                "Only posts you follow, or your own, go to Background",
                None,
                cx,
            );
            return;
        }
        let to = if row.is_background() {
            Placement::Foreground
        } else {
            Placement::Background
        };
        self.place_row(row, Some(to), cx);
    }

    fn place_row(
        &mut self,
        row: ReadingItem,
        placement: Option<Placement>,
        cx: &mut Context<Self>,
    ) {
        let backend = self.backend.clone();
        let key = vm::key(&row);
        let withdrawn = row.state == "tombstone";
        let task = cx.background_spawn(async move {
            backend.set_reading_placement(&key, withdrawn, placement)
        });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| match r {
                Ok(()) => {
                    v.show_toast(placed_toast(placement), None, cx);
                    v.reading_changed(cx);
                }
                Err(e) => v.show_toast(format!("Couldn't move it: {e}"), None, cx),
            });
        })
        .detach();
    }

    fn place_blyg(&mut self, row: ReadingItem, on: bool, cx: &mut Context<Self>) {
        let backend = self.backend.clone();
        let sub = row.subscription_id.clone();
        let name = row.subscription_title.clone();
        let task = cx.background_spawn(async move { backend.set_blyg_background(&sub, on) });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| match r {
                Ok(()) => {
                    v.show_toast(
                        if on {
                            format!("{name} moved to Background")
                        } else {
                            format!("{name} is back in the main lists")
                        },
                        None,
                        cx,
                    );
                    v.reading_changed(cx);
                }
                Err(e) => v.show_toast(format!("Couldn't move the blyg: {e}"), None, cx),
            });
        })
        .detach();
    }

    /// Read the blyg's Background rules (Settings opening asks).
    pub(crate) fn background_rules_refresh(&mut self, cx: &mut Context<Self>) {
        if matches!(self.reading.background.rules, Load::Loading) {
            return;
        }
        if !matches!(self.reading.background.rules, Load::Ready(_)) {
            self.reading.background.rules = Load::Loading;
        }
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.background_rules() });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                v.reading.background.rules = match r {
                    Ok(Some(s)) => Load::Ready(s),
                    Ok(None) => Load::Unavailable,
                    Err(e) => match std::mem::take(&mut v.reading.background.rules) {
                        Load::Ready(s) => Load::Ready(s),
                        _ => Load::Failed(e.to_string()),
                    },
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// Settings' keyboard stops on BACKGROUND: the rules this blyg offers,
    /// in the page's order (none until they're read).
    pub(crate) fn background_rule_picks(&self) -> Vec<crate::app::pref_preview::Pick> {
        let Load::Ready(rules) = &self.reading.background.rules else {
            return vec![];
        };
        RULES
            .into_iter()
            .filter(|r| rules.offers(r))
            .map(crate::app::pref_preview::Pick::Rule)
            .collect()
    }

    /// Space or ⏎ on a rule in Settings: turn it off, or back on.
    pub(crate) fn toggle_background_rule(&mut self, rule: &'static str, cx: &mut Context<Self>) {
        if self.reading.background.saving || !self.reading.background.placing {
            return;
        }
        if let Load::Ready(r) = &self.reading.background.rules {
            if BackgroundRules::needs(rule).is_some_and(|n| !r.on(n)) {
                return; // greyed out: it acts only under the rule it needs
            }
            let on = r.on(rule);
            self.save_background_rule(rule, !on, cx);
        }
    }

    /// Settings: turn one rule on or off (only that key is written).
    pub(crate) fn save_background_rule(
        &mut self,
        rule: &'static str,
        on: bool,
        cx: &mut Context<Self>,
    ) {
        // A sign-in that can't place can't change the rules either: the
        // studio would refuse the write.
        if !self.reading.background.placing {
            return;
        }
        let Load::Ready(cur) = &mut self.reading.background.rules else {
            return;
        };
        if cur.on(rule) == on || !cur.offers(rule) {
            return;
        }
        match cur.rules.iter_mut().find(|(r, _)| r == rule) {
            Some(x) => x.1 = on,
            None => cur.rules.push((rule.to_string(), on)),
        }
        self.reading.background.saving = true;
        cx.notify();
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move {
            let saved = backend.save_background_rule(rule, on)?;
            // The studio moves the rule's posts at once: read its lens now
            // (the write moved `settings`), not at the next pull.
            let _ = backend.sync_now();
            Ok::<_, blyg_core::CoreError>(saved)
        });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                v.reading.background.saving = false;
                match r {
                    Ok(s) => {
                        v.reading.background.rules = Load::Ready(s);
                        v.show_toast(rule_toast(rule, on), None, cx);
                        v.reading_changed(cx);
                    }
                    Err(e) => {
                        v.show_toast(format!("Couldn't save the Background rule: {e}"), None, cx);
                        v.background_rules_refresh(cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Settings › Reading: the Background rules; `None` on a blyg without them.
    pub(crate) fn render_background_settings(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.palette;
        let theme = &*self.theme;
        let rules = match &self.reading.background.rules {
            Load::Ready(r) => r,
            Load::Unavailable | Load::Idle => return None,
            Load::Loading => {
                return Some(
                    div()
                        .text_size(px(11.5))
                        .text_color(p.muted)
                        .child("Reading your blyg's Background rules…")
                        .into_any_element(),
                );
            }
            Load::Failed(e) => {
                return Some(
                    div()
                        .text_size(px(11.5))
                        .text_color(p.muted)
                        .child(format!("Couldn't read your blyg's Background rules: {e}"))
                        .into_any_element(),
                );
            }
        };
        // Settings' keyboard cursor on a rule (⇥ past THEME; `pref_preview`).
        let focused = match &self.preview.cursor {
            Some((
                crate::app::pref_preview::Row::Background,
                crate::app::pref_preview::Pick::Rule(r),
            )) => Some(*r),
            _ => None,
        };
        // On: the accent edge and a check; off: a plain edge and "off".
        let chip = |id: SharedString, label: &'static str, on: bool, ring: bool| {
            div()
                .id(id.clone())
                .debug_selector(move || id.to_string())
                .px(px(10.))
                .py(px(3.))
                .flex()
                .items_center()
                .gap(px(5.))
                .map(|d| crate::theme_ext::chip(d, theme, on))
                .when(ring, |d| {
                    d.bg(p.bg).shadow(crate::app::settings_page::focus_ring(&p))
                })
                .cursor_pointer()
                .hover(|s| s.border_color(p.accent).bg(p.hover()))
                .child(if on { "✓" } else { "○" })
                .child(label)
                .when(!on, |d| d.child(div().text_color(p.muted).child("off")))
        };
        // Withdrawn posts can't be turned off: said as a fact, not a chip.
        let mut col = div().flex().flex_col().gap(px(6.)).child(
            div()
                .id("bg-withdrawn-fixed")
                .debug_selector(|| "bg-withdrawn-fixed".into())
                .flex()
                .items_center()
                .gap(px(6.))
                .child(
                    div()
                        .flex_none()
                        .px(px(10.))
                        .py(px(3.))
                        .rounded(px(theme.chip_radius))
                        .border_1()
                        .border_dashed()
                        .border_color(p.edge())
                        .text_color(p.muted)
                        .child("Withdrawn posts"),
                )
                .child(crate::app::settings_page::side_note(
                    &p,
                    "always in Background; this one can't be turned off",
                )),
        );
        // A sign-in that can't place: the rules show, greyed, and say why.
        let locked = !self.reading.background.placing;
        if locked {
            col = col.child(
                div()
                    .id("bg-rules-locked")
                    .debug_selector(|| "bg-rules-locked".into())
                    .child(crate::app::settings_page::side_note(
                        &p,
                        blyg_core::background::NOT_PLACING,
                    )),
            );
        }
        // Only the rules this blyg offers: a 0.61 studio refuses the others.
        for rule in RULES.into_iter().filter(|r| rules.offers(r)) {
            let on = rules.on(rule);
            let ring = focused == Some(rule);
            let (label, hint) = rule_label(rule);
            // "Including rewritten quotes" acts only under "Quotes
            // refreshed" (studio `enabledRules`): greyed out while it's off.
            let waits = BackgroundRules::needs(rule).filter(|n| !rules.on(n));
            let c = chip(format!("bg-rule-{rule}").into(), label, on, ring)
                .flex_none()
                .when(BackgroundRules::needs(rule).is_some(), |d| d.ml(px(18.)))
                .when(waits.is_some() || locked, |d| d.opacity(0.5))
                .when(waits.is_none() && !locked, |d| {
                    d.on_click(
                        cx.listener(move |this, _, _, cx| this.save_background_rule(rule, !on, cx)),
                    )
                });
            // The ring's own selector (tests), and the scroll to it after ⇥.
            let c: AnyElement = if ring {
                let reveal = self.preview.reveal.clone();
                let scroll = self
                    .sheet
                    .get::<crate::app::settings_page::SettingsSheet>()
                    .map(|s| s.scroll.clone());
                div()
                    .flex_none()
                    .debug_selector(|| "bg-rule-focused".into())
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
                    .child(c)
                    .into_any_element()
            } else {
                c.into_any_element()
            };
            col = col.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(c)
                    .child(crate::app::settings_page::side_note(&p, hint)),
            );
        }
        Some(
            col.child(crate::app::settings_page::note(
                &p,
                "Saved on your blyg. Background holds what you and these rules put \
                 there; every other list leaves it out. Nothing is marked read.",
            ))
            .when(self.reading.background.saving, |d| d.opacity(0.6))
            .into_any_element(),
        )
    }

    /// Where `p` and the part numbers act: the open thread (`true`), else
    /// the stream's selected card (`false`); `None` when neither has parts.
    fn parts_key_target(&self) -> Option<(Key, bool)> {
        let bg = &self.reading.background;
        if let Some(o) = &self.reading.opened {
            if !bg.parts_of(&o.key).is_empty() {
                return Some((o.key.clone(), true));
            }
            return None;
        }
        let sel = self.reading.sel.as_ref()?;
        (self.reading.mode == super::stream_vm::ReadMode::Stream && !bg.parts_of(sel).is_empty())
            .then(|| (sel.clone(), false))
    }

    /// Fold or unfold a thread's parts (a click on "+N parts", or `p`).
    fn toggle_burst_parts(&mut self, key: &Key, open: bool, cx: &mut Context<Self>) {
        self.reading.background.toggle_parts(key, open);
        if !open && let Some(ix) = self.reading.shown_pos(key) {
            self.reading.stream.list.remeasure_items(ix..ix + 1);
        }
        cx.notify();
    }

    /// The reading screen's parts keys: `p` folds or unfolds the parts of
    /// the open thread (else the selected stream card), and 1–9 open a
    /// listed part. Returns whether the key was taken.
    pub(crate) fn burst_parts_key(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((thread, open)) = self.parts_key_target() else {
            return false;
        };
        if key == "p" {
            self.toggle_burst_parts(&thread, open, cx);
            return true;
        }
        let Some(n) = key.parse::<usize>().ok().filter(|n| (1..=9).contains(n)) else {
            return false;
        };
        if !self.reading.background.parts_listed(&thread, open) {
            return false;
        }
        match self
            .reading
            .background
            .parts_of(&thread)
            .get(n - 1)
            .cloned()
        {
            Some(part) => self.open_reading(part, window, cx),
            None => return false,
        }
        true
    }

    /// "+N parts" under a thread published with its parts (studio 0.64's
    /// `BurstParts`): a toggle, and the parts by their link labels, each
    /// opening that post. `open`: the open post, listed until folded; else
    /// a stream card, folded until unfolded. `None` for a post with none.
    pub(crate) fn render_burst_parts(
        &self,
        r: &ReadingItem,
        open: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let key = vm::key(r);
        let parts = self.reading.background.parts_of(&key);
        if parts.is_empty() {
            return None;
        }
        let p = self.palette.on_page();
        let listed = self.reading.background.parts_listed(&key, open);
        let label = parts_label(parts.len(), listed);
        let named: Vec<(Key, String)> = if listed {
            parts
                .iter()
                .map(|k| {
                    let held = self.reading.rows.iter().find(|r| vm::key(r) == *k);
                    let name = match held {
                        Some(h) => super::stream_vm::link_label(Some(h)),
                        None => k.1.chars().take(8).collect(),
                    };
                    (k.clone(), name)
                })
                .collect()
        } else {
            vec![]
        };
        let toggle_key = key.clone();
        let toggle = div()
            .id(if open {
                "burst-toggle-open"
            } else {
                "burst-toggle"
            })
            .debug_selector(move || {
                if open {
                    "burst-toggle-open".into()
                } else {
                    "burst-toggle".into()
                }
            })
            .cursor_pointer()
            .font_weight(FontWeight::MEDIUM)
            .text_color(p.accent)
            .hover(|s| s.underline())
            .tooltip(move |_, cx| {
                cx.new(|_| {
                    super::Tip(
                        if listed {
                            "Fold the parts away"
                        } else {
                            "Posts published with this thread; they wait in Background"
                        }
                        .into(),
                    )
                })
                .into()
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.toggle_burst_parts(&toggle_key, open, cx);
            }))
            .child(label);
        // The keys, where they act: the open thread, else the selected card
        // (as "Read more ⏎" shows its key).
        let keyed = self
            .parts_key_target()
            .is_some_and(|(k, o)| k == key && o == open);
        let hint = keyed.then(|| {
            let n = parts.len();
            div()
                .debug_selector(|| "burst-key-hint".into())
                .text_size(px(11.))
                .text_color(p.muted)
                .child(match (listed, n) {
                    (false, _) => "p unfolds".to_string(),
                    (true, 1) => "p folds · 1 opens".to_string(),
                    (true, n) => format!("p folds · 1–{} opens", n.min(9)),
                })
        });
        Some(
            div()
                .id(if open {
                    "burst-parts-open"
                } else {
                    "burst-parts"
                })
                .flex()
                .flex_col()
                .gap(px(2.))
                .font_family(self.chrome())
                .text_size(px(12.))
                .text_color(p.muted)
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap(px(8.))
                        .child(toggle)
                        .children(hint),
                )
                .children(named.into_iter().enumerate().map(|(i, (k, name))| {
                    div()
                        .id(("burst-part", i))
                        .debug_selector(move || format!("burst-part-{i}"))
                        .pl(px(14.))
                        .min_w_0()
                        .truncate()
                        .cursor_pointer()
                        .text_color(p.ink)
                        .hover(|s| s.text_color(p.accent))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.open_reading(k.clone(), window, cx)
                        }))
                        .child(format!("{}. {name}", i + 1))
                }))
                .into_any_element(),
        )
    }

    /// Why the post is in Background, as a quiet badge (its row in the
    /// Background list, and its open view); `None` in the foreground.
    pub(crate) fn render_background_why(&self, r: &ReadingItem) -> Option<AnyElement> {
        let why = self.background_why(r)?;
        let (fg, bg) = self.palette.on_page().quiet_badge();
        Some(
            div()
                .flex_none()
                .whitespace_nowrap()
                .px(px(6.))
                .rounded_full()
                .text_size(px(10.5))
                .text_color(fg)
                .bg(bg)
                .child(why)
                .into_any_element(),
        )
    }

    /// A thread's "+N parts" on its row in the list (opening the thread
    /// lists them); `None` without parts.
    pub(crate) fn render_parts_count(&self, key: &Key) -> Option<AnyElement> {
        let n = self.reading.background.parts_of(key).len();
        // Raw folds nothing: its parts are listed as they came in.
        let raw =
            self.reading.source == super::sources_vm::Source::Smart(super::sources_vm::Smart::Raw);
        if n == 0 || raw {
            return None;
        }
        let (fg, bg) = self.palette.on_page().quiet_badge();
        Some(
            div()
                .flex_none()
                .whitespace_nowrap()
                .px(px(6.))
                .rounded_full()
                .text_size(px(10.5))
                .text_color(fg)
                .bg(bg)
                .child(parts_label(n, false))
                .into_any_element(),
        )
    }

    /// The `rd-burst` scene: the stream at the sample burst thread, its
    /// parts unfolded under the card and listed in the open pane.
    pub(crate) fn burst_demo(&mut self, step: usize, window: &mut Window, cx: &mut Context<Self>) {
        use super::View;
        match step {
            0 => self.show_view(View::Reading, window, cx),
            1 => {
                let thread = crate::fake::background_seed::thread_key();
                if let Some(ix) = self.reading.shown_pos(&thread) {
                    for _ in 0..=ix {
                        self.stream_move(1, window, cx);
                    }
                }
                self.reading.background.toggle_parts(&thread, false);
                self.open_reading(thread, window, cx);
            }
            _ => {}
        }
    }
}
