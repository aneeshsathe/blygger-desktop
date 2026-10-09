//! --- browser macros --- Running an extension's macro in the browser pane
//! (docs/EXTENSIONS.md § Browser: capture and macros).
//!
//! - `runner`: the run over a [`runner::Page`], with its two sheets; pure of
//!   GPUI and of any one engine (no `cfg(target_os)` here or below).
//! - `insert`: the insert chain (trusted paste, insertText, synthetic paste,
//!   by hand), each checked by reading the box back.
//! - `js`: the in-page operations, run through `BrowserSurface::eval_json`.
//!
//! This file is the app's side: [`start`] (what the ⇧⌘P palette calls),
//! the pane as a `Page` ([`UiPage`]), the Preview and Post sheets, the
//! per-site `min-interval`, the outcome toast and `macro.log`.

pub mod insert;
pub mod js;
pub mod runner;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::base::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

// What the palette (extensions/macros.rs) and the tests use.
pub use runner::{MacroRun, Outcome, PostChoice};

use self::js::{Op, Reply};
use self::runner::{Confirm, Pacing, Page, Preview};
use super::surface::PageState;
use crate::app::{MainView, Sheet};

/// How long one in-page operation may take to answer.
const EVAL_TIMEOUT: Duration = Duration::from_secs(3);

/// The pane's macro state (one run at a time).
#[derive(Default)]
pub struct State {
    running: Option<Running>,
    /// Hosts whose content blocking is off for this run only
    /// (`content-blocking = false` on the site): read by the pane's
    /// `BlockingFor`, never written to `state.json`.
    pub(crate) run_unblocked: Rc<RefCell<BTreeSet<String>>>,
    pacing: Pacing,
}

struct Running {
    stop: Rc<Cell<bool>>,
}

impl State {
    /// A macro is running (the pane is held open, and not torn down).
    pub fn running(&self) -> bool {
        self.running.is_some()
    }
}

/// The Preview sheet: the text, editable, before anything happens.
pub struct PreviewSheet {
    pub info: Preview,
    pub editor: Entity<TextareaState>,
    reply: async_channel::Sender<Option<String>>,
}

/// The Post sheet: what the composer holds, before `submit`.
pub struct PostSheet {
    pub info: Confirm,
    pub focus: FocusHandle,
    reply: async_channel::Sender<PostChoice>,
}

/// Run `run` in the browser pane: what the ⇧⌘P palette calls for a macro
/// (after `Host::macro_entry` re-checked the grant and
/// `extension/macro.prepare` shaped the text). Shows the Preview sheet
/// first; nothing is loaded or typed before the user continues.
pub fn start(view: &mut MainView, run: MacroRun, window: &mut Window, cx: &mut Context<MainView>) {
    view.macro_start(run, window, cx);
}

/// `<data dir>/extensions/<ext>/macro.log`.
pub fn log_path(data_dir: &Path, ext: &str) -> PathBuf {
    data_dir.join("extensions").join(ext).join("macro.log")
}

/// Append `line` to the extension's `macro.log` (keeping the last
/// [`runner::LOG_LINES`]).
pub fn append_log(data_dir: &Path, ext: &str, line: &str) {
    let path = log_path(data_dir, ext);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let old = std::fs::read_to_string(&path).unwrap_or_default();
    let _ = std::fs::write(&path, runner::ring_append(&old, line, runner::LOG_LINES));
}

/// `BLYGGER_DEMO=br-macro` (or `br-macro-ext`) in a debug build: the sheets answer themselves
/// (the smoke test). Never in a release build.
pub(crate) fn demo_auto() -> bool {
    cfg!(debug_assertions)
        && std::env::var("BLYGGER_DEMO").is_ok_and(|d| d == "br-macro" || d == "br-macro-ext")
}

