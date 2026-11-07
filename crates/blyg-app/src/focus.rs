//! --- focus --- Focus mode (Fable's proposal 6): the line you're on stays
//! put, the rest steps back.
//!
//! ⌥⌘F (View › Focus) toggles it, esc leaves. It is the editor alone (⌘3,
//! then ⌘E: no list, no preview) plus three things:
//! - Typewriter scroll: the caret's row is kept at a fraction of the
//!   editor's height (`focus-line`, 0.45 by default). gpui-base's textarea
//!   has no scroll policy of its own, so this is done from the app: after
//!   the caret moves or the text changes, the editor's scroll offset is set
//!   so the caret's row lands on that line (a frame or two later, once the
//!   textarea has laid it out), with padding above the first line and below
//!   the last so either end can reach it too. Scrolling by hand still
//!   works until the next keystroke.
//! - Fading (`focus-fade = paragraph | off`): the paragraphs the caret isn't
//!   in sit under a veil of the page's colour, so they read at about a
//!   third. The textarea has no per-paragraph colour runs, so the veil is
//!   painted over it (as the spelling underlines are).
//! - The chrome collapses: no omnibar (⌘L still brings it back, above the
//!   text, until you return to the editor), no toolbar, and the status bar
//!   becomes a hairline with the sync dot and a private session word meter
//!   (words this sitting; nothing counted is kept or shown anywhere else).
//!   Hovering the hairline shows the status bar.
//!
//! esc or ⌥⌘F again leaves, putting the view back as it was; another view
//! key (⌘1/⌘2/⌘3/⌘E) or another screen also ends it, in the view chosen. A child module of `app`; `app.rs` keeps small hooks
//! marked `// --- focus ---`.

use std::ops::Range;

use super::*;
use crate::app::studio::ViewMode;
use gpui_kit::base::input::RopeExt as _;

gpui_kit::actions!(
    blygger,
    [
        /// ⌥⌘F: focus mode on or off.
        ToggleFocus
    ]
);

/// The hairline's height (it holds the dot and the meter).
pub const BAR_H: f32 = 18.;
/// How dim the paragraphs you're not in read (the veil's opacity).
pub const VEIL: f32 = 0.68;
/// The meter's line is full at this many words (it just keeps going).
const METER_FULL: usize = 750;

/// `focus-fade`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fade {
    Paragraph,
    Off,
}

/// One sitting in focus mode.
pub(crate) struct Session {
    /// The view to go back to.
    prev_view: ViewMode,
    /// Words written before this post (other posts this sitting).
    banked: usize,
    /// The post being counted and its words when counting began.
    post: Option<LocalId>,
    baseline: usize,
    /// Its words at the last frame.
    last: usize,
    /// The last (caret, text length) the scroll was set for.
    anchor: Option<(usize, usize)>,
    /// Frames left to settle the scroll after a change.
    settle: u8,
    pub seq: usize,
}

#[derive(Default)]
pub(crate) struct State {
    pub on: Option<Session>,
    /// The hairline is hovered: show the status bar.
    pub hover: bool,
    seq: usize,
    /// The editor's text area as last painted (its padding included), so
    /// the line is measured against the pane, not the padded text.
    pane: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
}

/// `focus-line`: where the caret's row sits, as a fraction of the height.
pub fn line(cx: &App) -> f32 {
    cx.try_global::<crate::settings::AppConfig>()
        .and_then(|c| {
            c.store
                .config()
                .get("focus-line")
                .and_then(|v| v.parse().ok())
        })
        .unwrap_or(0.45f32)
        .clamp(0.1, 0.9)
}

/// `focus-fade`.
pub fn fade(cx: &App) -> Fade {
    match cx
        .try_global::<crate::settings::AppConfig>()
        .and_then(|c| c.store.config().get("focus-fade"))
    {
        Some("off") => Fade::Off,
        _ => Fade::Paragraph,
    }
}

