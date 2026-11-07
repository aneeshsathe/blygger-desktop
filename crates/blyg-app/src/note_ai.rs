//! --- access-ai --- The blyg's AI in the publish sheet (studio 0.56's
//! publish dialog): for a post's second version on, when the blyg offers
//! them,
//!
//! - **Draft a note** (⌥⌘N, Post › Draft a Change Note, or the chip): the
//!   blyg's change-notes model drafts the version note from the change
//!   (`POST …/note-draft`) and fills the field. With automatic change notes
//!   on, an empty note is drafted as the sheet opens.
//! - **The change classifier**: the blyg labels the change typo, minor or
//!   major (`POST …/classify`); the label shows quietly under the field,
//!   and a click changes or clears it. With "start the note with it" on,
//!   the label leads the note (`minor: …`), there, where you see it.
//!
//! You always edit and confirm: nothing publishes until ⏎. A note published
//! exactly as drafted is marked as generated (`noteGenerated`); the label
//! you leave chosen is kept with the version, privately (`changeCategory`).
//! Neither helper ever blocks publishing.
//!
//! A child module of `app`; `app.rs` keeps small hooks marked
//! `// --- access-ai ---`.

use blyg_core::access_ai::{
    ChangeCategory, NoteHelpers, PublishExtras, compose_note, note_is_generated, recategorize,
};

use super::*;

gpui_kit::actions!(
    blygger,
    [
        /// The publish sheet: draft the version note with the blyg's AI.
        DraftChangeNote
    ]
);

/// The classifier's answer, as the sheet holds it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Classified {
    pub loading: bool,
    /// The label chosen now (the model's, until you change it).
    pub category: Option<ChangeCategory>,
    /// What the classifier said (to tell an untouched note from an edited one).
    pub model: Option<ChangeCategory>,
    pub probability: Option<f64>,
    /// Why there's no label.
    pub reason: Option<String>,
}

/// The publish sheet's AI state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NoteAi {
    /// Which sheet this belongs to (`sheet_gen` when it opened).
    pub sheet: usize,
    /// What the blyg offers; `None` until read (or when it offers nothing).
    pub helpers: Option<NoteHelpers>,
    pub drafting: bool,
    /// The note as the model drafted it (before any label).
    pub drafted: Option<String>,
    pub draft_error: Option<String>,
    /// `Some` while the classifier is on.
    pub classify: Option<Classified>,
}

impl NoteAi {
    fn in_note(&self) -> bool {
        self.helpers.is_some_and(|h| h.category_in_note)
    }

    /// The machine's text, if the note began as the machine's: the drafted
    /// note led by the classifier's own label, or that label alone.
    pub fn machine(&self) -> Option<String> {
        let label = if self.in_note() {
            self.classify.as_ref().and_then(|c| c.model)
        } else {
            None
        };
        match &self.drafted {
            Some(d) => Some(compose_note(d, label)),
            None => label.map(|c| c.as_str().to_string()),
        }
    }

    /// What the publish carries beyond `note`.
    pub fn extras(&self, note: &str) -> PublishExtras {
        PublishExtras {
            note_generated: note_is_generated(note, self.machine().as_deref()),
            change_category: self
                .classify
                .as_ref()
                .filter(|c| !c.loading)
                .map(|c| c.category),
        }
    }
}

impl MainView {
    /// Hook: the action on the main window.
    pub(super) fn note_ai_actions(
        &self,
        d: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        d.on_action(
            cx.listener(|this, _: &DraftChangeNote, window, cx| this.draft_change_note(window, cx)),
        )
    }

    fn note_ai(&mut self, sheet: usize) -> Option<(&LocalId, &mut NoteAi, &Entity<InputState>)> {
        match self.sheet.as_mut() {
            Some(Sheet::Publish { id, ai, note, .. }) if ai.sheet == sheet => Some((id, ai, note)),
            _ => None,
        }
    }

