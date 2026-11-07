//! --- side --- The window's side drawers and resizable dividers.
//!
//! The notes pane and Nearby thoughts are **drawers docked at the right
//! edge** of the window, beside the editor (and, for the notes, beside the
//! reading screens). They share one mechanism:
//!
//! - **The tab bar.** A slim column at the window's right edge holds a
//!   labelled tab per drawer ("Notes", and "Nearby" while a post is open in
//!   Posts). A click opens or closes that drawer without taking the
//!   keyboard from the editor; the keys still work (⇧⌘N, ⌥⌘U). An open
//!   drawer's tab is lit. Focus mode hides the bar (open drawers stay).
//! - **Docked, not overlaid.** An open drawer is a layout column: the
//!   editor and the screens fit to its left, and clicking or typing there
//!   never closes it. Closing is explicit: its ×, its tab, or its key.
//! - **Resizable.** Each drawer's left edge drags; a double-click puts the
//!   default width back. Widths are clamped to a [`Span`] and to a share of
//!   the window, and kept in `state.json` with whether each is open.
//!
//! And the **editor | preview divider** (⌘E's preview beside the editor):
//! it drags too, a double-click centres it, and its place is kept in
//! `state.json` (`preview_left`), as split view's divider is
//! (`split_left`).
//!
//! The layout logic is pure (unit-tested); `app.rs` keeps small hooks
//! marked `// --- side ---`.

use std::path::PathBuf;

use super::*;

/// The tab bar's width.
pub const TAB_BAR_W: f32 = 34.;

/// A drawer's draggable left edge (or a divider), in points.
pub const EDGE_W: f32 = 5.;

/// How long after the last move of a drag a width is saved.
pub const SAVE_MS: u64 = 400;

/// The editor's share of the editor | preview row by default.
pub const PREVIEW_DEFAULT: f32 = 0.5;
/// The editor's narrowest, beside the preview.
pub const EDITOR_MIN_PX: f32 = 320.;
/// The preview's narrowest.
pub const PREVIEW_MIN_PX: f32 = 260.;

/// A drawer's widths: its default, and the range a drag keeps it in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub default: f32,
    pub min: f32,
    pub max: f32,
}

impl Span {
    /// A remembered width (`state.json`) back in range; `None`: the default.
    pub fn restore(&self, saved: Option<u32>) -> f32 {
        saved
            .map(|w| (w as f32).clamp(self.min, self.max))
            .unwrap_or(self.default)
    }

    /// The width a drag of the drawer's left edge to `x` asks for, in a
    /// drawer whose right edge is at `right`.
    pub fn dragged(&self, right: f32, x: f32) -> f32 {
        (right - x).clamp(self.min, self.max)
    }

    /// The width it's drawn at: the chosen `width` in range, and at most
    /// `max_share` of a window `window_w` wide (but never under the minimum,
    /// unless the window is).
    pub fn fit(&self, width: f32, window_w: f32, max_share: f32) -> f32 {
        let w = width.clamp(self.min, self.max);
        let cap = (window_w * max_share).max(self.min).min(window_w.max(0.));
        w.min(cap)
    }

    /// What `state.json` keeps for `width`: nothing for the default.
    pub fn saved(&self, width: f32) -> Option<u32> {
        let w = width.round() as u32;
        (w != self.default.round() as u32).then_some(w)
    }
}

/// The left side's share of a row `total` wide, kept so the left is at
/// least `min_left` and the right at least `min_right` points (when the row
/// is too narrow for both, the middle).
pub fn clamp_share(share: f32, total: f32, min_left: f32, min_right: f32) -> f32 {
    if total <= 0. || min_left + min_right >= total {
        return 0.5;
    }
    share.clamp(min_left / total, 1. - min_right / total)
}

/// The share a drag of a divider to `x` asks for, in a row from `left`,
/// `total` wide.
pub fn dragged_share(left: f32, total: f32, x: f32) -> f32 {
    if total <= 0. {
        return 0.5;
    }
    ((x - left) / total).clamp(0., 1.)
}

/// The editor's share as `state.json` keeps it (thousandths), and back.
pub fn share_to_saved(share: f32) -> Option<u32> {
    let v = (share * 1000.).round() as u32;
    (v != (PREVIEW_DEFAULT * 1000.) as u32).then_some(v)
}

pub fn share_from_saved(saved: Option<u32>) -> f32 {
    saved
        .map(|v| (v as f32 / 1000.).clamp(0.1, 0.9))
        .unwrap_or(PREVIEW_DEFAULT)
}

/// A drawer's left edge, being dragged (one type per drawer, so a drag
/// moves only its own).
#[derive(Debug, Clone)]
pub struct DraggedEdge(pub Which);

/// Which drawer (or divider).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    Notes,
    Nearby,
    Preview,
}

