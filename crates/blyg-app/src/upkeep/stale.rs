//! --- upkeep --- Blyg › Stale Quotes…: Studio's Updates tab and Refresh.
//!
//! The sheet lists your published threads whose quoted posts have newer
//! versions, stalest first (the blyg's order: the versions missed, then
//! the stale quotes). The highlighted thread shows each quote and what
//! changed (`v1 → v3 at its blyg`), asked of the quoted blygs when it's
//! highlighted. ⏎ refreshes it, ⇧⏎ refreshes every thread that can be; a
//! refresh republishes the published words with the quotes brought up to
//! date (a new version, the words unchanged), with the note typed below.
//!
//! Studio's advice stands: refresh in batches, as a thread drifts far
//! enough, rather than a version per alert.

use std::collections::HashMap;

use blyg_core::upkeep::{Refreshed, ThreadFreshness};

use super::*;

/// The note a refresh publishes with, unless you type another (Studio's).
pub const DEFAULT_NOTE: &str = "refreshed quoted snapshots";

/// One thread in the sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// Your post, when this Mac holds it.
    pub id: Option<LocalId>,
    pub title: String,
    /// Edits here not yet published.
    pub edited_here: bool,
    pub report: ThreadFreshness,
}

/// The sheet.
pub(crate) struct StaleSheet {
    pub focus: FocusHandle,
    pub note: Entity<InputState>,
    pub list: Fetch<Vec<Row>>,
    pub sel: usize,
    /// Each thread's quotes as their blygs answer now, by server id.
    pub probed: HashMap<String, Fetch<ThreadFreshness>>,
    /// What each refresh did (or why it failed), by server id.
    pub done: HashMap<String, Result<Refreshed, String>>,
    /// A refresh is running.
    pub busy: bool,
}

/// The rows for `list`: `own(sid)` finds your post by its server id.
pub fn rows(list: Vec<ThreadFreshness>, own: impl Fn(&str) -> Option<Item>) -> Vec<Row> {
    list.into_iter()
        .map(|report| {
            let item = own(&report.id);
            Row {
                id: item.as_ref().map(|i| i.local_id.clone()),
                title: item.as_ref().map(vm::item_title).unwrap_or_else(|| {
                    format!("Thread {}", report.id.chars().take(8).collect::<String>())
                }),
                edited_here: item.as_ref().is_some_and(vm::has_unpublished_edits),
                report,
            }
        })
        .collect()
}

/// Why `row` can't be refreshed now, if it can't.
pub fn why_not(row: &Row) -> Option<&'static str> {
    if row.id.is_none() {
        return Some("This thread isn't on this Mac yet");
    }
    if row.report.blocking > 0 {
        return Some("A quote no longer matches its post: edit the thread first");
    }
    if row.report.dirty || row.edited_here {
        return Some("It has unpublished edits: publish or discard them first");
    }
    if row.report.stale == 0 {
        return Some("Nothing in it is stale");
    }
    None
}

