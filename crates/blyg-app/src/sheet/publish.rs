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

/// --- tangle --- The blyg answered a publish with 409
/// `tangle-confirmation-required` (studio 0.63): the thread's `![[+ …]]`
/// spans and `[[+ …]]` placeholders would publish `count` new items first.
/// `⏎` (publish all) sends the same publish again with `tangle: true`.
pub(crate) struct TangleSheet {
    pub(crate) id: LocalId,
    pub(crate) note: Option<String>,
    pub(crate) extras: blyg_core::access_ai::PublishExtras,
    pub(crate) count: usize,
    /// The new items' names as the blyg gave them (a placeholder's ends
    /// " (linked)").
    pub(crate) parts: Vec<String>,
    /// The same text as the last tangle of this post: publishing it again
    /// would make the new items twice.
    pub(crate) repeat: bool,
    pub(crate) focus: FocusHandle,
}

/// The confirm step's heading (the studio's dialog title).
pub(crate) fn tangle_heading(count: usize) -> String {
    format!(
        "This will publish {count} new item{} and this thread",
        if count == 1 { "" } else { "s" }
    )
}

/// Item `i` of the confirm step's list. The blyg's label for a passage cut
/// at eight words ends "…" even after a full stop; shown, a sentence's own
/// end needs none (user testing, W5). The label itself stays the blyg's.
pub(crate) fn part_line(i: usize, label: &str) -> String {
    let label = match label.strip_suffix('…') {
        Some(cut) if cut.ends_with(['.', '!', '?']) => cut,
        _ => label,
    };
    format!("{}. {label}", i + 1)
}

/// The confirm step's warning when the text is the one just tangled.
pub(crate) const TANGLE_REPEAT: &str = "⚠ You just published these as new items. Publishing them again makes a second copy of each. Press esc and check the text first.";

/// The studio's hint under the list.
pub(crate) const TANGLE_HINT: &str = "Each new item is a thread of its own, published first: this thread quotes each ![[+ …]] and links each [[+ …]]. To change one later, open that item.";

impl SheetView for TangleSheet {
    fn render(&self, _: &MainView, kit: &Kit, cx: &mut Context<MainView>) -> Shown {
        let p = kit.p;
        let TangleSheet {
            count,
            parts,
            repeat,
            focus,
            ..
        } = self;
        Shown::from((
            460.,
            div()
                .id("tangle-confirm")
                .debug_selector(|| "tangle-confirm".into())
                .track_focus(focus)
                .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                    match ev.keystroke.key.as_str() {
                        "enter" => this.publish_tangled(window, cx),
                        "escape" => this.close_sheet(window, cx),
                        _ => return,
                    }
                    cx.stop_propagation();
                }))
                .child(kit.heading(tangle_heading(*count)))
                .when(*repeat, |d| {
                    d.child(
                        div()
                            .id("tangle-repeat")
                            .debug_selector(|| "tangle-repeat".into())
                            .mb(px(8.))
                            .text_size(px(12.5))
                            .text_color(p.warn_text())
                            .line_height(relative(1.45))
                            .child(TANGLE_REPEAT),
                    )
                })
                .child(
                    div()
                        .mb(px(8.))
                        .pl(px(6.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .children(
                            parts
                                .iter()
                                .enumerate()
                                .map(|(i, label)| div().child(part_line(i, label))),
                        ),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(p.muted)
                        .line_height(relative(1.45))
                        .child(TANGLE_HINT),
                )
                .child(
                    kit.keys_row(vec![])
                        .child(crate::platform::key_button(
                            kit.key_hint("⏎", "publish all"),
                            "tangle-publish-all",
                            cx.listener(|this, _, window, cx| this.publish_tangled(window, cx)),
                        ))
                        .child(crate::platform::key_button(
                            kit.key_hint("esc", "cancel"),
                            "tangle-cancel",
                            cx.listener(|this, _, window, cx| this.close_sheet(window, cx)),
                        )),
                )
                .into_any_element(),
        ))
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

#[cfg(test)]
mod tests {
    use super::part_line;

    /// User testing (W5, R12): the studio's label for a cut passage ends
    /// "…" even after a full stop; the list doesn't show ".…".
    #[test]
    fn a_part_ending_in_punctuation_shows_no_ellipsis() {
        assert_eq!(
            part_line(0, "The tide tables were wrong by an hour.…"),
            "1. The tide tables were wrong by an hour."
        );
        assert_eq!(part_line(1, "Ready? Then go!…"), "2. Ready? Then go!");
        // A cut mid-sentence keeps its ellipsis.
        assert_eq!(
            part_line(0, "one two three four five six seven eight…"),
            "1. one two three four five six seven eight…"
        );
        assert_eq!(
            part_line(1, "the ferry timetable (linked)"),
            "2. the ferry timetable (linked)"
        );
    }
}