impl Render for DraggedEdge {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// The layout state that isn't a drawer's own (a field of `MainView`).
pub struct Side {
    /// The editor's share of the editor | preview row.
    pub preview_left: f32,
    preview_gen: u64,
    data_dir: Option<PathBuf>,
}

impl Side {
    pub fn new(data_dir: Option<PathBuf>) -> Side {
        let saved = data_dir
            .as_deref()
            .map(blyg_core::state::AppState::load)
            .and_then(|s| s.preview_left);
        Side {
            preview_left: share_from_saved(saved),
            preview_gen: 0,
            data_dir,
        }
    }

    fn save(&self) {
        let Some(d) = &self.data_dir else { return };
        let v = share_to_saved(self.preview_left);
        let _ = blyg_core::state::AppState::update(d, |s| s.preview_left = v);
    }
}

/// A draggable edge `w` wide down the left of a drawer, or a divider: the
/// drag carries `which`; a double-click calls `reset`.
pub fn edge_handle(
    id: &'static str,
    which: Which,
    p: &Palette,
    reset: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let line = p.line;
    div()
        .id(id)
        .debug_selector(move || id.into())
        .flex_none()
        .w(px(EDGE_W))
        .h_full()
        .cursor_col_resize()
        .hover(move |s| s.bg(line))
        .on_click(move |e, window, cx| {
            if e.click_count() == 2 {
                reset(e, window, cx);
            }
        })
        .on_drag(DraggedEdge(which), |d, _, _, cx| cx.new(|_| d.clone()))
}

impl MainView {
    /// How much of the window's width the docked drawers and the tab bar
    /// take, at `now` (the notes pane slides).
    pub(crate) fn side_room_at(&self, now: std::time::Instant) -> Pixels {
        px(self.side_tabs_w() + self.nearby.dock_w) + self.notes.room_at(now)
    }

    /// [`Self::side_room_at`], with the notes pane at its final place.
    pub(crate) fn side_room(&self) -> Pixels {
        px(self.side_tabs_w() + self.nearby.dock_w) + self.notes.room()
    }

    /// The tab bar's width (none in focus mode).
    fn side_tabs_w(&self) -> f32 {
        if self.side_tabs_shown() {
            TAB_BAR_W
        } else {
            0.
        }
    }

    fn side_tabs_shown(&self) -> bool {
        !self.focus_on()
    }

    /// The editor | preview divider is dragged (`e.bounds`: the row).
    fn side_drag_preview(&mut self, e: &DragMoveEvent<DraggedEdge>, cx: &mut Context<Self>) {
        let x = f32::from(e.event.position.x);
        let b = e.bounds;
        let total = f32::from(b.size.width);
        let share = dragged_share(f32::from(b.left()), total, x);
        self.set_preview_left(clamp_share(share, total, EDITOR_MIN_PX, PREVIEW_MIN_PX), cx);
    }

