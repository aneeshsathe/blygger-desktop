//! Catch up (⇧C on the reading screen, Post › Catch Up…, the "Catch up"
//! chip, `C` in the lineage sheet): the conversation that leads to a post,
//! unrolled oldest first, the order to read it in. Each post is a card with
//! its own words (its quotes turned into "↪ quotes …" markers, since the
//! quoted post is its own earlier card), indented by depth. j/k move, ⏎/o
//! open the post in the reader (the reader's own open: a post not held is
//! fetched on that click, never before), w adds the side branches ("whole
//! conversation"), s asks the AI for a summary at the top (reading-only,
//! labelled "AI summary"; esc cancels it), esc closes. The summary is the
//! blyg's when it offers one (studio 0.69's `summarizeCatchUp`, written by
//! its text helpers model and kept there; [`blyg_summary_entry`]), else
//! Burrow's own AI's. Model:
//! `blyg_core::catchup`; prompt: `blyg_ai::prompts::catch_up_summary`.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use blyg_ai::{CancelFlag, Provider};
use blyg_core::catchup::{self, Conversation, Words};
use blyg_core::post_key;
use blyg_render::Block;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::RSheet;
use super::lineage::rel_color;
use crate::app::MainView;

gpui_kit::actions!(
    blygger,
    [
        /// The conversation leading to the post in front of you, oldest first.
        CatchUp
    ]
);

/// How often a running summary is looked at.
const POLL: Duration = Duration::from_millis(60);
/// The list's height in the sheet.
const LIST_H: f32 = 470.;
/// Indent per depth.
const INDENT: f32 = 18.;

/// The AI summary panel.
#[derive(Debug, Clone, PartialEq)]
pub enum Summary {
    Off,
    /// Asked; the text so far.
    Running {
        job: u64,
        text: String,
    },
    Done {
        text: String,
        model: String,
    },
    /// No provider, or it failed: a headline and what to do.
    Failed {
        head: String,
        sub: String,
        setup: bool,
    },
    Cancelled,
}

pub struct Sheet {
    pub origin: String,
    pub id: String,
    pub whole: bool,
    pub convo: Conversation,
    /// Each entry's words as blocks (held posts only).
    pub blocks: Vec<Option<Rc<Vec<Block>>>>,
    pub sel: usize,
    pub list: ListState,
    pub focus: FocusHandle,
    pub summary: Summary,
    /// The running summary's cancel flag.
    pub cancel: Option<CancelFlag>,
}

fn blocks_of(c: &Conversation) -> Vec<Option<Rc<Vec<Block>>>> {
    c.entries
        .iter()
        .map(|e| match &e.words {
            Words::Own(md) if e.held.is_some() && !md.trim().is_empty() => {
                let kind = match catchup::kind_of(e) {
                    blyg_core::Kind::Thread => blyg_render::Kind::Thread,
                    blyg_core::Kind::Fragment => blyg_render::Kind::Fragment,
                };
                Some(Rc::new(blyg_render::native_blocks(md, kind)))
            }
            _ => None,
        })
        .collect()
}

/// A day as the card shows it: "4 Oct 2026", local time.
fn day_words(at: Option<i64>) -> Option<String> {
    let t = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(at?)?;
    Some(
        t.with_timezone(&chrono::Local)
            .format("%-d %b %Y")
            .to_string(),
    )
}

/// A new summary job's number (a late answer to an older one is dropped).
fn next_job() -> u64 {
    static JOBS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    JOBS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
}

/// The entry to ask the blyg's summary for: the starting post, when it's
/// held here (yours as `~own`). The blyg's summary reads the quotes baked
/// into that one post, so it isn't asked with "whole conversation" on (the
/// side branches are Burrow's own).
pub fn blyg_summary_entry(
    whole: bool,
    start: Option<&blyg_core::ReadingItem>,
) -> Option<blyg_core::shelf::Entry> {
    let r = start.filter(|_| !whole)?;
    Some(blyg_core::shelf::Entry::new(
        r.subscription_id.clone(),
        r.remote_id.clone(),
    ))
}

