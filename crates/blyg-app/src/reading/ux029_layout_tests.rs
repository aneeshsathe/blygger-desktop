//! --- ux 0.29 --- The 0.29 desktop pass's layout findings, at the default
//! window (1100 × 720) and a narrow one (800 × 720): each scene is played
//! in-process (`run_demo`, what `BLYGGER_DEMO` plays), then the drawn boxes
//! are checked.

use std::sync::Arc;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore};
use gpui_kit::{Bounds, Entity, Pixels, TestAppContext, VisualTestContext, px, size};

use crate::app::MainView;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;
use crate::scenes::Scene;
use crate::test_wait::settle_drawn as settle;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";
const WIDTHS: [f32; 2] = [1100., 800.];
const HEIGHT: f32 = 720.;

fn setup(cx: &mut TestAppContext) -> (Entity<MainView>, &mut VisualTestContext) {
    let prefs = Prefs::from_config(ConfigStore::in_memory(CONNECTED).config());
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::bind_keys(cx);
        crate::settings::init(
            ConfigStore::in_memory(CONNECTED),
            Arc::new(MemoryTokenStore::default()),
            None,
            cx,
        );
    });
    let fake = Arc::new(FakeBackend::with_timing(Timing::instant()).without_media_cache());
    let backend: Arc<dyn Backend> = fake.clone();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        MainView::new(
            backend,
            Some(fake),
            prefs,
            std::time::Instant::now(),
            window,
            cx,
        )
    });
    cx.run_until_parked();
    (view, cx)
}

/// A fresh window `width` wide with `scene`'s three steps played.
fn play(
    cx: &mut TestAppContext,
    scene: Scene,
    width: f32,
) -> (Entity<MainView>, &mut VisualTestContext) {
    let (view, cx) = setup(cx);
    cx.simulate_resize(size(px(width), px(HEIGHT)));
    settle(cx);
    view.update_in(cx, |v, window, cx| v.run_demo(scene, window, cx));
    // The steps run at about 0.4, 1.3 and 2.5 s (`demo.rs`).
    for _ in 0..4 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(900));
        settle(cx);
    }
    (view, cx)
}

