//! Import subscriptions from an OPML file (another feed reader's export),
//! and export every subscription as OPML (Blyg › Import Subscriptions from
//! OPML… / Export Subscriptions as OPML…, and the Subscriptions screen).
//!
//! Import: a file picker, then a preview sheet (the Publish/Delete sheets'
//! chrome) listing the file's feeds with ticks, grouped under the folders
//! they had in the other reader (display only). Feeds already followed say
//! so and start unticked. ⏎ imports the ticked ones through
//! `blyg_core::opml::run_import`, off the main thread, slowly (see
//! `opml::Pace`), into the local Reader folder "Imported feeds"; esc
//! cancels (after the one in flight). A summary follows, with each
//! failure's reason and Retry failed.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use blyg_core::opml::{self, EXPORT_FILE_NAME, OpmlFeed, Outcome, Pace, Progress, Summary};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::RSheet;
use crate::app::MainView;

gpui_kit::actions!(blygger, [ImportOpml, ExportOpml]);

/// Where imported feeds go, in the user's words.
pub const WHERE_THEY_GO: &str = "Added to the Imported feeds folder in the Reader";

/// --- onboarding --- The tour's sample export from "another reader":
/// one feed the sample data already follows, two it doesn't.
const TOUR_OPML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<opml version="2.0">
  <head><title>Another reader's subscriptions</title></head>
  <body>
    <outline text="Rue" type="rss" xmlUrl="https://rue.blyg.example.com/feed.json"/>
    <outline text="Tide Watch" type="rss" xmlUrl="https://tidewatch.example.com/feed.xml"/>
    <outline text="Harbour Log" type="rss" xmlUrl="https://harbourlog.example.org/feed.xml"/>
  </body>
</opml>
"#;

/// The import pace; tests set a quicker one.
pub struct ImportPace(pub Pace);

impl Global for ImportPace {}

/// One feed of the file.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub feed: OpmlFeed,
    /// The subscription that already follows it.
    pub already: Option<String>,
    pub ticked: bool,
    pub outcome: Option<Outcome>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    Preview,
    Running {
        done: usize,
        total: usize,
        /// Seconds the blyg asked Burrow to wait (a 429).
        waiting: Option<u64>,
        cancelling: bool,
    },
    Done(Summary),
}

/// The import sheet.
pub struct Sheet {
    /// The file's name, for the heading.
    pub file: String,
    /// Grouped by folder (first appearance), file order within each.
    pub rows: Vec<Row>,
    pub sel: usize,
    pub duplicates: usize,
    pub without_feed: usize,
    pub phase: Phase,
    pub cancel: Arc<AtomicBool>,
    pub focus: FocusHandle,
    pub scroll: ScrollHandle,
}

impl Sheet {
    pub fn new(
        file: String,
        parsed: opml::Parsed,
        subs: &[blyg_core::Subscription],
        focus: FocusHandle,
    ) -> Self {
        let mut order: Vec<Option<String>> = vec![];
        for f in &parsed.feeds {
            if !order.contains(&f.folder) {
                order.push(f.folder.clone());
            }
        }
        // Feeds in no folder first, then each folder.
        order.sort_by_key(|f| f.is_some());
        let mut rows = vec![];
        for group in &order {
            for f in parsed.feeds.iter().filter(|f| &f.folder == group) {
                let already = opml::followed(f, subs).map(|s| s.title.clone());
                rows.push(Row {
                    ticked: already.is_none(),
                    already,
                    feed: f.clone(),
                    outcome: None,
                });
            }
        }
        Sheet {
            file,
            rows,
            sel: 0,
            duplicates: parsed.duplicates,
            without_feed: parsed.without_feed,
            phase: Phase::Preview,
            cancel: Arc::new(AtomicBool::new(false)),
            focus,
            scroll: ScrollHandle::new(),
        }
    }

    pub fn ticked(&self) -> usize {
        self.rows.iter().filter(|r| r.ticked).count()
    }

