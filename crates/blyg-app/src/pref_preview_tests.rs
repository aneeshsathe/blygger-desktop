//! --- preview --- Trying themes, fonts and sizes before keeping them, in a
//! headless window with real keystrokes and pointer moves: Settings' rows
//! (keys, hover, click), the command palette's theme and font rows, and the
//! light and dark slots. Nothing is written while trying; ⏎ or a click
//! writes the key; esc and leaving put the kept value back.

use std::sync::Arc;
use std::time::Duration;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore};
use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext};

use super::MainView;
use super::pref_preview::{Pick, Row};
use crate::app::toolbar::tests::scroll_into_view;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;
use crate::test_wait::still as settled_bounds;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";

fn setup<'a>(
    cx: &'a mut TestAppContext,
    extra: &str,
) -> (Entity<MainView>, &'a mut VisualTestContext) {
    let config = format!("{CONNECTED}{extra}");
    let prefs = Prefs::from_config(ConfigStore::in_memory(&config).config());
    cx.update(|cx| {
        gpui_kit::init(cx);
        super::bind_keys(cx);
        crate::settings::init(
            ConfigStore::in_memory(&config),
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

fn frame(cx: &mut VisualTestContext) {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

fn keys(cx: &mut VisualTestContext, k: &str) {
    cx.simulate_keystrokes(&crate::keymap::keys(k));
    cx.run_until_parked();
    frame(cx);
}

fn config_text(cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| {
        crate::settings::get(cx)
            .store
            .text()
            .unwrap_or_default()
            .to_string()
    })
}

fn open_settings(cx: &mut VisualTestContext) {
    keys(cx, "cmd-,");
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(150));
        frame(cx);
    }
}

/// Past the hover dwell.
fn dwell(cx: &mut VisualTestContext) {
    cx.executor()
        .advance_clock(super::pref_preview::HOVER_DWELL * 2);
    frame(cx);
}

/// (shown theme, prefs.theme, kept theme, previewing?)
fn theme_state(
    view: &Entity<MainView>,
    cx: &mut VisualTestContext,
) -> (String, String, String, bool) {
    view.read_with(cx, |v, _| {
        (
            v.theme.id.clone(),
            v.prefs.theme.clone(),
            v.committed_prefs().theme.clone(),
            v.preview.active.is_some(),
        )
    })
}

#[gpui_kit::test]
fn settings_arrows_try_themes_and_enter_keeps_one(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx, "theme = light\n");
    open_settings(cx);
    let before = config_text(cx);
    // ⇥ walks the rows: writing font, interface font, size, theme.
    for _ in 0..4 {
        keys(cx, "tab");
    }
    view.read_with(cx, |v, _| {
        assert_eq!(
            v.preview.cursor,
            Some((
                Row::Theme,
                Pick::Theme {
                    dark: false,
                    id: Some("light".into())
                }
            ))
        );
        assert!(v.preview.active.is_none(), "landing on a row tries nothing");
    });
    // → tries the next theme on the whole window, writing nothing.
    keys(cx, "right");
    assert_eq!(
        theme_state(&view, cx),
        ("dark".into(), "dark".into(), "light".into(), true)
    );
    keys(cx, "right");
    assert_eq!(theme_state(&view, cx).0, "cutaway");
    assert!(
        cx.debug_bounds("settings-specimen").is_some(),
        "the specimen is drawn"
    );
    assert_eq!(config_text(cx), before, "trying writes nothing");
    // esc puts the kept theme back, and the cursor on it.
    keys(cx, "escape");
    assert_eq!(
        theme_state(&view, cx),
        ("light".into(), "light".into(), "light".into(), false)
    );
    view.read_with(cx, |v, _| {
        assert!(matches!(&v.preview.cursor, Some((Row::Theme, Pick::Theme { id: Some(id), .. })) if id == "light"));
        assert!(v.sheet.is_some(), "esc while trying doesn't close Settings");
    });
    assert_eq!(config_text(cx), before);
    // ⏎ keeps the one being tried and writes it.
    keys(cx, "right");
    keys(cx, "right");
    keys(cx, "enter");
    assert_eq!(
        theme_state(&view, cx),
        ("cutaway".into(), "cutaway".into(), "cutaway".into(), false)
    );
    assert!(
        config_text(cx).ends_with("theme = cutaway\n"),
        "{}",
        config_text(cx)
    );
    // esc with nothing being tried leaves the row, then closes Settings.
    keys(cx, "escape");
    view.read_with(cx, |v, _| {
        assert!(v.preview.cursor.is_none() && v.sheet.is_some())
    });
    keys(cx, "escape");
    view.read_with(cx, |v, _| assert!(v.sheet.is_none()));
}

#[gpui_kit::test]
fn fonts_and_size_are_tried_and_put_back_on_leaving(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx, "");
    open_settings(cx);
    let before = config_text(cx);
    let kept_writing = view.read_with(cx, |v, _| v.prefs.writing().label);
    keys(cx, "tab");
    keys(cx, "right");
    let tried = view.read_with(cx, |v, _| v.prefs.writing().label);
    assert_ne!(tried, kept_writing, "→ tries the next writing font");
    view.read_with(cx, |v, _| {
        assert_eq!(v.committed_prefs().writing().label, kept_writing);
    });
    // Leaving the row puts it back.
    keys(cx, "tab");
    view.read_with(cx, |v, _| {
        assert_eq!(v.prefs.writing().label, kept_writing);
        assert!(v.preview.active.is_none());
    });
    // The interface font, the same way.
    keys(cx, "down");
    let kept_ui = view.read_with(cx, |v, _| v.committed_prefs().ui().label);
    view.read_with(cx, |v, _| assert_ne!(v.prefs.ui().label, kept_ui));
    // Size: → is a point bigger, tried.
    keys(cx, "tab");
    keys(cx, "right");
    keys(cx, "right");
    view.read_with(cx, |v, _| {
        assert_eq!(v.prefs.font_size, crate::prefs::DEFAULT_SIZE + 2.);
        assert_eq!(v.committed_prefs().font_size, crate::prefs::DEFAULT_SIZE);
    });
    assert_eq!(config_text(cx), before, "trying writes nothing");
    // Closing Settings (⌘, again) puts everything back.
    keys(cx, "cmd-,");
    view.read_with(cx, |v, _| {
        assert!(v.sheet.is_none());
        assert_eq!(v.prefs.font_size, crate::prefs::DEFAULT_SIZE);
        assert_eq!(v.prefs.ui().label, kept_ui);
        assert!(v.preview.active.is_none());
    });
    assert_eq!(config_text(cx), before);
    // ⏎ on a tried size keeps it.
    open_settings(cx);
    keys(cx, "shift-tab");
    keys(cx, "shift-tab");
    keys(cx, "left");
    keys(cx, "enter");
    view.read_with(cx, |v, _| {
        assert_eq!(v.prefs.font_size, crate::prefs::DEFAULT_SIZE - 1.);
        assert!(v.preview.active.is_none());
    });
    assert!(
        config_text(cx).contains("font-size = 18"),
        "{}",
        config_text(cx)
    );
}

