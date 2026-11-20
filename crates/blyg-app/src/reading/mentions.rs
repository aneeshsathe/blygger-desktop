//! Mentions & responses (⇧⌘M): a list, never a count. Who, origin,
//! relation, when; a dot for new; hide a mention; and per post, whether its
//! responses show on the public page. Keys: j/k (↓/↑) move a row cursor
//! over every row (link notices too), h hides or unhides, r replies, o or ⏎
//! opens the response (↗).

use blyg_core::ResponsesMode;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::Load;
use super::vm::{self, MentionRow};
use crate::app::MainView;
use crate::theme::Rule; // --- themes --- dividers

impl MainView {
    pub(crate) fn load_mentions(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.reading.mentions, Load::Ready(_)) {
            self.reading.mentions = Load::Loading;
        }
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.mentions() });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                v.reading.mentions = Load::from_result(r);
                v.reading.refilter(); // mentions of your posts mark them
                cx.notify();
            });
        })
        .detach();
        self.load_mention_extras(cx); // --- contacts --- group order, Sent
    }

    /// Mentions as listed, grouped by your post: (title, local id, show
    /// responses, rows).
    pub(crate) fn mention_groups(
        &self,
    ) -> Vec<(
        String,
        Option<blyg_core::LocalId>,
        Responses,
        Vec<MentionRow>,
    )> {
        let Some(ms) = self.reading.mentions.ready() else {
            return vec![];
        };
        let mut ms: Vec<&blyg_core::Mention> = ms
            .iter()
            .filter(|m| self.reading.show_hidden || !m.hidden)
            .collect();
        ms.sort_by(|a, b| b.first_seen.cmp(&a.first_seen));
        self.order_by_mention_groups(&mut ms); // --- contacts ---
        let rows = ms
            .into_iter()
            .map(|m| (vm::mention_row(m, &self.reading.mentions_seen, self.now), m))
            .collect();
        let items = self.backend.items();
        vm::group_mentions(rows, &items)
            .into_iter()
            .map(|(t, item, rows)| {
                (
                    t,
                    item.map(|i| i.local_id.clone()),
                    Responses {
                        show: item.is_some_and(|i| i.show_responses),
                        mode: item.and_then(|i| i.responses_mode),
                    },
                    rows,
                )
            })
            .collect()
    }

    /// Every string the Mentions screen shows (tests: no digits as counts).
    #[cfg(test)]
    pub(crate) fn mentions_screen_strings(&self) -> Vec<String> {
        let mut out = vec!["Responses".to_string(), self.mentions_hint()];
        for (title, _, show, rows) in self.mention_groups() {
            out.push(format!("On “{title}”"));
            out.push(show.label());
            if rows.iter().any(|r| r.notice) {
                out.extend([LINKED_FROM.to_string(), NOTICES.to_string()]);
            }
            for r in rows {
                out.extend([r.who, r.origin, r.relation, r.when]);
                out.push(if r.hidden { "Unhide" } else { "Hide" }.into());
            }
        }
        out
    }

    fn mentions_hint(&self) -> String {
        format!("who responded to your posts · {MENTION_KEYS} · esc back")
    }

    /// The rows in the order they're drawn (each post's responses, then
    /// its link notices).
    fn mention_order(&self) -> Vec<MentionRow> {
        self.mention_groups()
            .into_iter()
            .flat_map(|g| g.3)
            .collect()
    }

    /// Responses' keys (the Received side): j/k or ↓/↑ move the row cursor,
    /// h hides or unhides, r replies, o or ⏎ opens the response (↗).
    /// Returns whether the key was taken.
    pub(super) fn mentions_key(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.contacts.sent.showing || self.reading.mentions.ready().is_none() {
            return false;
        }
        let rows = self.mention_order();
        if rows.is_empty() {
            return false;
        }
        let at = self
            .reading
            .mention_sel
            .as_ref()
            .and_then(|id| rows.iter().position(|r| r.id == *id));
        let step = |delta: isize| -> usize {
            match at {
                None if delta > 0 => 0,
                None => rows.len() - 1,
                Some(i) => (i as isize + delta).clamp(0, rows.len() as isize - 1) as usize,
            }
        };
        match key {
            "j" | "down" | "k" | "up" => {
                let i = step(if matches!(key, "j" | "down") { 1 } else { -1 });
                self.reading.mention_sel = Some(rows[i].id.clone());
                cx.notify();
                true
            }
            "h" | "r" | "o" | "enter" => {
                let Some(i) = at else {
                    self.show_toast("Pick a response first (j/k)", None, cx);
                    return true;
                };
                let row = &rows[i];
                match key {
                    "h" => {
                        // Hidden rows leave the list: the cursor moves on.
                        if !row.hidden && !self.reading.show_hidden {
                            let next = rows
                                .get(i + 1)
                                .or(i.checked_sub(1).and_then(|j| rows.get(j)));
                            self.reading.mention_sel = next.map(|r| r.id.clone());
                        }
                        self.hide_mention(row.id.clone(), !row.hidden, cx);
                    }
                    "r" => {
                        let m = self
                            .reading
                            .mentions
                            .ready()
                            .and_then(|ms| ms.iter().find(|m| m.id == row.id).cloned());
                        match m {
                            Some(m) if m.source_id.is_some() && m.status != "gone" => {
                                self.reply_to_mention(&m, window, cx)
                            }
                            _ => self.show_toast(
                                "That response isn't a blyg post to reply to",
                                None,
                                cx,
                            ),
                        }
                    }
                    _ => cx.open_url(&row.source),
                }
                true
            }
            _ => false,
        }
    }

    fn hide_mention(&mut self, id: String, hidden: bool, cx: &mut Context<Self>) {
        if let Load::Ready(ms) = &mut self.reading.mentions
            && let Some(m) = ms.iter_mut().find(|m| m.id == id)
        {
            m.hidden = hidden;
        }
        cx.notify();
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.set_mention_hidden(&id, hidden) });
        cx.spawn(async move |this, cx| {
            if let Err(e) = task.await {
                let _ = this.update(cx, |v, cx| {
                    v.show_toast(format!("Couldn't change it: {e}"), None, cx);
                    v.load_mentions(cx);
                });
            }
        })
        .detach();
    }

    pub(super) fn set_responses(
        &mut self,
        id: blyg_core::LocalId,
        mode: ResponsesMode,
        cx: &mut Context<Self>,
    ) {
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.set_responses(&id, mode) });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                match r {
                    Ok(show) if mode == ResponsesMode::Default => v.show_toast(
                        "Responses follow your site setting",
                        Some(
                            if show {
                                "Shown on the post's page"
                            } else {
                                "Hidden from the post's page"
                            }
                            .into(),
                        ),
                        cx,
                    ),
                    Ok(show) => v.show_toast(
                        if show {
                            "Responses show on the post's page"
                        } else {
                            "Responses hidden from the post's page"
                        },
                        None,
                        cx,
                    ),
                    Err(e) => v.show_toast(format!("Couldn't change it: {e}"), None, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn render_mentions_screen(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette.on_page();
        let show_hidden = self.reading.show_hidden;
        let toggle_hidden = self
            .chip(
                "show-hidden",
                if show_hidden {
                    "Hide hidden"
                } else {
                    "Show hidden"
                },
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.reading.show_hidden = !this.reading.show_hidden;
                cx.notify();
            }))
            .into_any_element();
        let available = self.reading.available && self.reading.mentions != Load::Unavailable;
        // --- contacts --- Received | Sent.
        let sent = self.contacts.sent.showing;
        let mut actions = if available {
            self.mentions_side_chips(cx)
        } else {
            vec![]
        };
        let hint = if sent && available {
            "the mentions your blyg sent for your posts' links · esc back".to_string()
        } else {
            self.mentions_hint()
        };
        // --- end contacts ---
        if available && !sent {
            actions.push(toggle_hidden);
        }
        let header = self.screen_header("Responses", hint, actions);
        let body: AnyElement = match &self.reading.mentions {
            _ if !available => self.unavailable(),
            _ if sent => self.render_sent(cx), // --- contacts ---
            Load::Idle | Load::Loading => self.muted_note("Loading responses…"),
            Load::Failed(e) => self.muted_note(format!("Couldn't load responses: {e}")),
            _ => {
                let groups = self.mention_groups();
                if groups.is_empty() {
                    self.empty_note("Nobody has quoted, replied to or forked your posts yet.")
                } else {
                    div()
                        .id("mentions-list")
                        .flex_1()
                        .overflow_y_scroll()
                        .children(groups.into_iter().enumerate().map(
                            |(gi, (title, id, show, rows))| {
                                div()
                                    .px(px(16.))
                                    .pt(px(12.))
                                    .pb(px(6.))
                                    .rule_b(&p)
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .truncate()
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child(format!("On “{title}”")),
                                            )
                                            .when_some(id, |d, id| {
                                                d.child(
                                                    self.render_responses_control(gi, id, show, cx),
                                                )
                                            }),
                                    )
                                    .children(self.render_group_rows(gi, rows, cx))
                            },
                        ))
                        .into_any_element()
                }
            }
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .font_family(self.prefs.ui().family)
            .text_size(px(13.))
            .child(header)
            .child(body)
            .into_any_element()
    }

    /// One post's "On the post's page: Show | Hide | Site default" control:
    /// a muted label and one segment per choice, the chosen one in the
    /// accent; clicking another makes it the post's choice.
    fn render_responses_control(
        &self,
        gi: usize,
        id: blyg_core::LocalId,
        show: Responses,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette.on_page();
        div()
            .id(SharedString::from(format!("responses-{gi}")))
            .debug_selector(move || format!("responses-{gi}"))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(4.))
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(p.muted)
                    .mr(px(2.))
                    .child(ON_PAGE),
            )
            .children(
                show.options()
                    .into_iter()
                    .enumerate()
                    .map(|(i, (label, mode, chosen))| {
                        let id = id.clone();
                        self.chip(format!("responses-{gi}-{i}"), label)
                            .when(chosen, |d| {
                                d.border_color(p.accent)
                                    .text_color(p.accent)
                                    .cursor_default()
                            })
                            .when(!chosen, |d| {
                                d.on_click(cx.listener(move |this, _, _, cx| {
                                    this.set_responses(id.clone(), mode, cx)
                                }))
                            })
                    }),
            )
            .into_any_element()
    }

    /// A post's rows: its responses, then its link notices under "Linked
    /// from" (Studio's responses page: a notice is never one of the page's
    /// responses, so it's listed apart).
    fn render_group_rows(
        &self,
        gi: usize,
        rows: Vec<MentionRow>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let p = self.palette.on_page();
        let mut out = Vec::with_capacity(rows.len() + 1);
        let mut headed = false;
        for r in rows {
            if r.notice && !headed {
                headed = true;
                out.push(
                    div()
                        .id(SharedString::from(format!("linked-from-{gi}")))
                        .debug_selector(move || format!("linked-from-{gi}"))
                        .flex()
                        .items_baseline()
                        .gap(px(6.))
                        .pt(px(8.))
                        .pb(px(2.))
                        .child(div().font_weight(FontWeight::MEDIUM).child(LINKED_FROM))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(p.muted)
                                .child(format!("· {NOTICES}")),
                        )
                        .into_any_element(),
                );
            }
            out.push(self.render_mention_row(r, cx));
        }
        out
    }

    fn render_mention_row(&self, r: MentionRow, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette.on_page();
        let (id, hidden, source) = (r.id.clone(), r.hidden, r.source.clone());
        let profile_of = r.source.clone(); // --- profiles ---
        let selected = self.reading.mention_sel.as_deref() == Some(r.id.as_str());
        div()
            .id(SharedString::from(format!("mention-{}", r.id)))
            .flex()
            .items_center()
            .gap(px(8.))
            .py(px(6.))
            // The row cursor (j/k), drawn as the reading list's selection.
            .mx(px(-6.))
            .px(px(6.))
            .rounded(px(4.))
            .when(selected, |d| {
                d.debug_selector(|| "mention-selected".into()).bg(p.sel)
            })
            .when(hidden, |d| d.opacity(0.5))
            .child(
                div()
                    .w(px(8.))
                    .flex_none()
                    .flex()
                    .justify_center()
                    .when(r.new, |d| d.child(self.unread_marker(p.accent))),
            )
            .child(div().font_weight(FontWeight::MEDIUM).child(r.who))
            .child(div().text_color(p.muted).child(r.relation))
            // --- profiles --- the origin opens its profile; ↗ opens the source.
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .gap(px(6.))
                    .child(
                        div()
                            .id(SharedString::from(format!("mention-src-{}", r.id)))
                            .min_w_0()
                            .truncate()
                            .text_color(p.muted)
                            .cursor_pointer()
                            .border_b_1()
                            .border_dashed()
                            .border_color(p.muted.opacity(0.5))
                            .hover(|s| s.text_color(p.accent))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_profile(profile_of.clone(), window, cx)
                            }))
                            .child(r.origin),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("mention-open-{}", r.id)))
                            .flex_none()
                            .text_color(p.muted)
                            .cursor_pointer()
                            .hover(|s| s.text_color(p.accent))
                            .on_click(move |_, _, cx| cx.open_url(&source))
                            .child("↗"),
                    ),
            )
            // --- end profiles ---
            .child(div().text_size(px(11.5)).text_color(p.muted).child(r.when))
            .children(self.mention_reply_chip(&r.id, cx)) // --- contacts --- Reply
            .child(
                self.chip(
                    format!("hide-{}", r.id),
                    if hidden { "Unhide" } else { "Hide" },
                )
                .on_click(
                    cx.listener(move |this, _, _, cx| this.hide_mention(id.clone(), !hidden, cx)),
                ),
            )
            .into_any_element()
    }
}

