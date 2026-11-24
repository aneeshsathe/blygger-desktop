//! The resolve sheet for a post edited here and elsewhere: both sides as a
//! word diff; `1` keeps mine, `2` takes the server's, `3` keeps both.

use blyg_core::{LocalId, Resolution};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{Kit, SheetView, Shown};
use crate::app::MainView;
use crate::vm::DiffOp;

/// A side's pane, at most (it scrolls past this).
const PANE_MAX_H: f32 = 260.;

/// Whether `text` runs past its pane: about 34 characters a line in a
/// 560 px sheet's half, about 11 lines under the label.
fn runs_past_the_pane(text: &str) -> bool {
    let lines: usize = text
        .lines()
        .map(|l| l.chars().count().div_ceil(34).max(1))
        .sum();
    lines > 11
}

pub(crate) struct ConflictSheet {
    pub(crate) id: LocalId,
    pub(crate) mine: Vec<(DiffOp, String)>,
    pub(crate) theirs: Vec<(DiffOp, String)>,
    pub(crate) focus: FocusHandle,
}

impl SheetView for ConflictSheet {
    fn render(&self, v: &MainView, kit: &Kit, cx: &mut Context<MainView>) -> Shown {
        let p = kit.p;
        let ConflictSheet {
            mine,
            theirs,
            focus,
            ..
        } = self;
        Shown::from({
            let col = |label: &'static str, side: &'static str, runs: &Vec<(DiffOp, String)>| {
                let text: String = runs.iter().map(|(_, s)| s.as_str()).collect();
                let more = runs_past_the_pane(&text);
                let mut hl = Vec::new();
                let mut at = 0;
                for (op, s) in runs {
                    let r = at..at + s.len();
                    at += s.len();
                    match op {
                        DiffOp::Insert => hl.push((
                            r,
                            HighlightStyle {
                                background_color: Some(p.ins_bg),
                                ..Default::default()
                            },
                        )),
                        DiffOp::Delete => hl.push((
                            r,
                            HighlightStyle {
                                background_color: Some(p.del_bg),
                                strikethrough: Some(StrikethroughStyle {
                                    thickness: px(1.),
                                    color: Some(p.over),
                                }),
                                ..Default::default()
                            },
                        )),
                        DiffOp::Same => {}
                    }
                }
                let pane = div()
                    .id(label)
                    .max_h(px(PANE_MAX_H))
                    .overflow_y_scroll()
                    .p(px(8.))
                    .rounded(px(kit.theme.radius.min(8.)))
                    .border_1()
                    .border_color(p.edge())
                    .child(
                        div()
                            .mb(px(4.))
                            .font_family(kit.theme.chrome_font(&v.prefs))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(10.5))
                            .text_color(p.muted)
                            .child(label),
                    )
                    .child(
                        div()
                            .font_family(kit.body_font.clone())
                            .text_size(px(13.))
                            .line_height(relative(1.45))
                            .child(StyledText::new(text).with_highlights(hl)),
                    );
                // --- ux 0.29 --- a long text says it goes on (the pane
                // scrolls, with no other cue).
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(pane)
                    .when(more, |d| {
                        d.child(
                            div()
                                .debug_selector(move || format!("conflict-more-{side}"))
                                .mt(px(3.))
                                .font_family(kit.theme.chrome_font(&v.prefs))
                                .text_size(px(10.5))
                                .text_color(p.muted)
                                .child("↓ scroll for the rest"),
                        )
                    })
            };
            (
                560.,
                div()
                    .track_focus(focus)
                    .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                        match ev.keystroke.key.as_str() {
                            "1" => this.resolve(Resolution::KeepMine, window, cx),
                            "2" => this.resolve(Resolution::TakeServer, window, cx),
                            "3" => this.resolve(Resolution::KeepBoth, window, cx),
                            "escape" => this.close_sheet(window, cx),
                            _ => return,
                        }
                        cx.stop_propagation();
                    }))
                    .child(kit.heading("This post was also edited elsewhere".into()))
                    .child(
                        div()
                            .flex()
                            .gap(px(10.))
                            .my(px(8.))
                            .child(col("ON THIS MAC", "mine", mine))
                            .child(col("ON THE SERVER", "theirs", theirs)),
                    )
                    .child(kit.keys_row(vec![
                        kit.key_hint("1", "keep mine"),
                        kit.key_hint("2", "take server's"),
                        kit.key_hint("3", "keep both"),
                    ]))
                    .into_any_element(),
            )
        })
    }
}