#[gpui_kit::test]
fn resting_on_a_chip_tries_it_and_a_click_keeps_it(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx, "theme = light\n");
    open_settings(cx);
    let before = config_text(cx);
    scroll_into_view(cx, "th-kumiko");
    let kumiko = settled_bounds(cx, "th-kumiko");
    cx.simulate_mouse_move(kumiko.center(), None, Modifiers::none());
    // Not at once: a sweep across the chips doesn't flash every theme.
    frame(cx);
    assert_eq!(theme_state(&view, cx).0, "light");
    dwell(cx);
    assert_eq!(
        theme_state(&view, cx),
        ("kumiko".into(), "kumiko".into(), "light".into(), true)
    );
    // Straight on to the next chip: no stop at the kept theme.
    let shola = settled_bounds(cx, "th-shola");
    cx.simulate_mouse_move(shola.center(), None, Modifiers::none());
    dwell(cx);
    assert_eq!(theme_state(&view, cx).0, "shola");
    // Off the chips: back to the kept theme.
    let header = settled_bounds(cx, "settings-content");
    cx.simulate_mouse_move(
        gpui_kit::point(
            header.right() - gpui_kit::px(4.),
            header.bottom() - gpui_kit::px(4.),
        ),
        None,
        Modifiers::none(),
    );
    dwell(cx);
    assert_eq!(
        theme_state(&view, cx),
        ("light".into(), "light".into(), "light".into(), false)
    );
    assert_eq!(config_text(cx), before, "resting writes nothing");
    // A click keeps it.
    let kumiko = settled_bounds(cx, "th-kumiko");
    cx.simulate_click(kumiko.center(), Modifiers::none());
    dwell(cx);
    assert_eq!(
        theme_state(&view, cx),
        ("kumiko".into(), "kumiko".into(), "kumiko".into(), false)
    );
    assert!(
        config_text(cx).ends_with("theme = kumiko\n"),
        "{}",
        config_text(cx)
    );
}

