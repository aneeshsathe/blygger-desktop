//! --- reading slots --- What extensions add to each reading entry
//! (docs/EXTENSIONS.md § Reading slots; `blyg_ext::reading`), mirroring the
//! web Studio's `entryByline` and `entryActions`:
//!
//! - a **byline marker** at the end of each entry's byline, in the stream
//!   and in the reader's header. Asked of the extension off the main thread
//!   the first time an entry is drawn, and cached by (extension, entry,
//!   version): drawing never waits, and an entry with no answer yet (or
//!   none at all) simply shows nothing.
//! - a **⋯ chip** in an entry's actions when an extension adds rows: it
//!   opens a sheet listing them; choosing one asks the extension and shows
//!   its answer natively (text, label/value fields, a monospaced code block
//!   with a Copy button).
//!
//! Only running extensions granted `reading.read` take part
//! (`Host::entry_slots`). The bundled `reading-time` and `inspect` are the
//! two that ship, both off until the config names them.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use blyg_core::ReadingItem;
use blyg_ext::reading::{EntryActionSpec, EntryByline, EntrySheet, ReadingEntry};
use blyg_ext::{ExtError, Installed};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::Launch;
use crate::app::reading::Tip;
use crate::app::{MainView, TITLEBAR_H};

#[cfg(test)]
#[path = "reading_slots_tests.rs"]
mod tests;

/// The bundled reading-slot extensions, run from `launch` (each off until
/// the config names it).
pub(crate) fn bundled(launch: &Launch) -> Vec<Installed> {
    vec![
        blyg_ext_reading_time::bundled(launch.program.clone(), launch.reading_time_args.clone()),
        blyg_ext_inspect::bundled(launch.program.clone(), launch.inspect_args.clone()),
    ]
}

/// A marker's cache key: (extension, subscription id, remote id, version).
pub(crate) type MarkerKey = (String, String, String, u32);

fn marker_key(ext: &str, r: &ReadingItem) -> MarkerKey {
    (
        ext.to_string(),
        r.subscription_id.clone(),
        r.remote_id.clone(),
        r.version,
    )
}

/// The window's reading-slot state (a field of `Extensions`). The marker
/// cache is filled from drawing code, which only has `&self`, hence the
/// cells.
#[derive(Default)]
pub(crate) struct Slots {
    /// Answers: `None` = nothing to show (or no answer, an error).
    markers: RefCell<HashMap<MarkerKey, Option<EntryByline>>>,
    /// Asked and not answered yet.
    pending: RefCell<HashSet<MarkerKey>>,
    /// To ask at the next flush.
    queue: RefCell<Vec<(MarkerKey, ReadingEntry)>>,
    flush_scheduled: Cell<bool>,
    /// The ⋯ sheet, when it's up.
    pub sheet: Option<SlotSheet>,
    pub sheet_gen: usize,
}

impl Slots {
    /// The cached marker, if answered.
    pub fn marker(&self, key: &MarkerKey) -> Option<Option<EntryByline>> {
        self.markers.borrow().get(key).cloned()
    }

    /// Drop what `ext` said (it restarted: its settings may have changed).
    pub fn forget(&self, ext: &str) {
        self.markers.borrow_mut().retain(|k, _| k.0 != ext);
    }

    /// How many markers have been asked for, answered or not (tests).
    #[cfg(test)]
    pub fn asked(&self) -> usize {
        self.markers.borrow().len() + self.pending.borrow().len()
    }
}

/// The ⋯ sheet of one entry.
pub(crate) struct SlotSheet {
    pub item: ReadingItem,
    /// Each row with the extension it's from.
    pub rows: Vec<(String, EntryActionSpec)>,
    pub selected: usize,
    pub phase: Phase,
    pub focus: FocusHandle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Phase {
    /// The rows, to pick one.
    Choose,
    /// Asked the extension for row `n`'s sheet.
    Loading(usize),
    /// Its answer.
    Shown(usize, EntrySheet),
    /// It failed (the message names the extension).
    Failed(usize, String),
}

/// The monospaced font for code (Menlo has a Windows substitute).
fn mono() -> &'static str {
    if cfg!(target_os = "windows") {
        "Consolas"
    } else {
        "Menlo"
    }
}

impl MainView {
    // ------------------------------------------------------------ byline