    pub fn toggle(&mut self, i: usize) {
        if let Some(r) = self.rows.get_mut(i) {
            r.ticked = !r.ticked;
        }
    }

    /// ⌘A: tick every feed not already followed, or untick all when they
    /// already are.
    pub fn toggle_all(&mut self) {
        let all = self
            .rows
            .iter()
            .filter(|r| r.already.is_none())
            .all(|r| r.ticked);
        for r in &mut self.rows {
            r.ticked = !all && r.already.is_none();
        }
    }

    /// The list's child index of row `i` (folder headings are children too).
    fn child_index(&self, i: usize) -> usize {
        let mut heads = 0;
        let mut prev: Option<&Option<String>> = None;
        for (j, r) in self.rows.iter().enumerate().take(i + 1) {
            if prev != Some(&r.feed.folder) && (r.feed.folder.is_some() || j > 0) {
                heads += 1;
            }
            prev = Some(&r.feed.folder);
        }
        i + heads
    }

    fn failed(&self) -> Vec<usize> {
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| matches!(r.outcome, Some(Outcome::Failed { .. })))
            .map(|(i, _)| i)
            .collect()
    }
}

impl MainView {
    /// Hook: the OPML actions on the root element.
    pub(super) fn opml_actions(&self, d: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        d.on_action(cx.listener(|this, _: &ImportOpml, window, cx| this.opml_pick(window, cx)))
            .on_action(cx.listener(|this, _: &ExportOpml, window, cx| this.opml_export(window, cx)))
    }

    /// Subscriptions live on the blyg: without one, say so.
    fn opml_needs_blyg(&mut self, cx: &mut Context<Self>) -> bool {
        if crate::connection::mode(cx) == Some(crate::connection::Mode::Disconnected) {
            self.show_toast(
                "Connect a blyg first",
                Some("Subscriptions live on your blyg".into()),
                cx,
            );
            return true;
        }
        false
    }

