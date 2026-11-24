//! --- agentic writer --- Burrow's own AI writes the blyg's Agentic feed
//! edition (`blyg_core::agentic_write`; the prompt is
//! `blyg_ai::prompts::agentic_edition_prompt`). The studio has no assistant
//! that writes one yet, so without this the tab says "No edition yet".
//!
//! - ⌥⌘E (Blyg › Write an Agentic Feed Edition Now, the palette, or the
//!   line above the tab) writes one now.
//! - `agentic-writer = after-sync` writes one in the background after a
//!   reading change, when `agentic-writer-gap` hours have passed since the
//!   last edition and posts arrived since; looked at no more often than
//!   [`CHECK_EVERY`].
//! - The provider is `agentic-writer-provider` (else the writing default),
//!   the model `agentic-writer-model` (else that provider's).
//! - [`run`] is the whole job, off the UI thread: ask the blyg whether this
//!   sign-in may write (`putAgentic`), read the edition, pick the posts, ask
//!   the model once, check its reply, send it. The tab says "Writing an
//!   edition with …" meanwhile, then shows the edition; a failure is said
//!   calmly above the tab, with Try again.
//!
//! What's sent where: the titles and opening words of up to 60 posts, and
//! the blyg's Agentic feed prompt, to the chosen provider; the edition to
//! the blyg. Settings › Agentic feed and `site/src/ai.md` say so.

use std::time::{Duration, Instant};

use blyg_ai::{CancelFlag, Provider, ProviderKind};
use blyg_core::Backend;
use blyg_core::agentic::{DEFAULT_FEED, Edition};
use blyg_core::agentic_write::{self, AgentInput, Sent};
use blyg_core::config::Change;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::app::MainView;

gpui_kit::actions!(
    blygger,
    [
        /// Burrow's AI writes the Agentic feed's edition now.
        WriteAgenticEdition
    ]
);

/// The fewest minutes between two looks after a reading change.
pub const CHECK_EVERY: Duration = Duration::from_secs(10 * 60);

/// The most the model may write.
const MAX_TOKENS: u32 = 8_000;

/// Who the edition says wrote it.
pub fn agent_name(kind: ProviderKind) -> String {
    format!("Burrow · {}", kind.label())
}

/// What the tab says while it writes.
pub fn writing_words(provider: &str) -> String {
    format!("Writing an edition with {provider}…")
}

/// What the tab says when it couldn't.
pub fn failed_words(why: &str) -> String {
    format!("Couldn't write an edition: {why}")
}

/// One run's settings.
#[derive(Debug, Clone)]
pub struct Job {
    /// After a sync (the gap and "anything new" apply), not asked for.
    pub auto: bool,
    pub now_ms: i64,
    pub gap_ms: i64,
    /// `None`: the provider's own model.
    pub model: Option<String>,
    /// [`agent_name`].
    pub agent: String,
}

/// What a run came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Written(Edition),
    /// After a sync: not due, or nothing new.
    Skipped,
    /// Asked for, but there's nothing unread or new to write about.
    Nothing,
    /// The blyg doesn't let this sign-in write an edition.
    NotOffered,
    Failed(String),
}

