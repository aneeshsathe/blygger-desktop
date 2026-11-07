//! --- contacts --- The Mentions screen beyond the inbound list, as Studio's
//! `MentionsPage` has it:
//!
//! - **Received | Sent.** Sent lists the Webmentions your blyg sent for
//!   your posts' links (`GET /api/mentions?direction=outbound`): the post
//!   and version, the page it links to, the delivery status (sent, pending,
//!   failed, no endpoint), tries and the last error. `GET
//!   /api/mentions/summary` says first whether anything was sent at all.
//!   A list, never a count.
//! - Received follows the blyg's own order of your posts with responses
//!   (`GET /api/mention-groups`, newest verified first) when it serves it.
//! - **Reply** on a response: `GET /api/mentions/{id}/source` says whether
//!   the responding post is held here (a stub of it opens), or its blyg is
//!   subscribed but hasn't fetched it, or not subscribed (Subscribe opens,
//!   filled in).

use blyg_core::Mention;
use blyg_core::outbound::{MentionSource, OutboundMention};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::super::{Load, RSheet, View};
use crate::app::MainView;
use crate::theme::Rule;

/// The Mentions screen's other side, and the blyg's group order.
pub(crate) struct Sent {
    /// The Sent side is showing (else Received).
    pub showing: bool,
    pub load: Load<Vec<OutboundMention>>,
    /// The blyg has no `siteUrl`: other sites may refuse what it sends.
    pub no_site_url: bool,
    /// Your posts with responses, in the blyg's order (`mention-groups`).
    pub group_order: Vec<String>,
    /// A response whose source is being looked up (Reply).
    pub replying: Option<String>,
}

impl Default for Sent {
    fn default() -> Self {
        Sent {
            showing: false,
            load: Load::Idle,
            no_site_url: false,
            group_order: Vec::new(),
            replying: None,
        }
    }
}

