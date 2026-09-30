//! Responses to a post (issue #7): who quoted, stubbed or forked it, as a
//! list and never a count (spec rule 1). Two sources:
//!
//! - your own posts: the verified mentions your blyg already collects
//!   (`GET /api/mentions`);
//! - anyone's post: posts in your reading list whose `transclusions`,
//!   `stub_of` or `forked_from` point at it ("seen in your network",
//!   `Backend::responses`, an index lookup in the store).
//!
//! The list sits at the bottom of the reading pane (and under your own post
//! in ⌘Y); the stream shows only a small "↩" marker on posts that have any.

use blyg_core::profile::Relation;
use blyg_core::{Mention, Response};
use gpui_kit::*;

use super::vm;
use crate::app::MainView;
use crate::theme::Rule; // --- themes --- dividers

/// One line of the list: who · what · when, and where a click goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseRow {
    pub who: String,
    /// "quoted this" / "stubbed this" / "forked this".
    pub what: &'static str,
    pub when: String,
    /// The responding post: its origin, id and version.
    pub origin: String,
    pub id: Option<String>,
    pub version: Option<u32>,
    /// Its page, when there's no id to open it by.
    pub url: Option<String>,
    at: i64,
}

fn what_of(r: Relation) -> &'static str {
    match r {
        Relation::Quotes => "quoted this",
        Relation::Stubs => "stubbed this",
        Relation::Forks => "forked this",
    }
}

fn mention_what(rel: Option<&str>) -> &'static str {
    match rel {
        Some("stub") => "stubbed this",
        Some("fork") => "forked this",
        Some("transclusion") => "quoted this",
        _ => "mentioned this",
    }
}

fn ts(s: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|t| t.timestamp_millis())
        .unwrap_or(0)
}

/// The list for the post `target_id`: verified, unhidden mentions of it
/// (your own posts) and the network's responses, one row per responding
/// post and relation, newest first.
pub fn response_rows(
    network: &[Response],
    mentions: &[Mention],
    target_id: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Vec<ResponseRow> {
    let mut rows: Vec<ResponseRow> = Vec::new();
    for m in mentions.iter().filter(|m| {
        m.status == "verified" && !m.hidden && m.target_item_id.eq_ignore_ascii_case(target_id)
    }) {
        let origin = m.source_origin.clone().unwrap_or_else(|| m.source.clone());
        let when = m
            .verified_at
            .clone()
            .unwrap_or_else(|| m.first_seen.clone());
        rows.push(ResponseRow {
            who: m
                .source_author
                .as_ref()
                .and_then(|a| a.name.clone())
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| vm::host(&origin)),
            what: mention_what(m.relation.as_deref()),
            when: crate::vm::relative_time(&when, now),
            origin,
            id: m.source_id.clone(),
            version: m.source_version,
            url: Some(m.source.clone()),
            at: ts(&when),
        });
    }
    for r in network {
        let it = &r.item;
        let at = it.post_time().at;
        rows.push(ResponseRow {
            who: it
                .author
                .as_ref()
                .and_then(|a| a.name.clone())
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| it.subscription_title.clone()),
            what: what_of(r.relation),
            when: crate::vm::relative_time(&at, now),
            origin: it.origin.clone(),
            id: Some(it.remote_id.clone()),
            version: Some(it.version),
            url: None,
            at: ts(&at),
        });
    }
    rows.sort_by_key(|r| std::cmp::Reverse(r.at));
    let mut seen = std::collections::HashSet::new();
    rows.retain(|r| {
        let key = match &r.id {
            Some(id) => blyg_core::post_key(&r.origin, id),
            None => (r.url.clone().unwrap_or_default(), String::new()),
        };
        seen.insert((key, r.what))
    });
    rows
}

impl MainView {
    /// The blyg you're connected to is `origin`.
    pub(crate) fn is_own_origin(&self, origin: &str) -> bool {
        self.backend
            .base_url()
            .is_some_and(|b| blyg_core::profile::same_origin(&b, origin))
    }

    /// The stream's marker: this post has responses in your network.
    pub(crate) fn has_responses(&self, origin: &str, id: &str) -> bool {
        self.reading
            .responded
            .contains(&blyg_core::post_key(origin, id))
    }