/// The paragraph `cursor` is in: from after the blank line before it to
/// the blank line after it (the whole text when there's none).
pub fn paragraph_at(text: &str, cursor: usize) -> Range<usize> {
    let cursor = cursor.min(text.len());
    let start = text[..cursor].rfind("\n\n").map_or(0, |i| i + 2);
    let end = text[cursor..]
        .find("\n\n")
        .map_or(text.len(), |i| cursor + i);
    start..end.max(start)
}

/// Words in `text`, as the meter counts them.
pub fn words(text: &str) -> usize {
    text.split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .count()
}

/// "1 word", "312 words".
pub fn meter_label(n: usize) -> String {
    match n {
        1 => "1 word this sitting".into(),
        n => format!("{n} words this sitting"),
    }
}

impl MainView {
    /// Hook: the action on the main window.
    pub(super) fn focus_actions(&self, d: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        d.on_action(cx.listener(|this, _: &ToggleFocus, window, cx| {
            if this.focus_mode.on.is_some() {
                this.leave_focus(window, cx)
            } else {
                this.enter_focus(window, cx)
            }
        }))
    }

    /// The editor's text area as last painted in focus mode (tests).
    #[cfg(test)]
    pub(crate) fn focus_mode_pane(&self) -> Option<Bounds<Pixels>> {
        self.focus_mode.pane.get()
    }

    pub(crate) fn focus_on(&self) -> bool {
        self.focus_mode.on.is_some()
    }

