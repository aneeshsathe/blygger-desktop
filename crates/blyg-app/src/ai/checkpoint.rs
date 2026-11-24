//! --- studio 0.71 --- ⌘G on the blyg (`Backend::generate_on_blyg`), and its
//! checkpoint: studio 0.70 reads every link the author placed before it
//! generates, and when some can't be read it stops and saves nothing
//! (`blyg_core::generate::Checkpoint`). This sheet asks, as the studio's
//! editor does: "n of m links in this TK could not be read. Generate
//! anyway?", the failures (link and why) under Details (open already when
//! there are a few; `d` shows or hides it). ⏎ generates without them
//! (`proceed: true`); esc leaves the draft as it was and says what to do.
//!
//! A child of `app::ai_view` (declared there), so it works on `MainView`.

use blyg_ai::{AiError, ProviderKind};
use blyg_core::generate::Checkpoint;
use blyg_core::{CoreError, LocalId};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::app::MainView;
use crate::app::sheet::{Kit, SheetView, Shown};

/// The blyg's own generation as a provider for `ai_spawn`: the work calls
/// `Backend::generate_on_blyg`, never this.
pub(crate) struct OnBlyg;

impl blyg_ai::Provider for OnBlyg {
    fn kind(&self) -> ProviderKind {
        ProviderKind::BlygServer
    }

    fn generate(
        &self,
        _: blyg_ai::GenRequest,
        _: &mut dyn FnMut(&str),
    ) -> blyg_ai::Result<blyg_ai::GenResult> {
        Err(AiError::Provider(
            "your blyg generates through Backend::generate_on_blyg".into(),
        ))
    }
}

/// The sheet: the item and scope ⌘G was for, and what the blyg answered.
pub(crate) struct TkCheckpointSheet {
    pub(crate) item: LocalId,
    pub(crate) index: usize,
    /// The provider ⌘G picked (the blyg's own), to go on with.
    pub(crate) kind: ProviderKind,
    pub(crate) checkpoint: Checkpoint,
    /// Details is open.
    pub(crate) details: bool,
    pub(crate) focus: FocusHandle,
}

/// Details opens by itself up to this many failures.
pub(crate) const DETAILS_OPEN_UP_TO: usize = 3;

/// The studio's question.
pub(crate) fn heading(c: &Checkpoint) -> String {
    format!(
        "{} of {} {} in this TK could not be read. Generate anyway?",
        c.failed.len(),
        c.total,
        if c.total == 1 { "link" } else { "links" }
    )
}

/// Under the question: what generating anyway means.
pub(crate) fn hint(c: &Checkpoint) -> String {
    let rest = match (c.read > 0, c.failed.len()) {
        (true, 1) => "the model sees only the address of the other and is told not to describe it."
            .to_string(),
        (true, n) => format!(
            "the model sees only the addresses of the other {n} and is told not to describe them."
        ),
        (false, 1) => {
            "The model sees only the link's address and is told not to describe it.".to_string()
        }
        (false, n) => format!(
            "The model sees only the addresses of the {n} links and is told not to describe them."
        ),
    };
    match c.read {
        0 => rest,
        1 => format!("Burrow will draw on the 1 readable link; {rest}"),
        n => format!("Burrow will draw on the {n} readable links; {rest}"),
    }
}

/// After esc: what happened, and what to do about the links.
pub(crate) fn cancelled(c: &Checkpoint) -> (&'static str, String) {
    let what = if c.failed.len() == 1 {
        "the link that couldn't be read"
    } else {
        "the links that couldn't be read"
    };
    (
        "Nothing was generated",
        crate::keymap::hint_owned(format!("Remove or replace {what}, then ⌘G again")),
    )
}

/// The toast's second line once the blyg has generated.
pub(crate) const DONE_ON_BLYG: &str = "Your blyg noted what it read";

/// ⏎ while another AI job runs: the sheet stays, and this says why.
pub(crate) const BUSY: (&str, crate::platform_text::Text) = (
    "Another AI job is still running",
    crate::platform_text::ui_text("When it's done, press ⏎ again"),
);

/// One failure under Details.
pub(crate) fn failure_line(link: &str, reason: &str) -> String {
    format!("{link}: {reason}")
}

