//! --- read/unread --- ⌘A in the Reader picks every post its list shows,
//! as each platform binds it: Ctrl+A on Windows. The list used to read
//! GPUI's `platform` modifier (the Windows key there) and refuse Ctrl, so
//! Ctrl+A did nothing on Windows while the header's hint said Ctrl+A. The
//! search field's own ⌘A still selects its text.
//!
//! ⌘-click picks one row the same way: Ctrl+click on Windows. It read
//! `platform` too, so a Ctrl+click opened the post there instead.

use std::sync::Arc;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore};
use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext};

use crate::app::MainView;
use crate::fake::{FakeBackend, Timing};
use crate::platform_text::Platform;
use crate::prefs::Prefs;
use crate::test_wait::settle;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";

fn setup(p: Platform, cx: &mut TestAppContext) -> (Entity<MainView>, &mut VisualTestContext) {
    let prefs = Prefs::from_config(ConfigStore::in_memory(CONNECTED).config());
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::windows_keys_tests::bind_keys_as(p, cx);
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
    view.update_in(cx, |v, window, cx| {
        v.set_read_mode(super::stream_vm::ReadMode::Reader, window, cx);
        window.focus(&v.reading.focus, cx);
    });
    settle(cx);
    (view, cx)
}

fn select_all(p: Platform) -> &'static str {
    match p {
        Platform::Mac => "cmd-a",
        Platform::Windows => "ctrl-a",
    }
}

fn picks_all_with(p: Platform, cx: &mut TestAppContext) {
    let (view, cx) = setup(p, cx);
    let shown = view.read_with(cx, |v, _| v.shown_keys());
    assert!(shown.len() >= 2, "rows to pick: {}", shown.len());
    assert!(view.read_with(cx, |v, _| v.reading.picked.is_empty()));
    cx.simulate_keystrokes(select_all(p));
    settle(cx);
    view.read_with(cx, |v, _| {
        for k in &shown {
            assert!(v.reading.picked.contains(k), "{p:?}: {k:?} picked");
        }
    });
}

#[gpui_kit::test]
fn select_all_picks_every_row_on_the_windows_key_table(cx: &mut TestAppContext) {
    picks_all_with(Platform::Windows, cx);
}

#[gpui_kit::test]
fn select_all_picks_every_row_on_the_mac_key_table(cx: &mut TestAppContext) {
    picks_all_with(Platform::Mac, cx);
}

/// In the search field ⌘A is the field's: it selects the query, and
/// nothing is picked.
#[gpui_kit::test]
fn select_all_in_the_search_field_picks_nothing_on_windows(cx: &mut TestAppContext) {
    let (view, cx) = setup(Platform::Windows, cx);
    view.update_in(cx, |v, window, cx| v.focus_reading_search(window, cx));
    settle(cx);
    cx.simulate_input("tide");
    settle(cx);
    cx.simulate_keystrokes(select_all(Platform::Windows));
    settle(cx);
    assert!(view.read_with(cx, |v, _| v.reading.picked.is_empty()));
}

/// The ⌘ of a ⌘-click as `p` holds it: Ctrl on Windows (a Mac turns a
/// Ctrl+click into a right-click, the row's menu, before the app sees it).
fn cmd_click(p: Platform) -> Modifiers {
    match p {
        Platform::Mac => Modifiers {
            platform: true,
            ..Default::default()
        },
        Platform::Windows => Modifiers {
            control: true,
            ..Default::default()
        },
    }
}

/// Click the Reader row showing `key` (it's on screen) with `m` held.
fn click_row(
    view: &Entity<MainView>,
    key: &super::vm::Key,
    m: Modifiers,
    cx: &mut VisualTestContext,
) {
    let ix = view
        .read_with(cx, |v, _| v.shown_keys().iter().position(|k| k == key))
        .expect("the row is shown");
    let sel: &'static str = Box::leak(format!("reading-row-{ix}").into_boxed_str());
    let b = crate::test_wait::still(cx, sel);
    cx.simulate_click(b.center(), m);
    settle(cx);
}

fn cmd_click_picks_a_row_with(p: Platform, cx: &mut TestAppContext) {
    let (view, cx) = setup(p, cx);
    let shown = view.read_with(cx, |v, _| v.shown_keys());
    assert!(shown.len() >= 2, "rows to pick: {}", shown.len());
    let opened = |view: &Entity<MainView>, cx: &mut VisualTestContext| {
        view.read_with(cx, |v, _| {
            v.reading.opened.as_ref().map(|o| super::vm::key(&o.item))
        })
    };
    let before = opened(&view, cx);
    click_row(&view, &shown[1], cmd_click(p), cx);
    view.read_with(cx, |v, _| {
        assert!(
            v.reading.picked.contains(&shown[1]),
            "{p:?}: the {:?}-click picked the row",
            cmd_click(p)
        );
    });
    assert_eq!(opened(&view, cx), before, "{p:?}: picking doesn't open it");
    // Again: let go of it.
    click_row(&view, &shown[1], cmd_click(p), cx);
    assert!(
        view.read_with(cx, |v, _| !v.reading.picked.contains(&shown[1])),
        "{p:?}: a second click lets go"
    );
}

#[gpui_kit::test]
fn ctrl_click_picks_a_row_on_the_windows_key_table(cx: &mut TestAppContext) {
    cmd_click_picks_a_row_with(Platform::Windows, cx);
}

#[gpui_kit::test]
fn cmd_click_picks_a_row_on_the_mac_key_table(cx: &mut TestAppContext) {
    cmd_click_picks_a_row_with(Platform::Mac, cx);
}

/// On a Mac, Ctrl isn't the ⌘ of a ⌘-click: a Ctrl+click is the system's
/// right-click (the row's menu). Should one arrive as a left click with
/// Ctrl held, it opens the post as a plain click does, and picks nothing.
#[gpui_kit::test]
fn ctrl_click_picks_nothing_on_the_mac_key_table(cx: &mut TestAppContext) {
    let (view, cx) = setup(Platform::Mac, cx);
    let shown = view.read_with(cx, |v, _| v.shown_keys());
    click_row(&view, &shown[1], cmd_click(Platform::Windows), cx);
    view.read_with(cx, |v, _| {
        assert!(v.reading.picked.is_empty(), "nothing picked");
        assert_eq!(
            v.reading.opened.as_ref().map(|o| super::vm::key(&o.item)),
            Some(shown[1].clone()),
            "opened, as a plain click"
        );
    });
}
