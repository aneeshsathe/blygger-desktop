//! --- discover --- Blyg › Discover Blygs… (and Discover… on the
//! Subscriptions screen): the blygs listed on blygger.com that you don't
//! follow yet, newest listing first, five at a time (studio 0.60's ✦ sheet;
//! `blyg_core::discover` holds the rules).
//!
//! Opening the sheet reads the directory off the main thread, once per
//! visit, and moves `discoverSeenAt` to the newest listing shown; rows
//! listed since the last visit say "new". j/k move, ⏎ subscribes (the same
//! `Backend::subscribe` as Subscribe…), o opens the blyg's home page in the
//! browser, m shows more, esc closes. Desktop only: Burrow on the web reads
//! nothing but its own blyg.
//!
//! Every text in a row is remote input and is drawn as plain text; links
//! are https only (`discover::safe_url`).

use std::collections::{HashMap, HashSet};

use blyg_core::CoreError;
use blyg_core::discover::{self, DirectoryBlyg, PAGE_SIZE, Visit};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::RSheet;
use crate::app::MainView;

gpui_kit::actions!(blygger, [ShowDiscover]);

/// What one Subscribe in this visit came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowState {
    Working,
    Added,
    /// The blyg said you already follow it (409).
    Exists,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Loading,
    Failed(String),
    Ready(Visit),
}

/// The Discover sheet.
pub struct Sheet {
    pub phase: Phase,
    /// What you follow, as the visit began (`discover::subscribed_keys`).
    pub keys: HashSet<String>,
    /// Rows subscribed during this visit, by origin.
    pub rows: HashMap<String, RowState>,
    /// How many of the list are shown.
    pub limit: usize,
    pub sel: usize,
    pub focus: FocusHandle,
    pub scroll: ScrollHandle,
}

impl Sheet {
    pub fn new(subs: &[blyg_core::Subscription], focus: FocusHandle) -> Self {
        Sheet {
            phase: Phase::Loading,
            keys: discover::subscribed_keys(subs),
            rows: HashMap::new(),
            limit: PAGE_SIZE,
            sel: 0,
            focus,
            scroll: ScrollHandle::new(),
        }
    }

    fn visit(&self) -> Option<&Visit> {
        match &self.phase {
            Phase::Ready(v) => Some(v),
            _ => None,
        }
    }

    /// Every blyg not followed (and the ones touched this visit), newest
    /// listing first.
    pub fn list(&self) -> Vec<&DirectoryBlyg> {
        let Some(v) = self.visit() else {
            return vec![];
        };
        discover::unfollowed(&v.blygs, &self.keys, &|b| self.rows.contains_key(&b.origin))
    }

    /// The rows on screen: the first `limit` of [`list`](Self::list).
    pub fn shown(&self) -> Vec<&DirectoryBlyg> {
        let mut l = self.list();
        l.truncate(self.limit);
        l
    }

    /// How many "show more" adds.
    pub fn more(&self) -> usize {
        self.list().len().saturating_sub(self.limit).min(PAGE_SIZE)
    }

    pub fn followed(&self, b: &DirectoryBlyg) -> bool {
        discover::is_subscribed(b, &self.keys)
            || matches!(
                self.rows.get(&b.origin),
                Some(RowState::Added | RowState::Exists)
            )
    }

    pub fn busy(&self) -> bool {
        self.rows.values().any(|r| *r == RowState::Working)
    }

    pub fn selected(&self) -> Option<DirectoryBlyg> {
        self.shown().get(self.sel).map(|b| (*b).clone())
    }

    pub fn show_more(&mut self) {
        self.limit += self.more();
    }

    pub fn step(&mut self, by: isize) {
        let n = self.shown().len();
        if n == 0 {
            return;
        }
        self.sel = (self.sel as isize + by).clamp(0, n as isize - 1) as usize;
        self.scroll.scroll_to_item(self.sel);
    }
}

impl MainView {
    /// Hook: the Discover action on the root element.
    pub(super) fn discover_actions(
        &self,
        d: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        d.on_action(
            cx.listener(|this, _: &ShowDiscover, window, cx| this.open_discover(window, cx)),
        )
    }