/// The pane, the sheets and the clipboard, as the runner's [`Page`].
struct UiPage<'a> {
    this: WeakEntity<MainView>,
    cx: &'a mut AsyncWindowContext,
    t0: Instant,
    stop: Rc<Cell<bool>>,
    edit_commands: bool,
    /// The user's clipboard before the run (`Some(None)`: it was empty).
    saved: Option<Option<ClipboardItem>>,
    /// The extension and macro (for trace entries in `macro.log`).
    ext: String,
    macro_id: String,
}

/// `BLYGGER_MACRO_TRACE=1`: `macro.log` gets the page's outline after every
/// step (tags, classes, roles, aria labels, button labels; never the text).
pub(crate) fn tracing() -> bool {
    std::env::var("BLYGGER_MACRO_TRACE").is_ok_and(|v| v == "1")
}

impl Page for UiPage<'_> {
    async fn eval(&mut self, op: &Op) -> Option<Reply> {
        let (tx, rx) = async_channel::bounded::<String>(2);
        let js = js::call(op);
        let sent = self
            .this
            .update(self.cx, |v, _| {
                v.browser.with(|s| s.eval_json(&js, tx.clone())).is_some()
            })
            .unwrap_or(false);
        if !sent {
            return None;
        }
        // A page that never answers (it's navigating) mustn't hang the run.
        let timer = self.cx.background_executor().timer(EVAL_TIMEOUT);
        self.cx
            .background_executor()
            .spawn(async move {
                timer.await;
                let _ = tx.try_send("null".into());
            })
            .detach();
        Reply::parse(&rx.recv().await.ok()?)
    }

    fn state(&mut self) -> Option<PageState> {
        self.this
            .update(self.cx, |v, cx| v.browser_run_state(cx))
            .ok()
            .flatten()
    }

    fn load(&mut self, url: &str) {
        let url = url.to_string();
        self.edit_commands = self
            .this
            .update_in(self.cx, |v, window, cx| {
                v.browser_run_open(&url, window, cx);
                v.browser.with(|s| s.has_edit_commands()).unwrap_or(false)
            })
            .unwrap_or(false);
    }

    fn focus_page(&mut self) {
        let _ = self
            .this
            .update(self.cx, |v, _| v.browser.with(|s| s.focus_page()));
    }

    fn paste(&mut self) {
        let _ = self
            .this
            .update(self.cx, |v, _| v.browser.with(|s| s.edit_command("paste:")));
    }

    fn has_edit_commands(&self) -> bool {
        self.edit_commands
    }

    fn now(&self) -> Duration {
        self.t0.elapsed()
    }

    async fn sleep(&mut self, d: Duration) {
        self.cx.background_executor().timer(d).await;
    }

    fn save_clipboard(&mut self) {
        if self.saved.is_none() {
            self.saved = Some(
                self.cx
                    .update(|_, cx| cx.read_from_clipboard())
                    .ok()
                    .flatten(),
            );
        }
    }

    fn set_clipboard(&mut self, text: &str) {
        let item = ClipboardItem::new_string(text.to_string());
        let _ = self.cx.update(|_, cx| cx.write_to_clipboard(item));
    }

    fn restore_clipboard(&mut self) {
        if let Some(prev) = self.saved.take() {
            let item = prev.unwrap_or_else(|| ClipboardItem::new_string(String::new()));
            let _ = self.cx.update(|_, cx| cx.write_to_clipboard(item));
        }
    }

    fn stopped(&self) -> bool {
        self.stop.get()
    }

    async fn preview(&mut self, p: Preview) -> Option<String> {
        let (tx, rx) = async_channel::bounded(1);
        self.this
            .update_in(self.cx, |v, window, cx| {
                v.macro_show_preview(p, tx, window, cx)
            })
            .ok()?;
        rx.recv().await.ok().flatten()
    }

    async fn confirm(&mut self, c: Confirm) -> PostChoice {
        let (tx, rx) = async_channel::bounded(1);
        if self
            .this
            .update_in(self.cx, |v, window, cx| {
                v.macro_show_post(c, tx, window, cx)
            })
            .is_err()
        {
            return PostChoice::Cancel;
        }
        rx.recv().await.unwrap_or(PostChoice::Cancel)
    }

    fn tracing(&self) -> bool {
        tracing()
    }

    fn trace(&mut self, lines: &[String]) {
        let dir = self.this.update(self.cx, |v, _| v.browser_data_dir());
        if let Ok(Some(dir)) = dir {
            let time = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
            append_log(
                &dir,
                &self.ext,
                &runner::trace_entry(&time, &self.macro_id, lines),
            );
        }
    }

    fn progress(&mut self, index: usize, step: &blyg_ext::recipe::Step) {
        if demo_auto() || std::env::var_os("BLYGGER_TIMING").is_some() {
            println!(
                "macro-step {} {} {}",
                index + 1,
                step.name(),
                step.selector().unwrap_or("")
            );
        }
    }
}

