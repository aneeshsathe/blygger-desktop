//! --- read/unread --- ⌘A in the Reader picks every post its list shows,
//! as each platform binds it: Ctrl+A on Windows. The list used to read
//! GPUI's `platform` modifier (the Windows key there) and refuse Ctrl, so
//! Ctrl+A did nothing on Windows while the header's hint said Ctrl+A. The
//! search field's own ⌘A still selects its text.

use std::sync::Arc;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

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
