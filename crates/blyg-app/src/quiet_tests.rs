//! --- quiet --- Quiet writing in the window: the lineage line, the
//! loose-ends line and their work go; ⌘J, the operators, Settings' row,
//! the menu item and a reload of the file all do what they say. (Nearby's
//! side of it is in `nearby/tests.rs`, on a real store.)

use blyg_core::LocalId;
use blyg_core::config::Change;
use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext};

use super::MainView;
use super::quiet::{self, ToggleQuietWriting};
use crate::app::trails_tests::{settle, setup};

/// A published sample post that others quote and reply to (its line reads
/// "drawn on by 2").
const FRICTION: &str = "01J9M2A";
/// A draft fragment.
const DRAFT: &str = "01J9QK3";

fn config_text(cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| {
        crate::settings::get(cx)
            .store
            .text()
            .unwrap_or_default()
            .to_string()
    })
}

fn set(view: &Entity<MainView>, on: bool, cx: &mut VisualTestContext) {
    view.update(cx, |v, cx| assert!(v.set_quiet_writing(on, cx)));
    redraw(cx);
}

fn redraw(cx: &mut VisualTestContext) {
    settle(cx);
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

fn open(view: &Entity<MainView>, id: &str, cx: &mut VisualTestContext) {
    view.update_in(cx, |v, window, cx| v.open(&LocalId(id.into()), window, cx));
    redraw(cx);
}

#[gpui_kit::test]
fn off_by_default(cx: &mut TestAppContext) {
    let (_view, _fake, cx) = setup(cx);
    assert!(!cx.update(|_, cx| quiet::on(cx)));
}

/// The line isn't drawn or built; ⌘J still reaches the lineage: the sheet
/// for a published post, the lists (and so the line) for a draft.
#[gpui_kit::test]
fn the_lineage_line_hides_and_cmd_j_still_reaches_it(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(cx);
    set(&view, true, cx);
    // Nothing is built for the post: the line is neither drawn nor worked out.
    open(&view, FRICTION, cx);
    assert!(view.read_with(cx, |v, _| v.lineage_strip_shown().is_none()));
    assert!(cx.debug_bounds("lineage-strip").is_none(), "hidden");
    // Off: there it is.
    set(&view, false, cx);
    assert!(cx.debug_bounds("lineage-strip").is_some(), "shown when off");
    set(&view, true, cx);
    assert!(cx.debug_bounds("lineage-strip").is_none(), "hidden again");
    // ⌘J opens the sheet for a published post, as ever.
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-j"));
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.lineage_open()));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    // A draft that quotes: ⌘J opens the lists, and the line with them.
    let text = format!("Pools at dusk\n\n![[{}]]\n", crate::fake::SAMPLE_QUOTE_ID);
    view.update_in(cx, |v, window, cx| {
        v.open(&LocalId(DRAFT.into()), window, cx);
        v.set_editor_text(&text, window, cx);
        v.after_edit(cx);
    });
    redraw(cx);
    assert!(cx.debug_bounds("lineage-strip").is_none());
    view.update(cx, |v, cx| assert!(v.lineage_strip_toggle(cx)));
    redraw(cx);
    assert!(cx.debug_bounds("lineage-strip").is_some(), "⌘J shows it");
    assert!(cx.debug_bounds("lineage-strip-lists").is_some());
    // A click folds it, and quiet writing hides it again.
    let b = cx.debug_bounds("lineage-strip").unwrap();
    cx.simulate_click(b.center(), Modifiers::none());
    redraw(cx);
    assert!(cx.debug_bounds("lineage-strip").is_none());
}

/// The summary over the list isn't worked out or drawn; the operators
/// still filter; off brings the line back.
#[gpui_kit::test]
fn the_loose_ends_line_hides_and_the_operators_still_work(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(cx);
    redraw(cx);
    assert!(
        cx.debug_bounds("loose-ends").is_some(),
        "the sample is loose"
    );

    set(&view, true, cx);
    assert!(cx.debug_bounds("loose-ends").is_none());
    view.update(cx, |v, cx| v.loose_ends_refresh(cx));
    redraw(cx);
    assert!(
        view.read_with(cx, |v, _| v.loose_ends.summary.parts().is_empty()),
        "nothing worked out"
    );
    cx.simulate_input("is:draft");
    cx.run_until_parked();
    view.read_with(cx, |v, _| {
        assert!(!v.list.results().is_empty());
        assert!(
            v.list
                .results()
                .iter()
                .all(|i| i.status == blyg_core::Status::Draft)
        );
    });

    view.update_in(cx, |v, window, cx| v.set_query_text("", window, cx));
    set(&view, false, cx);
    assert!(cx.debug_bounds("loose-ends").is_some(), "back");
}

/// Settings' row writes the key; View › Quiet Writing flips it and says so;
/// an edit to the file applies on reload.
#[gpui_kit::test]
fn settings_the_menu_and_the_file_all_set_it(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(cx);
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-,"));
    redraw(cx);
    crate::app::toolbar::tests::scroll_into_view(cx, "quiet-writing-on");
    let on = cx.debug_bounds("quiet-writing-on").expect("the row");
    cx.simulate_click(on.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(config_text(cx).contains("quiet-writing = true"));
    assert!(cx.update(|_, cx| quiet::on(cx)));
    let off = cx.debug_bounds("quiet-writing-off").expect("the row");
    cx.simulate_click(off.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(config_text(cx).contains("quiet-writing = false"));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    // The menu item (and the palette's row) flips it, with a word.
    cx.dispatch_action(ToggleQuietWriting);
    cx.run_until_parked();
    assert!(cx.update(|_, cx| quiet::on(cx)));
    assert_eq!(
        view.read_with(cx, |v, _| v.toast.as_ref().map(|t| t.text.to_string())),
        Some("Quiet writing on".into())
    );

    // Edited in the file, then ⌘⇧, : what was opened for the sitting goes.
    view.update(cx, |v, _| {
        v.nearby.sitting = true;
        v.lineage_strip.open = true;
    });
    cx.update(|_, cx| {
        crate::settings::write(&[(quiet::KEY, Change::Set("false".into()))], cx).unwrap();
    });
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-shift-,"));
    cx.run_until_parked();
    assert!(!cx.update(|_, cx| quiet::on(cx)));
    view.read_with(cx, |v, _| {
        assert!(!v.nearby.sitting);
        assert!(!v.lineage_strip.open);
    });
}
