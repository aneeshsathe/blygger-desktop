//! --- ux 0.29 --- The desktop user-testing pass's keyboard-only journeys
//! over 0.28's features, typed in a headless window: real keystrokes
//! through GPUI's dispatch, against a zero-latency FakeBackend.
//!
//! A journey that fails today is `#[ignore]`d with the finding it shows
//! (`docs/private/ux-pass-0-29-desktop.md`); the fix un-ignores it.

use std::sync::Arc;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::sources_vm::{Pane, Smart, Source};
use super::vm::Key;
use crate::app::MainView;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;
use crate::test_wait::settle;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";

fn setup(cx: &mut TestAppContext) -> (Entity<MainView>, Arc<FakeBackend>, &mut VisualTestContext) {
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
    (view, fake, cx)
}

fn keys(cx: &mut VisualTestContext, k: &str) {
    cx.simulate_keystrokes(&crate::keymap::keys(k));
    settle(cx);
}

fn opened(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Option<Key> {
    view.read_with(cx, |v, _| {
        v.reading.opened.as_ref().map(|o| super::vm::key(&o.item))
    })
}

fn shelved(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Vec<Key> {
    view.read_with(cx, |v, _| v.reading.shelved.clone())
}

/// Keyboard-only: Reading (⌘R), the Reader (⌥⌘2), j opens a post, and
/// "+ Save"'s own key (⋯ More lists ⌥⌘B beside it) saves that post, not
/// the Write side's open post (ux 0.29 B3).
#[gpui_kit::test]
fn the_save_key_saves_the_reading_post_that_is_open(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    let write_saved_before = fake.shelf();
    keys(cx, "cmd-r");
    keys(cx, "alt-cmd-2");
    cx.simulate_keystrokes("j");
    settle(cx);
    let key = opened(&view, cx).expect("j opens the first post");
    assert!(!shelved(&view, cx).contains(&key));
    keys(cx, "alt-cmd-b");
    assert!(
        shelved(&view, cx).contains(&key),
        "the reading post is saved"
    );
    assert_eq!(
        fake.shelf(),
        write_saved_before,
        "your own posts' Saved order is untouched"
    );
}

/// Keyboard-only: from the Reader's list, ← to the sources, ↓ to the
/// Agentic feed, j opens the edition's first post, esc puts the edition
/// back. The keys are all shown in the header.
#[gpui_kit::test]
fn the_agentic_feed_is_reached_and_read_by_keys(cx: &mut TestAppContext) {
    let (view, _, cx) = setup(cx);
    keys(cx, "cmd-r");
    keys(cx, "alt-cmd-2");
    cx.simulate_keystrokes("left");
    settle(cx);
    assert_eq!(view.read_with(cx, |v, _| v.reading.pane), Pane::Sources);
    // Down from the source the Reader starts on, until the Agentic feed.
    for _ in 0..12 {
        if view.read_with(cx, |v, _| v.reading.source.clone()) == Source::Smart(Smart::Agentic) {
            break;
        }
        cx.simulate_keystrokes("down");
        settle(cx);
    }
    assert_eq!(
        view.read_with(cx, |v, _| v.reading.source.clone()),
        Source::Smart(Smart::Agentic)
    );
    let first = view.read_with(cx, |v, _| v.reading.agentic.keys().first().cloned());
    cx.simulate_keystrokes("j");
    settle(cx);
    assert_eq!(opened(&view, cx), first, "j opens the edition's first post");
}
