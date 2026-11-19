//! The publish sheet (an optional version note, then `⏎`), and the
//! held-labels sheet that comes instead when the blyg is below studio 0.47.

use blyg_core::LocalId;
use gpui_kit::base::input::{Escape, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{Kit, SheetView, Shown, Submit};
use crate::app::{MainView, first_publish_note, note_ai, server_notice};

pub(crate) struct PublishSheet {
    pub(crate) id: LocalId,
    pub(crate) title: String,
    pub(crate) next_version: u32,
    pub(crate) note: Entity<InputState>,
    /// --- access-ai --- the blyg's draft note and change classifier.
    pub(crate) ai: note_ai::NoteAi,
}

/// Publishing a post with labelled links to a blyg below studio 0.47
/// (`server_notice::HELD_*`): `⏎` writes them bare and publishes.
pub(crate) struct HeldLabelsSheet {
    pub(crate) id: LocalId,
    pub(crate) focus: FocusHandle,
}

impl SheetView for PublishSheet {
    fn render(&self, v: &MainView, kit: &Kit, cx: &mut Context<MainView>) -> Shown {
        let p = kit.p;
        let PublishSheet {
            title,
            next_version,
            note,
            ..
        } = self;
        Shown::from((
            460.,
            div()
                .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                    cx.stop_propagation();
                    this.close_sheet(window, cx);
                }))
                .child(kit.heading(format!("Publish “{title}” as v{next_version}")))
                // A first publish: say it goes public, and for good.
                .when(*next_version == 1, |d| {
                    let host =
                        crate::settings::blyg_url(cx).map(|u| crate::vm::url_host(&u).unwrap_or(u));
                    d.child(
                        div()
                            .id("publish-first-note")
                            .debug_selector(|| "publish-first-note".into())
                            .mb(px(8.))
                            .text_size(px(12.))
                            .text_color(p.muted)
                            .line_height(relative(1.45))
                            .child(first_publish_note(host.as_deref())),
                    )
                })
                // --- full editor ---
                .children(v.studio_publish_warning(cx).map(|w| {
                    div()
                        .mb(px(8.))
                        .text_size(px(12.5))
                        .text_color(p.warn_text())
                        .child(w)
                }))
                .child(kit.input_box(gpui_kit::base::input::Input::new(note).into_any_element()))
                .children(v.render_note_ai(cx)) // --- access-ai ---
                .child(kit.keys_row(vec![
                    kit.key_hint("⏎", "publish"),
                    kit.key_hint("esc", "cancel"),
                ]))
                .into_any_element(),
        ))
    }

    fn inputs(&self) -> Vec<&Entity<InputState>> {
        vec![&self.note]
    }

    /// ⌘⏎ in the note field publishes, like ⏎.
    fn submit(&self) -> Option<Submit> {
        Some(|v, window, cx| {
            let Some(s) = v.sheet.get::<PublishSheet>() else {
                return;
            };
            let text = s.note.read(cx).value().to_string();
            v.do_publish(text, window, cx);
        })
    }
}

impl SheetView for HeldLabelsSheet {
    fn render(&self, _: &MainView, kit: &Kit, cx: &mut Context<MainView>) -> Shown {
        let p = kit.p;
        let HeldLabelsSheet { focus, .. } = self;
        Shown::from((
            460.,
            div()
                .track_focus(focus)
                .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                    match ev.keystroke.key.as_str() {
                        "enter" => this.publish_without_labels(window, cx),
                        "escape" => this.close_sheet(window, cx),
                        _ => return,
                    }
                    cx.stop_propagation();
                }))
                .child(kit.heading("Labelled links".into()))
                .child(
                    div()
                        .text_color(p.muted)
                        .line_height(relative(1.45))
                        .child(crate::keymap::hint(server_notice::HELD_SHEET)),
                )
                .child(
                    kit.keys_row(vec![])
                        .child(crate::platform::key_button(
                            kit.key_hint("⏎", server_notice::HELD_FIX),
                            "held-fix",
                            cx.listener(|this, _, window, cx| {
                                this.publish_without_labels(window, cx)
                            }),
                        ))
                        .child(crate::platform::key_button(
                            kit.key_hint("esc", "cancel"),
                            "held-cancel",
                            cx.listener(|this, _, window, cx| this.close_sheet(window, cx)),
                        )),
                )
                .into_any_element(),
        ))
    }
}