    /// The editor's share of the editor | preview row (saved once it stops
    /// changing).
    pub(crate) fn set_preview_left(&mut self, share: f32, cx: &mut Context<Self>) {
        if (share - self.side.preview_left).abs() < 0.0005 {
            return;
        }
        self.side.preview_left = share;
        self.side.preview_gen += 1;
        let gen_ = self.side.preview_gen;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(SAVE_MS))
                .await;
            let _ = this.update(cx, |v, _| {
                if v.side.preview_gen == gen_ {
                    v.side.save();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Hook: the editor | preview row, with its divider. `editor` and
    /// `preview` are the two panes; the row is `total` wide.
    pub(super) fn side_preview_row(
        &self,
        editor: AnyElement,
        preview: AnyElement,
        total: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let share = clamp_share(
            self.side.preview_left,
            f32::from(total),
            EDITOR_MIN_PX,
            PREVIEW_MIN_PX,
        );
        div()
            .id("preview-row")
            .debug_selector(|| "preview-row".into())
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_row()
            .on_drag_move::<DraggedEdge>(cx.listener(
                |this, e: &DragMoveEvent<DraggedEdge>, _, cx| {
                    if e.drag(cx).0 == Which::Preview {
                        this.side_drag_preview(e, cx);
                    }
                },
            ))
            .child(
                div()
                    .flex_none()
                    .w(relative(share))
                    .h_full()
                    .min_h_0()
                    .flex()
                    .child(editor),
            )
            .child(edge_handle(
                "preview-divider",
                Which::Preview,
                &p,
                cx.listener(|this, _, _, cx| this.set_preview_left(PREVIEW_DEFAULT, cx)),
            ))
            .child(div().flex_1().min_w_0().h_full().flex().child(preview))
            .into_any_element()
    }

    /// The editor's share of the row this frame (for its text measure).
    pub(super) fn side_preview_share(&self, total: Pixels) -> f32 {
        clamp_share(
            self.side.preview_left,
            f32::from(total),
            EDITOR_MIN_PX,
            PREVIEW_MIN_PX,
        )
    }

    /// Hook: the docked drawers and the tab bar, right of `main`.
    pub(super) fn side_row(&mut self, main: AnyElement, cx: &mut Context<Self>) -> AnyElement {
        let nearby = self.render_nearby_rail(cx);
        let notes = self.render_notes_drawer(cx);
        let tabs = self.render_side_tabs(cx);
        div()
            .id("side-row")
            .flex_1()
            .min_h_0()
            .w_full()
            .flex()
            .flex_row()
            .on_drag_move::<DraggedEdge>(cx.listener(
                |this, e: &DragMoveEvent<DraggedEdge>, _, cx| {
                    if e.drag(cx).0 != Which::Preview {
                        this.side_drag_drawer(e, cx);
                    }
                },
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    // The screens never spill under the drawers.
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .child(main),
            )
            .children(nearby)
            .children(notes)
            .children(tabs)
            .into_any_element()
    }

    /// A drawer's edge is dragged: its right edge is where the drawers
    /// end (the tab bar's left, less the notes pane for Nearby).
    fn side_drag_drawer(&mut self, e: &DragMoveEvent<DraggedEdge>, cx: &mut Context<Self>) {
        let x = f32::from(e.event.position.x);
        let right = f32::from(e.bounds.right()) - self.side_tabs_w();
        match e.drag(cx).0 {
            Which::Notes => {
                let w = crate::app::notes::SPAN.dragged(right, x);
                self.notes_set_width(w, cx);
            }
            Which::Nearby => {
                let notes = f32::from(self.notes.room());
                let w = nearby::SPAN.dragged(right - notes, x);
                self.nearby_set_width(w, cx);
            }
            Which::Preview => {}
        }
    }

    /// The tab bar: a labelled tab per drawer.
    fn render_side_tabs(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.side_tabs_shown() {
            return None;
        }
        let p = self.palette;
        let chrome = self.theme.chrome_font(&self.prefs);
        let tab =
            |id: &'static str, glyph: &'static str, label: &'static str, tip: String, on: bool| {
                div()
                    .id(id)
                    .debug_selector(move || id.into())
                    .w_full()
                    .py(px(7.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(2.))
                    .cursor_pointer()
                    .border_l_2()
                    .border_color(if on { p.accent } else { transparent_black() })
                    .when(on, |d| d.bg(p.pick()))
                    .hover(|s| s.bg(p.hover()))
                    .text_color(if on { p.ink } else { p.muted })
                    // A click on a tab leaves the keyboard where it is (the
                    // editor keeps it while you open a drawer to read).
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .tooltip(move |_, cx| cx.new(|_| crate::app::reading::Tip(tip.clone())).into())
                    .child(div().text_size(px(14.)).line_height(px(16.)).child(glyph))
                    .child(div().text_size(px(8.5)).child(label))
            };
        let notes_tip = format!("Notes · {}", crate::keymap::hint("⇧⌘N"));
        let nearby_tip = format!("Nearby thoughts · {}", crate::keymap::hint("⌥⌘U"));
        Some(
            div()
                .id("side-tabs")
                .debug_selector(|| "side-tabs".into())
                .flex_none()
                .w(px(TAB_BAR_W))
                .h_full()
                .flex()
                .flex_col()
                .pt(px(6.))
                .gap(px(2.))
                .rule_l(&p)
                .bg(p.bg)
                .font_family(chrome)
                .child(
                    tab("side-tab-notes", "✎", "Notes", notes_tip, self.notes.open).on_click(
                        cx.listener(|this, _, window, cx| this.notes_tab_click(window, cx)),
                    ),
                )
                .when(self.nearby_available(cx), |d| {
                    d.child(
                        tab(
                            "side-tab-nearby",
                            "≈",
                            "Nearby",
                            nearby_tip,
                            self.nearby.open,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.nearby_toggle(cx))),
                    )
                })
                .into_any_element(),
        )
    }

    /// `BLYGGER_DEMO=side-…` (screenshots): `side-writing` (a draft in the
    /// editor with a markdown-notes note open in the notes pane and Nearby
    /// open, the keyboard back in the draft, typing), `side-preview` (the
    /// preview beside the editor, its divider moved).
    pub(crate) fn side_demo(
        &mut self,
        scenario: &str,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match (scenario, n) {
            ("side-writing", 0) | ("side-preview", 0) => {
                self.open(&LocalId("01J9QK3".into()), window, cx)
            }
            ("side-writing", 1) => {
                self.nearby_set_open(true, cx);
                self.ext_lib_show(window, cx);
            }
            ("side-writing", 2) => {
                let id = self
                    .ext
                    .lib
                    .entries
                    .iter()
                    .find(|e| !e.is_dir)
                    .map(|e| e.id.clone());
                if let Some(id) = id {
                    self.ext_lib_open(id, window, cx);
                }
                cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(700))
                        .await;
                    let _ = this.update_in(cx, |v, window, cx| {
                        // esc in the pane: back to the draft; the pane stays.
                        v.notes_leave(window, cx);
                        v.demo_type(" Benches are small promises.", window, cx);
                    });
                })
                .detach();
            }
            ("side-preview", 1) => self.studio_set_view(studio::ViewMode::Split, cx),
            ("side-preview", 2) => self.set_preview_left(0.64, cx),
            _ => {}
        }
        cx.notify();
    }
}