impl MainView {
    /// Hook (`load_mentions`): the blyg's group order, and the Sent side
    /// when it's showing.
    pub(crate) fn load_mention_extras(&mut self, cx: &mut Context<Self>) {
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.mention_groups() });
        cx.spawn(async move |this, cx| {
            let Ok(Some(groups)) = task.await else { return };
            let _ = this.update(cx, |v, cx| {
                v.contacts.sent.group_order =
                    groups.into_iter().map(|g| g.target_item_id).collect();
                cx.notify();
            });
        })
        .detach();
        if self.contacts.sent.showing {
            self.load_sent(cx);
        }
    }

    /// Read what your blyg sent (and whether it has a site URL).
    pub(crate) fn load_sent(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.contacts.sent.load, Load::Ready(_)) {
            self.contacts.sent.load = Load::Loading;
        }
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move {
            // Nothing sent at all: no need to list it.
            let none_sent = matches!(backend.mention_summary(), Ok(Some(s)) if s.outbound == 0);
            let list = if none_sent {
                Ok(Some(Vec::new()))
            } else {
                backend.outbound_mentions()
            };
            let no_site = backend
                .settings()
                .map(|s| s.site_url.as_deref().is_none_or(|u| u.trim().is_empty()))
                .unwrap_or(false);
            (list, no_site)
        });
        cx.spawn(async move |this, cx| {
            let (list, no_site) = task.await;
            let _ = this.update(cx, |v, cx| {
                v.contacts.sent.load = match list {
                    Ok(Some(mut l)) => {
                        // Newest first.
                        l.sort_by(|a, b| b.created.cmp(&a.created));
                        Load::Ready(l)
                    }
                    Ok(None) => Load::Unavailable,
                    Err(e) => Load::from_result(Err(e)),
                };
                v.contacts.sent.no_site_url = no_site;
                cx.notify();
            });
        })
        .detach();
    }

    /// Received | Sent.
    pub(crate) fn set_mentions_side(&mut self, sent: bool, cx: &mut Context<Self>) {
        self.contacts.sent.showing = sent;
        if sent {
            self.load_sent(cx);
        }
        cx.notify();
    }

    /// Hook (`mention_groups`): the blyg's order of your posts with
    /// responses, when it gave one; the rest after, as they were.
    pub(crate) fn order_by_mention_groups(&self, ms: &mut [&Mention]) {
        let order = &self.contacts.sent.group_order;
        if order.is_empty() {
            return;
        }
        ms.sort_by_key(|m| {
            order
                .iter()
                .position(|t| t.eq_ignore_ascii_case(&m.target_item_id))
                .unwrap_or(usize::MAX)
        });
    }

    /// Hook (the Mentions header): the Received | Sent chips.
    pub(crate) fn mentions_side_chips(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p = self.palette.on_page();
        let sent = self.contacts.sent.showing;
        [(false, "Received"), (true, "Sent")]
            .into_iter()
            .map(|(side, label)| {
                self.chip(format!("mentions-side-{label}"), label)
                    .debug_selector(move || format!("mentions-side-{label}"))
                    .when(side == sent, |d| {
                        d.border_color(p.accent).text_color(p.accent)
                    })
                    .on_click(cx.listener(move |this, _, _, cx| this.set_mentions_side(side, cx)))
                    .into_any_element()
            })
            .collect()
    }

    /// Every string the Sent side shows (tests).
    #[cfg(test)]
    pub(crate) fn sent_screen_strings(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Load::Ready(l) = &self.contacts.sent.load {
            for o in l {
                out.push(o.status_label());
                out.push(self.sent_post_label(o));
                out.push(o.target.clone());
                out.push(sent_detail(o));
                out.extend(o.last_error.clone());
            }
        }
        out
    }

    /// "v2 of “Title”": your post and the version that sent it.
    fn sent_post_label(&self, o: &OutboundMention) -> String {
        match self.backend.own_item(&o.item_id) {
            Some(i) => format!("v{} of “{}”", o.version, crate::vm::item_title(&i)),
            None => format!("v{} of a post of yours", o.version),
        }
    }

    fn open_own_post(&mut self, server_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.backend.own_item(server_id) else {
            return;
        };
        self.show_view(View::Posts, window, cx);
        self.open(&item.local_id, window, cx);
    }

    /// Hook (the Mentions body): the Sent side.
    pub(crate) fn render_sent(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette.on_page();
        let banner = self.contacts.sent.no_site_url.then(|| {
            div()
                .mx(px(16.))
                .mt(px(10.))
                .px(px(10.))
                .py(px(7.))
                .rounded(px(self.theme.corner(7.)))
                .bg(p.notice_bg)
                .text_color(p.notice)
                .text_size(px(12.))
                .child(
                    "Your blyg has no site URL set, so the sites it sends to may not accept its \
                     mentions. Set it in your studio's settings.",
                )
        });
        let body: AnyElement = match &self.contacts.sent.load {
            Load::Unavailable => self.muted_note("Your blyg doesn't list the mentions it sent."),
            Load::Idle | Load::Loading => self.muted_note("Loading what your blyg sent…"),
            Load::Failed(e) => self.muted_note(format!("Couldn't load them: {e}")),
            Load::Ready(l) if l.is_empty() => self.empty_note(
                "Nothing sent yet. When a published post links to another site, your blyg \
                 tells it, and that shows here.",
            ),
            Load::Ready(l) => div()
                .id("sent-list")
                .flex_1()
                .overflow_y_scroll()
                .children(
                    l.iter()
                        .enumerate()
                        .map(|(i, o)| self.render_sent_row(i, o, cx)),
                )
                .into_any_element(),
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .children(banner)
            .child(body)
            .into_any_element()
    }

    fn render_sent_row(&self, i: usize, o: &OutboundMention, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette.on_page();
        let colour = match o.status.as_str() {
            "sent" => p.accent,
            "pending" => p.amber,
            "failed" => p.over,
            _ => p.muted,
        };
        let (post, target) = (o.item_id.clone(), o.target.clone());
        div()
            .id(SharedString::from(format!("sent-{i}")))
            .px(px(16.))
            .py(px(8.))
            .rule_b(&p)
            .flex()
            .gap(px(10.))
            .child(
                div().flex_none().w(px(78.)).child(
                    div()
                        .px(px(6.))
                        .rounded_full()
                        .border_1()
                        .border_color(colour)
                        .text_color(colour)
                        .text_size(px(10.5))
                        .flex()
                        .justify_center()
                        .child(o.status_label()),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .id(SharedString::from(format!("sent-post-{i}")))
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .cursor_pointer()
                            .hover(|d| d.text_color(p.accent))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_own_post(&post, window, cx)
                            }))
                            .child(self.sent_post_label(o)),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(6.))
                            .text_size(px(11.5))
                            .text_color(p.muted)
                            .child(
                                div()
                                    .id(SharedString::from(format!("sent-target-{i}")))
                                    .min_w_0()
                                    .truncate()
                                    .cursor_pointer()
                                    .hover(|d| d.text_color(p.accent))
                                    .on_click(move |_, _, cx| cx.open_url(&target))
                                    .child(format!("→ {}", o.target)),
                            )
                            .child(div().flex_none().child(format!("· {}", sent_detail(o)))),
                    )
                    .when_some(o.last_error.clone(), |d, e| {
                        d.child(div().text_size(px(11.5)).text_color(p.over).child(e))
                    }),
            )
            .into_any_element()
    }

    /// Hook (a Received row): Reply, on a response from a blyg post.
    pub(crate) fn mention_reply_chip(
        &self,
        mention_id: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let m = self
            .reading
            .mentions
            .ready()?
            .iter()
            .find(|m| m.id == mention_id && m.source_id.is_some() && m.status != "gone")?
            .clone();
        let busy = self.contacts.sent.replying.as_deref() == Some(mention_id);
        Some(
            self.chip(
                format!("reply-{mention_id}"),
                if busy { "Looking…" } else { "Reply" },
            )
            .tooltip(|_, cx| {
                cx.new(|_| {
                    super::super::Tip(
                        "Reply · new stub of their post (when your blyg holds it)".into(),
                    )
                })
                .into()
            })
            .on_click(cx.listener(move |this, _, window, cx| this.reply_to_mention(&m, window, cx)))
            .into_any_element(),
        )
    }

    /// Reply to a response: a stub of its source post when it's held here.
    pub(crate) fn reply_to_mention(
        &mut self,
        m: &Mention,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(source_id) = m.source_id.clone() else {
            return self.show_toast("That response isn't a blyg post", None, cx);
        };
        if self.contacts.sent.replying.is_some() {
            return;
        }
        self.contacts.sent.replying = Some(m.id.clone());
        let origin = m.source_origin.clone().unwrap_or_else(|| m.source.clone());
        let (backend, id) = (self.backend.clone(), m.id.clone());
        let task = cx.background_spawn(async move { backend.mention_source(&id) });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |v, window, cx| {
                v.contacts.sent.replying = None;
                let src = r.ok().flatten();
                v.reply_from_source(src, &source_id, &origin, window, cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn reply_from_source(
        &mut self,
        src: Option<MentionSource>,
        source_id: &str,
        origin: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let held = self.backend.reading_with_id(source_id);
        let row = match src.as_ref().and_then(|s| s.holder.as_deref()) {
            Some(h) => held.into_iter().find(|r| r.subscription_id == h),
            // An older blyg without the source route: what's held here.
            None if src.is_none() => held.into_iter().next(),
            None => None,
        };
        if let Some(row) = row {
            return self.item_action(row, "Reply", window, cx);
        }
        match src.and_then(|s| s.subscription) {
            Some((_, title)) => self.show_toast(
                format!("Your blyg hasn't fetched that post from {title} yet"),
                Some("Check All Feeds Now (⇧⌘R), then reply".into()),
                cx,
            ),
            None => {
                self.show_toast(
                    format!(
                        "Subscribe to {} to reply to it",
                        super::super::vm::host(origin)
                    ),
                    None,
                    cx,
                );
                self.open_subscribe(window, cx);
                if let Some(RSheet::Subscribe { input, .. }) = self.reading.sheet.as_ref() {
                    let input = input.clone();
                    let origin = origin.to_string();
                    input.update(cx, |s, cx| s.set_value(origin, window, cx));
                }
            }
        }
    }
}

/// A Sent row's detail, with the next try as a local time ("at 16:28", or
/// "on Nov 9, 16:28" another day).
pub(crate) fn sent_detail(o: &OutboundMention) -> String {
    use chrono::{DateTime, Local};
    let d = o.detail();
    let Some(at) = o
        .next_attempt_at
        .as_deref()
        .filter(|_| d.ends_with("trying again"))
        .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
    else {
        return d;
    };
    let at = at.with_timezone(&Local);
    if at.date_naive() == Local::now().date_naive() {
        format!("{d} at {}", at.format("%H:%M"))
    } else {
        format!("{d} on {}", at.format("%b %-d, %H:%M"))
    }
}
