//! The shelf in a headless window: real keystrokes through GPUI's dispatch,
//! against a zero-latency FakeBackend.

use std::sync::Arc;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore, LocalId};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use crate::app::{MainView, Sheet};
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";
const DRAFT: &str = "01J9QK3"; // the first row: a draft fragment
const PUBLIC: &str = "01J9PX1"; // the second: public v1

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

fn id(s: &str) -> LocalId {
    LocalId(s.into())
}

fn keys(cx: &mut VisualTestContext, k: &str) {
    cx.simulate_keystrokes(&crate::keymap::keys(k));
    cx.run_until_parked();
}

fn first_rows(view: &Entity<MainView>, cx: &mut VisualTestContext, n: usize) -> Vec<String> {
    view.read_with(cx, |v, _| {
        v.list
            .results()
            .iter()
            .take(n)
            .map(|i| i.local_id.0.clone())
            .collect()
    })
}

fn open(view: &Entity<MainView>, cx: &mut VisualTestContext, which: &str) {
    view.update_in(cx, |v, window, cx| v.open(&id(which), window, cx));
    cx.run_until_parked();
}

fn current(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Option<LocalId> {
    view.read_with(cx, |v, _| v.current.as_ref().map(|c| c.local_id.clone()))
}

fn sheet_rows(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Option<Vec<String>> {
    view.read_with(cx, |v, _| match &v.sheet {
        Some(Sheet::Shelf(s)) => Some(s.posts.iter().map(|p| p.local_id.0.clone()).collect()),
        _ => None,
    })
}

#[gpui_kit::test]
fn shelving_puts_the_post_on_top_and_the_shelf_opens_it(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    assert_eq!(first_rows(&view, cx, 2), [DRAFT, PUBLIC]);

    open(&view, cx, PUBLIC);
    keys(cx, "alt-cmd-b");
    assert_eq!(fake.shelf(), [id(PUBLIC)]);
    assert_eq!(first_rows(&view, cx, 2), [PUBLIC, DRAFT], "shelved first");
    view.read_with(cx, |v, _| {
        assert_eq!(v.list.shelf_len(), 1);
        assert_eq!(v.list.selected(), Some(&id(PUBLIC)), "still selected");
        assert_eq!(
            v.toast.as_ref().map(|t| t.text.to_string()).as_deref(),
            Some("On your shelf · ⌘B")
        );
    });

    // ⌘B, then 1, from another post.
    open(&view, cx, DRAFT);
    keys(cx, "cmd-b");
    assert_eq!(sheet_rows(&view, cx), Some(vec![PUBLIC.to_string()]));
    cx.simulate_keystrokes("1");
    cx.run_until_parked();
    assert_eq!(sheet_rows(&view, cx), None, "the shelf closes");
    assert_eq!(current(&view, cx), Some(id(PUBLIC)));

    // ⌥⌘B again takes it off.
    keys(cx, "alt-cmd-b");
    assert!(fake.shelf().is_empty());
    assert_eq!(first_rows(&view, cx, 2), [DRAFT, PUBLIC]);
}

#[gpui_kit::test]
fn the_shelf_reorders_and_takes_posts_off(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    fake.set_on_shelf(&id(DRAFT), true).unwrap();
    fake.set_on_shelf(&id(PUBLIC), true).unwrap();
    view.update_in(cx, |v, window, cx| v.requery(window, cx));
    cx.run_until_parked();
    assert_eq!(first_rows(&view, cx, 2), [DRAFT, PUBLIC]);

    open(&view, cx, DRAFT);
    keys(cx, "cmd-b");
    cx.simulate_keystrokes("down alt-up");
    cx.run_until_parked();
    assert_eq!(fake.shelf(), [id(PUBLIC), id(DRAFT)]);
    assert_eq!(
        sheet_rows(&view, cx),
        Some(vec![PUBLIC.to_string(), DRAFT.to_string()])
    );
    assert_eq!(
        first_rows(&view, cx, 2),
        [PUBLIC, DRAFT],
        "the list follows"
    );

    // ⌫ takes the highlighted one (PUBLIC, moved up with it) off.
    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    assert_eq!(fake.shelf(), [id(DRAFT)]);
    assert_eq!(sheet_rows(&view, cx), Some(vec![DRAFT.to_string()]));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(sheet_rows(&view, cx), None);
}

#[gpui_kit::test]
fn opening_from_the_shelf_clears_a_search_that_hides_the_post(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    fake.set_on_shelf(&id(PUBLIC), true).unwrap();
    view.update_in(cx, |v, window, cx| {
        v.set_query_text("lighthouse", window, cx)
    });
    cx.run_until_parked();
    let hidden = view.read_with(cx, |v, _| {
        !v.list.results().iter().any(|i| i.local_id == id(PUBLIC))
    });
    assert!(hidden, "the search hides it");
    keys(cx, "cmd-b");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(current(&view, cx), Some(id(PUBLIC)));
    view.read_with(cx, |v, _| {
        assert_eq!(v.list.query(), "");
        assert_eq!(v.list.selected(), Some(&id(PUBLIC)));
    });
}
