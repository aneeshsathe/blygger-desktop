//! --- access-ai --- Blyg › AI on Your Blyg…: the blyg's own AI settings, as
//! Studio's Settings › AI has them (studio 0.56): a model per function from
//! the studio's model list (`GET /api/ai/models`, with each provider's key
//! status), the house style added to every `[TK]` generated on the blyg,
//! automatic change notes, and the change classifier.
//!
//! These run on the blyg, for the blyg. Burrow's own AI (Settings › AI,
//! `blyg-ai`) runs from this computer and is configured there.
//!
//! Studio 0.69 adds the text helpers model (Catch up's summary on the
//! blyg), the Agentic feed tab's name (`agenticTitle`), and, under the
//! change classifier, judging other blygs' changes (`bgModelImported`, its
//! optional instruction `bgInstruction`, and `bgFlagDisagreement`; Studio
//! has these under Settings › Background, Burrow beside the classifier
//! they need). A row shows only where the studio has its key.
//!
//! Keys: ↑/↓ choose a row, ←/→ change its model (or flip its switch, as
//! Space does), ⌘⏎ saves, esc closes. "other…" types any model id.
//!
//! A child module of `app`; `app.rs` keeps small hooks marked
//! `// --- access-ai ---`.

use blyg_core::access_ai::{AiFunction, AiModels, ServerAiSettings};

use super::*;

gpui_kit::actions!(
    blygger,
    [
        /// Blyg › AI on Your Blyg…
        ShowServerAi
    ]
);

/// The one line that tells this sheet from Settings › AI.
pub const DIFFERENCE: &str = "These models run on your blyg, for things your blyg does: TK filled on the server, change notes, the change classifier. Burrow's own AI (Settings › AI) runs from this computer and is set up there.";

// --- dictionary --- Blyg › Dictionary… (studio 0.69), a sheet beside this one.
#[path = "dictionary.rs"]
pub(crate) mod dictionary;

/// A row of the sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Model(AiFunction),
    AutoNotes,
    CategoryInNote,
    /// `bgModelImported` (studio 0.69).
    JudgeImported,
    /// `bgFlagDisagreement` (studio 0.69).
    FlagDisagreement,
}

/// The rows, in order.
pub const ROWS: [Row; 9] = [
    Row::Model(AiFunction::Tk),
    Row::Model(AiFunction::Changelog),
    Row::AutoNotes,
    Row::Model(AiFunction::Classify),
    Row::CategoryInNote,
    Row::JudgeImported,
    Row::FlagDisagreement,
    Row::Model(AiFunction::Helpers),
    Row::Model(AiFunction::Feed),
];

/// A row's place in [`ROWS`].
fn ix(r: Row) -> usize {
    ROWS.iter().position(|x| *x == r).unwrap_or(0)
}

/// Whether the sheet shows `row`: the studio 0.69 rows only where the studio
/// has their keys, the keep-in-view switch only with an instruction (as
/// Studio's Settings shows them).
pub fn row_shown(row: Row, s: &ServerAiSettings) -> bool {
    match row {
        Row::Model(AiFunction::Helpers) => s.model_helpers.is_some(),
        Row::JudgeImported => s.judging().offered,
        Row::FlagDisagreement => s.judging().flag,
        _ => true,
    }
}

/// The next shown row from `from`, `step` ±1 (stays put at either end).
fn next_shown(from: usize, step: i32, s: &ServerAiSettings) -> usize {
    let mut at = from as i32;
    loop {
        at += step;
        if at < 0 || at >= ROWS.len() as i32 {
            return from;
        }
        if row_shown(ROWS[at as usize], s) {
            return at as usize;
        }
    }
}

pub enum AiLoad {
    Loading,
    Ready,
    /// The studio has no AI settings (older than them).
    Unavailable,
    Failed(String),
}