    /// Import Subscriptions from OPML…: the file picker.
    pub(crate) fn opml_pick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.opml_needs_blyg(cx) || self.reading.sheet.is_some() {
            return;
        }
        // --- onboarding --- the tour's opml step: a sample file, no picker
        // (importing it subscribes in the tour's sample data).
        if self.onboarding.tutorial.is_some() {
            self.tutorial_key(crate::app::onboarding::Key::OpmlImport, window, cx);
            if let Ok(p) = opml::parse(TOUR_OPML) {
                self.opml_open_preview("another-reader.opml".into(), p, window, cx);
            }
            return;
        }
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let p2 = path.clone();
            let parsed = cx
                .background_spawn(async move { opml::read_file(&p2) })
                .await;
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let _ = this.update_in(cx, |v, window, cx| match parsed {
                Ok(p) => v.opml_open_preview(name, p, window, cx),
                Err(e) => v.show_toast(e.to_string(), None, cx),
            });
        })
        .detach();
    }

    /// The preview sheet for a parsed file.
    pub(crate) fn opml_open_preview(
        &mut self,
        file: String,
        parsed: opml::Parsed,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let subs = self.backend.subscriptions();
        let sheet = Sheet::new(file, parsed, &subs, focus);
        self.open_reading_sheet(RSheet::Opml(Box::new(sheet)), cx);
    }

    fn opml_sheet(&mut self) -> Option<&mut Sheet> {
        match self.reading.sheet.as_mut() {
            Some(RSheet::Opml(s)) => Some(s),
            _ => None,
        }
    }

    /// The preview's keys: 1–9 and space tick, ↑/↓ move, ⌘A all/none, ⏎
    /// import, esc cancel. While importing, esc stops it; at the end ⏎ or
    /// esc closes and r retries the failed ones.
    fn opml_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let key = ev.keystroke.key.as_str();
        let cmd = ev.keystroke.modifiers.secondary();
        let Some(s) = self.opml_sheet() else {
            return false;
        };
        match (&s.phase, key) {
            (Phase::Preview, "enter") => self.opml_start(None, cx),
            (Phase::Preview, "escape") => self.close_reading_sheet(window, cx),
            (Phase::Preview, "a") if cmd => s.toggle_all(),
            (Phase::Preview, "space") => {
                let i = s.sel;
                s.toggle(i);
            }
            (Phase::Preview, "down") => {
                s.sel = (s.sel + 1).min(s.rows.len().saturating_sub(1));
                s.scroll.scroll_to_item(s.child_index(s.sel));
            }
            (Phase::Preview, "up") => {
                s.sel = s.sel.saturating_sub(1);
                s.scroll.scroll_to_item(s.child_index(s.sel));
            }
            (Phase::Preview, k) if !cmd => match k.parse::<usize>() {
                Ok(n @ 1..=9) => {
                    s.toggle(n - 1);
                    s.sel = (n - 1).min(s.rows.len().saturating_sub(1));
                }
                _ => return false,
            },
            (Phase::Running { .. }, "escape") => self.opml_cancel(cx),
            (Phase::Done(_), "enter" | "escape") => self.close_reading_sheet(window, cx),
            (Phase::Done(sum), "r") if !sum.failed.is_empty() => self.opml_retry(cx),
            _ => return false,
        }
        cx.notify();
        true
    }

    /// Esc while importing: stop after the ones in flight.
    pub(crate) fn opml_cancel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.opml_sheet() {
            s.cancel.store(true, Ordering::Relaxed);
            if let Phase::Running { cancelling, .. } = &mut s.phase {
                *cancelling = true;
            }
            cx.notify();
        }
    }

    /// Retry failed: tick only the failed ones and run them again.
    pub(crate) fn opml_retry(&mut self, cx: &mut Context<Self>) {
        let Some(s) = self.opml_sheet() else {
            return;
        };
        let failed = s.failed();
        self.opml_start(Some(failed), cx);
    }

    /// ⏎ Import N: run the ticked feeds (or `only` these rows).
    pub(crate) fn opml_start(&mut self, only: Option<Vec<usize>>, cx: &mut Context<Self>) {
        let pace = cx
            .try_global::<ImportPace>()
            .map(|p| p.0)
            .unwrap_or_default();
        let Some(s) = self.opml_sheet() else {
            return;
        };
        let picked: Vec<usize> = match only {
            Some(v) => v,
            None => (0..s.rows.len()).filter(|&i| s.rows[i].ticked).collect(),
        };
        if picked.is_empty() {
            return;
        }
        for &i in &picked {
            s.rows[i].outcome = None;
        }
        let feeds: Vec<OpmlFeed> = picked.iter().map(|&i| s.rows[i].feed.clone()).collect();
        s.cancel = Arc::new(AtomicBool::new(false));
        let cancel = s.cancel.clone();
        s.phase = Phase::Running {
            done: 0,
            total: feeds.len(),
            waiting: None,
            cancelling: false,
        };
        let backend = self.backend.clone();
        let (tx, rx) = async_channel::unbounded::<Progress>();
        let task = cx.background_spawn(async move {
            opml::run_import(backend.as_ref(), &feeds, &pace, &cancel, &|p| {
                let _ = tx.send_blocking(p);
            })
        });
        cx.spawn(async move |this, cx| {
            while let Ok(p) = rx.recv().await {
                let picked = picked.clone();
                let _ = this.update(cx, |v, cx| {
                    if let Some(s) = v.opml_sheet()
                        && let Phase::Running { done, waiting, .. } = &mut s.phase
                    {
                        match p {
                            Progress::Waiting { seconds } => *waiting = Some(seconds),
                            Progress::Done { index, outcome } => {
                                *done += 1;
                                *waiting = None;
                                if let Some(&row) = picked.get(index) {
                                    s.rows[row].outcome = Some(outcome);
                                }
                            }
                        }
                    }
                    cx.notify();
                });
            }
            let results = task.await;
            let _ = this.update(cx, |v, cx| {
                if let Some(s) = v.opml_sheet() {
                    s.phase = Phase::Done(Summary::of(&results));
                }
                v.reading_changed(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Export Subscriptions as OPML…: a save dialog, then the file.
    pub(crate) fn opml_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.opml_needs_blyg(cx) {
            return;
        }
        let subs = self.backend.subscriptions();
        if subs.is_empty() {
            self.show_toast("No subscriptions to export", None, cx);
            return;
        }
        let dir = blyg_core::config::paths::home_var()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let rx = cx.prompt_for_new_path(&dir, Some(EXPORT_FILE_NAME));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = rx.await else {
                return;
            };
            let n = subs.len();
            let r = std::fs::write(&path, opml::export(&subs));
            let _ = this.update(cx, |v, cx| match r {
                Ok(()) => v.show_toast(
                    format!("Exported {n} subscription{}", if n == 1 { "" } else { "s" }),
                    Some(blyg_core::config::paths::tilde(&path).into()),
                    cx,
                ),
                Err(e) => v.show_toast(format!("Couldn't save the file: {e}"), None, cx),
            });
        })
        .detach();
    }

    // ------------------------------------------------------------ render

    pub(super) fn render_opml_sheet(&self, s: &Sheet, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette.on_page();
        let theme = &*self.theme;
        let running = matches!(s.phase, Phase::Running { .. });
        let mut list = div()
            .id("opml-list")
            .debug_selector(|| "opml-list".into())
            .flex()
            .flex_col()
            .max_h(px(340.))
            .overflow_y_scroll()
            .track_scroll(&s.scroll);
        let mut prev: Option<&Option<String>> = None;
        for (i, r) in s.rows.iter().enumerate() {
            if prev != Some(&r.feed.folder) && (r.feed.folder.is_some() || i > 0) {
                list = list.child(
                    div()
                        .mt(px(if i == 0 { 0. } else { 8. }))
                        .mb(px(2.))
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.muted)
                        .child(r.feed.folder.clone().unwrap_or_else(|| "No folder".into())),
                );
            }
            prev = Some(&r.feed.folder);
            let on = r.ticked;
            let selected = i == s.sel && matches!(s.phase, Phase::Preview);
            let status: Option<(String, Hsla)> = match &r.outcome {
                Some(Outcome::Added { .. }) => Some(("added".into(), p.green)),
                Some(Outcome::AlreadyFollowing) => Some(("already following".into(), p.muted)),
                Some(Outcome::Failed { kind, reason }) => {
                    Some((format!("{}: {reason}", kind.label()), p.over))
                }
                None if r.already.is_some() => Some(("already following".into(), p.muted)),
                None => None,
            };
            list = list.child(
                div()
                    .id(("opml-row", i))
                    .debug_selector(move || format!("opml-row-{i}"))
                    .flex()
                    .items_start()
                    .gap(px(8.))
                    .px(px(4.))
                    .py(px(3.))
                    .rounded(px(5.))
                    .when(selected, |d| d.bg(p.sel))
                    .when(!running, |d| d.cursor_pointer())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(s) = this.opml_sheet()
                            && s.phase == Phase::Preview
                        {
                            s.toggle(i);
                            s.sel = i;
                            cx.notify();
                        }
                    }))
                    .child(
                        div()
                            .flex_none()
                            .mt(px(1.))
                            .size(px(15.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .map(|d| crate::theme_ext::chip(d, theme, on))
                            .text_size(px(11.))
                            .when(on, |d| d.bg(p.pick()))
                            .child(if on { "✓" } else { "" }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .when(!on, |d| d.text_color(p.muted))
                            .child(div().truncate().child(r.feed.title.clone()))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(p.muted)
                                    .truncate()
                                    .child(r.feed.xml_url.clone()),
                            )
                            .when_some(status, |d, (text, color)| {
                                d.child(div().text_size(px(11.5)).text_color(color).child(text))
                            }),
                    )
                    .when(i < 9, |d| {
                        d.child(
                            div()
                                .flex_none()
                                .text_size(px(10.5))
                                .text_color(p.muted)
                                .child(format!("{}", i + 1)),
                        )
                    }),
            );
        }
        let n = s.ticked();
        let mut notes = vec![];
        if s.duplicates > 0 {
            notes.push(format!(
                "{} duplicate{} collapsed",
                s.duplicates,
                if s.duplicates == 1 { "" } else { "s" }
            ));
        }
        if s.without_feed > 0 {
            notes.push(format!("{} without a feed address skipped", s.without_feed));
        }
        let (status_line, keys): (String, Vec<AnyElement>) = match &s.phase {
            Phase::Preview => (
                format!(
                    "{WHERE_THEY_GO}, slowly (each new subscription fetches its archive). \
                     Imported feeds aren't added to your public blogroll."
                ),
                vec![
                    crate::platform::key_button(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .child(self.kbd("⏎"))
                            .child(format!("import {n}")),
                        "opml-import",
                        cx.listener(|this, _, _, cx| this.opml_start(None, cx)),
                    )
                    .when(n > 0, |d| d.text_color(p.accent_text()))
                    .into_any_element(),
                    crate::platform::key_button(
                        self.key_hint("esc", "cancel"),
                        "opml-cancel",
                        cx.listener(|this, _, window, cx| this.close_reading_sheet(window, cx)),
                    )
                    .into_any_element(),
                    self.key_hint(crate::keymap::hint("⌘A"), "all/none")
                        .into_any_element(),
                    div().child("1–9 or space tick").into_any_element(),
                ],
            ),
            Phase::Running {
                done,
                total,
                waiting,
                cancelling,
            } => (
                match (cancelling, waiting) {
                    (true, _) => {
                        format!("Stopping after the current one… ({done} of {total} done)")
                    }
                    (false, Some(w)) => format!(
                        "Importing {} of {total}. The blyg asked Burrow to slow down: waiting {w} s",
                        (done + 1).min(*total)
                    ),
                    (false, None) => format!("Importing {} of {total}…", (done + 1).min(*total)),
                },
                vec![
                    crate::platform::key_button(
                        self.key_hint("esc", "stop"),
                        "opml-stop",
                        cx.listener(|this, _, _, cx| this.opml_cancel(cx)),
                    )
                    .into_any_element(),
                ],
            ),
            Phase::Done(sum) => {
                let mut keys = vec![
                    crate::platform::key_button(
                        self.key_hint("⏎", "done"),
                        "opml-done",
                        cx.listener(|this, _, window, cx| this.close_reading_sheet(window, cx)),
                    )
                    .into_any_element(),
                ];
                if !sum.failed.is_empty() {
                    keys.push(
                        self.key_hint("r", "retry failed")
                            .id("opml-retry")
                            .debug_selector(|| "opml-retry".into())
                            .cursor_pointer()
                            .text_color(p.accent_text())
                            .on_click(cx.listener(|this, _, _, cx| this.opml_retry(cx)))
                            .into_any_element(),
                    );
                }
                let mut line = sum.line();
                if sum.added > 0 {
                    line.push_str(&format!(". {WHERE_THEY_GO}."));
                }
                (line, keys)
            }
        };
        let heading = match &s.phase {
            Phase::Done(_) => "Import finished".to_string(),
            _ => format!("Import subscriptions from {}", s.file),
        };
        div()
            .track_focus(&s.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                if this.opml_key(ev, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .child(self.sheet_heading(heading))
            .child(div().mb(px(8.)).text_color(p.muted).child(format!(
                "{} feed{} in the file{}",
                s.rows.len(),
                if s.rows.len() == 1 { "" } else { "s" },
                if notes.is_empty() {
                    String::new()
                } else {
                    format!(" · {}", notes.join(" · "))
                }
            )))
            .child(list)
            .child(
                div()
                    .id("opml-status")
                    .debug_selector(|| "opml-status".into())
                    .mt(px(10.))
                    .text_size(px(12.))
                    .line_height(relative(1.45))
                    .text_color(p.muted)
                    .child(status_line),
            )
            .child(self.keys_row(vec![]).children(keys))
            .into_any_element()
    }
}