    /// Hook (the stream's byline and the reader's header): each byline
    /// extension's marker for `r`, as "· text" with its tip on hover.
    /// Draws what's cached and asks for what isn't, off the main thread.
    pub(crate) fn slots_byline(&self, r: &ReadingItem, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(host) = &self.ext.host else {
            return vec![];
        };
        let p = self.palette.on_page();
        let mut out = vec![];
        for s in host.entry_slots().into_iter().filter(|s| s.byline) {
            let key = marker_key(&s.ext, r);
            match self.ext.slots.marker(&key) {
                Some(Some(m)) => {
                    let tip = m.tip.clone();
                    let ext = s.ext.clone();
                    out.push(div().child("·").into_any_element());
                    out.push(
                        div()
                            .id(SharedString::from(format!("slot-byline-{ext}")))
                            .debug_selector(move || format!("slot-byline-{ext}"))
                            .text_color(p.muted)
                            .child(m.text)
                            .when_some(tip, |d, tip| {
                                d.tooltip(move |_, cx| cx.new(|_| Tip(tip.clone())).into())
                            })
                            .into_any_element(),
                    );
                }
                Some(None) => {}
                None => self.slots_ask(key, r, cx),
            }
        }
        out
    }

    /// Queue `key` (once) and schedule a flush after this frame.
    fn slots_ask(&self, key: MarkerKey, r: &ReadingItem, cx: &mut Context<Self>) {
        let slots = &self.ext.slots;
        if !slots.pending.borrow_mut().insert(key.clone()) {
            return;
        }
        slots.queue.borrow_mut().push((key, ReadingEntry::of(r)));
        if slots.flush_scheduled.replace(true) {
            return;
        }
        let this = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = this.update(cx, |v, cx| v.slots_flush(cx));
        });
    }

    /// Ask the extensions for the queued markers, one after another on a
    /// background thread; cache the answers and redraw.
    pub(crate) fn slots_flush(&mut self, cx: &mut Context<Self>) {
        self.ext.slots.flush_scheduled.set(false);
        let queue = std::mem::take(&mut *self.ext.slots.queue.borrow_mut());
        let Some(host) = self.ext.host.clone() else {
            self.ext.slots.pending.borrow_mut().clear();
            return;
        };
        if queue.is_empty() {
            return;
        }
        let task = cx.background_spawn(async move {
            queue
                .into_iter()
                .map(|(key, entry)| {
                    let r = host.entry_byline(&key.0, &entry);
                    (key, r)
                })
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let answers = task.await;
            let _ = this.update(cx, |v, cx| {
                let slots = &v.ext.slots;
                for (key, r) in answers {
                    slots.pending.borrow_mut().remove(&key);
                    // An error shows nothing, and isn't asked again until
                    // the extension restarts (`Slots::forget`).
                    let m = r.unwrap_or_else(|e: ExtError| {
                        eprintln!("{}: byline: {}", key.0, e.message(&key.0));
                        None
                    });
                    // A marker can wrap the byline: the stream measures
                    // that post again.
                    if m.is_some()
                        && let Some(ix) = v.reading.shown_pos(&(key.1.clone(), key.2.clone()))
                    {
                        v.reading.stream.list.remeasure_items(ix..ix + 1);
                    }
                    slots.markers.borrow_mut().insert(key, m);
                }
                cx.notify();
            });
        })
        .detach();
    }

    // ------------------------------------------------------------ ⋯

    /// The rows extensions add to every entry's ⋯ sheet.
    fn slots_rows(&self) -> Vec<(String, EntryActionSpec)> {
        let Some(host) = &self.ext.host else {
            return vec![];
        };
        host.entry_slots()
            .into_iter()
            .flat_map(|s| {
                let ext = s.ext;
                s.actions.into_iter().map(move |a| (ext.clone(), a))
            })
            .collect()
    }

    /// Hook (the stream's and the reader's action rows): "⋯" when an
    /// extension adds rows to `r`'s ⋯ sheet.
    pub(crate) fn slots_more_chip(
        &self,
        r: &ReadingItem,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.slots_rows().is_empty() {
            return None;
        }
        let p = self.palette.on_page();
        let item = r.clone();
        Some(
            div()
                .id("slot-more")
                .debug_selector(|| "slot-more".into())
                .px(px(8.))
                .py(px(3.))
                .rounded(px(self.theme.chip_radius))
                .map(|d| crate::ornament::border(d, self.theme.chip_border, p.line))
                .cursor_pointer()
                .font_family(self.chrome())
                .text_size(px(11.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(p.ink)
                .hover(|s| s.border_color(p.accent))
                .child("⋯")
                .tooltip(|_, cx| cx.new(|_| Tip("More, from extensions".into())).into())
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.slots_open(item.clone(), window, cx)
                }))
                .into_any_element(),
        )
    }

    /// Open `item`'s ⋯ sheet (nothing when no extension adds a row, or
    /// another sheet is up).
    pub(crate) fn slots_open(
        &mut self,
        item: ReadingItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rows = self.slots_rows();
        // (The tour's slots step opens it, so the tutorial doesn't block it.)
        let blocked = self.ext_blocked() && !self.ext_tour_on();
        if rows.is_empty() || blocked {
            return;
        }
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.ext.slots.sheet_gen += 1;
        self.ext.slots.sheet = Some(SlotSheet {
            item,
            rows,
            selected: 0,
            phase: Phase::Choose,
            focus,
        });
        cx.notify();
    }

    pub(crate) fn slots_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ext.slots.sheet.take().is_none() {
            return;
        }
        if self.reading.view == crate::app::reading::View::Posts {
            self.focus_after_sheet(window, cx);
        } else {
            window.focus(&self.reading.focus, cx);
        }
        self.ext_next_ask(window, cx);
        cx.notify();
    }

    /// Run row `n`: ask its extension for the sheet, off the main thread.
    pub(crate) fn slots_choose(&mut self, n: usize, cx: &mut Context<Self>) {
        let Some(host) = self.ext.host.clone() else {
            return;
        };
        let Some(s) = self.ext.slots.sheet.as_mut() else {
            return;
        };
        let Some((ext, action)) = s.rows.get(n).cloned() else {
            return;
        };
        s.selected = n;
        s.phase = Phase::Loading(n);
        let item = s.item.clone();
        let gen_ = self.ext.slots.sheet_gen;
        let task = cx.background_spawn(async move { host.entry_action(&ext, &action.id, &item) });
        let ext = s.rows[n].0.clone();
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |v, cx| {
                if v.ext.slots.sheet_gen != gen_ {
                    return; // closed, or another entry's
                }
                if let Some(s) = v.ext.slots.sheet.as_mut() {
                    s.phase = match r {
                        Ok(sheet) => Phase::Shown(n, sheet),
                        Err(e) => Phase::Failed(n, e.message(&ext)),
                    };
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Copy the shown sheet's code (else its text) to the clipboard.
    pub(crate) fn slots_copy(&mut self, cx: &mut Context<Self>) {
        let Some(SlotSheet {
            phase: Phase::Shown(_, sheet),
            ..
        }) = &self.ext.slots.sheet
        else {
            return;
        };
        let text = sheet.code.clone().unwrap_or_else(|| sheet.text.clone());
        let json = sheet.copy_label() == "Copy JSON";
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.show_toast(if json { "JSON copied" } else { "Copied" }, None, cx);
    }

    fn slots_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.ext.slots.sheet.as_mut() else {
            return;
        };
        let k = &ev.keystroke;
        let n = s.rows.len();
        let choosing = s.phase == Phase::Choose;
        let cmd = k.modifiers.platform || k.modifiers.control;
        match k.key.as_str() {
            "escape" => self.slots_close(window, cx),
            "up" | "k" if choosing && !cmd => {
                s.selected = (s.selected + n - 1) % n;
                cx.notify();
            }
            "down" | "j" if choosing && !cmd => {
                s.selected = (s.selected + 1) % n;
                cx.notify();
            }
            "enter" if choosing => {
                let i = s.selected;
                self.slots_choose(i, cx);
            }
            "c" if cmd && !choosing => self.slots_copy(cx),
            d if choosing && !cmd && d.len() == 1 => {
                if let Some(i) = d
                    .parse::<usize>()
                    .ok()
                    .filter(|i| (1..=n.min(9)).contains(i))
                {
                    self.slots_choose(i - 1, cx);
                }
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    /// `BLYGGER_DEMO=ext-slots` (the reading stream with the first post
    /// selected: its byline markers and ⋯ chip) and `ext-slots-inspect`
    /// (then its ⋯ sheet, running the first row). The config decides what
    /// runs.
    pub(crate) fn slots_demo(
        &mut self,
        scenario: &str,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match n {
            1 => {
                self.show_view(crate::app::reading::View::Reading, window, cx);
                self.stream_move(1, window, cx);
            }
            2 if scenario == "ext-slots-inspect" => {
                let item = self.reading.sel.as_ref().and_then(|k| {
                    self.reading
                        .rows
                        .iter()
                        .find(|r| r.subscription_id == k.0 && r.remote_id == k.1)
                        .cloned()
                });
                if let Some(item) = item {
                    self.slots_open(item, window, cx);
                    self.slots_choose(0, cx);
                }
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------ render

    /// Hook: the ⋯ sheet, over everything.
    pub(crate) fn render_slots_sheet(
        &self,
        ui_font: &SharedString,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let s = self.ext.slots.sheet.as_ref()?;
        let p = self.palette;
        let content = match &s.phase {
            Phase::Choose => self.render_slots_rows(s, cx),
            Phase::Loading(n) => self.render_slots_status(s, *n, "…", p.muted),
            Phase::Failed(n, m) => self.render_slots_status(s, *n, m, p.warn),
            Phase::Shown(n, sheet) => self.render_slots_shown(s, *n, sheet, cx),
        };
        let gen_ = self.ext.slots.sheet_gen;
        Some(
            div()
                .absolute()
                .top(px(TITLEBAR_H))
                .bottom_0()
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .items_start()
                .child(
                    div()
                        .id("slot-sheet")
                        .debug_selector(|| "slot-sheet".into())
                        .track_focus(&s.focus)
                        .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                            this.slots_key(ev, window, cx)
                        }))
                        .occlude()
                        .w(px(560.))
                        .max_w(relative(0.92))
                        .max_h(relative(0.86))
                        .overflow_y_scroll()
                        .map(|d| crate::theme_ext::sheet(d, &self.theme))
                        .px(px(18.))
                        .py(px(16.))
                        .font_family(ui_font.clone())
                        .text_size(px(13.))
                        .text_color(p.ink)
                        .child(content)
                        .with_animation(
                            ("slot-sheet-in", gen_),
                            Animation::new(std::time::Duration::from_millis(220))
                                .with_easing(ease_out_quint()),
                            |d, t| d.mt(px(-240.0 * (1.0 - t))).opacity(t.min(1.0) * 0.4 + 0.6),
                        ),
                )
                .into_any_element(),
        )
    }

    /// The post the sheet is about, as its first line.
    fn slots_subject(&self, s: &SlotSheet) -> Div {
        let r = &s.item;
        let title = crate::app::reading::vm::post_title(r);
        div()
            .mb(px(10.))
            .text_size(px(12.))
            .text_color(self.palette.muted)
            .truncate()
            .child(format!(
                "{} · {}",
                crate::app::reading::vm::host(&r.origin),
                title
            ))
    }

    fn render_slots_rows(&self, s: &SlotSheet, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        div()
            .flex()
            .flex_col()
            .child(Self::slot_heading("More"))
            .child(self.slots_subject(s))
            .children(s.rows.iter().enumerate().map(|(i, (ext, a))| {
                let on = i == s.selected;
                div()
                    .id(("slot-row", i))
                    .debug_selector(move || format!("slot-row-{i}"))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(8.))
                    .py(px(6.))
                    .rounded(px(self.theme.corner(6.)))
                    .cursor_pointer()
                    .when(on, |d| d.bg(p.sel))
                    .hover(|d| d.bg(p.sel))
                    .on_click(cx.listener(move |this, _, _, cx| this.slots_choose(i, cx)))
                    .child(
                        div()
                            .w(px(28.))
                            .flex_none()
                            .font_family(mono())
                            .text_size(px(12.))
                            .text_color(p.muted)
                            .child(a.icon.clone().unwrap_or_default()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(div().font_weight(FontWeight::MEDIUM).child(a.title.clone()))
                            .when(!a.detail.is_empty(), |d| {
                                d.child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(p.muted)
                                        .child(a.detail.clone()),
                                )
                            }),
                    )
                    .when(i < 9, |d| d.child(self.slot_kbd(DIGITS[i])))
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(11.5))
                            .text_color(p.muted)
                            .child(ext.clone()),
                    )
            }))
            .child(
                self.slot_keys_row()
                    .child(self.slot_key_hint("↑↓", "choose"))
                    .child(self.slot_key_hint("⏎", "open"))
                    .child(self.slot_key_hint("esc", "close")),
            )
            .into_any_element()
    }

    fn render_slots_status(&self, s: &SlotSheet, n: usize, text: &str, color: Hsla) -> AnyElement {
        let title = s.rows.get(n).map(|r| r.1.title.clone()).unwrap_or_default();
        div()
            .flex()
            .flex_col()
            .child(Self::slot_heading(title))
            .child(self.slots_subject(s))
            .child(
                div()
                    .id("slot-status")
                    .debug_selector(|| "slot-status".into())
                    .text_color(color)
                    .child(text.to_string()),
            )
            .child(
                self.slot_keys_row()
                    .child(self.slot_key_hint("esc", "close")),
            )
            .into_any_element()
    }

    fn render_slots_shown(
        &self,
        s: &SlotSheet,
        n: usize,
        sheet: &EntrySheet,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let ext = s.rows.get(n).map(|r| r.0.clone()).unwrap_or_default();
        let title = if sheet.title.trim().is_empty() {
            s.rows.get(n).map(|r| r.1.title.clone()).unwrap_or_default()
        } else {
            sheet.title.clone()
        };
        let copyable = sheet.code.is_some() || !sheet.text.is_empty();
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(8.))
                    .child(Self::slot_heading(title).mb_0())
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(p.muted)
                            .child(format!("from {ext}")),
                    ),
            )
            .child(self.slots_subject(s).mb_0())
            .when_some(sheet.description.clone(), |d, t| {
                d.child(div().text_color(p.muted).child(t))
            })
            .children(
                sheet
                    .text
                    .split("\n\n")
                    .filter(|t| !t.trim().is_empty())
                    .map(|t| div().child(t.to_string())),
            )
            .when(!sheet.fields.is_empty(), |d| {
                d.child(
                    div()
                        .id("slot-fields")
                        .debug_selector(|| "slot-fields".into())
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .children(sheet.fields.iter().map(|f| {
                            div()
                                .flex()
                                .gap(px(8.))
                                .child(
                                    div()
                                        .w(px(110.))
                                        .flex_none()
                                        .text_color(p.muted)
                                        .child(f.label.clone()),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .font_family(mono())
                                        .text_size(px(11.5))
                                        .child(f.value.clone()),
                                )
                        })),
                )
            })
            .when_some(sheet.code.clone(), |d, code| {
                d.child(
                    div()
                        .id("slot-code")
                        .debug_selector(|| "slot-code".into())
                        .max_h(px(340.))
                        .overflow_y_scroll()
                        .p(px(8.))
                        .rounded(px(self.theme.corner(8.)))
                        .border_1()
                        .border_color(p.line)
                        .font_family(mono())
                        .text_size(px(11.))
                        .line_height(relative(1.35))
                        .child(code),
                )
            })
            .child(
                self.slot_keys_row()
                    .items_center()
                    .when(copyable, |d| {
                        d.child(
                            div()
                                .id("slot-copy")
                                .debug_selector(|| "slot-copy".into())
                                .px(px(8.))
                                .py(px(3.))
                                .rounded(px(self.theme.chip_radius))
                                .map(|d| crate::ornament::border(d, self.theme.chip_border, p.line))
                                .cursor_pointer()
                                .text_size(px(11.5))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(p.ink)
                                .hover(|s| s.border_color(p.accent))
                                .child(sheet.copy_label())
                                .on_click(cx.listener(|this, _, _, cx| this.slots_copy(cx))),
                        )
                    })
                    .when(copyable, |d| d.child(self.slot_key_hint(COPY_KEY, "copy")))
                    .child(self.slot_key_hint("esc", "close")),
            )
            .into_any_element()
    }
}

const DIGITS: [&str; 9] = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];

const COPY_KEY: &str = if cfg!(target_os = "windows") {
    "Ctrl+C"
} else {
    "⌘C"
};

// The extension sheets' small parts, as `sheets.rs` draws them.
impl MainView {
    fn slot_kbd(&self, k: &'static str) -> Div {
        crate::theme_ext::kbd(div(), &self.theme)
            .px(px(6.))
            .py(px(1.))
            .min_w(px(20.))
            .flex()
            .justify_center()
            .font_weight(FontWeight::MEDIUM)
            .text_size(px(11.5))
            .child(k)
    }

    fn slot_key_hint(&self, k: &'static str, label: &'static str) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(5.))
            .child(self.slot_kbd(k))
            .child(label)
    }

    fn slot_heading(s: impl Into<SharedString>) -> Div {
        div()
            .mb(px(8.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(px(13.))
            .child(s.into())
    }

    fn slot_keys_row(&self) -> Div {
        div()
            .mt(px(12.))
            .flex()
            .flex_wrap()
            .gap(px(14.))
            .text_color(self.palette.muted)
    }
}
