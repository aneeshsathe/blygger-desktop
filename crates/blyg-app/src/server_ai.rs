//! --- access-ai --- Blyg › AI on Your Blyg…: the blyg's own AI settings, as
//! Studio's Settings › AI has them (studio 0.56): a model per function from
//! the studio's model list (`GET /api/ai/models`, with each provider's key
//! status), the house style added to every `[TK]` generated on the blyg,
//! automatic change notes, and the change classifier.
//!
//! These run on the blyg, for the blyg. Burrow's own AI (Settings › AI,
//! `blyg-ai`) runs from this computer and is configured there.
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

/// A row of the sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Model(AiFunction),
    AutoNotes,
    CategoryInNote,
}

/// The rows, in order.
pub const ROWS: [Row; 6] = [
    Row::Model(AiFunction::Tk),
    Row::Model(AiFunction::Changelog),
    Row::AutoNotes,
    Row::Model(AiFunction::Classify),
    Row::CategoryInNote,
    Row::Model(AiFunction::Feed),
];

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
    /// The typed model id of the row in "other…".
    pub other: Entity<InputState>,
    pub load: AiLoad,
    pub models: AiModels,
    /// As read, then as edited (saved on ⌘⏎).
    pub settings: ServerAiSettings,
    pub sel: usize,
    /// The function whose model is being typed.
    pub other_for: Option<AiFunction>,
    pub busy: bool,
    pub error: Option<String>,
}

impl ServerAiSheet {
    pub fn inputs(&self) -> Vec<&Entity<InputState>> {
        vec![&self.other]
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
    }

    /// Blyg › AI on Your Blyg…: open it (or close it).
    pub(crate) fn open_server_ai(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.sheet, Some(Sheet::ServerAi(_))) {
            return self.close_sheet(window, cx);
        }
        let style = cx.new(|cx| TextareaState::new(window, cx).soft_wrap(true));
        let feed = cx.new(|cx| TextareaState::new(window, cx).soft_wrap(true));
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
        self.sheet_gen += 1;
        self.sheet = Some(Sheet::ServerAi(Box::new(ServerAiSheet {
            focus,
            style,
            feed,
            other,
            load: AiLoad::Loading,
            models: AiModels::default(),
            settings: ServerAiSettings::default(),
            sel: 0,
            other_for: None,
            busy: false,
            error: None,
        })));
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
                let Some(Sheet::ServerAi(s)) = v.sheet.as_mut() else {
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
        match self.sheet.as_mut() {
            Some(Sheet::ServerAi(s)) => Some(s),
            _ => None,
        }
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
                s.sel = sel.saturating_sub(1);
                cx.notify();
            }
            "down" => {
                s.sel = (sel + 1).min(ROWS.len() - 1);
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
                    let typing = matches!(
                        &this.sheet,
                        Some(Sheet::ServerAi(s)) if s.other_for.is_some()
                    );
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
        let mut rows = div()
            .id("server-ai-rows")
            .max_h(px(470.))
            .overflow_y_scroll()
            .flex()
            .flex_col();
        rows = rows
            .child(section("TK GENERATION"))
            .child(row(0, "Model", model_value(AiFunction::Tk), true))
            .children(other_box(AiFunction::Tk))
            .when(s.settings.style_prompt.is_some(), |d| {
                d.child(div().mx(px(8.)).mt(px(4.)).text_size(px(11.5)).text_color(p.muted).child(
                    "House style (optional): added to every [TK] generated on the blyg",
                ))
                .child(area(&s.style, 56.))
            })
            .child(section("CHANGE NOTES"))
            .child(row(1, "Model", model_value(AiFunction::Changelog), true))
            .children(other_box(AiFunction::Changelog))
            .child(row(
                2,
                "Draft automatically",
                switch(s.settings.auto_change_notes),
                helps.draft,
            ))
            .child(div().mx(px(8.)).text_size(px(11.)).text_color(p.muted).child(
                "When a new version has no note, the publish sheet drafts one. You see it, and edit it, before anything is published; a note published exactly as drafted is marked as generated.",
            ))
            .child(section("CHANGE CLASSIFIER"))
            .child(row(3, "Decision model", model_value(AiFunction::Classify), true))
            .children(other_box(AiFunction::Classify))
            .child(row(
                4,
                "Start the note with it",
                switch(s.settings.change_category_in_note),
                helps.classify,
            ))
            .child(div().mx(px(8.)).text_size(px(11.)).text_color(p.muted).child(
                "Off unless you choose a model. It labels each new version typo, minor or major before you publish; you can change or clear the label. Readers see it only as words in your change note.",
            ))
            .child(section("SMART FEED · saved now, not used yet"))
            .child(row(5, "Model", model_value(AiFunction::Feed), true))
            .children(other_box(AiFunction::Feed))
            .when(s.settings.feed_prompt.is_some(), |d| {
                d.child(div().mx(px(8.)).mt(px(4.)).text_size(px(11.5)).text_color(p.muted).child(
                    "Scoring prompt: how the agent should score new items",
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
