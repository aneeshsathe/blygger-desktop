//! --- unread counts / mark all read --- Headless GPUI tests on the
//! zero-latency FakeBackend (pure UI): the reading header, All unread and
//! the status bar say the same number, and Mark All Read… asks with the
//! count, then marks exactly the posts to read in the source.

use std::sync::Arc;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::sources::{MenuCmd, MenuTarget};
use super::sources_vm::{self, Entry, Smart, Source};
use super::stream_vm::ReadMode;
use super::{RSheet, View};
use crate::app::MainView;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";

fn setup(
    cx: &mut TestAppContext,
    mode: ReadMode,
) -> (Entity<MainView>, Arc<FakeBackend>, &mut VisualTestContext) {
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
    let f2 = fake.clone();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        MainView::new(
            backend,
            Some(f2),
            prefs,
            std::time::Instant::now(),
            window,
            cx,
        )
    });
    cx.run_until_parked();
    view.update_in(cx, |v, window, cx| {
        if v.reading.view != View::Reading {
            v.show_view(View::Reading, window, cx);
        }
        v.set_read_mode(mode, window, cx)
    });
    settle(cx);
    (view, fake, cx)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..20 {
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

/// (the header's "N to read", All unread's count, the status bar's count).
fn counts(view: &Entity<MainView>, cx: &mut VisualTestContext) -> (usize, usize, usize) {
    view.read_with(cx, |v, _| {
        let header = sources_vm::unread_total(&v.reading.rows, &v.reading.hopper_only);
        let all_unread = v
            .source_entries()
            .into_iter()
            .find_map(|e| match e {
                Entry::Smart {
                    smart: Smart::Unread,
                    unread,
                } => Some(unread),
                _ => None,
            })
            .expect("All unread is listed");
        (header, all_unread, v.unread)
    })
}

/// The open Mark All question: (what, how many).
fn question(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Option<(String, usize)> {
    view.read_with(cx, |v, _| match &v.reading.sheet {
        Some(RSheet::MarkAll(m)) => Some((m.what.clone(), m.keys.len())),
        _ => None,
    })
}

fn to_read_in(view: &Entity<MainView>, sub: &str, cx: &mut VisualTestContext) -> usize {
    view.read_with(cx, |v, _| {
        v.reading
            .rows
            .iter()
            .filter(|r| r.subscription_id == sub && sources_vm::needs_reading(r))
            .count()
    })
}

#[gpui_kit::test]
fn the_header_all_unread_and_the_status_bar_agree(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx, ReadMode::Reader);
    // The sample data has a post edited since it was read: the old header
    // (never read only) and status bar left it out, All unread counted it.
    let edited = view.read_with(cx, |v, _| {
        v.reading
            .rows
            .iter()
            .any(|r| r.edited_since_read() && !sources_vm::is_background(r))
    });
    assert!(edited, "the seed has an edited-since-read post");
    let (header, all, status) = counts(&view, cx);
    assert!(header > 0);
    assert_eq!((header, all), (status, status), "one number everywhere");
    assert_eq!(fake.unread_count(), status);
    // Marking one read moves all three together.
    cx.simulate_keystrokes("j r");
    settle(cx);
    let (h2, a2, s2) = counts(&view, cx);
    assert_eq!((h2, a2), (s2, s2));
    assert_eq!(s2, status - 1);
}

#[gpui_kit::test]
fn shift_r_asks_with_the_count_then_marks_the_source_read(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx, ReadMode::Reader);
    view.update(cx, |v, cx| {
        v.select_source(Source::Sub("sub-rue".into()), cx)
    });
    settle(cx);
    let n = to_read_in(&view, "sub-rue", cx);
    assert!(n > 0, "Rue has posts to read");
    let before = counts(&view, cx).2;
    cx.simulate_keystrokes("shift-r");
    settle(cx);
    let (what, asked) = question(&view, cx).expect("it asks first");
    assert_eq!(asked, n);
    assert!(!what.is_empty());
    assert_eq!(to_read_in(&view, "sub-rue", cx), n, "nothing marked yet");
    // esc: nothing marked.
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert!(question(&view, cx).is_none());
    assert_eq!(to_read_in(&view, "sub-rue", cx), n);
    // ⏎: every one of them, in one call, and the counts follow.
    cx.simulate_keystrokes("shift-r");
    settle(cx);
    let sends_before = fake.reading_sends().len();
    cx.simulate_keystrokes("enter");
    settle(cx);
    assert!(question(&view, cx).is_none());
    assert_eq!(to_read_in(&view, "sub-rue", cx), 0);
    assert_eq!(
        fake.reading_sends().len() - sends_before,
        n,
        "one set_read naming each post"
    );
    let (h, a, s) = counts(&view, cx);
    assert_eq!((h, a, s), (before - n, before - n, before - n));
    // Nothing left: it says so instead of asking.
    cx.simulate_keystrokes("shift-r");
    settle(cx);
    assert!(question(&view, cx).is_none());
}