    /// Discover Blygs…: the sheet, then the directory (off the main thread).
    pub(crate) fn open_discover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.reading.sheet.is_some() {
            return;
        }
        if crate::connection::mode(cx) == Some(crate::connection::Mode::Disconnected) {
            self.show_toast(
                "Connect a blyg first",
                Some("Subscriptions live on your blyg".into()),
                cx,
            );
            return;
        }
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let subs = self.backend.subscriptions();
        self.open_reading_sheet(RSheet::Discover(Box::new(Sheet::new(&subs, focus))), cx);
        let gen_ = self.reading.sheet_gen;
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { discover::open(backend.as_ref()) });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                if v.reading.sheet_gen != gen_ {
                    return;
                }
                if let Some(s) = v.discover_sheet() {
                    s.phase = match r {
                        Ok(visit) => Phase::Ready(visit),
                        Err(e) => Phase::Failed(match e {
                            CoreError::Offline => "Could not reach your blyg.".into(),
                            e => e.to_string(),
                        }),
                    };
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn discover_sheet(&mut self) -> Option<&mut Sheet> {
        match self.reading.sheet.as_mut() {
            Some(RSheet::Discover(s)) => Some(s),
            _ => None,
        }
    }

    /// ⏎ / Subscribe: follow the row, through the same path as Subscribe….
    /// One at a time, as the studio paces them.
    pub(crate) fn discover_subscribe(&mut self, origin: &str, cx: &mut Context<Self>) {
        let gen_ = self.reading.sheet_gen;
        let Some(s) = self.discover_sheet() else {
            return;
        };
        let Some(b) = s.list().into_iter().find(|b| b.origin == origin).cloned() else {
            return;
        };
        if s.busy() || s.followed(&b) {
            return;
        }
        s.rows.insert(b.origin.clone(), RowState::Working);
        let backend = self.backend.clone();
        let url = b.origin.clone();
        let task = cx.background_spawn(async move { backend.subscribe(&url, None) });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                let state = match r {
                    Ok(_) => RowState::Added,
                    Err(CoreError::Rejected { status: 409, .. }) => RowState::Exists,
                    Err(CoreError::Offline) => RowState::Failed("couldn't reach your blyg".into()),
                    Err(e) => RowState::Failed(e.to_string()),
                };
                v.reading.subs = v.backend.subscriptions();
                if v.reading.sheet_gen == gen_
                    && let Some(s) = v.discover_sheet()
                {
                    s.rows.insert(b.origin.clone(), state);
                } else if state == RowState::Added {
                    // Closed meanwhile: say so outside the sheet.
                    v.show_toast(format!("Discover: subscribed to {}", b.title), None, cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// o: the blyg's home page in the browser (https only, checked when
    /// the directory was read).
    fn discover_open_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(b) = self.discover_sheet().and_then(|s| s.selected()) {
            cx.open_url(&b.home);
        }
    }

    /// j/k (↓/↑) move, ⏎ subscribe, o open, m more, esc close.
    fn discover_key(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let k = &ev.keystroke;
        let m = k.modifiers;
        if m.secondary() || m.alt || m.control {
            return false;
        }
        let Some(s) = self.discover_sheet() else {
            return false;
        };
        match k.key.as_str() {
            "escape" => self.close_reading_sheet(window, cx),
            "j" | "down" => s.step(1),
            "k" | "up" => s.step(-1),
            "m" => s.show_more(),
            "o" => self.discover_open_selected(cx),
            "enter" => {
                if let Some(b) = s.selected() {
                    self.discover_subscribe(&b.origin, cx);
                }
            }
            _ => return false,
        }
        cx.notify();
        true
    }

    // ------------------------------------------------------------ render

    pub(super) fn render_discover_sheet(&self, s: &Sheet, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette.on_page();
        let now = chrono::Utc::now().timestamp_millis();
        let body: AnyElement = match &s.phase {
            Phase::Loading => div()
                .id("discover-loading")
                .text_color(p.muted)
                .child("Reading the directory at blygger.com…")
                .into_any_element(),
            Phase::Failed(msg) => div()
                .id("discover-error")
                .debug_selector(|| "discover-error".into())
                .text_color(p.over)
                .child(msg.clone())
                .into_any_element(),
            Phase::Ready(visit) => {
                let shown = s.shown();
                if shown.is_empty() {
                    div()
                        .text_color(p.muted)
                        .child("You follow every blyg listed on blygger.com.")
                        .into_any_element()
                } else {
                    let busy = s.busy();
                    let mut list = div()
                        .id("discover-list")
                        .debug_selector(|| "discover-list".into())
                        .flex()
                        .flex_col()
                        .max_h(px(380.))
                        .overflow_y_scroll()
                        .track_scroll(&s.scroll);
                    for (i, b) in shown.iter().enumerate() {
                        let followed = s.followed(b);
                        let state = s.rows.get(&b.origin);
                        let new = discover::badge_new(b, visit.baseline.as_deref(), followed);
                        let selected = i == s.sel;
                        let status: Option<(String, Hsla)> = match state {
                            Some(RowState::Working) => Some(("adding…".into(), p.muted)),
                            Some(RowState::Added) => Some(("added".into(), p.green)),
                            Some(RowState::Exists) => Some(("subscribed".into(), p.muted)),
                            Some(RowState::Failed(r)) => Some((format!("failed: {r}"), p.over)),
                            None if followed => Some(("subscribed".into(), p.muted)),
                            None => None,
                        };
                        let can_subscribe =
                            !followed && state.is_none_or(|r| matches!(r, RowState::Failed(_)));
                        let (o1, o2, home) = (b.origin.clone(), b.origin.clone(), b.home.clone());
                        let level = b.activity.level.map(|l| l.as_str());
                        list = list.child(
                            div()
                                .id(("discover-row", i))
                                .debug_selector(move || format!("discover-row-{i}"))
                                .flex()
                                .items_start()
                                .gap(px(10.))
                                .px(px(6.))
                                .py(px(5.))
                                .rounded(px(5.))
                                .when(selected, |d| d.bg(p.sel))
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(s) = this.discover_sheet()
                                        && let Some(i) =
                                            s.shown().iter().position(|b| b.origin == o1)
                                    {
                                        s.sel = i;
                                        cx.notify();
                                    }
                                }))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(6.))
                                                .child(
                                                    div()
                                                        .truncate()
                                                        .font_weight(FontWeight::MEDIUM)
                                                        .child(b.title.clone()),
                                                )
                                                .when(new, |d| {
                                                    d.child(
                                                        div()
                                                            .flex_none()
                                                            .px(px(5.))
                                                            .rounded(px(4.))
                                                            .text_size(px(10.5))
                                                            .bg(p.pick())
                                                            .text_color(p.accent_text())
                                                            .child("new"),
                                                    )
                                                }),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(11.5))
                                                .text_color(p.muted)
                                                .truncate()
                                                .child(b.home_label()),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(6.))
                                                .text_size(px(11.5))
                                                .text_color(p.muted)
                                                .when_some(level, |d, l| {
                                                    d.child(
                                                        div()
                                                            .px(px(5.))
                                                            .rounded(px(4.))
                                                            .border_1()
                                                            .border_color(p.line)
                                                            .text_size(px(10.5))
                                                            .child(l),
                                                    )
                                                })
                                                .child(discover::activity_line(b, now)),
                                        )
                                        .when_some(status, |d, (text, color)| {
                                            d.child(
                                                div()
                                                    .text_size(px(11.5))
                                                    .text_color(color)
                                                    .child(text),
                                            )
                                        }),
                                )
                                .child(
                                    self.chip(
                                        SharedString::from(format!("discover-open-{i}")),
                                        "open ↗",
                                    )
                                    .flex_none()
                                    .on_click(move |_, _, cx| cx.open_url(&home)),
                                )
                                .when(can_subscribe, |d| {
                                    d.child(if busy {
                                        self.chip_disabled(
                                            SharedString::from(format!("discover-sub-{i}")),
                                            "subscribe",
                                        )
                                        .flex_none()
                                    } else {
                                        self.chip(
                                            SharedString::from(format!("discover-sub-{i}")),
                                            "subscribe",
                                        )
                                        .flex_none()
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.discover_subscribe(&o2, cx)
                                            }),
                                        )
                                    })
                                }),
                        );
                    }
                    list.into_any_element()
                }
            }
        };
        let (count, more) = match &s.phase {
            Phase::Ready(_) => (Some(s.list().len()), s.more()),
            _ => (None, 0),
        };
        let mut keys = vec![
            self.key_hint("j/k", "move").into_any_element(),
            self.key_hint("⏎", "subscribe").into_any_element(),
            self.key_hint("o", "open in browser").into_any_element(),
        ];
        if more > 0 {
            keys.push(
                self.key_hint("m", "show more")
                    .id("discover-more")
                    .debug_selector(|| "discover-more".into())
                    .cursor_pointer()
                    .text_color(p.accent_text())
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(s) = this.discover_sheet() {
                            s.show_more();
                            cx.notify();
                        }
                    }))
                    .into_any_element(),
            );
        }
        keys.push(
            crate::platform::key_button(
                self.key_hint("esc", "close"),
                "discover-close",
                cx.listener(|this, _, window, cx| this.close_reading_sheet(window, cx)),
            )
            .into_any_element(),
        );
        let note = match s.visit() {
            Some(v) if v.seen_error.is_some() => Some(
                "Burrow couldn't save which blygs you've seen; they may show as new again."
                    .to_string(),
            ),
            _ => None,
        };
        let shown_n = s.shown().len();
        div()
            .track_focus(&s.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                if this.discover_key(ev, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .child(self.sheet_heading("Discover blygs"))
            .child(div().mb(px(8.)).text_color(p.muted).child(match count {
                Some(n) => format!(
                    "Blygs on blygger.com you don't follow yet, newest listing first · {shown_n} of {n}"
                ),
                None => "Blygs on blygger.com you don't follow yet".to_string(),
            }))
            .child(body)
            .when_some(note, |d, n| {
                d.child(div().mt(px(8.)).text_size(px(12.)).text_color(p.muted).child(n))
            })
            .child(
                div()
                    .id("discover-status")
                    .mt(px(10.))
                    .text_size(px(12.))
                    .line_height(relative(1.45))
                    .text_color(p.muted)
                    .child("Subscribing doesn't add a blyg to your public blogroll."),
            )
            .child(self.keys_row(vec![]).children(keys))
            .into_any_element()
    }
}
