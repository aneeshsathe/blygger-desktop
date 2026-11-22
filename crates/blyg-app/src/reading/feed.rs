//! --- reader order --- Reading in studio 0.68's six tabs, on Burrow's
//! sources: **Feed** (the All smart feed, renamed), **Mentions**, **Agentic
//! feed** (studio 0.69's edition, `agentic.rs`), **Saved** (the shelf, renamed),
//! **Background** and **Raw**. Burrow's other smart feeds, hoppers, trails
//! and folders stay.
//!
//! - The Feed is pace-layered by default (`blyg_core::pace`), or
//!   chronological: the `readerOrder` setting, saved on the blyg when it
//!   has it (studio 0.68), else on this computer. As the studio's lists
//!   (every one but a hopper's, Saved and Raw), the Stream and every
//!   source but hoppers, trails, Saved, Raw and the Agentic feed follow it.
//! - The places are worked out off the UI thread whenever the rows change
//!   (`feed_rebuild`); until they arrive the last ones stand, and a row
//!   with none yet goes on top.
//! - The Feed's own controls sit in the reading header: All · Threads ·
//!   Fragments (in Reader, on the Feed and its two kinds) and Pace-layered ·
//!   Chronological.
//! - Raw is what came in from the blygs you read, newest first: nothing
//!   moved to Background, nothing folded, withdrawn posts included.
//! - The Agentic feed draws the edition an assistant wrote (`agentic.rs`),
//!   listing its posts in its order; its prompt is Blyg › AI on Your
//!   Blyg…'s "Agentic feed prompt".
//!
//! The rules are `feed_vm.rs` (pure, shared with Burrow in a browser);
//! this file is the app's half: rebuilding off the UI thread, the blyg's
//! setting, and the controls.

use blyg_core::order::ReaderOrder;
use blyg_core::pace::RowFacts;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

pub(crate) use super::feed_vm::*;
use super::sources_vm::{Smart, Source};
use crate::app::MainView;

impl MainView {
    /// The rows (or the order) changed: the Feed's places again, worked
    /// out off the UI thread. Here only each row's facts are taken (no
    /// text); the cached changelogs are read and the replay runs in the
    /// background, and the places land if no later rebuild started.
    pub(crate) fn feed_rebuild(&mut self, cx: &mut Context<Self>) {
        let ticket = self.reading.feed.start_rebuild();
        if self.reading.feed.order == ReaderOrder::Newest {
            self.reading.feed.clear_places();
            return;
        }
        let facts = self.feed_facts();
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move {
            let logs = backend.cached_changelogs();
            places(facts, &logs)
        });
        cx.spawn(async move |this, cx| {
            let placed = task.await;
            let _ = this.update(cx, |v, cx| {
                if v.reading.feed.land(ticket, placed) {
                    v.reading.refilter();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// What the Feed's order reads of each held row (no text), an RSS
    /// row (the studio's l0) lending no quotes.
    pub(crate) fn feed_facts(&self) -> Vec<RowFacts> {
        let r = &self.reading;
        r.rows
            .iter()
            .map(|row| RowFacts::of(row, is_l0(row, &r.subs)))
            .collect()
    }

    /// Show `order` (the places worked out again for it).
    fn show_reader_order(&mut self, order: ReaderOrder, cx: &mut Context<Self>) {
        if self.reading.feed.order != order {
            self.reading.feed.order = order;
            self.feed_rebuild(cx);
            self.reading.refilter();
        }
        cx.notify();
    }

    /// Read the blyg's `readerOrder` once a session (and when Settings
    /// opens); a blyg without it keeps the local choice. A choice made
    /// here while it was being read wins: it's sent again, now that the
    /// blyg is known to keep it, instead of being undone.
    pub(crate) fn reader_order_refresh(&mut self, force: bool, cx: &mut Context<Self>) {
        if self.reading.feed.loaded && !force {
            return;
        }
        self.reading.feed.loaded = true;
        let chosen = self.reading.feed.chosen;
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.load_reader_order() });
        cx.spawn(async move |this, cx| {
            let Ok(Some(order)) = task.await else {
                return;
            };
            let _ = this.update(cx, |v, cx| {
                let now = v.reading.feed.order;
                if v.reading.feed.chosen != chosen {
                    if now != order {
                        let at = v.reading.feed.chosen;
                        v.save_reader_order(now, at, cx);
                    }
                } else {
                    v.reading.feed.confirmed = order;
                    v.show_reader_order(order, cx);
                }
            });
        })
        .detach();
    }

    /// Choose the Feed's order: at once here, then saved (the blyg's
    /// setting, that key alone, when it has it).
    pub(crate) fn set_reader_order(&mut self, order: ReaderOrder, cx: &mut Context<Self>) {
        if self.reading.feed.order == order {
            return;
        }
        let at = self.reading.feed.choose(order);
        self.feed_rebuild(cx);
        self.reading.refilter();
        cx.notify();
        self.save_reader_order(order, at, cx);
    }

    /// Save `order` (already shown) for the choice `at`: the Feed shows
    /// what was kept, or the order last confirmed when the save fails;
    /// an answer overtaken by a later choice changes nothing.
    fn save_reader_order(&mut self, order: ReaderOrder, at: u64, cx: &mut Context<Self>) {
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.save_reader_order(order) });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                let Some(shown) = v.reading.feed.answer(at, r.as_ref().ok().copied()) else {
                    return;
                };
                if let Err(e) = &r {
                    v.show_toast(format!("Couldn't change the order: {e}"), None, cx);
                }
                v.show_reader_order(shown, cx);
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
            "The Feed's order, also chosen in the reading header. Hoppers, Saved, trails \
             and Raw keep their own order. Saved on your blyg when it keeps the setting \
             (blygger-studio 0.68 or later), else on this computer.",
        ))
        .into_any_element()
    }
}
