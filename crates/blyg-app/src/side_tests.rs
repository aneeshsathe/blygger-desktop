//! --- side --- The side drawers' and the preview divider's layout state:
//! clamping, resetting, persisting.

use super::side::{
    PREVIEW_DEFAULT, Span, clamp_share, dragged_share, share_from_saved, share_to_saved,
};

const SPAN: Span = Span {
    default: 380.,
    min: 280.,
    max: 760.,
};

#[test]
fn a_drawer_fits_its_range_and_the_window() {
    // In range, in a wide window: as chosen.
    assert_eq!(SPAN.fit(420., 1600., 0.6), 420.);
    // Clamped to its range.
    assert_eq!(SPAN.fit(100., 1600., 0.6), 280.);
    assert_eq!(SPAN.fit(2000., 1600., 0.6), 760.);
    // At most 60% of the window, but not under the minimum...
    assert_eq!(SPAN.fit(700., 1000., 0.6), 600.);
    assert_eq!(SPAN.fit(700., 400., 0.6), 280.);
    // ...unless the window itself is narrower.
    assert_eq!(SPAN.fit(700., 200., 0.6), 200.);
}

#[test]
fn a_drag_of_the_edge_sets_the_width_in_range() {
    assert_eq!(SPAN.dragged(1200., 700.), 500.);
    assert_eq!(SPAN.dragged(1200., 1150.), 280.);
    assert_eq!(SPAN.dragged(1200., 0.), 760.);
}

#[test]
fn widths_persist_only_when_they_differ_from_the_default() {
    assert_eq!(SPAN.saved(380.), None);
    assert_eq!(SPAN.saved(452.4), Some(452));
    assert_eq!(SPAN.restore(Some(452)), 452.);
    assert_eq!(SPAN.restore(None), 380.);
    // A hand-edited state.json can't make it unusable.
    assert_eq!(SPAN.restore(Some(5)), 280.);
    assert_eq!(SPAN.restore(Some(99999)), 760.);
}

#[test]
fn the_preview_divider_keeps_both_sides_usable() {
    // Within the minimums: as dragged.
    assert_eq!(clamp_share(0.6, 1000., 320., 260.), 0.6);
    // The editor keeps 320 points, the preview 260.
    assert_eq!(clamp_share(0.1, 1000., 320., 260.), 0.32);
    assert_eq!(clamp_share(0.95, 1000., 320., 260.), 0.74);
    // Too narrow for both: the middle.
    assert_eq!(clamp_share(0.8, 500., 320., 260.), 0.5);
    assert_eq!(dragged_share(100., 1000., 600.), 0.5);
    assert_eq!(dragged_share(100., 1000., 0.), 0.);
}

#[test]
fn the_preview_share_persists_and_resets() {
    assert_eq!(
        share_to_saved(PREVIEW_DEFAULT),
        None,
        "the default isn't kept"
    );
    assert_eq!(share_to_saved(0.62), Some(620));
    assert_eq!(share_from_saved(Some(620)), 0.62);
    assert_eq!(share_from_saved(None), PREVIEW_DEFAULT);
    assert_eq!(share_from_saved(Some(5)), 0.1, "clamped");
}

#[test]
fn layout_state_round_trips_through_state_json() {
    let dir = tempfile::tempdir().unwrap();
    blyg_core::state::AppState::update(dir.path(), |s| {
        s.notes_open = true;
        s.notes_width = Some(500);
        s.preview_left = Some(620);
        s.nearby_open = true;
        s.nearby_width = Some(300);
    })
    .unwrap();
    let notes = crate::app::notes::Notes::new(Some(dir.path().to_path_buf()));
    assert!(notes.reopen, "opens again at launch");
    assert_eq!(notes.width, 500.);
    let side = super::side::Side::new(Some(dir.path().to_path_buf()));
    assert_eq!(side.preview_left, 0.62);
    // Nothing saved: closed, default widths, the middle.
    let empty = tempfile::tempdir().unwrap();
    let notes = crate::app::notes::Notes::new(Some(empty.path().to_path_buf()));
    assert!(!notes.reopen);
    assert_eq!(notes.width, crate::app::notes::WIDTH);
    assert_eq!(
        super::side::Side::new(Some(empty.path().to_path_buf())).preview_left,
        PREVIEW_DEFAULT
    );
}

