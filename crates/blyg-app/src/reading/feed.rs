//! --- reader order --- Reading in studio 0.68's six tabs, on Burrow's
//! sources: **Feed** (the All smart feed, renamed), **Mentions**, **Agentic
//! feed** (a placeholder, as upstream), **Saved** (the shelf, renamed),
//! **Background** and **Raw**. Burrow's other smart feeds, hoppers, trails
//! and folders stay.
//!
//! - The Feed is pace-layered by default (`blyg_core::pace`), or
//!   chronological: the `readerOrder` setting, saved on the blyg when it
//!   has it (studio 0.68), else on this computer. The Stream and every
//!   source but hoppers, trails, Saved, Mentions, Background and Raw
//!   follow it.
//! - The Feed's own controls sit in the reading header: All · Threads ·
//!   Fragments (in Reader, on the Feed and its two kinds) and Pace-layered ·
//!   Chronological.
//! - Raw is what came in from the blygs you read, newest first: nothing
//!   moved to Background, nothing folded, withdrawn posts included.
//! - The Agentic feed lists nothing yet; its prompt is Settings › AI's
//!   "Agentic feed prompt".

use std::collections::HashMap;

use blyg_core::ReadingItem;
use blyg_core::order::ReaderOrder;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::sources_vm::{Smart, Source};
use super::stream_vm::ReadMode;
use super::vm::{self, Key};
use crate::app::MainView;

/// What the Agentic feed says (studio's placeholder).
pub const AGENTIC_NOTE: &str = "Coming soon. An assistant who organizes your reading for you: \
     what you should see, and how. You can already set its prompt in Settings › AI \
     (Agentic feed prompt).";

/// What Raw says when it holds nothing.
pub const RAW_EMPTY: &str = "Nothing has come in yet. Raw lists what came in from the blygs \
     you read, newest first: nothing moved to Background, nothing folded.";

/// The Feed's kinds, its own control (studio's All · Threads · Fragments).
pub const KINDS: [Smart; 3] = [Smart::All, Smart::Threads, Smart::Fragments];

/// A kind's name in the Feed's control.
pub fn kind_label(s: Smart) -> &'static str {
    match s {
        Smart::All => "All",
        other => other.label(),
    }
}

/// The Feed's order and each row's place in it.
#[derive(Debug, Default)]
pub struct Feed {
    pub order: ReaderOrder,
    /// Each row's place in the pace-layered order (top first); empty while
    /// the Feed is chronological.
    positions: HashMap<Key, usize>,
    /// The blyg's setting was read this session.
    pub loaded: bool,
}

impl Feed {
    pub fn new(order: ReaderOrder) -> Self {
        Feed {
            order,
            ..Default::default()
        }
    }

    /// The rows (or the order) changed: work out each row's place again,
    /// with the pins of the changelogs this computer has cached.
    pub fn rebuild(
        &mut self,
        rows: &[ReadingItem],
        changelogs: &HashMap<(String, String), Vec<blyg_core::RemoteVersion>>,
    ) {
        self.positions = match self.order {
            ReaderOrder::Newest => HashMap::new(),
            ReaderOrder::PaceLayered => blyg_core::order::feed_positions(rows, |r| {
                changelogs
                    .get(&(r.origin.clone(), r.remote_id.to_string()))
                    .cloned()
            }),
        };
    }

    /// Where `key` is in the pace-layered Feed.
    pub fn place(&self, key: &Key) -> Option<usize> {
        self.positions.get(key).copied()
    }
}

/// Whether what's shown follows the Feed's order: the Stream, and every
/// Reader source but those that keep their own (hoppers, trails, Saved,
/// Mentions, Background and Raw, as the studio's tabs) and the Agentic feed.
pub fn follows_feed(mode: ReadMode, source: &Source) -> bool {
    mode == ReadMode::Stream
        || !matches!(
            source,
            Source::Hopper(_)
                | Source::Trail(_)
                | Source::Mentions
                | Source::Smart(Smart::Shelf | Smart::Background | Smart::Raw | Smart::Agentic)
        )
}