pub struct ServerAiSheet {
    pub focus: FocusHandle,
    pub style: Entity<TextareaState>,
    pub feed: Entity<TextareaState>,
    /// `agenticTitle`: the Agentic feed tab's name.
    pub agentic_title: Entity<InputState>,
    /// `bgInstruction`: the classifier's routing instruction.
    pub instruction: Entity<TextareaState>,
    /// The typed model id of the row in "other…".
    pub other: Entity<InputState>,
    pub load: AiLoad,
    pub models: AiModels,
    /// As read, then as edited (saved on ⌘⏎).
    pub settings: ServerAiSettings,
    /// As read: a save sends only what differs from it.
    pub loaded: ServerAiSettings,
    pub sel: usize,
    /// The function whose model is being typed.
    pub other_for: Option<AiFunction>,
    pub busy: bool,
    pub error: Option<String>,
}

impl crate::app::sheet::SheetView for ServerAiSheet {
    fn render(
        &self,
        v: &crate::app::MainView,
        _: &crate::app::sheet::Kit,
        cx: &mut Context<crate::app::MainView>,
    ) -> crate::app::sheet::Shown {
        v.render_server_ai_sheet(self, cx).into()
    }

    fn inputs(&self) -> Vec<&Entity<InputState>> {
        self.inputs()
    }

    fn submit(&self) -> Option<crate::app::sheet::Submit> {
        Some(|v, window, cx| v.save_server_ai(window, cx))
    }
}

impl ServerAiSheet {
    pub fn inputs(&self) -> Vec<&Entity<InputState>> {
        vec![&self.other, &self.agentic_title]
    }
}

/// What ←/→ cycle through for a function: none, the listed models, the
/// current value when it's typed in (not listed), and "other…" (`None`).
pub fn choices(models: &AiModels, f: AiFunction, current: &str) -> Vec<Option<String>> {
    let mut out: Vec<Option<String>> = vec![Some(String::new())];
    out.extend(
        models
            .for_function(f)
            .into_iter()
            .map(|m| Some(m.id.clone())),
    );
    if !current.is_empty() && !out.iter().any(|c| c.as_deref() == Some(current)) {
        out.push(Some(current.to_string()));
    }
    out.push(None);
    out
}

/// The next choice after `current` (`step` ±1), wrapping.
pub fn step_choice(models: &AiModels, f: AiFunction, current: &str, step: i32) -> Option<String> {
    let c = choices(models, f, current);
    let at = c
        .iter()
        .position(|x| x.as_deref() == Some(current))
        .unwrap_or(0) as i32;
    let n = c.len() as i32;
    c[((at + step).rem_euclid(n)) as usize].clone()
}

/// A model's words on the sheet.
pub fn model_words(models: &AiModels, id: &str) -> String {
    if id.is_empty() {
        return "none · this never calls a model".into();
    }
    let mut s = models.label_of(id);
    if let Some(m) = models.models.iter().find(|m| m.id == id)
        && let Some(n) = &m.note
    {
        s.push_str(&format!(" — {n}"));
    }
    if let Some(p) = models.unconfigured_provider(id) {
        s.push_str(&format!(" ({}: {})", p.label, p.status()));
    }
    s
}

impl MainView {
    /// Hook: the sheet's action on the main window.
    pub(super) fn server_ai_actions(
        &self,
        d: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        d.on_action(
            cx.listener(|this, _: &ShowServerAi, window, cx| this.open_server_ai(window, cx)),
        )
        .map(|d| self.dictionary_actions(d, cx)) // --- dictionary ---
    }

