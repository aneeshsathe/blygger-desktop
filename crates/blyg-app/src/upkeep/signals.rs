//! --- upkeep --- Blyg › Signals…: Studio's Signals page. Your likes, your
//! dislikes and your private activity log (thumbs, hoppers, stubs, forks,
//! quotes), newest first, a page at a time. Nothing here is published and
//! nothing is counted: the tabs carry no numbers (site/src/faq.md).
//!
//! ⏎ (or a click) opens a post in Reading when this Mac holds it.

use blyg_core::upkeep::{Interaction, Page, Thumb, row_label};

use super::*;

/// The sheet's three lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Liked,
    Disliked,
    Activity,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Liked, Tab::Disliked, Tab::Activity];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Liked => "👍 Liked",
            Tab::Disliked => "👎 Disliked",
            Tab::Activity => "Activity",
        }
    }

    fn empty(self) -> &'static str {
        match self {
            Tab::Liked => "Nothing liked yet. Use 👍 on a post in Reading.",
            Tab::Disliked => "Nothing disliked yet. Use 👎 on a post in Reading.",
            Tab::Activity => "No activity yet.",
        }
    }
}

/// One line of a list, whichever list it's from.
#[derive(Debug, Clone, PartialEq)]
pub struct SignalRow {
    pub label: String,
    /// What happened (activity only): "added to hopper “Garden”".
    pub what: Option<String>,
    pub origin: String,
    pub at: String,
    /// (subscription, remote id), while you follow it.
    pub key: Option<(String, String)>,
}

impl SignalRow {
    pub fn of_thumb(t: &Thumb) -> Self {
        SignalRow {
            label: row_label(t.label.as_deref(), &t.origin, &t.remote_id),
            what: None,
            origin: t.origin.clone(),
            at: t.at.clone(),
            key: t.subscription_id.clone().map(|s| (s, t.remote_id.clone())),
        }
    }

    pub fn of_interaction(i: &Interaction) -> Self {
        let mut what = i.what();
        if i.backfilled {
            what.push_str(" · from earlier records");
        }
        SignalRow {
            label: row_label(i.label.as_deref(), &i.origin, &i.remote_id),
            what: Some(what),
            origin: i.origin.clone(),
            at: i.at.clone(),
            key: i.subscription_id.clone().map(|s| (s, i.remote_id.clone())),
        }
    }
}

/// One list as far as it's been read.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Listed {
    pub rows: Vec<SignalRow>,
    pub next: Option<String>,
}

impl Listed {
    fn extend<T>(&mut self, page: Page<T>, row: impl Fn(&T) -> SignalRow) {
        self.rows.extend(page.items.iter().map(row));
        self.next = page.next;
    }
}

/// The sheet.
pub(crate) struct SignalsSheet {
    pub focus: FocusHandle,
    pub tab: Tab,
    pub sel: usize,
    pub liked: Option<Fetch<Listed>>,
    pub disliked: Option<Fetch<Listed>>,
    pub activity: Option<Fetch<Listed>>,
    /// A further page is on its way.
    pub more: bool,
}

impl SignalsSheet {
    pub fn list(&self, tab: Tab) -> Option<&Fetch<Listed>> {
        match tab {
            Tab::Liked => self.liked.as_ref(),
            Tab::Disliked => self.disliked.as_ref(),
            Tab::Activity => self.activity.as_ref(),
        }
    }

    fn list_mut(&mut self, tab: Tab) -> &mut Option<Fetch<Listed>> {
        match tab {
            Tab::Liked => &mut self.liked,
            Tab::Disliked => &mut self.disliked,
            Tab::Activity => &mut self.activity,
        }
    }

    pub fn rows(&self) -> &[SignalRow] {
        match self.list(self.tab) {
            Some(Fetch::Ready(l)) => &l.rows,
            _ => &[],
        }
    }
}