impl MainView {
    /// See [`start`].
    pub(crate) fn macro_start(
        &mut self,
        run: MacroRun,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.browser.automation.running() {
            return self.show_toast(
                "A macro is already running in the browser pane",
                Some("Stop it in the pane's bar first".into()),
                cx,
            );
        }
        if let Err(e) = blyg_ext::recipe::check_macro(&run.spec, std::slice::from_ref(&run.site)) {
            return self.show_toast(format!("{}: {e}", run.site.title), None, cx);
        }
        let key = run.pace_key();
        let min = run.site.min_interval();
        if let Some(left) = self
            .browser
            .automation
            .pacing
            .wait_left(&key, min, Instant::now())
        {
            if demo_auto() {
                println!("macro-too-soon wait_s={}", left.as_secs().max(1));
            }
            return self.show_toast(
                format!(
                    "{}: wait {} s before posting there again",
                    run.site.title,
                    left.as_secs().max(1)
                ),
                Some("min-interval in the extension's site settings".into()),
                cx,
            );
        }
        // The site's shield off for this run only.
        if !run.site.content_blocking
            && let Some(host) = super::host_of(&run.site.origin)
        {
            self.browser
                .automation
                .run_unblocked
                .borrow_mut()
                .insert(host);
            self.browser.with(|s| s.refresh_blocking());
        }
        let stop = Rc::new(Cell::new(false));
        self.browser.automation.running = Some(Running { stop: stop.clone() });
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let mut page = UiPage {
                this: this.clone(),
                cx,
                t0: Instant::now(),
                stop,
                edit_commands: false,
                saved: None,
                ext: run.ext.clone(),
                macro_id: run.spec.id.clone(),
            };
            let out = runner::run(&mut page, &run).await;
            let _ = this.update_in(cx, |v, window, cx| v.macro_finished(&run, out, window, cx));
        })
        .detach();
    }

    /// Stop the running macro (the pane's ■ Stop): nothing more is typed or
    /// clicked.
    pub(crate) fn macro_stop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(r) = &self.browser.automation.running {
            r.stop.set(true);
        }
        if matches!(
            self.sheet,
            Some(Sheet::MacroPreview(_)) | Some(Sheet::MacroPost(_))
        ) {
            self.close_sheet(window, cx);
        }
    }

    fn macro_finished(
        &mut self,
        run: &MacroRun,
        out: Outcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.browser.automation.running = None;
        if matches!(
            self.sheet,
            Some(Sheet::MacroPreview(_)) | Some(Sheet::MacroPost(_))
        ) {
            self.close_sheet(window, cx);
        }
        if let Some(host) = super::host_of(&run.site.origin)
            && self
                .browser
                .automation
                .run_unblocked
                .borrow_mut()
                .remove(&host)
        {
            self.browser.with(|s| s.refresh_blocking());
        }
        if out.counts_as_post() {
            self.browser
                .automation
                .pacing
                .record(&run.pace_key(), Instant::now());
        }
        if let Some(dir) = self.browser_data_dir() {
            let time = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
            append_log(&dir, &run.ext, &runner::log_line(&time, run, &out));
        }
        if demo_auto() || std::env::var_os("BLYGGER_TIMING").is_some() {
            match &out {
                Outcome::Posted { message, via } => {
                    println!("macro-posted via={} {message:?}", via.name())
                }
                o => println!("macro-outcome {}", o.log_text()),
            }
        }
        let site = &run.site.title;
        let (text, sub): (String, Option<String>) = match &out {
            Outcome::Posted { message, .. } => (message.clone(), None),
            Outcome::Handed { manual: false } => (
                format!("The text is in the {site} box"),
                Some("Click Post in the pane when you're ready".into()),
            ),
            Outcome::Handed { manual: true } => (
                format!("{site}: the box didn't take the text"),
                Some("It's on the clipboard: paste it (⌘V), then click Post yourself".into()),
            ),
            Outcome::Cancelled {
                after_submit: false,
            } => ("Cancelled. Nothing was posted.".into(), None),
            Outcome::Cancelled { after_submit: true } => (
                format!("{site}: stopped after Post was clicked"),
                Some("It may have been posted: check the pane".into()),
            ),
            Outcome::Failed(e) => {
                let step = run.spec.steps.get(e.step.saturating_sub(1));
                (
                    e.headline(&run.site, step),
                    Some(e.detail(&run.spec.tested)),
                )
            }
        };
        self.show_toast(text, sub.map(Into::into), cx);
        cx.notify();
    }

    fn macro_show_preview(
        &mut self,
        info: Preview,
        reply: async_channel::Sender<Option<String>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = info.payload.clone();
        let editor = cx.new(|cx| {
            let mut s = TextareaState::new(window, cx).soft_wrap(true);
            s.set_value(text, window, cx);
            s
        });
        self._subs.push(cx.subscribe_in(
            &editor,
            window,
            |this, _, ev: &InputEvent, window, cx| {
                if let InputEvent::PressEnter {
                    secondary: true, ..
                } = ev
                {
                    this.macro_preview_answer(true, window, cx);
                }
            },
        ));
        editor.update(cx, |s, cx| s.focus(window, cx));
        self.sheet_gen += 1;
        self.sheet = Some(Sheet::MacroPreview(PreviewSheet {
            info,
            editor,
            reply,
        }));
        cx.notify();
        if demo_auto() {
            self.macro_demo_answer("preview", window, cx);
        }
    }

    /// Continue (`true`) or Cancel on the Preview sheet.
    pub(crate) fn macro_preview_answer(
        &mut self,
        go: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(Sheet::MacroPreview(s)) = self.sheet.take() else {
            return;
        };
        let text = s.editor.read(cx).value().to_string();
        let _ = s.reply.try_send(go.then_some(text));
        self.focus_after_sheet(window, cx);
        cx.notify();
    }

    fn macro_show_post(
        &mut self,
        info: Confirm,
        reply: async_channel::Sender<PostChoice>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.sheet_gen += 1;
        self.sheet = Some(Sheet::MacroPost(PostSheet { info, focus, reply }));
        cx.notify();
        if demo_auto() {
            self.macro_demo_answer("post", window, cx);
        }
    }

    /// The Post sheet's answer.
    pub(crate) fn macro_post_answer(
        &mut self,
        choice: PostChoice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(Sheet::MacroPost(s)) = self.sheet.take() else {
            return;
        };
        let _ = s.reply.try_send(choice);
        if choice != PostChoice::Post {
            self.focus_after_sheet(window, cx);
        }
        cx.notify();
    }

    /// `BLYGGER_DEMO=br-macro`: answer the sheet on screen after a moment
    /// (saving a frame of it first when `BLYGGER_SNAPSHOT_DIR` is set, in
    /// a snapshot build).
    fn macro_demo_answer(
        &mut self,
        which: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dir = std::env::var("BLYGGER_SNAPSHOT_DIR").ok();
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(700))
                .await;
            if let Some(dir) = dir {
                for _ in 0..2 {
                    let _ = cx.update(|window, cx| window.draw(cx).clear(cx));
                    cx.background_executor()
                        .timer(Duration::from_millis(300))
                        .await;
                }
                let _ = cx.update(|window, cx| {
                    window.draw(cx).clear(cx);
                    let path = format!("{dir}/macro-{which}.png");
                    crate::app::reading::demo::snap::save_frame(window, &path);
                });
            }
            let _ = this.update_in(cx, |v, window, cx| {
                println!("macro-sheet {which}");
                match (which, &v.sheet) {
                    ("preview", Some(Sheet::MacroPreview(_))) => {
                        v.macro_preview_answer(true, window, cx)
                    }
                    ("post", Some(Sheet::MacroPost(s))) => {
                        println!(
                            "macro-readback differs={} {:?}",
                            s.info.differs, s.info.read_back
                        );
                        v.macro_post_answer(PostChoice::Post, window, cx)
                    }
                    _ => {}
                }
            });
        })
        .detach();
    }

    // ------------------------------------------------------------ sheets

    /// The Preview sheet's body (its width and content).
    pub(crate) fn render_macro_preview(
        &self,
        s: &PreviewSheet,
        cx: &mut Context<Self>,
    ) -> (f32, AnyElement) {
        let p = self.palette;
        let theme = &*self.theme;
        s.editor.update(cx, |e, _| e.set_editor_style(p.field()));
        let info = &s.info;
        let unverified = info.tested.trim().eq_ignore_ascii_case("unverified");
        let chars = s.editor.read(cx).value().chars().count();
        let host = info
            .origin
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .to_string();
        let body = div()
            .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                let k = &ev.keystroke;
                if k.key == "escape" {
                    cx.stop_propagation();
                    this.macro_preview_answer(false, window, cx);
                } else if k.key == "enter" && k.modifiers.platform {
                    cx.stop_propagation();
                    this.macro_preview_answer(true, window, cx);
                }
            }))
            .child(
                div()
                    .mb(px(6.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(13.))
                    .child(info.title.clone()),
            )
            .child(
                div()
                    .mb(px(8.))
                    .text_color(p.muted)
                    .line_height(relative(1.45))
                    .child(format!(
                        "Burrow opens {host} in the browser pane, puts this text in the \
                         box, and asks you again before it clicks Post. Edit it here."
                    )),
            )
            .child(
                div()
                    .id("macro-tested")
                    .debug_selector(|| "macro-tested".into())
                    .mb(px(10.))
                    .px(px(10.))
                    .py(px(6.))
                    .rounded(px(6.))
                    .border_1()
                    .when(unverified, |d| {
                        d.border_color(p.amber)
                            .bg(p.amber.opacity(0.14))
                            .text_color(p.ink)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!(
                                "⚠ Unverified recipe: its selectors for {} haven't been \
                                 checked by hand. Watch the pane.",
                                info.site
                            ))
                    })
                    .when(!unverified, |d| {
                        d.border_color(p.line).text_color(p.muted).child(format!(
                            "Recipe last checked {} (tested in extension.toml)",
                            info.tested.trim()
                        ))
                    }),
            )
            .when_some(info.note.clone(), |d, n| {
                d.child(div().mb(px(6.)).text_color(p.muted).child(n))
            })
            .child(
                crate::theme_ext::field_box(div(), theme)
                    .h(px(150.))
                    .px(px(10.))
                    .py(px(8.))
                    .text_size(px(14.))
                    .child(Textarea::new(&s.editor)),
            )
            .child(
                div()
                    .mt(px(4.))
                    .text_size(px(11.5))
                    .text_color(p.muted)
                    .child(format!("{chars} characters")),
            )
            .child(
                div()
                    .mt(px(12.))
                    .flex()
                    .gap(px(14.))
                    .text_color(p.muted)
                    .child(
                        sheet_key(theme, "⌘⏎", "continue")
                            .id("macro-continue")
                            .debug_selector(|| "macro-continue".into())
                            .cursor_pointer()
                            .text_color(p.over_text())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.macro_preview_answer(true, window, cx)
                            })),
                    )
                    .child(
                        sheet_key(theme, "esc", "cancel")
                            .id("macro-cancel")
                            .debug_selector(|| "macro-cancel".into())
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.macro_preview_answer(false, window, cx)
                            })),
                    ),
            );
        (500., body.into_any_element())
    }

    /// The Post sheet's body.
    pub(crate) fn render_macro_post(
        &self,
        s: &PostSheet,
        cx: &mut Context<Self>,
    ) -> (f32, AnyElement) {
        let p = self.palette;
        let theme = &*self.theme;
        let info = &s.info;
        let shown = if info.read_back.trim().is_empty() {
            "(the box is empty)".to_string()
        } else {
            info.read_back.clone()
        };
        let body = div()
            .track_focus(&s.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                match ev.keystroke.key.as_str() {
                    "enter" => this.macro_post_answer(PostChoice::Post, window, cx),
                    "escape" => this.macro_post_answer(PostChoice::Cancel, window, cx),
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .child(
                div()
                    .mb(px(6.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(13.))
                    .child(format!("Post this to {}?", info.site)),
            )
            .child(
                div()
                    .mb(px(8.))
                    .text_color(if info.differs { p.warn } else { p.muted })
                    .line_height(relative(1.45))
                    .child(if info.differs {
                        "The box holds something other than the text you previewed. Check it \
                         in the pane before posting."
                    } else {
                        "This is what the box on the page holds now."
                    }),
            )
            .child(
                div()
                    .id("macro-readback")
                    .debug_selector(|| "macro-readback".into())
                    .max_h(px(180.))
                    .overflow_y_scroll()
                    .px(px(10.))
                    .py(px(8.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(if info.differs { p.warn } else { p.line })
                    .bg(if info.differs {
                        p.warn.opacity(0.08)
                    } else {
                        p.panel()
                    })
                    .text_color(if info.differs { p.warn } else { p.ink })
                    .whitespace_normal()
                    .line_height(relative(1.45))
                    .child(shown),
            )
            .child(
                div()
                    .mt(px(12.))
                    .flex()
                    .flex_wrap()
                    .gap(px(14.))
                    .text_color(p.muted)
                    .child(
                        sheet_key(theme, "⏎", "post")
                            .id("macro-post")
                            .debug_selector(|| "macro-post".into())
                            .cursor_pointer()
                            .text_color(p.over_text())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.macro_post_answer(PostChoice::Post, window, cx)
                            })),
                    )
                    .child(
                        div()
                            .id("macro-myself")
                            .debug_selector(|| "macro-myself".into())
                            .cursor_pointer()
                            .child("I'll click Post myself")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.macro_post_answer(PostChoice::Myself, window, cx)
                            })),
                    )
                    .child(
                        sheet_key(theme, "esc", "cancel")
                            .id("macro-post-cancel")
                            .debug_selector(|| "macro-post-cancel".into())
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.macro_post_answer(PostChoice::Cancel, window, cx)
                            })),
                    ),
            );
        (500., body.into_any_element())
    }
}