/// The whole job (see the module's notes). Blocking: run it off the UI
/// thread.
pub fn run(
    backend: &dyn Backend,
    provider: &dyn Provider,
    job: &Job,
    cancel: &CancelFlag,
) -> Outcome {
    match backend.agentic_writable() {
        Ok(true) => {}
        Ok(false) => return Outcome::NotOffered,
        Err(e) => return Outcome::Failed(e.to_string()),
    }
    let current = match backend.agentic_edition(DEFAULT_FEED) {
        Ok(Some(e)) => e,
        Ok(None) => return Outcome::NotOffered,
        Err(e) => return Outcome::Failed(e.to_string()),
    };
    let since = agentic_write::last_written(&current);
    if job.auto && !agentic_write::due(since.as_deref(), job.now_ms, job.gap_ms) {
        return Outcome::Skipped;
    }
    let given = agentic_write::candidates(&backend.reading(), since.as_deref());
    if job.auto && !agentic_write::has_new(&given, since.as_deref()) {
        return Outcome::Skipped;
    }
    if given.is_empty() {
        return Outcome::Nothing;
    }
    let brief = backend
        .server_ai_settings()
        .ok()
        .and_then(|s| s.feed_prompt);
    let prompt = blyg_ai::prompts::agentic_edition_prompt(brief.as_deref(), &given);
    let req = blyg_ai::GenRequest {
        max_tokens: Some(MAX_TOKENS),
        ..prompt.request(cancel).with_model(job.model.clone())
    };
    let out = match provider.generate(req, &mut |_| {}) {
        Ok(o) => o,
        Err(e) => {
            let (head, _) =
                crate::ai::failure_toast(&crate::ai::Failure::Ai(e), Some(provider.kind()));
            return Outcome::Failed(head);
        }
    };
    let model = Some(out.model.trim().to_string())
        .filter(|m| !m.is_empty())
        .or_else(|| job.model.clone());
    let agent = AgentInput {
        name: job.agent.clone(),
        model,
    };
    let input = match agentic_write::from_reply(
        &out.text,
        &given,
        agent,
        &crate::ai::generate::now_iso(),
    ) {
        Ok(i) => i,
        Err(e) => return Outcome::Failed(format!("{e}; nothing was sent")),
    };
    match agentic_write::send(backend, DEFAULT_FEED, &input) {
        Ok(Sent::Written(e)) => Outcome::Written(e),
        Ok(Sent::NotOffered) => Outcome::NotOffered,
        Ok(Sent::Conflicted) => Outcome::Failed(
            "the edition changed on your blyg while Burrow wrote it; nothing was sent".into(),
        ),
        Err(e) => Outcome::Failed(e.to_string()),
    }
}

/// What the writer is doing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum WriterState {
    #[default]
    Idle,
    Writing {
        provider: String,
    },
    /// The last run's trouble, in words (shown with Try again).
    Failed(String),
}

/// The writer's state on the reading screen.
#[derive(Default)]
pub struct Writer {
    state: WriterState,
    task: Option<Task<()>>,
    /// When a reading change last looked whether one is due.
    looked: Option<Instant>,
}

impl Writer {
    #[cfg(test)]
    pub fn state(&self) -> &WriterState {
        &self.state
    }
}

/// The provider for the edition: `agentic-writer-provider`, else the
/// writing default.
fn pick(cx: &mut App) -> Result<crate::ai::Picked, String> {
    let chosen = crate::settings::get(cx)
        .store
        .config()
        .agentic_writer_provider()
        .and_then(ProviderKind::from_config_name);
    let tested = cx
        .try_global::<crate::ai::AiGlobal>()
        .is_some_and(|g| g.test_provider.is_some());
    let picked = match chosen {
        Some(kind) if !tested => crate::ai::with_accounts(cx, |a| {
            a.pick(Some(kind))
                .map(|p| crate::ai::Picked {
                    kind: p.kind(),
                    provider: std::sync::Arc::from(p),
                })
                .map_err(|e| (e, Some(kind)))
        }),
        _ => crate::ai::pick(cx),
    };
    picked.map_err(|(e, kind)| crate::ai::failure_toast(&crate::ai::Failure::Ai(e), kind).0)
}

impl MainView {
    pub(super) fn agentic_writer_actions(
        &self,
        d: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        d.on_action(cx.listener(|this, _: &WriteAgenticEdition, _, cx| this.agentic_write_now(cx)))
    }

    /// ⌥⌘E: write an edition now, whatever the gap.
    pub(crate) fn agentic_write_now(&mut self, cx: &mut Context<Self>) {
        self.agentic_write(false, cx);
    }

