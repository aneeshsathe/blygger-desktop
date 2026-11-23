//! --- studio 0.71 --- ⌘G on the blyg (`Backend::generate_on_blyg`), and its
//! checkpoint: studio 0.70 reads every link the author placed before it
//! generates, and when some can't be read it stops and saves nothing
//! (`blyg_core::generate::Checkpoint`). This sheet asks, as the studio's
//! editor does: "n of m links in the draft could not be read. Generate
//! anyway?", the failures (link and why) under Details. ⏎ generates without
//! them (`proceed: true`); esc leaves the draft as it was.
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

/// The studio's question.
pub(crate) fn heading(c: &Checkpoint) -> String {
    format!(
        "{} of {} {} in the draft could not be read. Generate anyway?",
        c.failed.len(),
        c.total,
        if c.total == 1 { "link" } else { "links" }
    )
}

/// Under the question: what generating anyway means.
pub(crate) fn hint(c: &Checkpoint) -> String {
    let read = if c.read > 0 {
        format!(
            "The {} that could be read will be drawn on and listed as sources. ",
            c.read
        )
    } else {
        String::new()
    };
    format!("{read}The model sees only the address of the others, so it should not describe them.")
}

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
            checkpoint,
            details: false,
            focus,
        });
        cx.notify();
    }

    /// ⏎: generate without the links that couldn't be read.
    pub(crate) fn tk_checkpoint_generate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.sheet.take::<TkCheckpointSheet>() else {
            return;
        };
        self.focus_after_sheet(window, cx);
        cx.notify();
        if self.ai_busy(cx) {
            return;
        }
        self.ai_on_blyg(s.kind, s.item, s.index, true, window, cx);
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
        // Names only when a post is among the sources (the items are read
        // once then).
        let items = std::cell::OnceCell::new();
        let name_of = |s: &blyg_core::ProvenanceSource| -> Option<String> {
            match &s.origin {
                None => items
                    .get_or_init(|| self.backend.items())
                    .iter()
                    .find(|i| i.server_id.as_ref().is_some_and(|sid| sid.0 == s.id))
                    .map(crate::vm::item_title),
                Some(o) => self
                    .backend
                    .reading_with_id(&blyg_core::RemoteId::from(s.id.as_str()))
                    .into_iter()
                    .find(|r| blyg_core::profile::same_origin(&r.origin, o))
                    .map(|r| crate::app::reading::vm::post_title(&r)),
            }
        };
        crate::ai::read::scopes(&scopes, &name_of)
    }

    /// "What generation read": each scope, its posts and pages, and which
    /// tell another blyg. `None` when nothing was read.
    pub(crate) fn render_tk_read(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use crate::ai::read;
        let scopes = self.tk_read(cx);
        if scopes.is_empty() {
            return None;
        }
        let p = self.palette;
        let mut col = div()
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
            );
        for (scope, entries) in scopes {
            col = col.child(div().text_color(p.muted).child(read::scope_label(scope)));
            for (i, e) in entries.into_iter().enumerate() {
                let id = format!("tk-read-{scope}-{i}");
                let url = e.url.clone();
                col = col.child(
                    div()
                        .debug_selector(move || id.clone())
                        .pl(px(10.))
                        .flex()
                        .flex_wrap()
                        .gap(px(6.))
                        .child(div().text_color(p.ink).child(e.what.clone()))
                        .when_some(url, |d, u| {
                            let open = u.clone();
                            d.child(
                                div()
                                    .id(SharedString::from(format!("tk-read-url-{scope}-{i}")))
                                    .text_color(p.accent)
                                    .cursor_pointer()
                                    .hover(|s| s.underline())
                                    .child(u)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_link(open.clone(), window, cx)
                                    })),
                            )
                        })
                        .child(
                            div()
                                .text_color(if e.tells { p.warn_text() } else { p.muted })
                                .child(format!("· {}", read::mark(&e))),
                        ),
                );
            }
        }
        Some(col.into_any_element())
    }
}

/// The demo's gap: one link the fake blyg can't read (a `.invalid` host)
/// and one it can.
const DEMO_GAP: &str = "[TK]one line on the forecast at https://forecast.invalid/harbour \
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
                                Scene::AiCheckpointDetails => v.tk_checkpoint_details(cx),
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
                        "escape" => this.close_sheet(window, cx),
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
                        .hover(|s| s.text_color(p.accent))
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
                            kit.key_hint("esc", "cancel")
                                .debug_selector(|| "tk-checkpoint-cancel".into()),
                            "tk-checkpoint-cancel",
                            cx.listener(|this, _, window, cx| this.close_sheet(window, cx)),
                        )),
                )
                .into_any_element(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use blyg_core::generate::{Checkpoint, UnreadLink};

    #[test]
    fn the_question_is_the_studios() {
        let c = Checkpoint {
            read: 1,
            total: 2,
            failed: vec![UnreadLink {
                link: "https://gone.invalid/a".into(),
                reason: "HTTP 404".into(),
            }],
        };
        assert_eq!(
            super::heading(&c),
            "1 of 2 links in the draft could not be read. Generate anyway?"
        );
        assert!(super::hint(&c).starts_with("The 1 that could be read will be drawn on"));
        let one = Checkpoint {
            read: 0,
            total: 1,
            ..c
        };
        assert_eq!(
            super::heading(&one),
            "1 of 1 link in the draft could not be read. Generate anyway?"
        );
        assert!(super::hint(&one).starts_with("The model sees only"));
    }
}