#[gpui_kit::test]
fn the_palette_tries_themes_as_you_move_and_esc_puts_yours_back(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx, "theme = light\n");
    let before = config_text(cx);
    keys(cx, "cmd-shift-p");
    // Opening it tries nothing.
    assert_eq!(theme_state(&view, cx).0, "light");
    cx.simulate_input("theme: kumiko");
    cx.run_until_parked();
    assert_eq!(
        theme_state(&view, cx),
        ("kumiko".into(), "kumiko".into(), "light".into(), true)
    );
    // A font row tries the font instead, and the theme goes back.
    view.update_in(cx, |v, window, cx| {
        v.palette_preview_selected(window, cx);
        let o = v.cmd_palette.open.as_mut().unwrap();
        let i = o
            .entries
            .iter()
            .position(|e| e.label == "Writing font: ET Book")
            .unwrap();
        o.shown = vec![i];
        o.sel = 0;
        v.palette_preview_selected(window, cx);
    });
    view.read_with(cx, |v, _| {
        assert_eq!(v.theme.id, "light");
        assert_eq!(v.prefs.writing().label, "ET Book");
    });
    assert_eq!(config_text(cx), before, "trying writes nothing");
    // esc closes and puts it all back.
    keys(cx, "escape");
    view.read_with(cx, |v, _| {
        assert!(v.cmd_palette.open.is_none());
        assert!(v.preview.active.is_none());
        assert_eq!(v.prefs.writing().label, "Literata");
    });
    assert_eq!(config_text(cx), before);
    // ⏎ keeps the highlighted theme.
    keys(cx, "cmd-shift-p");
    cx.simulate_input("theme: kumiko");
    cx.run_until_parked();
    keys(cx, "enter");
    assert_eq!(
        theme_state(&view, cx),
        ("kumiko".into(), "kumiko".into(), "kumiko".into(), false)
    );
    assert!(
        config_text(cx).ends_with("theme = kumiko\n"),
        "{}",
        config_text(cx)
    );
}

#[gpui_kit::test]
fn the_dark_slot_tries_and_writes_theme_dark(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx, "theme = cutaway\ntheme-dark = fortress\n");
    // The test window is light: the light slot is in use.
    assert_eq!(theme_state(&view, cx).0, "cutaway");
    open_settings(cx);
    view.read_with(cx, |v, _| assert!(!v.preview.dark_slot));
    let slot = settled_bounds(cx, "theme-slot-dark");
    cx.simulate_click(slot.center(), Modifiers::none());
    frame(cx);
    view.read_with(cx, |v, _| assert!(v.preview.dark_slot));
    for _ in 0..4 {
        keys(cx, "tab");
    }
    view.read_with(cx, |v, _| {
        assert_eq!(
            v.preview.cursor,
            Some((
                Row::Theme,
                Pick::Theme {
                    dark: true,
                    id: Some("fortress".into())
                }
            ))
        );
    });
    // Trying a dark-mode theme while macOS is light shows it anyway.
    keys(cx, "right");
    view.read_with(cx, |v, _| {
        assert_eq!(v.theme.id, "portolan");
        assert_eq!(v.prefs.theme_dark.as_deref(), Some("portolan"));
        assert_eq!(v.prefs.theme, "cutaway", "the light slot is untouched");
        assert_eq!(v.committed_prefs().theme_dark.as_deref(), Some("fortress"));
    });
    // Even trying the kept dark theme shows it (not the light one).
    keys(cx, "left");
    assert_eq!(theme_state(&view, cx).0, "fortress");
    keys(cx, "right");
    keys(cx, "enter");
    let text = config_text(cx);
    assert!(text.contains("theme = cutaway\n"), "{text}");
    assert!(text.ends_with("theme-dark = portolan\n"), "{text}");
    view.read_with(cx, |v, _| {
        // Kept for dark mode; the light window shows the light theme again.
        assert_eq!(v.theme.id, "cutaway");
        assert!(
            v.toast
                .as_ref()
                .is_some_and(|t| t.text.contains("dark-mode theme"))
        );
    });
    // "Same as light" removes theme-dark.
    scroll_into_view(cx, "thd-same");
    let same = settled_bounds(cx, "thd-same");
    cx.simulate_click(same.center(), Modifiers::none());
    frame(cx);
    let text = config_text(cx);
    assert!(!text.contains("theme-dark"), "{text}");
    view.read_with(cx, |v, _| assert_eq!(v.prefs.theme_dark, None));
}

#[gpui_kit::test]
fn a_kept_change_elsewhere_never_writes_what_is_being_tried(cx: &mut TestAppContext) {
    let (view, cx) = setup(cx, "theme = light\n");
    open_settings(cx);
    for _ in 0..4 {
        keys(cx, "tab");
    }
    keys(cx, "right");
    assert_eq!(theme_state(&view, cx).0, "dark");
    // ⌘+ keeps a size; the theme being tried isn't written with it.
    keys(cx, "cmd-=");
    let text = config_text(cx);
    assert!(text.contains("font-size = 20"), "{text}");
    assert!(text.contains("theme = light"), "{text}");
    assert!(!text.contains("theme = dark"), "{text}");
    view.read_with(cx, |v, _| {
        assert_eq!(v.prefs.theme, "light");
        assert!(v.preview.active.is_none());
    });
}