    /// Hook: a reading change (a sync). With `agentic-writer = after-sync`,
    /// look whether an edition is due, at most every [`CHECK_EVERY`].
    pub(super) fn agentic_writer_after_reading_changed(&mut self, cx: &mut Context<Self>) {
        let on = cx
            .try_global::<crate::settings::AppConfig>()
            .is_some_and(|a| a.store.config().agentic_writer());
        let w = &self.reading.agentic_writer;
        if !on || w.task.is_some() || w.looked.is_some_and(|t| t.elapsed() < CHECK_EVERY) {
            return;
        }
        self.reading.agentic_writer.looked = Some(Instant::now());
        self.agentic_write(true, cx);
    }

    fn agentic_write(&mut self, auto: bool, cx: &mut Context<Self>) {
        if self.reading.agentic_writer.task.is_some() {
            return;
        }
        let picked = match pick(cx) {
            Ok(p) => p,
            Err(why) => {
                if !auto {
                    self.reading.agentic_writer.state = WriterState::Failed(why);
                    cx.notify();
                }
                return;
            }
        };
        let config = crate::settings::get(cx).store.config();
        let job = Job {
            auto,
            now_ms: chrono::Utc::now().timestamp_millis(),
            gap_ms: (f64::from(config.agentic_writer_gap_hours()) * 3_600_000.0) as i64,
            model: config.agentic_writer_model().map(str::to_string),
            agent: agent_name(picked.kind),
        };
        if !auto {
            self.reading.agentic_writer.state = WriterState::Writing {
                provider: picked.kind.label().to_string(),
            };
        }
        let backend = self.backend.clone();
        let cancel = CancelFlag::new();
        let limit = crate::ai::timeout(cx);
        let stop = cancel.clone();
        let label = picked.kind.label().to_string();
        let work = cx.background_spawn(async move {
            run(backend.as_ref(), picked.provider.as_ref(), &job, &cancel)
        });
        // The app's limit: the provider stops when the flag is set.
        let timer = cx.background_executor().timer(limit);
        let ticking = stop.clone();
        cx.background_spawn(async move {
            timer.await;
            ticking.cancel();
        })
        .detach();
        self.reading.agentic_writer.task = Some(cx.spawn(async move |this, cx| {
            let mut out = work.await;
            if stop.is_cancelled() && !matches!(out, Outcome::Written(_)) {
                out = Outcome::Failed(format!(
                    "{label} took longer than {} s; nothing was sent",
                    limit.as_secs()
                ));
            }
            stop.cancel();
            let _ = this.update(cx, |v, cx| v.agentic_written(out, auto, cx));
        }));
        cx.notify();
    }

    fn agentic_written(&mut self, out: Outcome, auto: bool, cx: &mut Context<Self>) {
        self.reading.agentic_writer.task = None;
        self.reading.agentic_writer.state = match out {
            Outcome::Written(e) => {
                let r = &mut self.reading;
                r.agentic.error = None;
                r.agentic.set_edition(Some(e));
                r.agentic.refresh_cards(&r.rows);
                r.refilter();
                WriterState::Idle
            }
            Outcome::Skipped => WriterState::Idle,
            _ if auto => WriterState::Idle,
            Outcome::Nothing => {
                WriterState::Failed("nothing unread or new to write about; nothing was sent".into())
            }
            Outcome::NotOffered => {
                WriterState::Failed("your blyg doesn't let this sign-in write editions".into())
            }
            Outcome::Failed(why) => WriterState::Failed(why),
        };
        cx.notify();
    }

    /// The Agentic feed tab: the writer's line above the edition (or above
    /// what the tab is for).
    pub(super) fn render_agentic_page(&self, wide: bool, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .children(self.render_agentic_writer_line(cx))
            .child(self.render_agentic_edition(wide, cx))
            .into_any_element()
    }