impl MainView {
    /// Blyg › Signals…: open the sheet on your likes, or close it.
    pub(crate) fn toggle_signals_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.sheet, Some(Sheet::Signals(_))) {
            return self.close_sheet(window, cx);
        }
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.sheet_gen += 1;
        self.sheet = Some(Sheet::Signals(SignalsSheet {
            focus,
            tab: Tab::Liked,
            sel: 0,
            liked: None,
            disliked: None,
            activity: None,
            more: false,
        }));
        self.load_signals(Tab::Liked, None, cx);
        cx.notify();
    }

    fn signals_sheet(&mut self) -> Option<&mut SignalsSheet> {
        match &mut self.sheet {
            Some(Sheet::Signals(s)) => Some(s),
            _ => None,
        }
    }

    /// Read `tab`'s first page (`cursor` None) or its next one.
    fn load_signals(&mut self, tab: Tab, cursor: Option<String>, cx: &mut Context<Self>) {
        let Some(s) = self.signals_sheet() else {
            return;
        };
        if cursor.is_some() {
            if s.more {
                return;
            }
            s.more = true;
        } else {
            *s.list_mut(tab) = Some(Fetch::Loading);
        }
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move {
            let c = cursor.as_deref();
            match tab {
                Tab::Liked | Tab::Disliked => {
                    let thumb = if tab == Tab::Liked { 1 } else { -1 };
                    backend.thumbs_page(thumb, c).map(|p| {
                        let mut l = Listed::default();
                        l.extend(p, SignalRow::of_thumb);
                        l
                    })
                }
                Tab::Activity => backend.interactions_page(c).map(|p| {
                    let mut l = Listed::default();
                    l.extend(p, SignalRow::of_interaction);
                    l
                }),
            }
        });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                let Some(s) = v.signals_sheet() else { return };
                let more = std::mem::take(&mut s.more);
                let slot = s.list_mut(tab);
                match (r, slot.as_mut()) {
                    (Ok(page), Some(Fetch::Ready(l))) if more => {
                        l.rows.extend(page.rows);
                        l.next = page.next;
                    }
                    (Ok(page), _) => *slot = Some(Fetch::Ready(page)),
                    (Err(e), _) if more => {
                        v.show_toast(format!("Couldn't read more: {e}"), None, cx);
                    }
                    (Err(e), _) => *slot = Some(Fetch::Failed(e.to_string())),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn signals_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        let Some(s) = self.signals_sheet() else {
            return;
        };
        s.tab = tab;
        s.sel = 0;
        if s.list(tab).is_none() {
            self.load_signals(tab, None, cx);
        }
        cx.notify();
    }

    fn signals_more(&mut self, cx: &mut Context<Self>) {
        let Some(s) = self.signals_sheet() else {
            return;
        };
        let tab = s.tab;
        let next = match s.list(tab) {
            Some(Fetch::Ready(l)) => l.next.clone(),
            _ => None,
        };
        if next.is_some() {
            self.load_signals(tab, next, cx);
        }
    }

    /// Open the highlighted post in Reading, when this Mac holds it.
    fn open_signal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.signals_sheet() else {
            return;
        };
        let Some(row) = s.rows().get(s.sel).cloned() else {
            return;
        };
        let held = row.key.clone().filter(|(sub, rid)| {
            self.backend
                .reading()
                .iter()
                .any(|r| &r.subscription_id == sub && &r.remote_id == rid)
        });
        let Some(key) = held else {
            return self.show_toast(
                "That post isn't held on this Mac",
                Some(blyg_core::upkeep::host(&row.origin).into()),
                cx,
            );
        };
        self.sheet = None;
        if self.reading.view != reading::View::Reading {
            self.show_view(reading::View::Reading, window, cx);
        }
        self.select_source(
            reading::sources_vm::Source::Smart(reading::sources_vm::Smart::All),
            cx,
        );
        self.reading.sel = Some(key.clone());
        self.open_reading(key, window, cx);
        cx.notify();
    }

    fn signals_sheet_key(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(s) = self.signals_sheet() else {
            return;
        };
        let n = s.rows().len();
        let at = Tab::ALL.iter().position(|t| *t == s.tab).unwrap_or(0);
        match ev.keystroke.key.as_str() {
            "escape" => self.close_sheet(window, cx),
            "1" => self.signals_tab(Tab::Liked, cx),
            "2" => self.signals_tab(Tab::Disliked, cx),
            "3" => self.signals_tab(Tab::Activity, cx),
            "left" => self.signals_tab(Tab::ALL[at.saturating_sub(1)], cx),
            "right" => self.signals_tab(Tab::ALL[(at + 1).min(2)], cx),
            "up" => {
                s.sel = s.sel.saturating_sub(1);
                cx.notify();
            }
            "down" => {
                s.sel = (s.sel + 1).min(n.saturating_sub(1));
                if s.sel + 1 >= n {
                    self.signals_more(cx);
                }
                cx.notify();
            }
            "enter" => self.open_signal(window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    pub(crate) fn render_signals_sheet(
        &self,
        s: &SignalsSheet,
        cx: &mut Context<Self>,
    ) -> (f32, AnyElement) {
        let p = self.palette;
        let tabs = div().flex().gap(px(6.)).children(Tab::ALL.map(|t| {
            let on = t == s.tab;
            crate::theme_ext::chip(
                div().id(SharedString::from(format!("signals-tab-{t:?}"))),
                &self.theme,
                on,
            )
            .debug_selector(move || format!("signals-tab-{t:?}"))
            .px(px(10.))
            .py(px(3.))
            .text_size(px(12.))
            .cursor_pointer()
            .when(!on, |d| d.text_color(p.muted))
            .child(t.label())
            .on_click(cx.listener(move |this, _, _, cx| this.signals_tab(t, cx)))
        }));
        let list: AnyElement = match s.list(s.tab) {
            None | Some(Fetch::Loading) => self.upkeep_note("Loading…"),
            Some(Fetch::Failed(e)) => self.upkeep_note(format!("Couldn't load your signals: {e}")),
            Some(Fetch::Ready(l)) if l.rows.is_empty() => self.upkeep_note(s.tab.empty()),
            Some(Fetch::Ready(l)) => {
                let mut list = div()
                    .id("signals-list")
                    .max_h(px(380.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(1.));
                for (i, r) in l.rows.iter().enumerate() {
                    let selected = i == s.sel;
                    let muted = if selected { p.sel_muted } else { p.muted };
                    let mut sub = Vec::new();
                    if let Some(w) = &r.what {
                        sub.push(w.clone());
                    }
                    sub.push(blyg_core::upkeep::host(&r.origin));
                    sub.push(crate::vm::relative_time(&r.at, self.now));
                    list = list.child(
                        div()
                            .id(SharedString::from(format!("signal-row-{i}")))
                            .debug_selector(move || format!("signal-row-{i}"))
                            .px(px(8.))
                            .py(px(5.))
                            .rounded(px(6.))
                            .cursor_pointer()
                            .when(selected, |d| d.bg(p.sel))
                            .hover(|d| d.bg(p.sel))
                            .child(
                                div()
                                    .truncate()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(if selected { p.sel_ink } else { p.ink })
                                    .child(r.label.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(11.5))
                                    .text_color(muted)
                                    .child(sub.join(" · ")),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(s) = this.signals_sheet() {
                                    s.sel = i;
                                }
                                this.open_signal(window, cx)
                            })),
                    );
                }
                if l.next.is_some() {
                    list = list.child(
                        div().pt(px(6.)).child(
                            self.upkeep_chip("signals-more", "Load more", !s.more)
                                .on_click(cx.listener(|this, _, _, cx| this.signals_more(cx))),
                        ),
                    );
                }
                list.into_any_element()
            }
        };
        let body = div()
            .track_focus(&s.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                this.signals_sheet_key(ev, window, cx)
            }))
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(13.))
                    .child("Signals"),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .line_height(relative(1.45))
                    .text_color(p.muted)
                    .child(
                        "A private record of what you've liked, disliked, put in hoppers and \
                         responded to. It's never published, and never counted.",
                    ),
            )
            .child(tabs)
            .child(list)
            .child(div().my(px(4.)).h(px(1.)).bg(p.line))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .child("1–3 or ←/→ switch · ↑/↓ choose · ⏎ opens in Reading · esc"),
            );
        (480., body.into_any_element())
    }
}
