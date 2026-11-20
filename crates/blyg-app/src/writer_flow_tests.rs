//! --- user testing --- The writer's journeys for studio 0.64 parity, typed
//! in-process: a thread that tangles a passage and a placeholder, published
//! through the confirm step and revised; the format keys (bold, italic,
//! formula); a post with formulas; and a plain fragment as the everyday
//! check. Each step is a real keystroke through GPUI's dispatch, against a
//! zero-latency FakeBackend. `UT:` lines print what the writer saw (run
//! with `--nocapture`).

use super::*;

use crate::app::{PublishSheet, TangleSheet};

const THREAD: &str = crate::fake::tangle_seed::DEMO_THREAD;

fn toast(view: &Entity<MainView>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |v, _| {
        v.toast
            .as_ref()
            .map(|t| format!("{} | {}", t.text, t.sub.clone().unwrap_or_default()))
            .unwrap_or_default()
    })
}

fn version(fake: &FakeBackend, id: &str) -> u32 {
    fake.item(&LocalId(id.into())).unwrap().version
}

fn settle(cx: &mut VisualTestContext, what: &str, done: impl Fn(&mut VisualTestContext) -> bool) {
    crate::test_wait::until(cx, what, done);
}

/// Replace the open post's text by typing over a select-all.
fn type_over(cx: &mut VisualTestContext, text: &str) {
    keys(cx, "cmd-a");
    cx.simulate_input(text);
    cx.run_until_parked();
}

fn publish_warning(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |v, cx| v.studio_publish_warning(cx))
}

#[gpui_kit::test]
fn writer_tangles_a_passage_and_a_placeholder_confirms_publishes_and_revises(
    cx: &mut TestAppContext,
) {
    let (view, fake, cx) = setup(cx);
    open(&view, cx, THREAD);
    let v0 = version(&fake, THREAD);
    type_over(cx, crate::fake::tangle_seed::SAMPLE);
    assert_eq!(editor_text(&view, cx), crate::fake::tangle_seed::SAMPLE);

    // ⌘⏎: the usual publish sheet. Does it warn that new items are coming?
    keys(cx, "cmd-enter");
    assert!(view.read_with(cx, |v, _| v.sheet.is::<PublishSheet>()));
    eprintln!(
        "UT: publish sheet warning = {:?}",
        publish_warning(&view, cx)
    );

    // ⏎: the blyg asks first.
    keys(cx, "enter");
    settle(cx, "the tangle confirm step", |cx| {
        view.read_with(cx, |v, _| v.sheet.is::<TangleSheet>())
    });
    let (count, parts) = view.read_with(cx, |v, _| {
        let s = v.sheet.get::<TangleSheet>().unwrap();
        (s.count, s.parts.clone())
    });
    eprintln!(
        "UT: confirm = {:?} / {parts:?}",
        crate::app::sheet::publish::tangle_heading(count)
    );
    assert_eq!(count, 2);
    assert!(parts[1].ends_with("(linked)"), "{parts:?}");

    // esc backs out: nothing published, the text kept.
    keys(cx, "escape");
    assert!(!view.read_with(cx, |v, _| v.sheet.is_open()));
    assert_eq!(version(&fake, THREAD), v0, "esc publishes nothing");
    assert_eq!(editor_text(&view, cx), crate::fake::tangle_seed::SAMPLE);
    eprintln!("UT: after esc toast = {:?}", toast(&view, cx));

    // Again, all the way: ⌘⏎ ⏎ ⏎ (three keys).
    keys(cx, "cmd-enter");
    keys(cx, "enter");
    settle(cx, "the tangle confirm step again", |cx| {
        view.read_with(cx, |v, _| v.sheet.is::<TangleSheet>())
    });
    keys(cx, "enter");
    settle(cx, "the thread published", |_| {
        version(&fake, THREAD) == v0 + 1
    });
    settle(cx, "the toast", |cx| toast(&view, cx).contains("Published"));
    let after = editor_text(&view, cx);
    eprintln!("UT: toast = {:?}", toast(&view, cx));
    eprintln!("UT: editor after = {after:?}");
    assert!(
        !after.contains("[[+"),
        "the editor took the rewrite: {after}"
    );
    assert!(toast(&view, cx).contains("and 2 new items"));

    // Revise after publishing: type, publish again; no confirm step.
    keys(cx, "cmd-down");
    cx.simulate_input("Edited later.\n");
    cx.run_until_parked();
    keys(cx, "cmd-enter");
    keys(cx, "enter");
    settle(cx, "the revision published", |_| {
        version(&fake, THREAD) == v0 + 2
    });
    assert!(!view.read_with(cx, |v, _| v.sheet.is::<TangleSheet>()));
    let revised = editor_text(&view, cx);
    eprintln!("UT: revised = {revised:?}");
    assert!(revised.contains("Edited later."));
    // Undo after the rewrite: does ⌘Z bring the spans back?
    for n in 1..=3 {
        keys(cx, "cmd-z");
        eprintln!("UT: after {n}x undo = {:?}", editor_text(&view, cx));
    }
    // And publish what the undo left: the blyg would mint the parts again.
    let v2 = version(&fake, THREAD);
    keys(cx, "cmd-enter");
    keys(cx, "enter");
    settle(cx, "an answer to the undone text", |cx| {
        version(&fake, THREAD) > v2 || view.read_with(cx, |v, _| v.sheet.is::<TangleSheet>())
    });
    eprintln!(
        "UT: publish after undo → confirm again: {}",
        view.read_with(cx, |v, _| v.sheet.is::<TangleSheet>())
    );
}