/// A key and what it does, as the other sheets show them.
fn sheet_key(theme: &crate::theme::Theme, k: &'static str, label: &'static str) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(5.))
        .child(
            crate::theme_ext::kbd(div(), theme)
                .px(px(6.))
                .py(px(1.))
                .min_w(px(20.))
                .flex()
                .justify_center()
                .font_weight(FontWeight::MEDIUM)
                .text_size(px(11.5))
                .child(k),
        )
        .child(label)
}

impl MainView {
    /// The publish toast's " · ⇧⌘P to cross-post": a running extension has
    /// a granted macro for a published post.
    pub(crate) fn macro_cross_post_hint(&self) -> bool {
        self.ext.host.as_ref().is_some_and(|h| {
            h.macros()
                .iter()
                .any(|m| m.spec.when == blyg_ext::protocol::When::Published)
        })
    }

    /// `BLYGGER_DEMO=br-macro` (debug builds; `scripts/browser-macro-check.sh`):
    /// built-in test macros against the fixture site at `BLYGGER_DEMO_URL`
    /// (served on 127.0.0.1, never a real site), the sheets answering
    /// themselves. Signed out first, then signed in by the fixture's own
    /// button, then posted; again at once (min-interval); the feed shape
    /// (a prompt opening a modal composer, home.html); `/notes`, which
    /// redirects to the page the pane is already on; a password field
    /// (refused); a page that goes to another origin (off-origin).
    pub(crate) fn macro_demo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !demo_auto() {
            return;
        }
        let Ok(base) = std::env::var("BLYGGER_DEMO_URL") else {
            eprintln!("br-macro: set BLYGGER_DEMO_URL to the fixture site");
            return;
        };
        let base = base.trim_end_matches('/').to_string();
        let Some(origin) = blyg_core::config::parse::url_origin(&base) else {
            eprintln!("br-macro: {base} isn't an http(s) URL");
            return;
        };
        let runs = demo_runs(&base, &origin);
        cx.spawn_in(window, async move |this, cx| {
            let exec = cx.background_executor().clone();
            let pause = |ms| exec.timer(Duration::from_millis(ms));
            pause(300).await;
            for (label, run) in runs {
                if label == "sign-in" {
                    // The fixture's own sign-in button (a session cookie).
                    let url = format!("{base}/login.html");
                    let _ = this.update_in(cx, |v, window, cx| {
                        v.open_url_in_app(&url, super::OpenMode::Full, window, cx)
                    });
                    pause(1500).await;
                    let _ = this.update(cx, |v, _| {
                        v.browser.with(|s| {
                            s.probe(
                                "(document.getElementById('sign-in').click(), 'signed-in')",
                                "macro-demo",
                            )
                        })
                    });
                    pause(1500).await;
                    continue;
                }
                println!("macro-run {label}");
                let _ = this.update_in(cx, |v, window, cx| v.macro_start(run, window, cx));
                for _ in 0..600 {
                    pause(100).await;
                    let busy = this
                        .update(cx, |v, _| v.browser.automation.running())
                        .unwrap_or(false);
                    if !busy {
                        break;
                    }
                }
                if matches!(label, "post" | "feed" | "redirect") {
                    pause(500).await;
                    println!("macro-dom-of {label}");
                    let _ = this.update(cx, |v, _| {
                        v.browser.with(|s| {
                            s.probe(
                                "JSON.stringify([...document.querySelectorAll('#posted li')]\
                                 .map(li => li.innerText))",
                                "macro-dom-posted",
                            )
                        })
                    });
                }
                pause(800).await;
            }
            let _ = this.update(cx, |v, _| {
                v.browser.with(|s| {
                    s.probe(
                        "(document.cookie = 'fixture_session=; max-age=0; path=/', 'signed-out')",
                        "macro-demo",
                    )
                })
            });
            pause(500).await;
            println!("macro-demo-done");
            let _ = this.update(cx, |_, cx| cx.quit());
        })
        .detach();
    }
}

