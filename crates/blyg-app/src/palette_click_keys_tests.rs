//! --- palette --- A ⌘-click on a palette row, as each platform presses
//! it (Ctrl on Windows): it runs the row and keeps the palette open, as
//! ⌘⏎ does. The row read GPUI's `secondary()`, which a Mac test run can't
//! turn into Ctrl; it reads `keymap::secondary_held` now.

use std::sync::Arc;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore};
use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext};

use crate::app::MainView;
use crate::fake::{FakeBackend, Timing};
use crate::platform_text::Platform;
use crate::prefs::Prefs;

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
    (view, cx)
}

/// ⌘ as platform `p` holds it for a click: Ctrl on Windows.
fn cmd(p: Platform) -> Modifiers {
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

/// The palette open on one row (a theme), drawn.
fn palette_on_a_theme(view: &Entity<MainView>, cx: &mut VisualTestContext) {
    view.update_in(cx, |v, window, cx| {
        v.toggle_palette(window, cx);
        v.palette_set_query("Theme: Dark (Ink)", window, cx);
    });
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.cmd_palette.open.is_some()));
}

fn click_row(m: Modifiers, cx: &mut VisualTestContext) {
    let b = crate::test_wait::still(cx, "palette-row-0");
    cx.simulate_click(b.center(), m);
    cx.run_until_parked();
}

fn theme(view: &Entity<MainView>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |v, _| v.committed_prefs().theme.clone())
}

fn cmd_click_runs_a_row_and_keeps_the_palette_with(p: Platform, cx: &mut TestAppContext) {
    let (view, cx) = setup(p, cx);
    palette_on_a_theme(&view, cx);
    click_row(cmd(p), cx);
    assert_eq!(theme(&view, cx), "dark", "{p:?}: the row ran");
    assert!(
        view.read_with(cx, |v, _| v.cmd_palette.open.is_some()),
        "{p:?}: a {:?}-click keeps the palette open",
        cmd(p)
    );
}

#[gpui_kit::test]
fn ctrl_click_keeps_the_palette_open_on_the_windows_key_table(cx: &mut TestAppContext) {
    cmd_click_runs_a_row_and_keeps_the_palette_with(Platform::Windows, cx);
}

#[gpui_kit::test]
fn cmd_click_keeps_the_palette_open_on_the_mac_key_table(cx: &mut TestAppContext) {
    cmd_click_runs_a_row_and_keeps_the_palette_with(Platform::Mac, cx);
}

/// The click lands on the row: a plain one runs it and closes the palette.
#[gpui_kit::test]
fn a_plain_click_runs_a_row_and_closes_the_palette(cx: &mut TestAppContext) {
    let (view, cx) = setup(Platform::Windows, cx);
    palette_on_a_theme(&view, cx);
    click_row(Modifiers::none(), cx);
    assert_eq!(theme(&view, cx), "dark");
    assert!(view.read_with(cx, |v, _| v.cmd_palette.open.is_none()));
}