impl MainView {
    /// Blyg › Stale Quotes…: open the sheet (and ask the blyg), or close it.
    pub(crate) fn toggle_stale_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.sheet, Some(Sheet::StaleQuotes(_))) {
            return self.close_sheet(window, cx);
        }
        let note = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(DEFAULT_NOTE)
                .placeholder("version note")
        });
        let sub = cx.subscribe_in(&note, window, |this, _, ev, window, cx| {
            if let InputEvent::PressEnter { .. } = ev {
                this.refresh_selected(window, cx);
            }
        });
        self._subs.push(sub);
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        // What the list already knows, while the blyg is asked again.
        let cached = self.backend.cached_stale_quotes();
        let list = if cached.is_empty() {
            Fetch::Loading
        } else {
            Fetch::Ready(self.stale_rows(cached))
        };
        self.sheet_gen += 1;
        self.sheet = Some(Sheet::StaleQuotes(StaleSheet {
            focus,
            note,
            list,
            sel: 0,
            probed: HashMap::new(),
            done: HashMap::new(),
            busy: false,
        }));
        self.reload_stale_sheet(cx);
        cx.notify();
    }

    fn stale_rows(&self, list: Vec<ThreadFreshness>) -> Vec<Row> {
        rows(list, |sid| self.backend.own_item(sid))
    }

    fn stale_sheet(&mut self) -> Option<&mut StaleSheet> {
        match &mut self.sheet {
            Some(Sheet::StaleQuotes(s)) => Some(s),
            _ => None,
        }
    }

    /// Ask the blyg for the list again (`GET /freshness`).
    fn reload_stale_sheet(&mut self, cx: &mut Context<Self>) {
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.stale_quotes() });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                let rows = r.map(|l| v.stale_rows(l));
                let Some(s) = v.stale_sheet() else { return };
                match rows {
                    Ok(rows) => {
                        s.sel = s.sel.min(rows.len().saturating_sub(1));
                        s.list = Fetch::Ready(rows);
                    }
                    // A failed check keeps what was shown, if anything.
                    Err(e) if s.list.ready().is_none() => s.list = Fetch::Failed(e.to_string()),
                    Err(_) => {}
                }
                v.upkeep_reload_stale();
                v.probe_selected(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Ask the highlighted thread's quoted blygs for their newest versions.
    pub(crate) fn probe_selected(&mut self, cx: &mut Context<Self>) {
        let Some(s) = self.stale_sheet() else { return };
        let Some(row) = s.list.ready().and_then(|l| l.get(s.sel)).cloned() else {
            return;
        };
        let Some(id) = row.id.clone() else { return };
        let sid = row.report.id.clone();
        if s.probed.contains_key(&sid) {
            return;
        }
        s.probed.insert(sid.clone(), Fetch::Loading);
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.quote_freshness(&id) });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                if let Some(s) = v.stale_sheet() {
                    s.probed.insert(sid, Fetch::of(r));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn stale_note(&self, cx: &App) -> Option<String> {
        match &self.sheet {
            Some(Sheet::StaleQuotes(s)) => {
                let n = s.note.read(cx).value().trim().to_string();
                Some(if n.is_empty() { DEFAULT_NOTE.into() } else { n })
            }
            _ => None,
        }
    }

    /// ⏎: refresh the highlighted thread.
    pub(crate) fn refresh_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.stale_sheet() else { return };
        let Some(row) = s.list.ready().and_then(|l| l.get(s.sel)).cloned() else {
            return;
        };
        if let Some(why) = why_not(&row) {
            return self.show_toast("Can't refresh this one", Some(why.into()), cx);
        }
        self.run_refreshes(vec![row], window, cx);
    }

    /// ⇧⏎: refresh every thread that can be, one after another.
    pub(crate) fn refresh_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.stale_sheet() else { return };
        let rows: Vec<Row> = s
            .list
            .ready()
            .map(|l| l.iter().filter(|r| why_not(r).is_none()).cloned().collect())
            .unwrap_or_default();
        if rows.is_empty() {
            return self.show_toast("Nothing here can be refreshed now", None, cx);
        }
        self.run_refreshes(rows, window, cx);
    }

    fn run_refreshes(&mut self, rows: Vec<Row>, window: &mut Window, cx: &mut Context<Self>) {
        let note = self.stale_note(cx);
        let Some(s) = self.stale_sheet() else { return };
        if s.busy {
            return;
        }
        s.busy = true;
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move {
            rows.into_iter()
                .filter_map(|r| {
                    let id = r.id?;
                    let out = backend
                        .refresh_quotes(&id, note.as_deref())
                        .map_err(|e| e.to_string());
                    Some((r.report.id, r.title, out))
                })
                .collect::<Vec<_>>()
        });
        cx.spawn_in(window, async move |this, cx| {
            let results = task.await;
            let _ = this.update_in(cx, |v, window, cx| {
                let ok = results.iter().filter(|r| r.2.is_ok()).count();
                let failed = results.len() - ok;
                let toast = match (results.as_slice(), failed) {
                    ([(_, title, Ok(r))], _) => {
                        (format!("Refreshed “{title}”"), Some(r.words().into()))
                    }
                    ([(_, _, Err(e))], _) => {
                        ("Couldn't refresh".to_string(), Some(e.clone().into()))
                    }
                    (_, 0) => (format!("Refreshed {ok} threads"), None),
                    _ => (
                        format!("Refreshed {ok} of {} threads", results.len()),
                        Some("The sheet says what went wrong".into()),
                    ),
                };
                if let Some(s) = v.stale_sheet() {
                    s.busy = false;
                    for (sid, _, r) in results {
                        s.probed.remove(&sid);
                        s.done.insert(sid, r);
                    }
                }
                v.show_toast(toast.0, toast.1, cx);
                v.requery(window, cx);
                v.reload_stale_sheet(cx);
            });
        })
        .detach();
        cx.notify();
    }

    /// Open the highlighted thread in the editor.
    fn open_stale(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.stale_sheet() else { return };
        let Some(id) = s
            .list
            .ready()
            .and_then(|l| l.get(s.sel))
            .and_then(|r| r.id.clone())
        else {
            return;
        };
        self.sheet = None;
        self.leave_reading();
        if !self.list.results().iter().any(|i| i.local_id == id) {
            self.set_query_text("", window, cx);
        }
        self.open(&id, window, cx);
    }

    fn stale_sheet_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.stale_sheet() else { return };
        let n = s.list.ready().map_or(0, Vec::len);
        match ev.keystroke.key.as_str() {
            "escape" => self.close_sheet(window, cx),
            "up" => {
                s.sel = s.sel.saturating_sub(1);
                self.probe_selected(cx);
                cx.notify();
            }
            "down" => {
                s.sel = (s.sel + 1).min(n.saturating_sub(1));
                self.probe_selected(cx);
                cx.notify();
            }
            "enter" if ev.keystroke.modifiers.shift => self.refresh_all(window, cx),
            "enter" => self.refresh_selected(window, cx),
            "o" => self.open_stale(window, cx),
            "r" => {
                self.reload_stale_sheet(cx);
                self.show_toast("Checking quotes…", None, cx);
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    pub(crate) fn render_stale_sheet(
        &self,
        s: &StaleSheet,
        cx: &mut Context<Self>,
    ) -> (f32, AnyElement) {
        let p = self.palette;
        let mut body = div()
            .track_focus(&s.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                this.stale_sheet_key(ev, window, cx)
            }))
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .mb(px(4.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(13.))
                            .child(format!("{} Stale quotes", super::STALE_GLYPH)),
                    )
                    .child(
                        self.upkeep_chip("stale-check", "Check again", true)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reload_stale_sheet(cx);
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .mb(px(8.))
                    .text_size(px(12.))
                    .line_height(relative(1.45))
                    .text_color(p.muted)
                    .child(
                        "Your threads whose quoted posts have newer versions, stalest first. \
                         A refresh republishes the same words with the quotes brought up to \
                         date: a new version. Let small drift wait and refresh in batches.",
                    ),
            );
        let list = match &s.list {
            Fetch::Loading => self.upkeep_note("Checking quotes…"),
            Fetch::Failed(e) => self.upkeep_note(format!("Couldn't check quotes: {e}")),
            Fetch::Ready(rows) if rows.is_empty() => {
                self.upkeep_note("Nothing is stale right now.")
            }
            Fetch::Ready(rows) => {
                let mut list = div()
                    .id("stale-list")
                    .max_h(px(380.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(2.));
                for (i, row) in rows.iter().enumerate() {
                    list = list.child(self.render_stale_row(s, i, row, cx));
                }
                list.into_any_element()
            }
        };
        body = body.child(list);
        let any = s
            .list
            .ready()
            .is_some_and(|l| l.iter().any(|r| why_not(r).is_none()));
        body = body
            .child(div().my(px(8.)).h(px(1.)).bg(p.line))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(12.))
                            .text_color(p.muted)
                            .child("Version note"),
                    )
                    .child(
                        crate::theme_ext::field_box(div(), &self.theme)
                            .flex_1()
                            .px(px(8.))
                            .py(px(4.))
                            .text_size(px(12.5))
                            .child(gpui_kit::base::input::Input::new(&s.note)),
                    )
                    .when(any, |d| {
                        d.child(
                            self.upkeep_chip("stale-refresh-all", "Refresh all", !s.busy)
                                .debug_selector(|| "stale-refresh-all".into())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.refresh_all(window, cx)
                                })),
                        )
                    }),
            )
            .child(
                div()
                    .mt(px(8.))
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .child(if s.busy {
                        "Refreshing…"
                    } else {
                        crate::keymap::hint(
                            "↑/↓ choose · ⏎ refresh · ⇧⏎ refresh all · o open · r check again · esc",
                        )
                    }),
            );
        (520., body.into_any_element())
    }

    fn render_stale_row(
        &self,
        s: &StaleSheet,
        i: usize,
        row: &Row,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let selected = i == s.sel;
        let (ink, muted) = if selected {
            (p.sel_ink, p.sel_muted)
        } else {
            (p.ink, p.muted)
        };
        let sid = row.report.id.clone();
        let done = s.done.get(&sid);
        let mut el = div()
            .id(SharedString::from(format!("stale-row-{i}")))
            .debug_selector(move || format!("stale-row-{i}"))
            .flex()
            .flex_col()
            .gap(px(2.))
            .px(px(8.))
            .py(px(6.))
            .rounded(px(6.))
            .cursor_pointer()
            .when(selected, |d| d.bg(p.sel))
            .hover(|d| d.bg(p.sel))
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(s) = this.stale_sheet() {
                    s.sel = i;
                }
                this.probe_selected(cx);
                cx.notify();
            }))
            .child(
                div()
                    .truncate()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(ink)
                    .child(row.title.clone()),
            )
            .child(div().text_size(px(11.5)).text_color(muted).child(format!(
                "v{} · {}",
                row.report.version,
                row.report.line()
            )));
        if let Some(r) = done {
            el = el.child(div().text_size(px(11.5)).text_color(muted).child(match r {
                Ok(r) => format!("Refreshed: {}", r.words()),
                Err(e) => format!("Couldn't refresh: {e}"),
            }));
        }
        if !selected {
            return el.into_any_element();
        }
        // The highlighted thread: each quote and what changed.
        let quotes = match s.probed.get(&sid) {
            Some(Fetch::Ready(t)) => t.quotes.clone(),
            _ => row.report.quotes.clone(),
        };
        let checking = matches!(s.probed.get(&sid), Some(Fetch::Loading));
        let mut detail = div().mt(px(4.)).flex().flex_col().gap(px(2.));
        for q in quotes {
            let source = q
                .origin
                .as_deref()
                .map(blyg_core::upkeep::host)
                .unwrap_or_else(|| "your blyg".into());
            let id8: String = q.id.chars().take(8).collect();
            detail = detail.child(
                div()
                    .flex()
                    .gap(px(6.))
                    .text_size(px(11.5))
                    .text_color(if q.status.is_stale() || q.status.is_blocking() {
                        ink
                    } else {
                        muted
                    })
                    .child(div().flex_none().child(if q.status.is_stale() {
                        super::STALE_GLYPH
                    } else if q.status.is_blocking() {
                        "!"
                    } else {
                        "·"
                    }))
                    .child(div().flex_1().min_w_0().truncate().child(format!(
                        "{source} · {id8}{} · {}",
                        if q.partial { " · excerpt" } else { "" },
                        q.words()
                    ))),
            );
        }
        if checking {
            detail = detail.child(
                div()
                    .text_size(px(11.))
                    .text_color(muted)
                    .child("Asking the quoted blygs…"),
            );
        }
        let actions = div()
            .mt(px(6.))
            .flex()
            .flex_wrap()
            .gap(px(6.))
            .child(match why_not(row) {
                None => self
                    .upkeep_chip(
                        "stale-refresh",
                        format!("Refresh → v{}", row.report.version + 1),
                        !s.busy,
                    )
                    .debug_selector(|| "stale-refresh".into())
                    .on_click(cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.refresh_selected(window, cx)
                    }))
                    .into_any_element(),
                Some(why) => div()
                    .text_size(px(11.5))
                    .text_color(muted)
                    .child(why)
                    .into_any_element(),
            })
            .when(row.id.is_some(), |d| {
                d.child(
                    self.upkeep_chip("stale-open", "Open", true)
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.open_stale(window, cx)
                        })),
                )
            });
        el.child(detail).child(actions).into_any_element()
    }
}