/// A `generate_on_blyg` error in the AI helpers' words.
pub(crate) fn on_blyg_error(e: CoreError) -> AiError {
    match e {
        CoreError::NotSynced => AiError::Provider(
            "your blyg doesn't have this version yet; wait for it to sync, then try again".into(),
        ),
        CoreError::NotFound => AiError::Provider(
            "item not found on the server (or the server has no /generate)".into(),
        ),
        CoreError::Unauthorized => AiError::Unauthorized {
            provider: "your blyg".into(),
        },
        CoreError::Offline => AiError::Provider("couldn't reach your blyg".into()),
        CoreError::Rejected {
            status, message, ..
        } => {
            if message.contains("declined") {
                AiError::Refused(message)
            } else {
                AiError::Provider(format!("blyg server: {message} ({status})"))
            }
        }
        e => AiError::Provider(format!("blyg server: {e}")),
    }
}

impl MainView {
    /// ⌘G stopped at a checkpoint: ask.
    pub(super) fn tk_checkpoint_open(
        &mut self,
        item: LocalId,
        index: usize,
        kind: ProviderKind,
        checkpoint: Checkpoint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.sheet.open(TkCheckpointSheet {
            item,
            index,
            kind,
            details: checkpoint.failed.len() <= DETAILS_OPEN_UP_TO,
            checkpoint,
            focus,
        });
        cx.notify();
    }

    /// ⏎: generate without the links that couldn't be read.
    /// While another AI job runs, the sheet stays and a toast says why.
    pub(crate) fn tk_checkpoint_generate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.sheet.is::<TkCheckpointSheet>() {
            return;
        }
        if self.ai.job.is_some() {
            self.show_toast(BUSY.0, Some(BUSY.1.get().into()), cx);
            return;
        }
        let Some(s) = self.sheet.take::<TkCheckpointSheet>() else {
            return;
        };
        self.focus_after_sheet(window, cx);
        cx.notify();
        self.ai_on_blyg(s.kind, s.item, s.index, true, window, cx);
    }

    /// esc: nothing generated; say what to do about the links.
    pub(crate) fn tk_checkpoint_cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.sheet.get::<TkCheckpointSheet>() else {
            return;
        };
        let (text, sub) = cancelled(&s.checkpoint);
        self.close_sheet(window, cx);
        self.show_toast(text, Some(sub.into()), cx);
    }

    /// Details: show or hide the links and why.
    pub(crate) fn tk_checkpoint_details(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.sheet.get_mut::<TkCheckpointSheet>() {
            s.details = !s.details;
            cx.notify();
        }
    }
}

impl MainView {
    /// What each generated scope of the open post read (`crate::ai::read`),
    /// from the provenance tracked for it: Burrow's own generations and,
    /// since studio 0.70, the blyg's. Empty when nothing was read.
    pub(crate) fn tk_read(&self, cx: &App) -> Vec<(usize, Vec<crate::ai::read::Entry>)> {
        let Some(item) = self.current.as_ref() else {
            return Vec::new();
        };
        let Some((keyed, scopes)) = self.backend.tracked_tk_provenance(&item.local_id) else {
            return Vec::new();
        };
        let text = self.editor.read(cx).value().to_string();
        let scopes = if keyed == text {
            scopes
        } else {
            blyg_core::tk::remap(&keyed, &scopes, &text).unwrap_or_default()
        };
        let names = crate::ai::read::Names::new(&*self.backend, None);
        crate::ai::read::scopes(&scopes, &|s| names.get(s), true)
    }

    /// A post's title (or "a post") in the list: opens it, your own here
    /// and another blyg's on its page.
    fn tk_read_open(
        &mut self,
        open: crate::ai::read::Open,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match open {
            crate::ai::read::Open::Own(id) => {
                self.close_sheet(window, cx);
                self.open(&id, window, cx);
            }
            crate::ai::read::Open::Web(url) => self.open_link(url, window, cx),
        }
    }