    /// ⌥⌘F: the editor alone, the caret held at its line.
    pub(crate) fn enter_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sheet.is_some() {
            return;
        }
        self.leave_reading(); // --- reading & versions --- back to Posts
        let prev_view = self.studio.view;
        // ⌘3 then ⌘E: no list, no preview. Not remembered in state.json:
        // leaving puts the view back.
        self.studio.view = ViewMode::Focus;
        self.studio_refresh(true, cx);
        let (post, baseline) = self.focus_count_now(cx);
        self.focus_mode.seq += 1;
        self.focus_mode.on = Some(Session {
            prev_view,
            banked: 0,
            post,
            baseline,
            last: baseline,
            anchor: None,
            settle: 0,
            seq: self.focus_mode.seq,
        });
        self.focus_mode.hover = false;
        if self.current.is_some() || self.new_post.is_some() {
            self.mode = Mode::Edit;
            self.editor.update(cx, |s, cx| s.focus(window, cx));
        }
        cx.notify();
    }

    /// esc / ⌥⌘F: everything back as it was.
    pub(crate) fn leave_focus(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.focus_mode.on.take() else {
            return;
        };
        if self.studio.view == ViewMode::Focus {
            self.studio.view = s.prev_view;
            self.studio_refresh(true, cx);
        }
        self.focus_mode.hover = false;
        cx.notify();
    }

    /// The open post and its words now.
    fn focus_count_now(&self, cx: &App) -> (Option<LocalId>, usize) {
        let id = self.current.as_ref().map(|c| c.local_id.clone());
        (id, words(self.editor.read(cx).value().as_ref()))
    }

    /// Hook (every frame): end the session when another view key took
    /// over, keep the meter, and hold the caret at its line.
    pub(super) fn focus_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.focus_mode.on.is_none() {
            return;
        }
        if self.studio.view != ViewMode::Focus || self.reading.view != reading::View::Posts {
            // ⌘1/⌘2/⌘3, Reading…: they win; the view stays as chosen.
            self.focus_mode.on = None;
            return;
        }
        let (post, now) = self.focus_count_now(cx);
        let line = line(cx);
        let Some(s) = self.focus_mode.on.as_mut() else {
            return;
        };
        if post != s.post {
            // Another post: what was written in the last one stays counted.
            s.banked += s.last.saturating_sub(s.baseline);
            s.post = post;
            s.baseline = now;
            s.anchor = None;
        }
        s.last = now;
        // Typewriter scroll.
        let ed = self.editor.read(cx);
        let key = (ed.cursor(), ed.text().len());
        if s.anchor != Some(key) {
            s.anchor = Some(key);
            s.settle = 4;
        }
        if s.settle == 0 {
            return;
        }
        s.settle -= 1;
        let Some(bounds) = self.focus_mode.pane.get() else {
            window.request_animation_frame();
            return;
        };
        let height = bounds.size.height;
        let Some((caret, _)) = ed.cursor_layout() else {
            window.request_animation_frame();
            return;
        };
        if height <= px(0.) {
            window.request_animation_frame();
            return;
        }
        let want = bounds.top() + height * line;
        // The caret's bounds are in the text's own space (scrolling not
        // applied): where it shows is that plus the scroll offset.
        let off = ed.scroll_offset();
        let delta = caret.top() + off.y - want;
        self.editor.update(cx, |e, cx| {
            if delta.abs() > px(1.) {
                e.set_scroll_offset(point(off.x, (want - caret.top()).min(px(0.))), cx);
            }
        });
        if delta.abs() > px(1.) {
            window.request_animation_frame();
        } else if let Some(s) = self.focus_mode.on.as_mut() {
            s.settle = 0;
        }
    }

    /// Hook: the editor's top and bottom padding in focus mode, so the
    /// first line and the last can sit at `focus-line` too (the textarea
    /// only scrolls past its last line in code-editor mode); `None`
    /// outside it.
    pub(super) fn focus_pads(&self, cx: &App) -> Option<(Pixels, Pixels)> {
        self.focus_mode.on.as_ref()?;
        let h = self.focus_mode.pane.get()?.size.height;
        let lh = self.editor.read(cx).line_height().unwrap_or(px(30.));
        let line = line(cx);
        Some(((h * line).max(px(26.)), (h * (1. - line) - lh).max(px(40.))))
    }

    /// Hook: the omnibar shows only while it has the keyboard (⌘L).
    pub(super) fn focus_hides_omnibar(&self) -> bool {
        self.focus_on() && self.mode != Mode::Search
    }

    /// Words this sitting.
    pub(crate) fn focus_words(&self, cx: &App) -> Option<usize> {
        let s = self.focus_mode.on.as_ref()?;
        let (_, now) = self.focus_count_now(cx);
        Some(s.banked + now.saturating_sub(s.baseline))
    }

    /// Hook (esc in the editor): leave focus mode first. `true` when it did.
    pub(super) fn focus_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.focus_mode.on.is_none() || self.sheet.is_some() {
            return false;
        }
        self.leave_focus(window, cx);
        true
    }

    /// Hook: the veil over the paragraphs the caret isn't in (and, in
    /// focus mode, where the editor's text area is).
    pub(super) fn render_focus_fade(&self, cx: &App) -> Option<AnyElement> {
        if !self.focus_on() {
            return None;
        }
        let editor = self.editor.clone();
        let veil = (fade(cx) == Fade::Paragraph && self.mode == Mode::Edit)
            .then(|| self.palette.editor.opacity(VEIL));
        let pane = self.focus_mode.pane.clone();
        Some(
            canvas(
                move |bounds, _, _| pane.set(Some(bounds)),
                move |bounds, _, window, cx| {
                    if let Some(veil) = veil {
                        paint_veil(&editor, veil, bounds, window, cx)
                    }
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .into_any_element(),
        )
    }

    /// Hook: the hairline in place of the status bar (`None` outside focus
    /// mode, or while it's hovered: the status bar then).
    pub(super) fn render_focus_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let s = self.focus_mode.on.as_ref()?;
        if self.focus_mode.hover {
            return None;
        }
        let p = self.palette;
        let n = self.focus_words(cx).unwrap_or(0);
        let (sync_text, dot) = vm::sync_label(self.sync, self.publishing);
        let dot_color = match dot {
            vm::SyncDot::Green => p.green,
            vm::SyncDot::Amber => p.amber,
            vm::SyncDot::Busy => p.accent,
            vm::SyncDot::Grey => p.grey,
            vm::SyncDot::Red => p.over,
        };
        let fill = (n as f32 / METER_FULL as f32).min(1.);
        let bar = div()
            .id("focus-bar")
            .debug_selector(|| "focus-bar".into())
            .h(px(BAR_H))
            .flex_none()
            .relative()
            .bg(p.editor)
            .flex()
            .items_center()
            .justify_end()
            .gap(px(6.))
            .px(px(14.))
            .font_family(self.theme.chrome_font(&self.prefs))
            .text_size(px(10.5))
            .text_color(p.muted.opacity(0.8))
            .on_hover(cx.listener(|this, on: &bool, _, cx| {
                this.focus_mode.hover = *on;
                cx.notify();
            }))
            .child(
                div()
                    .id("focus-meter")
                    .debug_selector(|| "focus-meter".into())
                    .child(meter_label(n)),
            )
            .child(
                div()
                    .id("focus-dot")
                    .size(px(6.))
                    .rounded_full()
                    .bg(dot_color)
                    .tooltip(move |_, cx| cx.new(|_| reading::Tip(sync_text.clone())).into()),
            )
            // The hairline itself, along the bottom: the session's words.
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .h(px(2.))
                    .bg(p.line.opacity(0.5))
                    .child(div().h_full().w(relative(fill)).bg(p.accent.opacity(0.55))),
            );
        let gen_ = s.seq;
        Some(if crate::app::delight::reduce_motion() {
            bar.into_any_element()
        } else {
            // --- delight --- the chrome slides away rather than vanishing.
            bar.with_animation(
                ("focus-bar-in", gen_),
                Animation::new(Duration::from_millis(220)).with_easing(ease_out_quint()),
                |d, t| d.opacity(t).mb(px(-(BAR_H) * (1. - t))),
            )
            .into_any_element()
        })
    }
}