#[gpui_kit::test]
fn a_subscriptions_menu_marks_that_source_from_anywhere(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(cx, ReadMode::Reader);
    // The list shows All; the menu is Ada's.
    view.update(cx, |v, cx| {
        v.open_source_menu(
            MenuTarget::Sub("sub-ada".into()),
            gpui_kit::point(gpui_kit::px(10.), gpui_kit::px(10.)),
            false,
            cx,
        )
    });
    settle(cx);
    let item = view.read_with(cx, |v, _| {
        v.menu_items()
            .into_iter()
            .find(|(label, ..)| label == "Mark All Read…")
            .and_then(|(_, cmd, _)| cmd)
    });
    let cmd = item.expect("the subscription's menu has Mark All Read…");
    assert_eq!(
        cmd,
        MenuCmd::MarkAllRead(Some(Source::Sub("sub-ada".into())))
    );
    let n = to_read_in(&view, "sub-ada", cx);
    view.update_in(cx, |v, window, cx| v.run_menu_cmd(cmd, window, cx));
    settle(cx);
    assert_eq!(question(&view, cx).map(|q| q.1), Some(n));
    cx.simulate_keystrokes("enter");
    settle(cx);
    assert_eq!(to_read_in(&view, "sub-ada", cx), 0);
    // The pane's own menu names the source on screen.
    view.update(cx, |v, cx| {
        v.open_source_menu(
            MenuTarget::Pane,
            gpui_kit::point(gpui_kit::px(10.), gpui_kit::px(10.)),
            false,
            cx,
        )
    });
    settle(cx);
    let labels: Vec<String> =
        view.read_with(cx, |v, _| v.menu_items().into_iter().map(|i| i.0).collect());
    assert!(
        labels.iter().any(|l| l == "Mark All Read in All…"),
        "{labels:?}"
    );
}

#[gpui_kit::test]
fn the_stream_marks_everything_it_shows(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx, ReadMode::Stream);
    let total = counts(&view, cx).2;
    assert!(total > 0);
    cx.simulate_keystrokes("shift-r");
    settle(cx);
    let (what, n) = question(&view, cx).expect("asked");
    assert_eq!(what, "the stream");
    assert_eq!(n, total, "everything to read");
    cx.simulate_keystrokes("enter");
    settle(cx);
    // The fake hands its editor sample ("sub-shoreline") back unread on
    // every read, so only that one can stay; the counts still agree.
    let left = to_read_in(&view, "sub-shoreline", cx);
    assert_eq!(counts(&view, cx), (left, left, left));
    assert_eq!(fake.unread_count(), left);
    assert!(left <= 1);
}

#[gpui_kit::test]
fn the_palette_lists_mark_all_read(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(cx, ReadMode::Reader);
    let row = view.update_in(cx, |v, window, cx| {
        v.palette_entries(window, cx)
            .into_iter()
            .find(|e| e.id == super::marks::PALETTE_ID)
    });
    let row = row.expect("listed");
    assert_eq!(row.label, "Mark All Read…");
    assert!(row.disabled.is_none(), "on the reading screen it runs");
    view.update_in(cx, |v, window, cx| v.palette_run(row, false, window, cx));
    settle(cx);
    assert!(question(&view, cx).is_some(), "it asks");
}