    /// "What generation read": each scope, its posts and pages, and which
    /// tell another blyg. `None` when nothing was read. The entries scroll
    /// past [`READ_LIST_MAX_H`], so a sheet holding them keeps its keys on
    /// screen.
    pub(crate) fn render_tk_read(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use crate::ai::read;
        let (scopes, more) = read::capped(self.tk_read(cx));
        if scopes.is_empty() {
            return None;
        }
        let p = self.palette;
        let link = |id: String, label: String, open: read::Open, cx: &mut Context<Self>| {
            div()
                .id(SharedString::from(id))
                .text_color(p.accent_text())
                .cursor_pointer()
                .hover(|s| s.underline())
                .child(label)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.tk_read_open(open.clone(), window, cx)
                }))
        };
        let mut list = div()
            .id("tk-read-list")
            .debug_selector(|| "tk-read-list".into())
            .max_h(px(READ_LIST_MAX_H))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(2.));
        for (scope, entries) in scopes {
            list = list.child(div().text_color(p.muted).child(read::scope_label(scope)));
            for (i, e) in entries.into_iter().enumerate() {
                let id = format!("tk-read-{scope}-{i}");
                // The words, with the post's title (or "a post") as its link.
                let words: Vec<AnyElement> = match &e.link {
                    Some((label, open)) if e.what.contains(label.as_str()) => {
                        let (before, after) = e.what.split_once(label.as_str()).unwrap_or_default();
                        let mut out = Vec::new();
                        if !before.is_empty() {
                            out.push(
                                div()
                                    .child(before.trim_end().to_string())
                                    .into_any_element(),
                            );
                        }
                        out.push(
                            link(
                                format!("tk-read-post-{scope}-{i}"),
                                label.clone(),
                                open.clone(),
                                cx,
                            )
                            .into_any_element(),
                        );
                        if !after.is_empty() {
                            out.push(
                                div()
                                    .child(after.trim_start().to_string())
                                    .into_any_element(),
                            );
                        }
                        out
                    }
                    _ => vec![div().child(e.what.clone()).into_any_element()],
                };
                list = list.child(
                    div()
                        .debug_selector(move || id.clone())
                        .pl(px(10.))
                        .flex()
                        .flex_wrap()
                        .gap(px(6.))
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(px(4.))
                                .text_color(p.ink)
                                .children(words),
                        )
                        .when_some(e.url.clone(), |d, u| {
                            d.child(link(
                                format!("tk-read-url-{scope}-{i}"),
                                u.clone(),
                                read::Open::Web(u),
                                cx,
                            ))
                        })
                        .when_some(read::mark(&e), |d, m| {
                            d.child(div().text_color(p.warn_text()).child(format!("· {m}")))
                        }),
                );
            }
        }
        if more > 0 {
            list = list.child(
                div()
                    .debug_selector(|| "tk-read-more".into())
                    .pl(px(10.))
                    .text_color(p.muted)
                    .child(read::more(more)),
            );
        }
        Some(
            div()
                .id("tk-read")
                .debug_selector(|| "tk-read".into())
                .mb(px(8.))
                .flex()
                .flex_col()
                .gap(px(2.))
                .text_size(px(12.))
                .line_height(relative(1.4))
                .child(
                    div()
                        .text_color(p.muted)
                        .font_weight(FontWeight::MEDIUM)
                        .child(read::HEADING),
                )
                .child(
                    div()
                        .debug_selector(|| "tk-read-tells-no-one".into())
                        .text_color(p.muted)
                        .child(read::TELLS_NO_ONE),
                )
                .child(list)
                .into_any_element(),
        )
    }
}

/// The most height the list of what generation read takes (as the
/// preview's pane under the page): longer lists scroll.
pub(crate) const READ_LIST_MAX_H: f32 = 160.;

/// The demo's gap: one link the fake blyg can't read (a `.invalid` host)
/// and one it can.
pub(crate) const DEMO_GAP: &str = "[TK]one line on the forecast at https://forecast.invalid/harbour \
                        and the tides at https://tides.example.org/harbour[/TK]";
/// The same, with only the readable link.
const DEMO_GAP_READ: &str = "[TK]one line on the tides at https://tides.example.org/harbour[/TK]";