    fn render_agentic_writer_line(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.reading.agentic.unoffered {
            return None;
        }
        let p = self.palette.on_page();
        let (words, action): (String, Option<&'static str>) =
            match &self.reading.agentic_writer.state {
                WriterState::Writing { provider } => (writing_words(provider), None),
                WriterState::Failed(why) => (failed_words(why), Some("Try again")),
                WriterState::Idle => (
                    crate::keymap::hint("Burrow's AI can write this edition · ⌥⌘E").to_string(),
                    Some("Write an edition now"),
                ),
            };
        let id = match self.reading.agentic_writer.state {
            WriterState::Failed(_) => "agentic-writer-retry",
            _ => "agentic-writer-now",
        };
        Some(
            div()
                .id("agentic-writer-line")
                .debug_selector(|| "agentic-writer-line".into())
                .w_full()
                .min_w_0()
                .px(px(14.))
                .pt(px(8.))
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(8.))
                .font_family(self.chrome())
                .text_size(px(11.5))
                .text_color(p.muted)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .line_height(relative(1.4))
                        .child(words),
                )
                .when_some(action, |d, label| {
                    d.child(
                        div()
                            .id(id)
                            .debug_selector(move || id.into())
                            .flex_none()
                            .px(px(8.))
                            .py(px(2.))
                            .rounded(px(4.))
                            .border_1()
                            .border_color(p.line)
                            .text_color(p.accent)
                            .cursor_pointer()
                            .hover(|d| d.bg(p.sel))
                            .on_click(cx.listener(|this, _, _, cx| this.agentic_write_now(cx)))
                            .child(label),
                    )
                })
                .into_any_element(),
        )
    }

    /// Settings › Agentic feed: write after a sync, with which provider,
    /// how often, and what that sends.
    pub(crate) fn render_agentic_writer_setting(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let theme = &*self.theme;
        let (on, provider, gap, enabled) = match cx.try_global::<crate::settings::AppConfig>() {
            Some(a) => {
                let c = a.store.config();
                (
                    c.agentic_writer(),
                    c.agentic_writer_provider().map(str::to_string),
                    c.agentic_writer_gap_hours(),
                    c.ai_enabled(),
                )
            }
            None => (false, None, 3.0, Vec::new()),
        };
        let chip = |id: SharedString, label: String, sel: bool| {
            div()
                .id(id.clone())
                .debug_selector(move || id.to_string())
                .px(px(10.))
                .py(px(3.))
                .map(|d| crate::theme_ext::chip(d, theme, sel))
                .cursor_pointer()
                .hover(|s| s.border_color(p.accent).bg(p.hover()))
                .child(label)
        };
        let set = |key: &'static str, v: String| {
            cx.listener(move |this: &mut Self, _: &ClickEvent, _, cx| {
                this.set_agentic_writer_setting(key, &v, cx)
            })
        };
        let mut providers = vec![
            chip(
                "agentic-writer-provider-default".into(),
                "Writing default".into(),
                provider.is_none(),
            )
            .on_click(set("agentic-writer-provider", "default".into())),
        ];
        for name in enabled.iter().filter(|n| n.as_str() != "server") {
            let Some(kind) = ProviderKind::from_config_name(name) else {
                continue;
            };
            providers.push(
                chip(
                    format!("agentic-writer-provider-{name}").into(),
                    kind.label().into(),
                    provider.as_deref() == Some(name.as_str()),
                )
                .on_click(set("agentic-writer-provider", name.clone())),
            );
        }
        let gaps = [1.0_f32, 3.0, 6.0, 12.0, 24.0].map(|h| {
            chip(
                format!("agentic-writer-gap-{h}").into(),
                format!("every {h} h at most"),
                (gap - h).abs() < 0.01,
            )
            .on_click(set("agentic-writer-gap", format!("{h}")))
        });
        crate::app::settings_page::row(
            &p,
            "AGENTIC FEED",
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    crate::app::settings_page::chips().child(
                        chip(
                            "agentic-writer-auto".into(),
                            "Write the edition after a sync".into(),
                            on,
                        )
                        .on_click(set(
                            "agentic-writer",
                            if on { "off" } else { "after-sync" }.into(),
                        )),
                    ),
                )
                .child(crate::app::settings_page::chips().children(providers))
                .when(on, |d| {
                    d.child(crate::app::settings_page::chips().children(gaps))
                })
                .child(crate::app::settings_page::note(
                    &p,
                    crate::keymap::hint(
                        "Burrow's own AI writes your blyg's Agentic feed edition: after a sync \
                         that brings new posts, when this is on, and whenever you press ⌥⌘E in \
                         the Agentic feed tab. It sends the titles and opening words of up to 60 \
                         unread or new posts, and your blyg's Agentic feed prompt, to the \
                         provider chosen here, and the edition to your blyg.",
                    ),
                )),
        )
        .into_any_element()
    }

    /// `BLYGGER_DEMO=rd-agentic-unwritten | rd-agentic-writing |
    /// rd-agentic-written` (fake mode): the tab with no edition, then ⌥⌘E
    /// with a stand-in provider (no network, no CLI).
    pub(super) fn agentic_writer_demo(
        &mut self,
        scene: crate::scenes::Scene,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use super::sources_vm::{Smart, Source};
        use crate::scenes::Scene;
        match n {
            0 => {
                // `BLYGGER_SNAPSHOT_WIDTH`: the narrow window the lines must
                // wrap in.
                if let Some(w) = crate::dev_flags::get().snapshot.width {
                    let h = window.viewport_size().height;
                    window.resize(size(px(w), h));
                }
                if let Some(f) = &self.fake {
                    f.agentic_clear();
                }
                let slow = scene == Scene::RdAgenticWriting;
                if cx.has_global::<crate::ai::AiGlobal>() {
                    cx.global_mut::<crate::ai::AiGlobal>().test_provider =
                        Some(std::sync::Arc::new(DemoWriter { slow }));
                }
                self.show_view(super::View::Reading, window, cx);
            }
            1 => {
                self.set_read_mode(super::stream_vm::ReadMode::Reader, window, cx);
                self.select_source(Source::Smart(Smart::Agentic), cx);
            }
            2 if scene != Scene::RdAgenticUnwritten => self.agentic_write_now(cx),
            _ => {}
        }
    }

    fn set_agentic_writer_setting(
        &mut self,
        key: &'static str,
        value: &str,
        cx: &mut Context<Self>,
    ) {
        if let Err(e) = crate::settings::write(&[(key, Change::Set(value.to_string()))], cx) {
            self.show_toast(e, None, cx);
            return;
        }
        self.config.restamp(cx);
        cx.notify();
    }
}