    /// Hook: the publish sheet opened for `next_version`. Ask the blyg what
    /// it offers, then classify, and draft when it drafts automatically.
    pub(super) fn note_ai_start(
        &mut self,
        next_version: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if next_version < 2 {
            return;
        }
        let sheet = self.sheet_gen;
        if let Some(Sheet::Publish { ai, .. }) = self.sheet.as_mut() {
            ai.sheet = sheet;
        }
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.server_ai_settings() });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |v, window, cx| {
                let Ok(s) = r else { return };
                let h = s.helpers();
                if !h.draft && !h.classify {
                    return;
                }
                let Some((_, ai, note)) = v.note_ai(sheet) else {
                    return;
                };
                ai.helpers = Some(h);
                let empty = note.read(cx).value().trim().is_empty();
                if h.classify {
                    ai.classify = Some(Classified {
                        loading: true,
                        ..Default::default()
                    });
                }
                cx.notify();
                if h.classify {
                    v.note_ai_classify(sheet, window, cx);
                }
                if h.auto && empty {
                    v.note_ai_draft(sheet, window, cx);
                }
            });
        })
        .detach();
    }

    /// ⌥⌘N / the chip: draft the note now.
    pub(crate) fn draft_change_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let offered = match &self.sheet {
            Some(Sheet::Publish { ai, .. }) => ai.helpers.map(|h| (h.draft, ai.sheet)),
            _ => {
                return self.show_toast(
                    crate::keymap::hint("Draft a note in the publish sheet: ⌘⏎ opens it"),
                    None,
                    cx,
                );
            }
        };
        match offered {
            Some((true, sheet)) => self.note_ai_draft(sheet, window, cx),
            _ => self.show_toast(
                "Your blyg drafts notes once it has a change-notes model",
                Some("Blyg › AI on Your Blyg…".into()),
                cx,
            ),
        }
    }

    fn note_ai_draft(&mut self, sheet: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, ai, _)) = self.note_ai(sheet) else {
            return;
        };
        if ai.drafting {
            return;
        }
        ai.drafting = true;
        ai.draft_error = None;
        let id = id.clone();
        cx.notify();
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.draft_note(&id) });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |v, window, cx| {
                let Some((_, ai, note)) = v.note_ai(sheet) else {
                    return;
                };
                ai.drafting = false;
                match r {
                    Ok(d) => {
                        let label = if ai.in_note() {
                            ai.classify.as_ref().and_then(|c| c.category)
                        } else {
                            None
                        };
                        let text = compose_note(&d.note, label);
                        ai.drafted = Some(d.note);
                        let note = note.clone();
                        note.update(cx, |s, cx| {
                            s.set_value(text, window, cx);
                            s.focus(window, cx)
                        });
                    }
                    Err(e) => ai.draft_error = Some(format!("No note could be drafted: {e}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn note_ai_classify(&mut self, sheet: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, _, _)) = self.note_ai(sheet) else {
            return;
        };
        let id = id.clone();
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move { backend.classify_change(&id) });
        cx.spawn_in(window, async move |this, cx| {
            let r = task.await;
            let _ = this.update_in(cx, |v, window, cx| {
                let Some((_, ai, note)) = v.note_ai(sheet) else {
                    return;
                };
                let c = match r {
                    Ok(c) => Classified {
                        loading: false,
                        category: c.category,
                        model: c.category,
                        probability: c.probability,
                        reason: c.reason,
                    },
                    Err(e) => Classified {
                        reason: Some(e.to_string()),
                        ..Default::default()
                    },
                };
                let (in_note, cat) = (ai.in_note(), c.category);
                ai.classify = Some(c);
                if in_note && cat.is_some() {
                    let note = note.clone();
                    let text = recategorize(&note.read(cx).value(), cat);
                    note.update(cx, |s, cx| s.set_value(text, window, cx));
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// A click on a label: choose it, or clear it when it's chosen.
    pub(crate) fn note_ai_choose(
        &mut self,
        c: ChangeCategory,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(Sheet::Publish { ai, note, .. }) = self.sheet.as_mut() else {
            return;
        };
        let in_note = ai.in_note();
        let Some(cl) = ai.classify.as_mut().filter(|c| !c.loading) else {
            return;
        };
        cl.category = if cl.category == Some(c) {
            None
        } else {
            Some(c)
        };
        let cat = cl.category;
        if in_note {
            let note = note.clone();
            let text = recategorize(&note.read(cx).value(), cat);
            note.update(cx, |s, cx| s.set_value(text, window, cx));
        }
        cx.notify();
    }

    /// Hook: the publish sheet's AI line (nothing when the blyg offers none).
    pub(super) fn render_note_ai(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let Some(Sheet::Publish { ai, note, .. }) = &self.sheet else {
            return None;
        };
        let h = ai.helpers?;
        let p = self.palette;
        let theme = &*self.theme;
        let mut line = div()
            .mt(px(8.))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .text_size(px(11.5))
            .text_color(p.muted);
        if let Some(c) = &ai.classify {
            line = line.child("Kind");
            if c.loading {
                line = line.child(div().child("classifying…"));
            } else {
                for cat in [
                    ChangeCategory::Typo,
                    ChangeCategory::Minor,
                    ChangeCategory::Major,
                ]
                .into_iter()
                .chain(
                    (c.model == Some(ChangeCategory::Refresh)).then_some(ChangeCategory::Refresh),
                ) {
                    let on = c.category == Some(cat);
                    let pct = match (c.model == Some(cat), c.probability) {
                        (true, Some(p)) => format!(" · {}%", (p * 100.).round() as i64),
                        _ => String::new(),
                    };
                    line = line.child(
                        div()
                            .id(SharedString::from(format!("note-kind-{}", cat.as_str())))
                            .debug_selector(move || format!("note-kind-{}", cat.as_str()))
                            .px(px(6.))
                            .rounded(px(4.))
                            .cursor_pointer()
                            .when(on, |d| d.border_1().border_color(p.line).text_color(p.ink))
                            .hover(|d| d.text_color(p.ink))
                            .child(format!("{}{pct}", cat.as_str()))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.note_ai_choose(cat, window, cx)
                            })),
                    );
                }
            }
        }
        if h.draft {
            let label = if ai.drafting {
                "drafting…"
            } else {
                "Draft a note"
            };
            line = line.child(div().flex_1()).child(
                crate::theme_ext::chip(div(), theme, false)
                    .id("note-draft")
                    .debug_selector(|| "note-draft".into())
                    .px(px(8.))
                    .cursor_pointer()
                    .text_color(p.ink)
                    .child(label)
                    .on_click(
                        cx.listener(|this, _, window, cx| this.draft_change_note(window, cx)),
                    ),
            );
            line = line.child(crate::keymap::hint("⌥⌘N"));
        }
        let mut out = div().flex().flex_col().child(line);
        let text = note.read(cx).value().to_string();
        let said = if ai.drafted.is_some() && note_is_generated(&text, ai.machine().as_deref()) {
            Some("Drafted by your blyg's model: it's marked as generated unless you edit it.")
        } else if ai.drafted.is_none()
            && ai.machine().is_some()
            && note_is_generated(&text, ai.machine().as_deref())
        {
            Some("The note is the classifier's label: it's marked as generated unless you edit it.")
        } else {
            None
        };
        if let Some(s) = said {
            out = out.child(
                div()
                    .mt(px(4.))
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .child(s),
            );
        }
        if let Some(c) = ai
            .classify
            .as_ref()
            .filter(|c| !c.loading && c.model.is_none())
        {
            out = out.child(
                div()
                    .mt(px(4.))
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .child(match &c.reason {
                        Some(r) => {
                            format!("No kind suggested: {r}. Choose one, or publish without.")
                        }
                        None => "No kind suggested. Choose one, or publish without.".into(),
                    }),
            );
        }
        if let Some(e) = &ai.draft_error {
            out = out.child(
                div()
                    .mt(px(4.))
                    .text_size(px(11.))
                    .text_color(p.warn_text())
                    .child(format!("{e}. Write one, or publish without.")),
            );
        }
        Some(out.into_any_element())
    }
}
// --- end access-ai ---

#[cfg(test)]
mod tests {
    use super::{ChangeCategory, Classified, NoteAi, NoteHelpers, PublishExtras};

    fn on(category_in_note: bool) -> NoteAi {
        NoteAi {
            helpers: Some(NoteHelpers {
                draft: true,
                classify: true,
                auto: false,
                category_in_note,
            }),
            classify: Some(Classified {
                category: Some(ChangeCategory::Minor),
                model: Some(ChangeCategory::Minor),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn an_unedited_draft_is_generated() {
        let mut ai = on(true);
        ai.drafted = Some("Added an example.".into());
        assert_eq!(ai.machine().as_deref(), Some("minor: Added an example."));
        let x = ai.extras("minor: Added an example.");
        assert!(x.note_generated);
        assert_eq!(x.change_category, Some(Some(ChangeCategory::Minor)));
        assert!(!ai.extras("minor: Added two examples.").note_generated);
    }

    #[test]
    fn the_bare_label_is_generated_and_your_words_are_not() {
        let ai = on(true);
        assert!(ai.extras("minor").note_generated);
        assert!(!ai.extras("my own words").note_generated);
        let ai = on(false);
        assert_eq!(ai.machine(), None);
        assert!(!ai.extras("minor").note_generated);
    }

    #[test]
    fn a_cleared_label_is_sent_as_cleared_and_no_classifier_sends_nothing() {
        let mut ai = on(false);
        ai.classify.as_mut().unwrap().category = None;
        assert_eq!(ai.extras("x").change_category, Some(None));
        ai.classify.as_mut().unwrap().loading = true;
        assert_eq!(ai.extras("x").change_category, None);
        assert_eq!(NoteAi::default().extras("x"), PublishExtras::default());
    }
}