/// The demo's runs, in order (`sign-in` is the demo's own step, not a run).
fn demo_runs(base: &str, origin: &str) -> Vec<(&'static str, MacroRun)> {
    use blyg_ext::protocol::{MacroSpec, SiteSpec, When};
    use blyg_ext::recipe::{self, Step};
    let site = |id: &str, min: Option<&str>| SiteSpec {
        id: id.into(),
        title: "Fixture Notes".into(),
        origin: origin.into(),
        home: format!("{base}/login.html"),
        signed_out: Some("a.sign-in".into()),
        content_blocking: false,
        min_interval: min.map(String::from),
    };
    let pm = "div.pm[contenteditable='true']".to_string();
    let spec = |id: &str, site: &str, steps: Vec<Step>| MacroSpec {
        id: id.into(),
        title: "Cross-post to Fixture Notes…".into(),
        detail: String::new(),
        site: site.into(),
        when: When::Published,
        template: recipe::DEFAULT_TEMPLATE.into(),
        tested: "unverified".into(),
        steps,
    };
    let note_steps = |url: String| {
        vec![
            Step::Open { url },
            Step::WaitFor {
                selector: pm.clone(),
                text: None,
                empty: false,
                absent: false,
                timeout: Some("10s".into()),
            },
            Step::Focus {
                selector: pm.clone(),
            },
            Step::Insert {
                selector: pm.clone(),
            },
            Step::Submit {
                selector: "button".into(),
                text: Some("Post".into()),
            },
            Step::WaitFor {
                selector: pm.clone(),
                text: None,
                empty: true,
                absent: false,
                timeout: Some("10s".into()),
            },
            Step::Done {
                text: "Posted to Fixture Notes".into(),
            },
        ]
    };
    let payload = recipe::expand(
        recipe::DEFAULT_TEMPLATE,
        &recipe::Vars {
            title: "Tide pools".into(),
            excerpt: "Tide pools are small oceans that forget, twice a day, that they \
                      belong to a larger one."
                .into(),
            permalink: "https://blyg.example.com/f/tide-pools/".into(),
        },
    );
    let notes = MacroRun::new(
        "demo",
        site("fixture", Some("60s")),
        spec(
            "fixture-note",
            "fixture",
            note_steps(format!("{base}/notes.html")),
        ),
        payload.clone(),
    );
    let login = MacroRun::new(
        "demo",
        site("fixture-login", None),
        spec(
            "fixture-password",
            "fixture-login",
            vec![
                Step::Open {
                    url: format!("{base}/login.html"),
                },
                Step::Insert {
                    selector: "input[type=password]".into(),
                },
                Step::Submit {
                    selector: "#sign-in".into(),
                    text: None,
                },
                Step::Done {
                    text: "never".into(),
                },
            ],
        ),
        payload.clone(),
    );
    let away = MacroRun::new(
        "demo",
        site("fixture-away", None),
        spec(
            "fixture-away",
            "fixture-away",
            note_steps(format!("{base}/notes.html?away=1")),
        ),
        payload.clone(),
    );
    // The bundled cross-post's shape: a home feed whose prompt opens a modal
    // composer (home.html). Then /notes, which the fixture server
    // redirects to home.html: the page the pane is already on.
    let feed_steps = |url: String| {
        let prompt = "[role='button'], button, div".to_string();
        let editor = blyg_ext_crosspost::EDITOR.to_string();
        let wait = |selector: String, text: Option<&str>, absent: bool| Step::WaitFor {
            selector,
            text: text.map(String::from),
            empty: false,
            absent,
            timeout: Some("10s".into()),
        };
        let mind = "What's on your mind?";
        vec![
            Step::Open { url },
            wait(prompt.clone(), Some(mind), false),
            Step::Click {
                selector: prompt,
                text: Some(mind.into()),
            },
            wait(editor.clone(), None, false),
            Step::Focus {
                selector: editor.clone(),
            },
            Step::Insert {
                selector: editor.clone(),
            },
            Step::Submit {
                selector: "[role='dialog'] button, [aria-modal='true'] button".into(),
                text: Some("Post".into()),
            },
            wait(editor, None, true),
            Step::Done {
                text: "Posted to Fixture Notes".into(),
            },
        ]
    };
    let feed = MacroRun::new(
        "demo",
        site("fixture-feed", None),
        spec(
            "fixture-feed",
            "fixture-feed",
            feed_steps(format!("{base}/home.html")),
        ),
        payload.clone(),
    );
    let redirect = MacroRun::new(
        "demo",
        site("fixture-redirect", None),
        spec(
            "fixture-redirect",
            "fixture-redirect",
            feed_steps(format!("{base}/notes")),
        ),
        payload,
    );
    vec![
        ("signed-out", notes.clone()),
        ("sign-in", notes.clone()),
        ("post", notes.clone()),
        ("too-soon", notes),
        ("feed", feed),
        ("redirect", redirect),
        ("refused", login),
        ("off-origin", away),
    ]
}