    /// Blyg › AI on Your Blyg…: open it (or close it).
    pub(crate) fn open_server_ai(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sheet.is::<ServerAiSheet>() {
            return self.close_sheet(window, cx);
        }
        let style = cx.new(|cx| TextareaState::new(window, cx).soft_wrap(true));
        let feed = cx.new(|cx| TextareaState::new(window, cx).soft_wrap(true));
        let agentic_title =
            cx.new(|cx| InputState::new(window, cx).placeholder(blyg_core::agentic::DEFAULT_TITLE));
        let instruction = cx.new(|cx| TextareaState::new(window, cx).soft_wrap(true));
        let other = cx.new(|cx| {
            InputState::new(window, cx).placeholder("a model id, e.g. claude-sonnet-5-5")
        });
        self._subs
            .push(cx.subscribe_in(&other, window, |this, _, ev, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    this.server_ai_commit_other(window, cx);
                }
            }));
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.sheet.open(ServerAiSheet {
            focus,
            style,
            feed,
            agentic_title,
            instruction,
            other,
            load: AiLoad::Loading,
            models: AiModels::default(),
            settings: ServerAiSettings::default(),
            loaded: ServerAiSettings::default(),
            sel: 0,
            other_for: None,
            busy: false,
            error: None,
        });
        cx.notify();
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move {
            let settings = backend.server_ai_settings();
            // No model list (an older studio, or it failed): "other…" still works.
            let models = backend.ai_models().unwrap_or_default();
            (settings, models)
        });
        cx.spawn_in(window, async move |this, cx| {
            let (settings, models) = task.await;
            let _ = this.update_in(cx, |v, window, cx| {
                let Some(s) = v.sheet.get_mut::<ServerAiSheet>() else {
                    return;
                };
                match settings {
                    Ok(set) => {
                        let (style, feed) = (s.style.clone(), s.feed.clone());
                        let (st, fp) = (
                            set.style_prompt.clone().unwrap_or_default(),
                            set.feed_prompt.clone().unwrap_or_default(),
                        );
                        style.update(cx, |e, cx| e.set_value(st, window, cx));
                        feed.update(cx, |e, cx| e.set_value(fp, window, cx));
                        let (title, ins) = (s.agentic_title.clone(), s.instruction.clone());
                        let (t, i) = (
                            set.agentic_title.clone().unwrap_or_default(),
                            set.bg_instruction.clone().unwrap_or_default(),
                        );
                        title.update(cx, |e, cx| e.set_value(t, window, cx));
                        ins.update(cx, |e, cx| e.set_value(i, window, cx));
                        s.loaded = set.clone();
                        s.settings = set;
                        s.models = models;
                        s.load = AiLoad::Ready;
                    }
                    Err(CoreError::NotFound) => s.load = AiLoad::Unavailable,
                    Err(e) => s.load = AiLoad::Failed(e.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn server_ai_sheet(&mut self) -> Option<&mut ServerAiSheet> {
        self.sheet.get_mut::<ServerAiSheet>()
    }

    /// ←/→ (or a click) on a row: the next model, or flip the switch.
    pub(crate) fn server_ai_step(
        &mut self,
        row: usize,
        step: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(s) = self.server_ai_sheet() else {
            return;
        };
        if !matches!(s.load, AiLoad::Ready) || s.busy {
            return;
        }
        s.sel = row;
        match ROWS[row] {
            Row::AutoNotes => {
                if let Some(on) = s.settings.auto_change_notes.as_mut() {
                    *on = !*on;
                }
            }
            Row::CategoryInNote => {
                if let Some(on) = s.settings.change_category_in_note.as_mut() {
                    *on = !*on;
                }
            }
            Row::JudgeImported => {
                if s.settings.judging().can_judge
                    && let Some(on) = s.settings.bg_model_imported.as_mut()
                {
                    *on = !*on;
                }
            }
            Row::FlagDisagreement => {
                let on = s.settings.judging().flag_on;
                s.settings.bg_flag_disagreement = Some(!on);
            }
            Row::Model(f) => {
                let Some(cur) = s.settings.model(f).map(str::to_string) else {
                    return;
                };
                match step_choice(&s.models, f, &cur, step) {
                    Some(next) => {
                        *s.settings.model_mut(f) = Some(next);
                        s.other_for = None;
                    }
                    None => {
                        s.other_for = Some(f);
                        let other = s.other.clone();
                        other.update(cx, |e, cx| {
                            e.set_value(cur, window, cx);
                            e.focus(window, cx)
                        });
                    }
                }
            }
        }
        cx.notify();
    }

    /// ⏎ in "other…": the typed id becomes the model.
    fn server_ai_commit_other(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.server_ai_sheet() else {
            return;
        };
        if let Some(f) = s.other_for.take() {
            let id = s.other.read(cx).value().trim().to_string();
            *s.settings.model_mut(f) = Some(id);
        }
        let focus = s.focus.clone();
        window.focus(&focus, cx);
        cx.notify();
    }

    /// ⌘⏎ / Save: write the AI keys the blyg reported.
    pub(crate) fn save_server_ai(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.server_ai_sheet() else {
            return;
        };
        if !matches!(s.load, AiLoad::Ready) || s.busy {
            return;
        }
        if let Some(f) = s.other_for.take() {
            let id = s.other.read(cx).value().trim().to_string();
            *s.settings.model_mut(f) = Some(id);
        }
        let mut out = s.settings.clone();
        if out.style_prompt.is_some() {
            out.style_prompt = Some(s.style.read(cx).value().to_string());
        }
        if out.feed_prompt.is_some() {
            out.feed_prompt = Some(s.feed.read(cx).value().to_string());
        }
        if out.agentic_title.is_some() {
            out.agentic_title = Some(s.agentic_title.read(cx).value().to_string());
        }
        if out.bg_instruction.is_some() {
            out.bg_instruction = Some(s.instruction.read(cx).value().to_string());
        }
        // Only what was changed here: a change made in Studio since the
        // sheet read them stays.
        let out = out.changes_from(&s.loaded);
        s.busy = true;
        s.error = None;
        cx.notify();
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.save_server_ai_settings(&out) });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |v, window, cx| match r {
                Ok(()) => {
                    v.close_sheet(window, cx);
                    v.agentic_refresh(true, cx); // --- agentic feed --- its name
                    v.show_toast(
                        "AI settings saved",
                        Some("Your blyg uses them from now on".into()),
                        cx,
                    );
                }
                Err(e) => {
                    if let Some(s) = v.server_ai_sheet() {
                        s.busy = false;
                        s.error = Some(format!("Couldn't save: {e}"));
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn server_ai_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.server_ai_sheet() else {
            return;
        };
        let sel = s.sel;
        match ev.keystroke.key.as_str() {
            "escape" if s.other_for.is_some() => {
                s.other_for = None;
                let focus = s.focus.clone();
                window.focus(&focus, cx);
                cx.notify();
            }
            "escape" => self.close_sheet(window, cx),
            "up" => {
                s.sel = next_shown(sel, -1, &s.settings);
                cx.notify();
            }
            "down" => {
                s.sel = next_shown(sel, 1, &s.settings);
                cx.notify();
            }
            "left" => self.server_ai_step(sel, -1, window, cx),
            "right" | "space" => self.server_ai_step(sel, 1, window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    pub(super) fn render_server_ai_sheet(
        &self,
        s: &ServerAiSheet,
        cx: &mut Context<Self>,
    ) -> (f32, AnyElement) {
        let p = self.palette;
        let theme = &*self.theme;
        let kbd = |k: &'static str| {
            crate::theme_ext::kbd(div(), theme)
                .px(px(5.))
                .py(px(1.))
                .min_w(px(18.))
                .flex()
                .justify_center()
                .text_size(px(10.5))
                .child(crate::keymap::hint(k))
        };
        let hint = |k: &'static str, label: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(px(5.))
                .child(kbd(k))
                .child(label)
        };
        let muted = |t: String| {
            div()
                .text_color(p.muted)
                .line_height(relative(1.45))
                .child(t)
        };
        let section = |t: &'static str| {
            div()
                .mt(px(12.))
                .mb(px(2.))
                .text_size(px(10.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.muted)
                .child(t)
        };
        let mut body =
            div()
                .track_focus(&s.focus)
                .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                    this.server_ai_key(ev, window, cx)
                }))
                .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                    cx.stop_propagation();
                    let typing = this
                        .sheet
                        .get::<ServerAiSheet>()
                        .is_some_and(|s| s.other_for.is_some());
                    if typing {
                        if let Some(s) = this.server_ai_sheet() {
                            s.other_for = None;
                            let focus = s.focus.clone();
                            window.focus(&focus, cx);
                        }
                        cx.notify();
                    } else {
                        this.close_sheet(window, cx)
                    }
                }))
                .flex()
                .flex_col()
                .child(
                    div()
                        .mb(px(4.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_size(px(13.))
                        .child("AI on your blyg"),
                )
                .child(muted(crate::keymap::hint(DIFFERENCE).to_string()));
        match &s.load {
            AiLoad::Loading => {
                return (
                    560.,
                    body.child(div().mt(px(12.)).text_color(p.muted).child("Loading…"))
                        .into_any_element(),
                );
            }
            AiLoad::Unavailable => {
                return (
                    560.,
                    body.child(div().mt(px(12.)).child(
                        "Your blyg's studio has no AI settings yet. Update it to use them.",
                    ))
                    .child(div().mt(px(12.)).child(hint("esc", "close")))
                    .into_any_element(),
                );
            }
            AiLoad::Failed(e) => {
                return (
                    560.,
                    body.child(
                        div()
                            .mt(px(12.))
                            .text_color(p.over_text())
                            .child(format!("Couldn't load them: {e}")),
                    )
                    .child(div().mt(px(12.)).child(hint("esc", "close")))
                    .into_any_element(),
                );
            }
            AiLoad::Ready => {}
        }
        // The providers' keys, as the studio reports them.
        if !s.models.providers.is_empty() {
            let line = s
                .models
                .providers
                .iter()
                .map(|pr| {
                    let mark = if pr.configured { "✓" } else { "–" };
                    let what = pr.key_secret.as_deref().or(pr.binding.as_deref());
                    match what {
                        Some(w) if !pr.configured => {
                            format!("{mark} {} ({}: {w})", pr.label, pr.status())
                        }
                        _ => format!("{mark} {} ({})", pr.label, pr.status()),
                    }
                })
                .collect::<Vec<_>>()
                .join("   ");
            body = body.child(
                div()
                    .mt(px(8.))
                    .text_size(px(11.5))
                    .text_color(p.muted)
                    .line_height(relative(1.45))
                    .child(line),
            );
            body = body.child(
                div()
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .child(if s.models.local {
                        "Models come from your studio's models.json and its models.local.json; keys are Worker secrets."
                    } else {
                        "Models come from your studio's models.json; keys are Worker secrets, set with wrangler."
                    }),
            );
        }
        let row = |i: usize, label: &'static str, value: String, enabled: bool| {
            let selected = i == s.sel;
            div()
                .id(SharedString::from(format!("server-ai-row-{i}")))
                .debug_selector(move || format!("server-ai-row-{i}"))
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(8.))
                .py(px(5.))
                .rounded(px(6.))
                .cursor_pointer()
                .when(selected, |d| d.bg(p.sel))
                .hover(|d| d.bg(p.sel))
                .when(!enabled, |d| d.opacity(0.5))
                .child(
                    div()
                        .w(px(150.))
                        .flex_none()
                        .text_color(if selected { p.sel_ink } else { p.ink })
                        .child(label),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(if selected { p.sel_ink } else { p.ink })
                        .child(value),
                )
                .child(
                    div()
                        .text_color(if selected { p.sel_muted } else { p.muted })
                        .child("‹ ›"),
                )
                .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                    let back = ev.modifiers().shift;
                    this.server_ai_step(i, if back { -1 } else { 1 }, window, cx)
                }))
        };
        let model_value = |f: AiFunction| match s.settings.model(f) {
            None => "not on this studio".to_string(),
            Some(_) if s.other_for == Some(f) => "other… (type the id below, ⏎)".to_string(),
            Some(id) => model_words(&s.models, id),
        };
        let switch = |on: Option<bool>| match on {
            None => "not on this studio".to_string(),
            Some(true) => "☑ on".to_string(),
            Some(false) => "☐ off".to_string(),
        };
        let other_box = |f: AiFunction| {
            (s.other_for == Some(f)).then(|| {
                crate::theme_ext::field_box(div(), theme)
                    .mx(px(8.))
                    .mt(px(2.))
                    .px(px(10.))
                    .py(px(6.))
                    .text_size(px(13.))
                    .child(gpui_kit::base::input::Input::new(&s.other))
            })
        };
        let area = |e: &Entity<TextareaState>, h: f32| {
            crate::theme_ext::field_box(div(), theme)
                .mx(px(8.))
                .mt(px(2.))
                .h(px(h))
                .px(px(10.))
                .py(px(6.))
                .text_size(px(13.))
                .child(gpui_kit::base::input::Textarea::new(e))
        };
        let helps = s.settings.helpers();
        let judging = s.settings.judging();
        let mut rows = div()
            .id("server-ai-rows")
            .max_h(px(470.))
            .overflow_y_scroll()
            .flex()
            .flex_col();
        rows = rows
            .child(section("TK GENERATION"))
            .child(row(
                ix(Row::Model(AiFunction::Tk)),
                "Model",
                model_value(AiFunction::Tk),
                true,
            ))
            .children(other_box(AiFunction::Tk))
            .when(s.settings.style_prompt.is_some(), |d| {
                d.child(div().mx(px(8.)).mt(px(4.)).text_size(px(11.5)).text_color(p.muted).child(
                    "House style (optional): added to every [TK] generated on the blyg",
                ))
                .child(area(&s.style, 56.))
            })
            .child(section("CHANGE NOTES"))
            .child(row(
                ix(Row::Model(AiFunction::Changelog)),
                "Model",
                model_value(AiFunction::Changelog),
                true,
            ))
            .children(other_box(AiFunction::Changelog))
            .child(row(
                ix(Row::AutoNotes),
                "Draft automatically",
                switch(s.settings.auto_change_notes),
                helps.draft,
            ))
            .child(div().mx(px(8.)).text_size(px(11.)).text_color(p.muted).child(
                "When a new version has no note, the publish sheet drafts one. You see it, and edit it, before anything is published; a note published exactly as drafted is marked as generated.",
            ))
            .child(section("CHANGE CLASSIFIER"))
            .child(row(
                ix(Row::Model(AiFunction::Classify)),
                "Decision model",
                model_value(AiFunction::Classify),
                true,
            ))
            .children(other_box(AiFunction::Classify))
            .child(row(
                ix(Row::CategoryInNote),
                "Start the note with it",
                switch(s.settings.change_category_in_note),
                helps.classify,
            ))
            .child(div().mx(px(8.)).text_size(px(11.)).text_color(p.muted).child(
                "Off unless you choose a model. It labels each new version typo, minor or major before you publish; you can change or clear the label. Readers see it only as words in your change note.",
            ))
            // --- studio 0.69 --- judging other blygs' changes (Studio:
            // Settings › Background), shown as Studio shows it.
            .when(judging.offered, |d| {
                d.child(row(
                    ix(Row::JudgeImported),
                    "Judge other blygs' changes",
                    switch(s.settings.bg_model_imported),
                    judging.can_judge,
                ))
                .child(div().mx(px(8.)).text_size(px(11.)).text_color(p.muted).child(
                    if judging.can_judge {
                        "One classifier call per new version Background's rules leave open, ten every quarter hour, under your daily AI budget; over it, the rules decide. A typo or minor label sends it to Background. Only the two texts are sent, unless you write an instruction below."
                    } else {
                        "Choose a decision model above first."
                    },
                ))
            })
            .when(judging.instruction, |d| {
                d.child(div().mx(px(8.)).mt(px(4.)).text_size(px(11.5)).text_color(p.muted).child(
                    "Your instruction (optional). While written, each version's note is read too, and a note calling a big change minor keeps the post in view, marked.",
                ))
                .child(area(&s.instruction, 48.))
            })
            .when(judging.flag, |d| {
                d.child(row(
                    ix(Row::FlagDisagreement),
                    "Keep in view if the note understates it",
                    switch(Some(judging.flag_on)),
                    true,
                ))
            })
            .when(row_shown(Row::Model(AiFunction::Helpers), &s.settings), |d| {
                d.child(section("TEXT HELPERS"))
                    .child(row(
                        ix(Row::Model(AiFunction::Helpers)),
                        "Model",
                        model_value(AiFunction::Helpers),
                        true,
                    ))
                    .children(other_box(AiFunction::Helpers))
                    .child(div().mx(px(8.)).text_size(px(11.)).text_color(p.muted).child(
                        "Small writing jobs you and your agents ask your blyg for, such as Catch up's summary. Each is written once for the versions it reads and kept. None uses the change-notes model.",
                    ))
            })
            .child(section("AGENTIC FEED"))
            .when(s.settings.agentic_title.is_some(), |d| {
                d.child(div().mx(px(8.)).mt(px(4.)).text_size(px(11.5)).text_color(p.muted).child(
                    "Tab name: what you call it in Reading, and the heading of each edition",
                ))
                .child(
                    crate::theme_ext::field_box(div(), theme)
                        .mx(px(8.))
                        .mt(px(2.))
                        .px(px(10.))
                        .py(px(6.))
                        .text_size(px(13.))
                        .child(gpui_kit::base::input::Input::new(&s.agentic_title)),
                )
            })
            .child(row(
                ix(Row::Model(AiFunction::Feed)),
                "Model · not used yet",
                model_value(AiFunction::Feed),
                true,
            ))
            .children(other_box(AiFunction::Feed))
            .when(s.settings.feed_prompt.is_some(), |d| {
                d.child(div().mx(px(8.)).mt(px(4.)).text_size(px(11.5)).text_color(p.muted).child(
                    "Agentic feed prompt: how your assistant should organize what you read",
                ))
                .child(area(&s.feed, 56.))
            });
        body = body.child(rows);
        if let Some(e) = &s.error {
            body = body.child(div().mt(px(8.)).text_color(p.over_text()).child(e.clone()));
        }
        body = body.child(
            div()
                .mt(px(12.))
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(14.))
                .text_size(px(11.5))
                .text_color(p.muted)
                .child(
                    crate::theme_ext::chip(div(), theme, true)
                        .id("server-ai-save")
                        .px(px(10.))
                        .cursor_pointer()
                        .child(if s.busy { "Saving…" } else { "Save" })
                        .on_click(
                            cx.listener(|this, _, window, cx| this.save_server_ai(window, cx)),
                        ),
                )
                .child(hint("⌘⏎", "save"))
                .child(hint("↑↓", "choose"))
                .child(hint("←→", "change"))
                .child(hint("esc", "close")),
        );
        (560., body.into_any_element())
    }
}
// --- end access-ai ---