/// Whether the Feed's kinds control shows: in Reader, on the Feed or one
/// of its kinds.
pub fn shows_kinds(mode: ReadMode, source: &Source) -> bool {
    mode == ReadMode::Reader && matches!(source, Source::Smart(s) if KINDS.contains(s))
}

/// Put `shown` (indices into `rows`) in the Feed's order, or Raw's.
pub fn arrange(
    rows: &[ReadingItem],
    shown: &mut [usize],
    mode: ReadMode,
    source: &Source,
    feed: &Feed,
) {
    if mode == ReadMode::Reader && *source == Source::Smart(Smart::Raw) {
        raw_order(rows, shown);
    } else if follows_feed(mode, source) && feed.order == ReaderOrder::PaceLayered {
        // A row with no place yet arrived since: above every place, as the
        // studio's does. The sort is stable, so those keep their order.
        shown.sort_by_key(|&i| {
            rows.get(i)
                .and_then(|r| feed.place(&vm::key(r)))
                .map_or(0, |p| p + 1)
        });
    }
}

/// Raw's order (studio's `rawIdentitySql`): newest first by the post's
/// date, clamped to when it came in; then subscription and remote id,
/// descending.
pub fn raw_order(rows: &[ReadingItem], shown: &mut [usize]) {
    let at = |r: &ReadingItem| {
        blyg_core::pace::clamped(
            r.updated.as_deref().or(r.created.as_deref()),
            &r.observed_at,
        )
        .unwrap_or(i64::MIN)
    };
    shown.sort_by(|&a, &b| {
        let (ra, rb) = (&rows[a], &rows[b]);
        at(rb)
            .cmp(&at(ra))
            .then_with(|| rb.subscription_id.cmp(&ra.subscription_id))
            .then_with(|| rb.remote_id.cmp(&ra.remote_id))
    });
}

impl MainView {
    /// The rows changed: the Feed's places again.
    pub(crate) fn feed_rebuild(&mut self) {
        let logs = match self.reading.feed.order {
            ReaderOrder::PaceLayered => self.backend.cached_changelogs(),
            ReaderOrder::Newest => HashMap::new(),
        };
        let r = &mut self.reading;
        r.feed.rebuild(&r.rows, &logs);
    }