fn sel(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn drawn(cx: &mut VisualTestContext, what: &str) -> Bounds<Pixels> {
    cx.debug_bounds(sel(what.to_string()))
        .unwrap_or_else(|| panic!("{what} is drawn"))
}

fn window_w(cx: &mut VisualTestContext) -> f32 {
    cx.update(|window, _| window.viewport_size()).width.as_f32()
}

fn l(b: Bounds<Pixels>) -> f32 {
    b.left().as_f32()
}
fn r(b: Bounds<Pixels>) -> f32 {
    b.right().as_f32()
}
fn t(b: Bounds<Pixels>) -> f32 {
    b.top().as_f32()
}
fn bot(b: Bounds<Pixels>) -> f32 {
    b.bottom().as_f32()
}
fn w(b: Bounds<Pixels>) -> f32 {
    b.size.width.as_f32()
}

/// `inner` lies inside `outer`, left to right.
fn within_x(inner: Bounds<Pixels>, outer: Bounds<Pixels>) -> bool {
    l(inner) >= l(outer) - 0.5 && r(inner) <= r(outer) + 0.5
}

/// User testing (B1): at 800 px the action ring's answers ("yes, as a
/// reply" …) stood one letter per line in a column beside the graph, and
/// ran off the window. Each answer keeps a readable width inside the sheet,
/// and so does "⏎ or R again does it".
#[gpui_kit::test]
fn the_lineage_rings_answers_stay_readable(cx: &mut TestAppContext) {
    for width in WIDTHS {
        let (_, cx) = play(cx, Scene::RdStreamLineageRing, width);
        let sheet = drawn(cx, "reading-sheet");
        assert!(
            r(sheet) <= window_w(cx) + 0.5,
            "{width}: the sheet fits the window"
        );
        for i in 0..super::lineage_vm::FACTS.len() {
            let a = drawn(cx, &format!("lineage-fact-{i}"));
            assert!(
                w(a) >= 120. && within_x(a, sheet),
                "{width}: answer {i} is {a:?}, in a sheet {sheet:?}"
            );
        }
        let does = drawn(cx, "lineage-does-it");
        assert!(
            w(does) >= 200. && within_x(does, sheet),
            "{width}: {does:?}"
        );
    }
}

/// User testing (B2): at 800 px Browsing history squeezed the reader to
/// about 55 px, and the post drew through the panel. The post keeps a
/// reading width, and the panel sits beside it, not over it.
#[gpui_kit::test]
fn browsing_history_leaves_the_post_its_room(cx: &mut TestAppContext) {
    for width in WIDTHS {
        let (_, cx) = play(cx, Scene::RdBrowseHistory, width);
        let post = drawn(cx, "reading-post");
        let hist = drawn(cx, "browse-history");
        assert!(
            w(post) >= super::pane_fit::POST_MIN - 0.5,
            "{width}: the post is {} wide",
            w(post)
        );
        assert!(
            r(post) <= l(hist) + 0.5,
            "{width}: {post:?} runs under {hist:?}"
        );
        assert!(r(hist) <= window_w(cx) + 0.5, "{width}: the panel fits");
    }
}

/// User testing (I1): the header's keys were cut mid-hint with "…" ("/
/// searc…", "r marks…"). Each hint is drawn whole, or not at all (the
/// least-used, last ones go first); the first is always there.
#[gpui_kit::test]
fn header_hints_are_whole_or_gone(cx: &mut TestAppContext) {
    for (scene, width) in [
        (Scene::RdFeed, 1100.),
        (Scene::RdStream, 800.),
        (Scene::RdPicks, 800.),
        (Scene::RdFeed, 800.),
        (Scene::RdSubs, 800.),
        (Scene::RdMentions, 800.),
    ] {
        let (_, cx) = play(cx, scene, width);
        let box_ = drawn(cx, "screen-hint");
        let first = drawn(cx, "screen-hint-0");
        assert!(
            within_x(first, box_) && t(first) < bot(box_),
            "{scene} at {width}: the first hint is shown"
        );
        for i in 1..20 {
            let Some(h) = cx.debug_bounds(sel(format!("screen-hint-{i}"))) else {
                break;
            };
            let shown = t(h) < bot(box_) - 0.5;
            assert!(
                !shown || within_x(h, box_),
                "{scene} at {width}: hint {i} is cut: {h:?} in {box_:?}"
            );
        }
    }
}

/// User testing (I2): at 800 px a side panel left the pane beside it a
/// sliver (Versions' preview about 150 px, the editor beside the notes
/// drawer 200 px, the author's site about 330 px). The pane keeps its
/// minimum; the list column makes way.
#[gpui_kit::test]
fn side_panels_leave_the_pane_its_room(cx: &mut TestAppContext) {
    use crate::app::side::{EDITOR_MIN_PX, VERSIONS_DOC_MIN_PX};
    for width in WIDTHS {
        let (_, cx) = play(cx, Scene::RdVersions, width);
        let doc = drawn(cx, "own-doc");
        assert!(
            w(doc) >= VERSIONS_DOC_MIN_PX - 0.5,
            "{width}: Versions' preview is {doc:?}"
        );

        let (_, cx) = play(cx, Scene::NotesPosts, width);
        let ed = drawn(cx, "editor-pane");
        let notes = drawn(cx, "notes-drawer");
        assert!(
            w(ed) >= EDITOR_MIN_PX - 0.5,
            "{width}: the editor is {ed:?}"
        );
        assert!(r(ed) <= l(notes) + 0.5, "{width}: beside the drawer");
    }
}

/// User testing (I2): at 800 px the notes drawer kept its 380 px and left
/// the editor 200. In a narrow window it takes a smaller share (never under
/// its minimum).
#[test]
fn the_notes_drawer_shares_a_narrow_window() {
    use crate::app::notes::{MAX_SHARE, NARROW_SHARE, max_share};
    assert_eq!(max_share(1100.), MAX_SHARE);
    assert_eq!(max_share(800.), NARROW_SHARE);
    const { assert!(800. * NARROW_SHARE < 380.) };
}

/// User testing (I2): over the notes drawer the browser pane's toolbar drew
/// on the drawer's header, and the page beside it was a sliver. In a narrow
/// room the pane takes the whole room; it never reaches past it.
#[test]
fn the_browser_pane_fills_a_narrow_room() {
    use crate::app::browser::{SLIDE_MIN_PX, SLIDE_WIDTH, slide_left_edge};
    // Wide: the slide-over leaves the left third.
    let wide = slide_left_edge(1066.);
    assert_eq!(wide, (1066. * (1. - SLIDE_WIDTH)).round());
    assert!(1066. - wide >= SLIDE_MIN_PX);
    // 800 px with the notes drawer: the room is 386 px; the pane takes it.
    assert_eq!(slide_left_edge(386.), 0.);
    // 800 px alone: 766 × 0.66 = 505 is still room for a page.
    assert!(766. - slide_left_edge(766.) >= SLIDE_MIN_PX);
}

/// User testing (I8): toasts were cut at the editor's edge ("…above the
/// edito") and covered the editor's footer and the Reading compose bar.
/// A toast stays inside the window, above those.
#[gpui_kit::test]
fn toasts_sit_above_the_footers(cx: &mut TestAppContext) {
    const LONG: &str = "Quoting the whole post · choose a passage above the editor to quote only part of it, then write your reply below";
    for width in WIDTHS {
        let (view, cx) = play(cx, Scene::RdStub, width);
        view.update(cx, |v, cx| {
            v.show_toast("Replying to lin.blyg.example.com", Some(LONG.into()), cx)
        });
        settle(cx);
        settle(cx);
        let toast = drawn(cx, "toast");
        let strip = drawn(cx, "lineage-strip");
        assert!(
            r(toast) <= window_w(cx) + 0.5 && l(toast) >= 0.,
            "{width}: {toast:?}"
        );
        assert!(w(toast) <= 440., "{width}: it wraps: {toast:?}");
        assert!(
            bot(toast) <= t(strip) + 0.5,
            "{width}: the toast {toast:?} covers the editor's footer {strip:?}"
        );

        let (view, cx) = play(cx, Scene::RdStream, width);
        view.update(cx, |v, cx| {
            v.show_toast("Nothing to catch up on", Some(LONG.into()), cx)
        });
        settle(cx);
        settle(cx);
        let toast = drawn(cx, "toast");
        let compose = drawn(cx, "reading-compose");
        assert!(
            bot(toast) <= t(compose) + 0.5,
            "{width}: the toast {toast:?} covers the compose bar {compose:?}"
        );
    }
}

/// User testing (I8): with the native preview open, the toast's right part
/// went under the web view (its place counted the preview's width but not
/// the drawers right of it). It ends left of the view's left edge.
#[test]
fn a_toast_ends_left_of_the_native_view() {
    use crate::app::studio::toast_right_of;
    // A 1100 px window, the tab bar (34) right of a preview from 667 to 1066.
    assert_eq!(toast_right_of(px(1100.), px(667.)), px(1100. - 667. + 14.));
}

/// User testing (I12): tall sheets ran over the status bar or off the
/// window (Site settings showed no Save; AI on your blyg's keys sat on the
/// status bar). A sheet stops above the status bar, and its Save stays in
/// view.
#[gpui_kit::test]
fn sheets_keep_their_actions_in_view(cx: &mut TestAppContext) {
    let floor = HEIGHT - crate::app::STATUS_H;
    for width in WIDTHS {
        for (scene, frame, save) in [
            (Scene::AaAi, "sheet", "server-ai-save"),
            (Scene::RdSite, "reading-sheet", "site-save"),
        ] {
            let (_, cx) = play(cx, scene, width);
            let sheet = drawn(cx, frame);
            assert!(
                bot(sheet) <= floor + 0.5,
                "{scene} at {width}: {sheet:?} reaches the status bar"
            );
            let s = drawn(cx, save);
            assert!(
                t(s) >= t(sheet) && bot(s) <= bot(sheet) + 0.5,
                "{scene} at {width}: Save {s:?} is outside the sheet {sheet:?}"
            );
        }
    }
}

/// User testing (I11): the conflict sheet's two texts stopped mid-sentence
/// with nothing to say they scroll. A long text says so.
#[gpui_kit::test]
fn the_conflict_panes_say_they_scroll(cx: &mut TestAppContext) {
    let (_, cx) = play(cx, Scene::Conflict, 1100.);
    for side in ["mine", "theirs"] {
        assert!(
            cx.debug_bounds(sel(format!("conflict-more-{side}")))
                .is_some(),
            "{side}: a cue that the text goes on"
        );
    }
}
