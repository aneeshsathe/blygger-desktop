//! --- background --- Background and Foreground in the reader (studio 0.61,
//! contract 5.10.0; `blyg_core::background`). The Background smart feed
//! lists what the studio's Background lens lists; every other source, the
//! counts and the stream leave it out (`sources_vm::is_background`).
//!
//! Under an open post, when the blyg serves it (`setReadingPlacement`):
//! **↓ Background** / **↑ Foreground** (`b`), **↺ Clear my choice** when
//! there is one, and, for someone's post, **⇊ Blyg to Background** / **⇈ Blyg
//! to Foreground**. Settings › Reading has the rules. None of it marks
//! anything read or changes anything published. An older studio gets none
//! of it: its Background lens holds withdrawn posts, as before.

use blyg_core::ReadingItem;
use blyg_core::background::{BackgroundRules, Placement, RULES, rule_label};
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
    /// Its rules, read when Settings opens.
    pub rules: Load<BackgroundRules>,
    /// A rule is being saved.
    pub saving: bool,
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

/// What a placement says when it's done.
pub fn placed_toast(placement: Option<Placement>) -> &'static str {
    match placement {
        Some(Placement::Background) => "Moved to Background",
        Some(Placement::Foreground) => "Moved to Foreground",
        None => "The rules decide again",
    }
}

impl MainView {
    /// The reading pane's Background chips for the open post.
    pub(crate) fn background_chips(&self, r: &ReadingItem) -> Vec<ActionChip> {
        chips(r, self.reading.background.served)
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
        if !self.reading.background.served {
            self.show_toast(blyg_core::background::NOT_SERVED, None, cx);
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

    /// Settings: turn one rule on or off (only that key is written).
    pub(crate) fn save_background_rule(
        &mut self,
        rule: &'static str,
        on: bool,
        cx: &mut Context<Self>,
    ) {
        let Load::Ready(cur) = &mut self.reading.background.rules else {
            return;
        };
        if cur.on(rule) == on {
            return;
        }
        match cur.rules.iter_mut().find(|(r, _)| r == rule) {
            Some(x) => x.1 = on,
            None => cur.rules.push((rule.to_string(), on)),
        }
        self.reading.background.saving = true;
        cx.notify();
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.save_background_rule(rule, on) });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                v.reading.background.saving = false;
                match r {
                    Ok(s) => v.reading.background.rules = Load::Ready(s),
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
        let chip = |id: SharedString, label: &'static str, on: bool| {
            div()
                .id(id.clone())
                .debug_selector(move || id.to_string())
                .px(px(10.))
                .py(px(3.))
                .map(|d| crate::theme_ext::chip(d, theme, on))
                .cursor_pointer()
                .hover(|s| s.border_color(p.accent).bg(p.hover()))
                .child(label)
        };
        let mut col = div().flex().flex_col().gap(px(6.)).child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(chip("bg-rule-withdrawn".into(), "Withdrawn posts", true))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(p.muted)
                        .child("always in Background"),
                ),
        );
        for rule in RULES {
            let on = rules.on(rule);
            let (label, hint) = rule_label(rule);
            col = col.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(chip(format!("bg-rule-{rule}").into(), label, on).on_click(
                        cx.listener(move |this, _, _, cx| this.save_background_rule(rule, !on, cx)),
                    ))
                    .child(div().text_size(px(11.)).text_color(p.muted).child(hint)),
            );
        }
        Some(
            col.child(div().text_size(px(11.)).text_color(p.muted).child(
                "Saved on your blyg. Background holds what you and these rules put \
                         there; every other list leaves it out. Nothing is marked read.",
            ))
            .when(self.reading.background.saving, |d| d.opacity(0.6))
            .into_any_element(),
        )
    }
}