impl MainView {
    pub(super) fn catch_up_actions(
        &self,
        d: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        d.on_action(cx.listener(|this, _: &CatchUp, window, cx| this.catch_up_key(window, cx)))
    }

    /// Whether `(origin, id)` draws on anything known here: the chip shows
    /// only then.
    pub(crate) fn has_ancestors(&self, origin: &str, id: &str) -> bool {
        !self.reading.lineage.up(origin, id).is_empty()
    }

    /// ⇧C / Post › Catch Up…: the post in front of you (the open reading
    /// post, the stream's selected one, or your own published post).
    pub(crate) fn catch_up_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.reading.sheet, Some(RSheet::CatchUp(_))) {
            self.close_reading_sheet(window, cx);
            return;
        }
        match self.lineage_target() {
            Some((origin, id)) => self.open_catch_up(origin, id, window, cx),
            None => self.show_toast(
                "No post to catch up on",
                Some("Select a post in Reading, or open one of your published posts".into()),
                cx,
            ),
        }
    }

    /// The conversation leading to `(origin, id)`, from what this Mac holds.
    pub(crate) fn catch_up_model(&self, origin: &str, id: &str, whole: bool) -> Conversation {
        let own = self.own_rows();
        self.with_lineage_src(|src| {
            let held = |o: &str, i: &str| {
                let key = post_key(o, i);
                src.rows
                    .iter()
                    .chain(own.iter())
                    .find(|r| post_key(&r.origin, &r.remote_id) == key)
                    .cloned()
            };
            catchup::build(origin, id, src.local, &held, whole)
        })
    }

    pub(crate) fn open_catch_up(
        &mut self,
        origin: String,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let convo = self.catch_up_model(&origin, &id, false);
        let n = convo.entries.len();
        let focus = cx.focus_handle();
        let list = ListState::new(n, ListAlignment::Top, px(400.));
        // Open at the oldest post: read down from it.
        let sheet = Sheet {
            origin,
            id,
            whole: false,
            blocks: blocks_of(&convo),
            convo,
            sel: 0,
            list,
            focus: focus.clone(),
            summary: Summary::Off,
            cancel: None,
        };
        self.cancel_catch_up_summary();
        self.open_reading_sheet(RSheet::CatchUp(Box::new(sheet)), cx);
        window.focus(&focus, cx);
    }

    pub(super) fn catch_up_mut(&mut self) -> Option<&mut Sheet> {
        match self.reading.sheet.as_mut() {
            Some(RSheet::CatchUp(s)) => Some(s),
            _ => None,
        }
    }

    /// w: side branches on or off.
    pub(crate) fn catch_up_toggle_whole(&mut self, cx: &mut Context<Self>) {
        let Some(s) = self.catch_up_mut() else { return };
        let (origin, id, whole) = (s.origin.clone(), s.id.clone(), !s.whole);
        let keep = s.convo.entries.get(s.sel).map(|e| e.peer.clone());
        let convo = self.catch_up_model(&origin, &id, whole);
        if let Some(s) = self.catch_up_mut() {
            s.whole = whole;
            s.blocks = blocks_of(&convo);
            s.sel = keep
                .and_then(|k| convo.entries.iter().position(|e| e.peer == k))
                .unwrap_or(0);
            s.list.reset(convo.entries.len());
            s.list.scroll_to_reveal_item(s.sel);
            s.convo = convo;
        }
        cx.notify();
    }

    fn catch_up_move(&mut self, delta: isize, cx: &mut Context<Self>) {
        if let Some(s) = self.catch_up_mut() {
            let n = s.convo.entries.len();
            if n > 0 {
                s.sel = (s.sel as isize + delta).clamp(0, n as isize - 1) as usize;
                s.list.scroll_to_reveal_item(s.sel);
            }
        }
        cx.notify();
    }

    /// ⏎ / o: open the selected post in the reader, as the lineage sheet
    /// does (a post not held is fetched on this click); a page opens in the
    /// browser.
    pub(crate) fn catch_up_open_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.catch_up_mut() else { return };
        let Some(e) = s.convo.entries.get(s.sel) else {
            return;
        };
        if let Some((o, i)) = e.post() {
            let (o, i) = (o.to_string(), i.to_string());
            self.cancel_catch_up_summary();
            self.close_reading_sheet(window, cx);
            self.open_original(o, i, None, window, cx);
        } else if let Some(u) = e
            .url()
            .filter(|u| u.starts_with("https://") || u.starts_with("http://"))
        {
            cx.open_url(u);
        } else {
            self.show_toast("That post can't be found: it names no blyg", None, cx);
        }
    }

    fn catch_up_key_down(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match key {
            "down" | "j" => self.catch_up_move(1, cx),
            "up" | "k" => self.catch_up_move(-1, cx),
            "enter" | "o" => self.catch_up_open_selected(window, cx),
            "w" => self.catch_up_toggle_whole(cx),
            "s" => self.catch_up_summarize(window, cx),
            "escape" => {
                let running = self
                    .catch_up_mut()
                    .is_some_and(|s| matches!(s.summary, Summary::Running { .. }));
                if running {
                    self.cancel_catch_up_summary();
                    if let Some(s) = self.catch_up_mut() {
                        s.summary = Summary::Cancelled;
                    }
                    cx.notify();
                } else {
                    self.close_reading_sheet(window, cx);
                }
            }
            _ => return false,
        }
        true
    }

    // ------------------------------------------------------------ the summary

    /// Stop a running summary (closing the sheet, opening a post, esc).
    pub(crate) fn cancel_catch_up_summary(&mut self) {
        if let Some(s) = self.catch_up_mut()
            && let Some(c) = s.cancel.take()
        {
            c.cancel();
            if matches!(s.summary, Summary::Running { .. }) {
                s.summary = Summary::Cancelled;
            }
        }
    }

    /// s: summarize the conversation: ask the blyg first when its summary
    /// covers it ([`blyg_summary_entry`]), and Burrow's own AI when the blyg
    /// doesn't offer one or can't write it. Reading-only: shown in the
    /// sheet, never in a post.
    pub(crate) fn catch_up_summarize(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.catch_up_mut() else { return };
        if matches!(s.summary, Summary::Running { .. }) {
            return;
        }
        let start = s.convo.entries.iter().find(|e| e.start);
        let Some(entry) = blyg_summary_entry(s.whole, start.and_then(|e| e.held.as_ref())) else {
            return self.catch_up_summarize_here(window, cx);
        };
        let job = next_job();
        s.summary = Summary::Running {
            job,
            text: String::new(),
        };
        cx.notify();
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.summarize_catch_up(&entry) });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |v, window, cx| {
                let Some(s) = v.catch_up_mut() else { return };
                if !matches!(s.summary, Summary::Running { job: j, .. } if j == job) {
                    return; // cancelled, or asked again
                }
                match r {
                    Ok(Some(sum)) => {
                        s.summary = Summary::Done {
                            text: sum.summary,
                            model: format!("{} · on your blyg", sum.model),
                        };
                        cx.notify();
                    }
                    // Not offered, nothing it can summarize, or no model:
                    // Burrow's own AI.
                    _ => {
                        s.summary = Summary::Off;
                        v.catch_up_summarize_here(window, cx);
                    }
                }
            });
        })
        .detach();
    }

    /// The summary from Burrow's own AI (the configured provider), off the
    /// UI thread.
    fn catch_up_summarize_here(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.catch_up_mut() else { return };
        if matches!(s.summary, Summary::Running { .. }) {
            return;
        }
        let prompt = blyg_ai::prompts::catch_up_summary(&s.convo);
        let picked = match crate::ai::pick(cx) {
            Ok(p) => p,
            Err((e, kind)) => {
                let setup = matches!(
                    e,
                    blyg_ai::AiError::NotConfigured(_) | blyg_ai::AiError::CliNotFound(_)
                );
                let (head, sub) = crate::ai::failure_toast(&crate::ai::Failure::Ai(e), kind);
                if let Some(s) = self.catch_up_mut() {
                    s.summary = Summary::Failed { head, sub, setup };
                }
                cx.notify();
                return;
            }
        };
        let job = next_job();
        let cancel = CancelFlag::new();
        if let Some(s) = self.catch_up_mut() {
            s.summary = Summary::Running {
                job,
                text: String::new(),
            };
            s.cancel = Some(cancel.clone());
        }
        cx.notify();
        let kind = picked.kind;
        let provider: Arc<dyn Provider> = picked.provider;
        // Text as it streams in, then the end.
        let (tx, rx) = async_channel::unbounded::<Result<String, blyg_ai::Result<String>>>();
        let req = prompt.request(&cancel);
        let spawned = std::thread::Builder::new()
            .name("blygger-ai-catchup".into())
            .spawn(move || {
                let delta_tx = tx.clone();
                let r = provider.generate(req, &mut |d| {
                    let _ = delta_tx.try_send(Ok(d.to_string()));
                });
                let _ = tx.try_send(Err(r.map(|g| g.model)));
            });
        if let Err(e) = spawned {
            if let Some(s) = self.catch_up_mut() {
                s.summary = Summary::Failed {
                    head: format!("Couldn't start the AI helper: {e}"),
                    sub: String::new(),
                    setup: false,
                };
            }
            return;
        }
        // Poll, as the editor's AI jobs do: the provider thread never wakes
        // the UI executor, and a cancelled job just stops being looked at.
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                let alive = this.update(cx, |v, cx| {
                    let Some(s) = v.catch_up_mut() else {
                        return false;
                    };
                    let Summary::Running { job: j, text } = &mut s.summary else {
                        return false;
                    };
                    if *j != job {
                        return false;
                    }
                    let mut end = None;
                    while let Ok(m) = rx.try_recv() {
                        match m {
                            Ok(d) => text.push_str(&d),
                            Err(r) => end = Some(r),
                        }
                    }
                    let alive = match end {
                        None => true,
                        Some(Ok(model)) => {
                            let text = std::mem::take(text);
                            s.summary = Summary::Done { text, model };
                            s.cancel = None;
                            false
                        }
                        Some(Err(blyg_ai::AiError::Cancelled)) => {
                            s.summary = Summary::Cancelled;
                            s.cancel = None;
                            false
                        }
                        Some(Err(e)) => {
                            let (head, sub) =
                                crate::ai::failure_toast(&crate::ai::Failure::Ai(e), Some(kind));
                            s.summary = Summary::Failed {
                                head,
                                sub,
                                setup: false,
                            };
                            s.cancel = None;
                            false
                        }
                    };
                    cx.notify();
                    alive
                });
                if !matches!(alive, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
        let limit = crate::ai::timeout(cx);
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(limit).await;
            let _ = this.update(cx, |v, cx| {
                if let Some(s) = v.catch_up_mut()
                    && matches!(s.summary, Summary::Running { job: j, .. } if j == job)
                {
                    if let Some(c) = s.cancel.take() {
                        c.cancel();
                    }
                    let (head, sub) =
                        crate::ai::failure_toast(&crate::ai::Failure::TimedOut(limit), Some(kind));
                    s.summary = Summary::Failed {
                        head,
                        sub,
                        setup: false,
                    };
                    cx.notify();
                }
            });
        })
        .detach();
    }

    // ------------------------------------------------------------ the sheet

    pub(super) fn render_catch_up_sheet(&self, s: &Sheet, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette.on_page();
        let n = s.convo.entries.len();
        let head =
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .mb(px(8.))
                .text_size(px(11.5))
                .text_color(p.muted)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(p.ink)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format!(
                            "Catch up · {n} post{} · {}",
                            if n == 1 { "" } else { "s" },
                            catchup::span_words(&s.convo)
                        )),
                )
                .child(
                    self.chip(
                        "catchup-whole",
                        if s.whole {
                            "✓ Whole conversation"
                        } else {
                            "Whole conversation"
                        },
                    )
                    .when(s.whole, |d| {
                        d.border_color(p.accent).text_color(p.accent_text())
                    })
                    .tooltip(|_, cx| {
                        cx.new(|_| {
                            super::Tip(
                                "w: also show held posts that draw on any of these (side replies)"
                                    .into(),
                            )
                        })
                        .into()
                    })
                    .on_click(cx.listener(|this, _, _, cx| this.catch_up_toggle_whole(cx))),
                )
                .child(self.chip("catchup-summarize", "Summarize (s)").on_click(
                    cx.listener(|this, _, window, cx| this.catch_up_summarize(window, cx)),
                ));
        let keys = div()
            .mt(px(8.))
            .text_size(px(11.))
            .text_color(p.muted)
            .child(if s.convo.truncated {
                crate::keymap::hint("Oldest first · cut short at 8 hops / 60 posts · j/k move · ⏎ open · w whole conversation · s summarize · esc closes")
            } else {
                crate::keymap::hint("Oldest first · j/k move · ⏎ open · w whole conversation · s summarize · esc closes")
            });
        let body = if n <= 1 {
            div()
                .p(px(14.))
                .text_size(px(13.))
                .text_color(p.muted)
                .child("This post draws on nothing known here: there's no conversation before it.")
                .into_any_element()
        } else {
            list(
                s.list.clone(),
                cx.processor(|this, ix: usize, _, cx| this.render_catch_up_entry(ix, cx)),
            )
            .w_full()
            .h(px(LIST_H))
            .into_any_element()
        };
        div()
            .id("catchup-sheet")
            .debug_selector(|| "catchup-sheet".into())
            .track_focus(&s.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                let k = &ev.keystroke;
                if k.modifiers.platform || k.modifiers.control || k.modifiers.alt {
                    return;
                }
                if this.catch_up_key_down(k.key.as_str(), window, cx) {
                    cx.stop_propagation();
                }
            }))
            .child(head)
            .children(self.render_catch_up_summary(s, cx))
            .child(body)
            .child(keys)
            .into_any_element()
    }

    fn render_catch_up_summary(&self, s: &Sheet, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.palette.on_page();
        let (text, foot, setup): (String, String, bool) = match &s.summary {
            Summary::Off => return None,
            Summary::Running { text, .. } if text.is_empty() => (
                "Reading the conversation…".into(),
                "esc cancels".into(),
                false,
            ),
            Summary::Running { text, .. } => (text.clone(), "esc cancels".into(), false),
            Summary::Done { text, model } => (
                text.trim().to_string(),
                format!("{model} · from the posts above only; check it against them"),
                false,
            ),
            Summary::Failed { head, sub, setup } => (head.clone(), sub.clone(), *setup),
            Summary::Cancelled => ("Summary cancelled.".into(), "s asks again".into(), false),
        };
        Some(
            div()
                .id("catchup-summary")
                .mb(px(10.))
                .p(px(10.))
                .rounded(px(self.theme.corner(7.)))
                .border_1()
                .border_color(p.line)
                .bg(p.sel)
                .text_size(px(12.5))
                .line_height(relative(1.45))
                .child(
                    div()
                        .text_size(px(10.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.muted)
                        .child("AI SUMMARY"),
                )
                .child(div().mt(px(4.)).text_color(p.ink).child(text))
                .child(
                    div()
                        .mt(px(4.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .text_size(px(11.))
                        .text_color(p.muted)
                        .child(div().flex_1().min_w_0().child(foot))
                        .when(setup, |d| {
                            d.child(self.chip("catchup-ai-settings", "Settings › AI").on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.close_reading_sheet(window, cx);
                                    this.ai_open_settings(window, cx);
                                }),
                            ))
                        }),
                )
                .into_any_element(),
        )
    }

    fn render_catch_up_entry(&mut self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(RSheet::CatchUp(s)) = self.reading.sheet.as_ref() else {
            return div().into_any_element();
        };
        let Some(e) = s.convo.entries.get(ix) else {
            return div().into_any_element();
        };
        let p = self.palette.on_page();
        let max = s.convo.max_depth();
        let selected = s.sel == ix;
        let rule = e
            .link
            .as_ref()
            .map_or(p.accent, |l| rel_color(&p, l.relation));
        let date = day_words(e.at);
        let mut meta = vec![e.who.clone()];
        if let Some(d) = date {
            meta.push(d);
        }
        let chip = div()
            .px(px(6.))
            .rounded_full()
            .border_1()
            .border_color(rule)
            .text_color(rule)
            .text_size(px(10.5))
            .child(e.link_words());
        let header = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .text_size(px(12.))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.ink)
                    .child(meta.join(" · ")),
            )
            .child(chip)
            .when(e.side, |d| {
                d.child(
                    div()
                        .text_color(p.muted)
                        .text_size(px(10.5))
                        .child("side branch"),
                )
            })
            .when(e.held.is_none(), |d| {
                d.child(
                    div()
                        .text_color(p.muted)
                        .text_size(px(10.5))
                        .child("not held here"),
                )
            });
        let muted = |t: String| {
            div()
                .mt(px(4.))
                .text_size(px(12.5))
                .text_color(p.muted)
                .child(t)
                .into_any_element()
        };
        let words: Vec<AnyElement> = match (&e.words, s.blocks.get(ix).cloned().flatten(), &e.held)
        {
            (_, Some(blocks), Some(post)) => {
                let post = post.clone();
                let mut n = 0;
                self.render_blocks(&blocks, &post, &mut n, cx)
            }
            (Words::Own(_), _, _) => vec![muted("(no text)".into())],
            (Words::Cited(x), _, _) => vec![
                div()
                    .mt(px(4.))
                    .pl(px(8.))
                    .border_l_2()
                    .border_color(p.line)
                    .italic()
                    .text_size(px(12.5))
                    .child(format!("“{}”", x.trim()))
                    .into_any_element(),
                muted(
                    crate::keymap::hint("What the quoting post cited · ⏎ opens it in the reader")
                        .into(),
                ),
            ],
            (Words::NotHeld, _, _) => {
                vec![muted(
                    crate::keymap::hint("Not held here · ⏎ opens it in the reader").into(),
                )]
            }
            (Words::None, _, _) => vec![muted(match e.url() {
                Some(u) => crate::keymap::hint_owned(format!(
                    "A web page, {u} · ⏎ opens it in the browser"
                )),
                None => "A post that names no blyg: it can't be found".into(),
            })],
        };
        div()
            .id(("catchup-entry", ix))
            .debug_selector(move || format!("catchup-entry-{ix}"))
            .pl(px(INDENT * max.saturating_sub(e.depth) as f32))
            .pb(px(8.))
            .child(
                div()
                    .pl(px(10.))
                    .pr(px(10.))
                    .py(px(6.))
                    .border_l_2()
                    .border_color(rule)
                    .rounded(px(self.theme.corner(6.)))
                    .when(selected, |d| d.bg(p.sel))
                    .when(e.side, |d| d.opacity(0.85))
                    .cursor_pointer()
                    .child(header)
                    .child(
                        div()
                            .text_size(px(13.5))
                            .line_height(relative(1.5))
                            .text_color(p.ink)
                            .children(words),
                    ),
            )
            .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                let again = this.catch_up_mut().is_some_and(|s| s.sel == ix);
                if let Some(s) = this.catch_up_mut() {
                    s.sel = ix;
                }
                if again && ev.click_count() > 1 {
                    this.catch_up_open_selected(window, cx);
                }
                cx.notify();
            }))
            .into_any_element()
    }
}