    /// The responses list for `(origin, id)`; `None` when there are none.
    pub(crate) fn render_responses(
        &self,
        origin: &str,
        id: &str,
        network: &[Response],
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let mentions: &[Mention] = if self.is_own_origin(origin) {
            self.reading
                .mentions
                .ready()
                .map(Vec::as_slice)
                .unwrap_or(&[])
        } else {
            &[]
        };
        let rows = response_rows(network, mentions, id, self.now);
        if rows.is_empty() {
            return None;
        }
        let p = self.palette.on_page();
        Some(
            div()
                .id("responses")
                .debug_selector(|| "responses".into())
                .flex_none()
                .max_h(px(180.))
                .overflow_y_scroll()
                .px(px(32.))
                .pt(px(10.))
                .pb(px(6.))
                .rule_t(&p)
                .font_family(self.chrome())
                .text_size(px(12.))
                .child(
                    div()
                        .mb(px(4.))
                        .text_size(px(11.))
                        .text_color(p.muted)
                        .child("↩ Responses"),
                )
                .children(rows.into_iter().enumerate().map(|(i, r)| {
                    let (o, rid, v, url) = (r.origin.clone(), r.id.clone(), r.version, r.url);
                    div()
                        .id(("response", i))
                        .flex()
                        .gap(px(6.))
                        .py(px(2.))
                        .cursor_pointer()
                        .hover(|s| s.text_color(p.accent))
                        .child(
                            div()
                                .text_color(p.ink)
                                .font_weight(FontWeight::MEDIUM)
                                .child(r.who),
                        )
                        .child(
                            div()
                                .text_color(p.muted)
                                .child(format!("· {} · {}", r.what, r.when)),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| match (&rid, &url) {
                            (Some(id), _) => {
                                this.open_original(o.clone(), id.clone(), v, window, cx)
                            }
                            (None, Some(u)) => cx.open_url(u),
                            _ => {}
                        }))
                }))
                .into_any_element(),
        )
    }

    /// The stream's small "↩" marker (no number).
    pub(crate) fn responses_marker(&self) -> AnyElement {
        let p = self.palette.on_page();
        div()
            .id("stream-responses")
            .text_color(p.accent)
            .child("↩")
            .tooltip(|_, cx| {
                cx.new(|_| {
                    super::Tip("Responses in your network · open the post to see who".into())
                })
                .into()
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{Mention, Relation, Response, response_rows};

    fn mention(
        id: &str,
        target: &str,
        rel: &str,
        status: &str,
        hidden: bool,
        ago_h: i64,
    ) -> Mention {
        let t = (chrono::Utc::now() - chrono::Duration::hours(ago_h)).to_rfc3339();
        Mention {
            id: id.into(),
            target_item_id: target.into(),
            status: status.into(),
            relation: Some(rel.into()),
            source: format!("https://rue.blyg.example.com/t/{id}"),
            source_origin: Some("https://rue.blyg.example.com/".into()),
            source_id: Some(id.into()),
            source_kind: Some("thread".into()),
            source_version: Some(1),
            source_author: Some(blyg_core::Author {
                name: Some("Rue".into()),
                url: None,
            }),
            first_seen: t.clone(),
            verified_at: Some(t),
            hidden,
        }
    }

    #[test]
    fn a_list_of_who_what_when_newest_first_never_a_count() {
        let now = chrono::Utc::now();
        let seed = crate::fake::reading_seed::seed(now);
        let lin = seed
            .reading
            .iter()
            .find(|r| r.remote_id == crate::fake::reading_seed::LIN_FORK)
            .unwrap()
            .clone();
        let network = vec![Response {
            item: lin,
            relation: Relation::Forks,
            version: Some(1),
        }];
        let mentions = vec![
            mention("m1", "01OWN", "stub", "verified", false, 1),
            mention("m2", "01OWN", "transclusion", "pending", false, 1),
            mention("m3", "01OWN", "fork", "verified", true, 1),
            mention("m4", "01OTHER", "fork", "verified", false, 1),
            mention("m1", "01OWN", "stub", "verified", false, 1),
        ];
        let rows = response_rows(&network, &mentions, "01own", now);
        let got: Vec<(&str, &str)> = rows.iter().map(|r| (r.who.as_str(), r.what)).collect();
        assert_eq!(got, [("Rue", "stubbed this"), ("Lin", "forked this")]);
        for r in &rows {
            let words = format!("{} {}", r.who, r.what);
            assert!(!words.chars().any(|c| c.is_ascii_digit()), "{words}");
        }
        assert!(response_rows(&[], &[], "x", now).is_empty());
    }
}