#[gpui_kit::test]
fn writer_tangles_in_a_fragment(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    open(&view, cx, DRAFT);
    type_over(cx, "A fragment that tries [[+ a new idea]] inline.");
    keys(cx, "cmd-enter");
    eprintln!("UT: fragment warning = {:?}", publish_warning(&view, cx));
    keys(cx, "enter");
    settle(cx, "something answered", |cx| {
        !toast(&view, cx).is_empty() || view.read_with(cx, |v, _| v.sheet.is_open())
    });
    eprintln!(
        "UT: fragment tangle → toast {:?}, v{}, text {:?}",
        toast(&view, cx),
        version(&fake, DRAFT),
        editor_text(&view, cx)
    );
}

#[gpui_kit::test]
fn the_format_keys_bold_italic_and_formula(cx: &mut TestAppContext) {
    let (view, _, cx) = setup(cx);
    open(&view, cx, DRAFT);
    type_over(cx, "Make this loud");
    // Select the last word, ⌃⌘B, then again (plain).
    keys(cx, "alt-shift-left");
    keys(cx, "ctrl-cmd-b");
    assert_eq!(editor_text(&view, cx), "Make this **loud**");
    keys(cx, "ctrl-cmd-b");
    assert_eq!(editor_text(&view, cx), "Make this loud");
    keys(cx, "ctrl-cmd-i");
    assert_eq!(editor_text(&view, cx), "Make this *loud*");
    // A formula with nothing selected: an empty pair, the caret inside.
    keys(cx, "cmd-down");
    cx.simulate_input(" and ");
    keys(cx, "ctrl-cmd-m");
    cx.simulate_input("x^2");
    cx.run_until_parked();
    assert_eq!(editor_text(&view, cx), "Make this *loud* and $x^2$");
    // Two lines selected: displayed maths.
    type_over(cx, "a\nb");
    keys(cx, "cmd-a");
    keys(cx, "ctrl-cmd-m");
    eprintln!("UT: formula over two lines = {:?}", editor_text(&view, cx));
    // Bold with no selection: what does the writer get?
    type_over(cx, "word");
    keys(cx, "ctrl-cmd-b");
    cx.simulate_input("bold");
    cx.run_until_parked();
    eprintln!(
        "UT: bold, no selection, then typed = {:?}",
        editor_text(&view, cx)
    );

    // From the search box rather than the editor.
    keys(cx, "cmd-l");
    keys(cx, "ctrl-cmd-b");
    eprintln!("UT: ⌃⌘B in the omnibar → toast {:?}", toast(&view, cx));

    // Findable: the palette by name.
    keys(cx, "escape");
    keys(cx, "cmd-shift-p");
    cx.simulate_input("formula");
    cx.run_until_parked();
    let rows = shown(&view, cx);
    eprintln!("UT: palette 'formula' = {rows:?}");
    keys(cx, "escape");
    keys(cx, "cmd-shift-p");
    cx.simulate_input("bold");
    cx.run_until_parked();
    eprintln!("UT: palette 'bold' = {:?}", shown(&view, cx));
    keys(cx, "escape");
    keys(cx, "cmd-shift-p");
    cx.simulate_input("maths");
    cx.run_until_parked();
    eprintln!("UT: palette 'maths' = {:?}", shown(&view, cx));
}

#[gpui_kit::test]
fn writer_publishes_formulas_with_a_broken_one_and_money(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    open(&view, cx, DRAFT);
    let v0 = version(&fake, DRAFT);
    type_over(cx, crate::fake::maths_seed::DRAFT_MATHS);
    keys(cx, "cmd-enter");
    eprintln!(
        "UT: maths publish warning = {:?}",
        publish_warning(&view, cx)
    );
    keys(cx, "enter");
    settle(cx, "an answer", |cx| !toast(&view, cx).is_empty());
    eprintln!(
        "UT: broken formula publish → toast {:?}, v{} (was v{v0})",
        toast(&view, cx),
        version(&fake, DRAFT)
    );
}

#[gpui_kit::test]
fn everyday_a_plain_fragment_from_the_omnibar(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    keys(cx, "cmd-l");
    cx.simulate_input("Rain on the tin roof tonight");
    cx.run_until_parked();
    keys(cx, "enter");
    let id = view
        .read_with(cx, |v, _| v.current.as_ref().map(|c| c.local_id.clone()))
        .expect("a new draft is open");
    cx.simulate_input(", loud and steady.");
    cx.run_until_parked();
    let text = editor_text(&view, cx);
    eprintln!("UT: new draft = {text:?}");
    keys(cx, "cmd-p");
    eprintln!(
        "UT: after ⌘P view = {:?}",
        view.read_with(cx, |v, _| v.studio.view)
    );
    keys(cx, "cmd-enter");
    assert!(view.read_with(cx, |v, _| v.sheet.is::<PublishSheet>()));
    keys(cx, "enter");
    settle(cx, "published", |_| fake.item(&id).unwrap().version == 1);
    settle(cx, "the toast", |cx| toast(&view, cx).contains("Published"));
    eprintln!("UT: everyday toast = {:?}", toast(&view, cx));
    assert!(
        fake.item(&id)
            .unwrap()
            .content_md
            .contains("loud and steady")
    );
}
