//! --- reading compose --- Headless GPUI tests of the reading screen's
//! compose box: it's there in both modes, typing in it never fires the
//! list's keys, ⌘⏎ posts and ⌘D saves a draft (quick capture's path, on
//! the FakeBackend), the text survives a view switch and a fresh start,
//! and the app launches on the reading screen.

use std::sync::Arc;
use std::time::Duration;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore, Kind, Status};
use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext};

use super::View;
use super::compose::Compose;
use crate::app::MainView;
use crate::fake::reading_seed::*;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";

fn setup_with<'a>(
    cx: &'a mut TestAppContext,
    config: &'static str,
) -> (
    Entity<MainView>,
    Arc<FakeBackend>,
    &'a mut VisualTestContext,
) {
    let prefs = Prefs::from_config(ConfigStore::in_memory(config).config());
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::bind_keys(cx);
        crate::settings::init(
            ConfigStore::in_memory(config),
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

fn setup(cx: &mut TestAppContext) -> (Entity<MainView>, Arc<FakeBackend>, &mut VisualTestContext) {
    setup_with(cx, CONNECTED)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..20 {
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(2));
    }
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

fn to_reading(cx: &mut VisualTestContext) {
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-r"));
    settle(cx);
}

fn box_text(view: &Entity<MainView>, cx: &mut VisualTestContext) -> String {
    view.update(cx, |v, cx| v.reading.compose.text(cx))
}

fn box_has_keys(view: &Entity<MainView>, cx: &mut VisualTestContext) -> bool {
    view.update_in(cx, |v, window, cx| {
        v.reading.compose.has_keyboard(window, cx)
    })
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let b = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} isn't on screen"));
    cx.simulate_click(b.center(), Modifiers::none());
    settle(cx);
}

/// The newest item, when the box made one since `before` items.
fn made(fake: &FakeBackend, before: usize) -> Option<blyg_core::Item> {
    let items = fake.items();
    (items.len() == before + 1).then(|| items[0].clone())
}

#[gpui_kit::test]
fn the_box_is_on_the_reading_screen_in_both_modes(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(cx);
    to_reading(cx);
    assert!(cx.debug_bounds("reading-compose").is_some(), "stream");
    assert!(cx.debug_bounds("compose-post").is_some());
    // With a post open in the stream's side pane, the box is still there,
    // below it, not over it.
    cx.simulate_keystrokes("j enter");
    settle(cx);
    assert!(view.read_with(cx, |v, _| v.reading.opened.is_some()));
    let bar = cx.debug_bounds("reading-compose").expect("bar");
    let close = cx.debug_bounds("stream-pane-close").expect("pane");
    assert!(close.bottom() <= bar.top(), "the pane ends above the box");
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes(&crate::keymap::keys("alt-cmd-2"));
    settle(cx);
    view.read_with(cx, |v, _| {
        assert_eq!(v.reading.mode, super::stream_vm::ReadMode::Reader)
    });
    assert!(cx.debug_bounds("reading-compose").is_some(), "reader");
    // Not on Posts.
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-r"));
    settle(cx);
    assert_eq!(view.read_with(cx, |v, _| v.reading.view), View::Posts);
    assert!(cx.debug_bounds("reading-compose").is_none());
}

#[gpui_kit::test]
fn c_focuses_the_box_and_typing_never_fires_the_lists_keys(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    to_reading(cx);
    let reads = |fake: &FakeBackend| -> Vec<(String, Option<u32>)> {
        fake.reading()
            .into_iter()
            .map(|r| (r.remote_id, r.read_version))
            .collect()
    };
    let read_before = reads(&fake);
    cx.simulate_keystrokes("c");
    settle(cx);
    assert!(box_has_keys(&view, cx));
    assert_eq!(box_text(&view, cx), "", "the c isn't typed");
    // j/k move, r/u mark, space opens, / searches: all text here.
    cx.simulate_keystrokes("j k r u space / [ ]");
    settle(cx);
    assert_eq!(box_text(&view, cx), "jkru /[]");
    view.read_with(cx, |v, _| {
        assert_eq!(v.reading.sel, None, "nothing selected");
        assert!(v.reading.opened.is_none(), "nothing opened");
        assert_eq!(v.reading.view, View::Reading);
    });
    assert_eq!(reads(&fake), read_before, "nothing marked");
    // esc leaves the box, keeps the text, and the keys are the list's again.
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert!(!box_has_keys(&view, cx));
    assert_eq!(box_text(&view, cx), "jkru /[]");
    assert_eq!(view.read_with(cx, |v, _| v.reading.view), View::Reading);
    cx.simulate_keystrokes("j");
    settle(cx);
    assert_eq!(
        view.read_with(cx, |v, _| v.reading.sel.as_ref().map(|k| k.1.clone())),
        Some(RUE_TRUST.to_string())
    );
}

#[gpui_kit::test]
fn cmd_shift_k_comes_from_posts_to_the_box(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(cx);
    assert_eq!(view.read_with(cx, |v, _| v.reading.view), View::Posts);
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-shift-k"));
    settle(cx);
    assert_eq!(view.read_with(cx, |v, _| v.reading.view), View::Reading);
    assert!(box_has_keys(&view, cx));
}

#[gpui_kit::test]
fn cmd_enter_posts_a_fragment_and_clears_the_box(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    to_reading(cx);
    let before = fake.items().len();
    cx.simulate_keystrokes("c");
    cx.simulate_input("posted while reading");
    settle(cx);
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-enter"));
    settle(cx);
    let it = made(&fake, before).expect("a post");
    assert_eq!(it.content_md, "posted while reading");
    assert_eq!(
        (it.status, it.version, it.kind),
        (Status::Public, 1, Kind::Fragment)
    );
    assert_eq!(box_text(&view, cx), "", "cleared");
    view.read_with(cx, |v, _| {
        assert_eq!(v.reading.view, View::Reading, "still reading");
        assert!(v.reading.compose.error.is_none());
    });
    assert!(!box_has_keys(&view, cx), "the keys go back to the list");
}

#[gpui_kit::test]
fn the_post_button_posts_too(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    to_reading(cx);
    let before = fake.items().len();
    click(cx, "compose-post");
    assert_eq!(fake.items().len(), before, "an empty box posts nothing");
    cx.simulate_keystrokes("c");
    cx.simulate_input("by a click");
    settle(cx);
    click(cx, "compose-post");
    assert_eq!(made(&fake, before).expect("a post").status, Status::Public);
    assert_eq!(box_text(&view, cx), "");
}

#[gpui_kit::test]
fn cmd_d_and_the_draft_button_save_a_draft(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    to_reading(cx);
    let before = fake.items().len();
    cx.simulate_keystrokes("c");
    cx.simulate_input("for later");
    settle(cx);
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-d"));
    settle(cx);
    let it = made(&fake, before).expect("a draft");
    assert_eq!(
        (it.status, it.content_md.as_str()),
        (Status::Draft, "for later")
    );
    assert_eq!(box_text(&view, cx), "");
    cx.simulate_keystrokes("c");
    cx.simulate_input("also later");
    settle(cx);
    click(cx, "compose-draft");
    assert_eq!(fake.items().len(), before + 2);
    assert_eq!(fake.items()[0].status, Status::Draft);
}

#[gpui_kit::test]
fn a_failed_post_keeps_the_text_and_the_retry_reuses_its_draft(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    to_reading(cx);
    let before = fake.items().len();
    fake.set_offline(true);
    cx.simulate_keystrokes("c");
    cx.simulate_input("on a train");
    settle(cx);
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-enter"));
    settle(cx);
    assert_eq!(box_text(&view, cx), "on a train", "kept");
    let err = view.read_with(cx, |v, _| v.reading.compose.error.clone());
    assert!(err.is_some_and(|e| e.starts_with("Couldn't post")));
    assert!(cx.debug_bounds("compose-error").is_some(), "shown");
    assert_eq!(made(&fake, before).expect("kept").status, Status::Draft);
    // Back online, the same draft goes out: no second item.
    fake.set_offline(false);
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-enter"));
    settle(cx);
    let it = made(&fake, before).expect("one item");
    assert_eq!((it.status, it.version), (Status::Public, 1));
    assert_eq!(box_text(&view, cx), "");
}

#[gpui_kit::test]
fn the_text_survives_a_view_switch_and_a_fresh_start(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(cx);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    view.update(cx, |v, _| {
        v.reading.compose = Compose::new(Some(path.clone()));
    });
    to_reading(cx);
    cx.simulate_keystrokes("c");
    cx.simulate_input("half a thought");
    settle(cx);
    // Posts and back.
    to_reading(cx);
    assert_eq!(view.read_with(cx, |v, _| v.reading.view), View::Posts);
    to_reading(cx);
    assert_eq!(box_text(&view, cx), "half a thought");
    // Kept in state.json, and read back by a fresh box (a restart).
    assert_eq!(
        blyg_core::state::AppState::load(dir.path())
            .compose_draft
            .as_deref(),
        Some("half a thought")
    );
    view.update(cx, |v, _| {
        v.reading.compose = Compose::new(Some(path.clone()));
    });
    settle(cx);
    assert_eq!(box_text(&view, cx), "half a thought");
}

#[gpui_kit::test]
fn quote_puts_the_open_posts_directive_in_the_box(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(cx);
    to_reading(cx);
    assert!(cx.debug_bounds("compose-quote").is_none(), "nothing open");
    cx.simulate_keystrokes("j enter");
    settle(cx);
    click(cx, "compose-quote");
    let text = box_text(&view, cx);
    assert!(text.starts_with(&format!("![[{RUE_TRUST}")), "{text}");
    assert_eq!(super::compose::kind_for(&text), Kind::Thread);
    assert!(box_has_keys(&view, cx));
    // The post stays open beside it.
    assert!(view.read_with(cx, |v, _| v.reading.opened.is_some()));
}

#[gpui_kit::test]
fn expand_moves_the_text_into_the_editor_as_a_draft(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    to_reading(cx);
    let before = fake.items().len();
    cx.simulate_keystrokes("c");
    cx.simulate_input("this wants more room");
    settle(cx);
    click(cx, "compose-expand");
    let it = made(&fake, before).expect("a draft");
    assert_eq!(it.status, Status::Draft);
    view.read_with(cx, |v, cx| {
        assert_eq!(v.reading.view, View::Posts);
        assert_eq!(
            v.current.as_ref().map(|c| c.local_id.clone()),
            Some(it.local_id.clone())
        );
        assert_eq!(v.reading.compose.text(cx), "");
    });
}

#[gpui_kit::test]
fn launch_lands_on_reading(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(cx);
    view.update_in(cx, |v, window, cx| v.launch_into_reading(window, cx));
    settle(cx);
    assert_eq!(view.read_with(cx, |v, _| v.reading.view), View::Reading);
    assert!(cx.debug_bounds("reading-compose").is_some());
}

#[gpui_kit::test]
fn launch_without_a_blyg_stays_with_the_connect_sheet(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup_with(cx, "# no blyg yet\n");
    view.update_in(cx, |v, window, cx| v.launch_into_reading(window, cx));
    settle(cx);
    view.read_with(cx, |v, _| {
        assert_eq!(v.reading.view, View::Posts);
        assert!(v.sheet.is_some(), "the Connect sheet");
    });
}
