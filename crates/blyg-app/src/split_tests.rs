//! --- split --- two posts side by side, in a headless window on a
//! zero-latency FakeBackend.

use blyg_core::{Backend, LocalId};
use gpui_kit::{Entity, Focusable as _, TestAppContext, VisualTestContext};

use super::split::{Beside, own_status};
use super::trails_tests::{reading_on, settle, setup};
use crate::app::MainView;
use crate::app::reading::View;
use crate::app::reading::stream_vm::ReadMode;
use crate::fake::reading_seed::*;

const DRAFT: &str = "01J9QK3"; // a draft fragment
const PUBLIC: &str = "01J9PX1"; // public v1

fn id(s: &str) -> LocalId {
    LocalId(s.into())
}

fn open(view: &Entity<MainView>, cx: &mut VisualTestContext, which: &str) {
    view.update_in(cx, |v, window, cx| v.open(&id(which), window, cx));
    settle(cx);
}

fn keys(cx: &mut VisualTestContext, k: &str) {
    cx.simulate_keystrokes(&crate::keymap::keys(k));
    settle(cx);
}

fn beside(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |v, _| match &v.split.beside {
        Some(Beside::Own { id, .. }) => Some(format!("own:{}", id.0)),
        Some(Beside::Held { row, .. }) => Some(format!("held:{}", row.remote_id)),
        None => None,
    })
}

fn left(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |v, _| v.current.as_ref().map(|c| c.local_id.0.clone()))
}

/// (left editor focused, right editor focused)
fn focus(view: &Entity<MainView>, cx: &mut VisualTestContext) -> (bool, bool) {
    view.update_in(cx, |v, window, cx| {
        let l = v.editor.read(cx).focus_handle(cx).is_focused(window);
        let r = match &v.split.beside {
            Some(Beside::Own { editor, .. }) => editor.read(cx).focus_handle(cx).is_focused(window),
            _ => false,
        };
        (l, r)
    })
}

#[gpui_kit::test]
fn alt_cmd_enter_puts_the_selected_post_beside_the_one_before(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    open(&view, cx, DRAFT);
    open(&view, cx, PUBLIC);
    keys(cx, "alt-cmd-enter");
    assert_eq!(beside(&view, cx), Some(format!("own:{PUBLIC}")));
    assert_eq!(left(&view, cx).as_deref(), Some(DRAFT));

    // ⌥⌘] gives the keyboard to the right post, which saves on its own.
    keys(cx, "alt-cmd-]");
    assert_eq!(focus(&view, cx), (false, true));
    cx.simulate_input(" beside");
    settle(cx);
    assert!(
        fake.item(&id(PUBLIC))
            .unwrap()
            .content_md
            .contains(" beside")
    );
    assert!(
        !fake
            .item(&id(DRAFT))
            .unwrap()
            .content_md
            .contains(" beside")
    );
    let status = view.read_with(cx, |v, cx| match &v.split.beside {
        Some(Beside::Own {
            item,
            editor,
            error,
            ..
        }) => own_status(
            item,
            editor.read(cx).value().chars().count(),
            error.as_deref(),
        ),
        _ => String::new(),
    });
    assert!(
        status.contains("unpublished edits") || status.contains("v1"),
        "{status}"
    );

    // ⌥⌘[ gives it back to the left post.
    keys(cx, "alt-cmd-[");
    assert_eq!(focus(&view, cx), (true, false));

    // Opening the right post on the left closes the split: one post is
    // never in two editors.
    open(&view, cx, PUBLIC);
    assert_eq!(beside(&view, cx), None);
}

#[gpui_kit::test]
fn alt_cmd_enter_again_on_the_same_row_closes_it(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(cx);
    open(&view, cx, DRAFT);
    open(&view, cx, PUBLIC);
    keys(cx, "alt-cmd-enter");
    assert!(beside(&view, cx).is_some());
    view.update(cx, |v, _| v.list.select(&id(PUBLIC)));
    keys(cx, "alt-cmd-enter");
    assert_eq!(beside(&view, cx), None);
}

#[gpui_kit::test]
fn a_held_post_opens_read_only_beside_from_reading(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(cx);
    open(&view, cx, DRAFT);
    reading_on(&view, ReadMode::Stream, ("sub-lin", LIN_GARDENS), cx);
    keys(cx, "alt-cmd-enter");
    assert_eq!(view.read_with(cx, |v, _| v.reading.view), View::Posts);
    assert_eq!(beside(&view, cx), Some(format!("held:{LIN_GARDENS}")));
    assert_eq!(left(&view, cx).as_deref(), Some(DRAFT));
    view.update_in(cx, |v, window, cx| v.close_split(window, cx));
    assert_eq!(beside(&view, cx), None);
}

#[test]
fn the_right_editor_says_what_it_is() {
    let item = crate::fake::FakeBackend::new()
        .item(&id(DRAFT))
        .expect("the sample draft");
    let s = own_status(&item, 12, None);
    assert!(s.contains(" · 12"), "{s}");
    assert!(s.ends_with("saved"), "{s}");
    assert!(own_status(&item, 12, Some("offline")).ends_with("not saved: offline"));
}

#[test]
fn the_history_remembers_the_post_before() {
    let mut s = super::split::Split::new(None);
    assert!((s.left - 0.5).abs() < f32::EPSILON);
    s.saw(&id("a"));
    s.saw(&id("a"));
    s.saw(&id("b"));
    assert_eq!(s.before(&id("b")), Some(id("a")));
    s.saw(&id("c"));
    assert_eq!(s.before(&id("c")), Some(id("b")));
    assert_eq!(s.history.len(), 2);
}