/// The demo scenes' stand-in provider: an edition over the first posts it's
/// given, at once or after a long wait (`slow`, for the "Writing…" line).
struct DemoWriter {
    slow: bool,
}

impl Provider for DemoWriter {
    fn kind(&self) -> ProviderKind {
        ProviderKind::LocalCodex
    }

    fn generate(
        &self,
        req: blyg_ai::GenRequest,
        on_delta: &mut dyn FnMut(&str),
    ) -> blyg_ai::Result<blyg_ai::GenResult> {
        let wait = if self.slow { 600 } else { 1 };
        for _ in 0..wait {
            req.cancel.check()?;
            crate::fake::simulate_latency(Duration::from_millis(100));
        }
        let text = serde_json::json!({
            "title": "Monday, quietly",
            "lede": "A slow week on the water: the harbour's tide tables, a long argument \
                     about what a quiet season is for, and a few short notes.",
            "sections": [
                { "title": "The lead", "kind": "lead", "entries": [
                    { "ref": 1, "headline": "The harbour's quiet season, explained",
                      "summary": "Why the slow months matter more than the busy ones.",
                      "reason": "You read every post from this blyg this month.",
                      "present": "full" }
                ]},
                { "title": "Worth your morning", "kind": "section", "entries": [
                    { "ref": 2, "reason": "New from a blyg you read daily" },
                    { "ref": 3, "reason": "It answers a post you saved" }
                ]},
                { "title": "Briefly", "kind": "briefs", "entries": [
                    { "ref": 4, "present": "title" }, { "ref": 5, "present": "title" }
                ]}
            ]
        })
        .to_string();
        on_delta(&text);
        Ok(blyg_ai::GenResult {
            text,
            model: "demo-model".into(),
            content_md: None,
        })
    }
}