#[cfg(test)]
mod tests {
    use super::{AiFunction, AiModels, choices, model_words, step_choice};
    use blyg_core::access_ai::{AiModel, AiProvider};

    fn models() -> AiModels {
        AiModels {
            providers: vec![AiProvider {
                id: "p".into(),
                label: "P".into(),
                key_secret: Some("P_KEY".into()),
                binding: None,
                configured: false,
            }],
            models: vec![
                AiModel {
                    id: "a".into(),
                    provider: "p".into(),
                    label: "A".into(),
                    note: None,
                    kind: None,
                },
                AiModel {
                    id: "j".into(),
                    provider: "p".into(),
                    label: "J".into(),
                    note: None,
                    kind: Some("decision".into()),
                },
            ],
            local: false,
        }
    }

    #[test]
    fn choices_cycle_through_none_models_and_other() {
        let m = models();
        assert_eq!(
            choices(&m, AiFunction::Tk, ""),
            vec![Some(String::new()), Some("a".into()), None]
        );
        // The classifier offers decision models only.
        assert_eq!(
            choices(&m, AiFunction::Classify, "")[1].as_deref(),
            Some("j")
        );
        // A typed-in id stays a choice.
        assert!(choices(&m, AiFunction::Tk, "typed").contains(&Some("typed".into())));
        assert_eq!(step_choice(&m, AiFunction::Tk, "", 1).as_deref(), Some("a"));
        assert_eq!(step_choice(&m, AiFunction::Tk, "a", 1), None);
        assert_eq!(
            step_choice(&m, AiFunction::Tk, "", -1),
            None,
            "wraps to other…"
        );
    }

    #[test]
    fn model_words_say_when_a_key_is_missing() {
        let m = models();
        assert_eq!(model_words(&m, ""), "none · this never calls a model");
        assert_eq!(model_words(&m, "a"), "A · P (P: no key)");
        assert_eq!(model_words(&m, "zz"), "zz");
    }
}