/// Paint the veil: everything above the caret's paragraph and everything
/// below it, inside `bounds` (the editor's text area).
fn paint_veil(
    editor: &Entity<TextareaState>,
    veil: Hsla,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let s = editor.read(cx);
    let (Some(rows), Some(_)) = (s.visible_row_range(), s.line_height()) else {
        return;
    };
    let rope = s.text();
    let lines = rope.lines_len();
    if lines == 0 || rope.len() == 0 {
        return;
    }
    let first = rope.line_start_offset(rows.start.min(lines.saturating_sub(1)));
    let last = if rows.end >= lines {
        rope.len()
    } else {
        rope.line_start_offset(rows.end)
    };
    let text = s.value();
    let para = paragraph_at(text.as_ref(), s.cursor());
    let top = if para.start <= first {
        None // it starts above the view: nothing above to veil
    } else if para.start > last {
        Some(bounds.bottom()) // (the caret is off screen: veil it all)
    } else {
        s.range_to_bounds(&(para.start..para.start))
            .map(|b| b.top())
    };
    let bottom = if para.end >= last {
        None
    } else {
        s.range_to_bounds(&(para.end..para.end)).map(|b| b.bottom())
    };
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        if let Some(y) = top.filter(|y| *y > bounds.top()) {
            let r =
                Bounds::from_corners(bounds.origin, point(bounds.right(), y.min(bounds.bottom())));
            window.paint_quad(fill(r, veil));
        }
        if let Some(y) = bottom.filter(|y| *y < bounds.bottom()) {
            let r = Bounds::from_corners(
                point(bounds.left(), y.max(bounds.top())),
                bounds.bottom_right(),
            );
            window.paint_quad(fill(r, veil));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{meter_label, paragraph_at, words};

    #[test]
    fn the_paragraph_under_the_caret() {
        let t = "One.\n\nTwo, here.\nStill two.\n\nThree.";
        assert_eq!(&t[paragraph_at(t, 0)], "One.");
        assert_eq!(&t[paragraph_at(t, 8)], "Two, here.\nStill two.");
        assert_eq!(&t[paragraph_at(t, t.len())], "Three.");
        assert_eq!(paragraph_at("", 0), 0..0);
        assert_eq!(&t[paragraph_at(t, 99)], "Three.");
    }

    #[test]
    fn the_meter_counts_words() {
        assert_eq!(words("A blyg is tended, not fed. — ok"), 7);
        assert_eq!(words("  \n "), 0);
        assert_eq!(meter_label(1), "1 word this sitting");
        assert_eq!(meter_label(12), "12 words this sitting");
    }
}