impl MainView {
    /// `BLYGGER_DEMO=ai-checkpoint…` / `ai-tk-read…`: the sample draft with
    /// a gap (step 0), ⌘G on the blyg once it has synced (step 2), then the
    /// next thing the scene shows when the answer is in.
    pub(super) fn checkpoint_demo(
        &mut self,
        scene: crate::scenes::Scene,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::scenes::Scene;
        let draft = LocalId("01J9QK3".into());
        match n {
            0 => {
                if scene == Scene::AiTkReadPreview {
                    self.studio_set_view(crate::app::studio::ViewMode::Split, cx);
                }
                self.open(&draft, window, cx);
                let old = self.editor.read(cx).value().to_string();
                let gap = if matches!(scene, Scene::AiTkRead | Scene::AiTkReadPreview) {
                    DEMO_GAP_READ
                } else {
                    DEMO_GAP
                };
                let new = format!("{old} {gap}");
                let caret = new.len() - 5;
                self.splice_editor(&old, &new, Some(caret), window, cx);
            }
            2 => {
                self.ai_on_blyg(ProviderKind::BlygServer, draft, 0, false, window, cx);
                // When the answer is in: open Details, or the publish sheet.
                cx.spawn_in(window, async move |this, cx| {
                    for _ in 0..50 {
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(100))
                            .await;
                        let done = this.update_in(cx, |v, window, cx| {
                            if v.ai.job.is_some() {
                                return false;
                            }
                            match scene {
                                Scene::AiCheckpointDetails => {
                                    if let Some(s) = v.sheet.get_mut::<TkCheckpointSheet>() {
                                        s.details = true;
                                        cx.notify();
                                    }
                                }
                                Scene::AiTkRead => v.publish(window, cx),
                                _ => {}
                            }
                            true
                        });
                        if !matches!(done, Ok(false)) {
                            break;
                        }
                    }
                })
                .detach();
            }
            _ => {}
        }
    }
}

impl SheetView for TkCheckpointSheet {
    fn render(&self, _: &MainView, kit: &Kit, cx: &mut Context<MainView>) -> Shown {
        let p = kit.p;
        let c = &self.checkpoint;
        let failures = self.details.then(|| {
            div()
                .id("tk-checkpoint-failed")
                .debug_selector(|| "tk-checkpoint-failed".into())
                .mt(px(4.))
                .pl(px(14.))
                .flex()
                .flex_col()
                .gap(px(2.))
                .text_size(px(12.))
                .children(c.failed.iter().enumerate().map(|(i, f)| {
                    div()
                        .debug_selector(move || format!("tk-checkpoint-failed-{i}"))
                        .child(failure_line(&f.link, &f.reason))
                }))
        });
        Shown::from((
            480.,
            div()
                .id("tk-checkpoint")
                .debug_selector(|| "tk-checkpoint".into())
                .track_focus(&self.focus)
                .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                    match ev.keystroke.key.as_str() {
                        "enter" => this.tk_checkpoint_generate(window, cx),
                        "escape" => this.tk_checkpoint_cancel(window, cx),
                        "d" if !ev.keystroke.modifiers.modified() => this.tk_checkpoint_details(cx),
                        _ => return,
                    }
                    cx.stop_propagation();
                }))
                .child(kit.heading(heading(c)))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(p.muted)
                        .line_height(relative(1.45))
                        .child(hint(c)),
                )
                .child(
                    div()
                        .id("tk-checkpoint-details")
                        .debug_selector(|| "tk-checkpoint-details".into())
                        .mt(px(8.))
                        .cursor_pointer()
                        .text_size(px(12.))
                        .text_color(p.ink)
                        .hover(|s| s.text_color(p.accent_text()))
                        .child(if self.details {
                            "▾ Details"
                        } else {
                            "▸ Details"
                        })
                        .on_click(cx.listener(|this, _, _, cx| this.tk_checkpoint_details(cx))),
                )
                .children(failures)
                .child(
                    kit.keys_row(vec![])
                        .child(crate::platform::key_button(
                            kit.key_hint("⏎", "generate anyway")
                                .debug_selector(|| "tk-checkpoint-generate".into()),
                            "tk-checkpoint-generate",
                            cx.listener(|this, _, window, cx| {
                                this.tk_checkpoint_generate(window, cx)
                            }),
                        ))
                        .child(crate::platform::key_button(
                            kit.key_hint("d", "details")
                                .debug_selector(|| "tk-checkpoint-details-key".into()),
                            "tk-checkpoint-details-key",
                            cx.listener(|this, _, _, cx| this.tk_checkpoint_details(cx)),
                        ))
                        .child(crate::platform::key_button(
                            kit.key_hint("esc", "cancel")
                                .debug_selector(|| "tk-checkpoint-cancel".into()),
                            "tk-checkpoint-cancel",
                            cx.listener(|this, _, window, cx| {
                                this.tk_checkpoint_cancel(window, cx)
                            }),
                        )),
                )
                .into_any_element(),
        ))
    }
}

#[cfg(test)]
#[path = "checkpoint_tests.rs"]
mod checkpoint_tests;