// ------------------------------------------------------------ in a window

mod ui {
    use blyg_core::LocalId;
    use gpui_kit::{Modifiers, MouseButton, TestAppContext, point, px, size};

    use crate::app::side::PREVIEW_DEFAULT;
    use crate::app::studio::ViewMode;
    use crate::app::trails_tests::{settle, setup};

    /// ⌘E's preview beside the editor: the divider between them drags (the
    /// editor keeps its minimum), and a double-click centres it.
    #[gpui_kit::test]
    fn the_preview_divider_drags_and_resets(cx: &mut TestAppContext) {
        let (view, _fake, cx) = setup(cx);
        cx.simulate_resize(size(px(1400.), px(900.)));
        view.update_in(cx, |v, window, cx| {
            v.open(&LocalId("01J9QK3".into()), window, cx);
            v.studio_set_view(ViewMode::Split, cx);
        });
        settle(cx);
        let row = cx.debug_bounds("preview-row").expect("editor | preview");
        let handle = cx.debug_bounds("preview-divider").expect("the divider");
        let from = handle.center();
        let to = point(row.left() + row.size.width * 0.7, from.y);
        cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
        // The first move starts the drag; the next one moves it.
        cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::none());
        cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::none());
        cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
        settle(cx);
        let share = view.read_with(cx, |v, _| v.side.preview_left);
        assert!((share - 0.7).abs() < 0.02, "{share}");
        let editor = cx.debug_bounds("editor-pane").expect("the editor");
        assert!(editor.size.width > row.size.width * 0.6, "{editor:?}");
        // Dragged hard left: the editor keeps its minimum.
        let handle = cx.debug_bounds("preview-divider").unwrap();
        let far = point(row.left() + px(10.), from.y);
        cx.simulate_mouse_down(handle.center(), MouseButton::Left, Modifiers::none());
        // The first move starts the drag; the next one moves it.
        cx.simulate_mouse_move(far, Some(MouseButton::Left), Modifiers::none());
        cx.simulate_mouse_move(far, Some(MouseButton::Left), Modifiers::none());
        cx.simulate_mouse_up(far, MouseButton::Left, Modifiers::none());
        settle(cx);
        let editor = cx.debug_bounds("editor-pane").unwrap();
        assert!(
            f32::from(editor.size.width) >= crate::app::side::EDITOR_MIN_PX - 1.,
            "{editor:?}"
        );
        // A double-click: the middle again.
        let handle = cx.debug_bounds("preview-divider").unwrap();
        cx.simulate_event(gpui_kit::MouseDownEvent {
            position: handle.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 2,
            first_mouse: false,
        });
        cx.simulate_event(gpui_kit::MouseUpEvent {
            position: handle.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
            click_count: 2,
        });
        settle(cx);
        assert_eq!(
            view.read_with(cx, |v, _| v.side.preview_left),
            PREVIEW_DEFAULT
        );
    }

    /// Focus mode hides the tab bar; leaving it brings the bar back.
    #[gpui_kit::test]
    fn focus_mode_hides_the_tab_bar(cx: &mut TestAppContext) {
        let (view, _fake, cx) = setup(cx);
        view.update_in(cx, |v, window, cx| {
            v.open(&LocalId("01J9QK3".into()), window, cx)
        });
        settle(cx);
        assert!(cx.debug_bounds("side-tabs").is_some());
        view.update_in(cx, |v, window, cx| v.enter_focus(window, cx));
        settle(cx);
        assert!(view.read_with(cx, |v, _| v.focus_on()));
        assert!(cx.debug_bounds("side-tabs").is_none());
        view.update_in(cx, |v, window, cx| v.leave_focus(window, cx));
        settle(cx);
        assert!(cx.debug_bounds("side-tabs").is_some());
    }
}