    /// Read the blyg's `readerOrder` once a session (and when Settings
    /// opens); a blyg without it keeps the local choice.
    pub(crate) fn reader_order_refresh(&mut self, force: bool, cx: &mut Context<Self>) {
        if self.reading.feed.loaded && !force {
            return;
        }
        self.reading.feed.loaded = true;
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.load_reader_order() });
        cx.spawn(async move |this, cx| {
            let Ok(Some(order)) = task.await else {
                return;
            };
            let _ = this.update(cx, |v, cx| {
                if v.reading.feed.order != order {
                    v.reading.feed.order = order;
                    v.feed_rebuild();
                    v.reading.refilter();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Choose the Feed's order: at once here, then saved (the blyg's
    /// setting, that key alone, when it has it).
    pub(crate) fn set_reader_order(&mut self, order: ReaderOrder, cx: &mut Context<Self>) {
        let was = self.reading.feed.order;
        if was == order {
            return;
        }
        self.reading.feed.order = order;
        self.feed_rebuild();
        self.reading.refilter();
        cx.notify();
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.save_reader_order(order) });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| match r {
                Ok(kept) if kept == order => {}
                Ok(kept) => {
                    v.reading.feed.order = kept;
                    v.feed_rebuild();
                    v.reading.refilter();
                    cx.notify();
                }
                Err(e) => {
                    v.reading.feed.order = was;
                    v.feed_rebuild();
                    v.reading.refilter();
                    v.show_toast(format!("Couldn't change the order: {e}"), None, cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The Feed's controls in the reading header (kinds in Reader on the
    /// Feed; the order wherever the list follows it). `None` elsewhere.
    pub(crate) fn render_feed_controls(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let mode = self.reading.mode;
        let source = &self.reading.source;
        let kinds = shows_kinds(mode, source);
        let order = follows_feed(mode, source);
        if !kinds && !order {
            return None;
        }
        let p = self.palette.on_page();
        let corner = self.theme.corner(5.);
        let seg = |id: String, label: &'static str, on: bool| {
            div()
                .id(SharedString::from(id.clone()))
                .debug_selector(move || id.clone())
                .px(px(9.))
                .py(px(2.))
                .rounded(px(corner))
                .cursor_pointer()
                .when(on, |d| d.bg(p.bg).text_color(p.ink).shadow_sm())
                .when(!on, |d| {
                    d.text_color(p.muted).hover(|s| s.text_color(p.ink))
                })
                .child(label)
        };
        let group = |id: &'static str| {
            div()
                .id(id)
                .flex()
                .p(px(2.))
                .gap(px(2.))
                .rounded(px(self.theme.corner(7.)))
                .bg(p.well())
        };
        let mut row = div()
            .id("feed-controls")
            .flex()
            .items_center()
            .gap(px(6.))
            .font_family(self.chrome())
            .text_size(px(11.5))
            .font_weight(FontWeight::MEDIUM);
        if kinds {
            row = row.child(group("feed-kinds").children(KINDS.map(|s| {
                seg(
                    format!("feed-kind-{}", kind_label(s).to_lowercase()),
                    kind_label(s),
                    *source == Source::Smart(s),
                )
                .on_click(
                    cx.listener(move |this, _, _, cx| this.select_source(Source::Smart(s), cx)),
                )
            })));
        }
        if order {
            let now = self.reading.feed.order;
            row = row.child(group("feed-order").children(ReaderOrder::ALL.map(|o| {
                let label = match o {
                    ReaderOrder::PaceLayered => "Pace-layered",
                    ReaderOrder::Newest => "Chronological",
                };
                seg(format!("feed-order-{}", o.as_str()), label, now == o)
                    .tooltip(move |_, cx| cx.new(|_| super::Tip(o.hint().to_string())).into())
                    .on_click(cx.listener(move |this, _, _, cx| this.set_reader_order(o, cx)))
            })));
        }
        Some(row.into_any_element())
    }

    /// Settings › READER ORDER: the two orders, the Feed's choice.
    pub(crate) fn render_reader_order_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let theme = &*self.theme;
        let now = self.reading.feed.order;
        let mut col = div().flex().flex_col().gap(px(6.));
        for o in [ReaderOrder::Newest, ReaderOrder::PaceLayered] {
            let on = now == o;
            let label = match o {
                ReaderOrder::Newest => "Newest first",
                ReaderOrder::PaceLayered => "Pace-layered",
            };
            let id = format!("reader-order-{}", o.as_str());
            col = col.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .id(SharedString::from(id.clone()))
                            .debug_selector(move || id.clone())
                            .flex_none()
                            .px(px(10.))
                            .py(px(3.))
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .map(|d| crate::theme_ext::chip(d, theme, on))
                            .cursor_pointer()
                            .hover(|s| s.border_color(p.accent).bg(p.hover()))
                            .child(if on { "●" } else { "○" })
                            .child(label)
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.set_reader_order(o, cx)),
                            ),
                    )
                    .child(crate::app::settings_page::side_note(&p, o.hint())),
            );
        }
        col.child(crate::app::settings_page::note(
            &p,
            "The Feed's order, also chosen in the reading header. Hoppers, Saved, trails, \
             Background and Raw keep their own order. Saved on your blyg when it keeps the setting \
             (blygger-studio 0.68 or later), else on this computer.",
        ))
        .into_any_element()
    }
}