/// The one control's label: what it decides.
const ON_PAGE: &str = "On the post's page:";

/// The heading over a post's link notices, and who sees them (Studio says
/// "notices, never shown on the page"; this says it plainly).
const LINKED_FROM: &str = "Linked from";
const NOTICES: &str = "only you see these";

/// The row keys, as the header shows them (the reading list's letters:
/// j/k move, as `⌥⌘R` replies and `⌘O` opens; h hides).
const MENTION_KEYS: &str = "j/k move · h hide · r reply · o open ↗";

/// One post's responses control: what its page shows now, and the post's own
/// choice (`None` on a server older than studio 0.8, which has no default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Responses {
    pub show: bool,
    pub mode: Option<ResponsesMode>,
}

impl Responses {
    /// The control's choices, as one labelled segmented control: Show,
    /// Hide and (where the server has one) Site default, which says what
    /// the site setting does when it's the one chosen. `(label, mode,
    /// chosen)`.
    pub(crate) fn options(self) -> Vec<(String, ResponsesMode, bool)> {
        let chose = |m: ResponsesMode| match self.mode {
            Some(own) => own == m,
            // No default on the server: what the page shows is the choice.
            None => (m == ResponsesMode::Show) == self.show,
        };
        let mut out = vec![
            (
                "Show".to_string(),
                ResponsesMode::Show,
                chose(ResponsesMode::Show),
            ),
            (
                "Hide".to_string(),
                ResponsesMode::Hide,
                chose(ResponsesMode::Hide),
            ),
        ];
        if self.mode.is_some() {
            let default = chose(ResponsesMode::Default);
            let label = if !default {
                "Site default".to_string()
            } else if self.show {
                "Site default (shown)".to_string()
            } else {
                "Site default (hidden)".to_string()
            };
            out.push((label, ResponsesMode::Default, default));
        }
        out
    }

    /// The control read as one line: "On the post's page: Hide".
    #[cfg(test)]
    fn label(self) -> String {
        let chosen = self
            .options()
            .into_iter()
            .find(|o| o.2)
            .map(|o| o.0)
            .unwrap_or_default();
        format!("{ON_PAGE} {chosen}")
    }
}

#[cfg(test)]
mod responses_control_tests {
    use super::{Responses, ResponsesMode};

    #[test]
    fn one_labelled_control_names_the_choice() {
        let r = |show, mode| Responses { show, mode };
        assert_eq!(
            r(true, Some(ResponsesMode::Show)).label(),
            "On the post's page: Show"
        );
        assert_eq!(
            r(false, Some(ResponsesMode::Default)).label(),
            "On the post's page: Site default (hidden)"
        );
        // An older studio has no site default: two choices.
        let old = r(false, None).options();
        assert_eq!(old.len(), 2);
        assert_eq!(r(false, None).label(), "On the post's page: Hide");
        // Exactly one choice is chosen.
        for o in [
            ResponsesMode::Show,
            ResponsesMode::Hide,
            ResponsesMode::Default,
        ] {
            let n = r(true, Some(o)).options().iter().filter(|x| x.2).count();
            assert_eq!(n, 1, "{o:?}");
        }
    }
}
