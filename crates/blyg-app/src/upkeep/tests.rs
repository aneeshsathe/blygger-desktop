//! --- upkeep --- Stale quotes, signals, version diffs, alt text and Copy
//! Link in a headless window, against a zero-latency FakeBackend (whose
//! sample thread "Notes on The Field Guide to Small Gardens" has a stale
//! quote, `fake_upkeep.rs`).

use std::sync::Arc;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore, LocalId};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use crate::app::{MainView, Sheet};
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";
/// The sample's published thread with a stale quote (v2).
const STALE: &str = crate::fake::upkeep_seed::STALE_THREAD;
/// The sample's fragment with three versions of text (`fake_reading.rs`).
const FRICTION: &str = "01J9M2A";

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

fn open(view: &Entity<MainView>, s: &str, cx: &mut VisualTestContext) {
    view.update_in(cx, |v, window, cx| v.open(&id(s), window, cx));
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_thread_with_a_stale_quote_is_marked_and_found(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(cx);
    let (ix, marked) = view.read_with(cx, |v, _| {
        let ix = v
            .list
            .results()
            .iter()
            .position(|i| i.local_id == id(STALE))
            .expect("the thread is listed");
        (ix, v.has_stale_quote(&v.list.results()[ix]))
    });
    assert!(marked);
    assert!(
        cx.debug_bounds(Box::leak(format!("row-stale-quote-{ix}").into_boxed_str()))
            .is_some(),
        "↻ is drawn on its row"
    );
    // The loose end, and its operator.
    view.update(cx, |v, cx| v.loose_ends_refresh(cx));
    cx.run_until_parked();
    let parts = view.read_with(cx, |v, _| v.loose_ends.summary.parts());
    assert!(
        parts
            .iter()
            .any(|(w, q)| *q == "is:stale-quote" && w == "1 thread with stale quotes"),
        "{parts:?}"
    );
    cx.simulate_input("is:stale-quote");
    cx.run_until_parked();
    view.read_with(cx, |v, _| {
        let ids: Vec<_> = v
            .list
            .results()
            .iter()
            .map(|i| i.local_id.0.clone())
            .collect();
        assert_eq!(ids, vec![STALE.to_string()]);
    });
}

#[gpui_kit::test]
fn the_stale_sheet_says_what_changed_and_refreshes(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    let before = fake.item(&id(STALE)).unwrap().version;
    cx.dispatch_action(super::ShowStaleQuotes);
    cx.run_until_parked();
    view.read_with(cx, |v, _| {
        let Some(Sheet::StaleQuotes(s)) = &v.sheet else {
            panic!("the sheet is open")
        };
        let rows = s.list.ready().expect("the list loaded");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, Some(id(STALE)));
        assert_eq!(rows[0].report.line(), "2 versions behind · 1 stale quote");
        // The highlighted thread's quotes, as their blygs answered.
        let probed = s.probed.get(&rows[0].report.id).and_then(|p| p.ready());
        let words: Vec<String> = probed.unwrap().quotes.iter().map(|q| q.words()).collect();
        assert!(
            words.contains(&"v1 → v3 at its blyg (fetched on refresh)".to_string()),
            "{words:?}"
        );
    });
    assert!(cx.debug_bounds("stale-row-0").is_some());
    assert!(cx.debug_bounds("stale-refresh").is_some());
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let item = fake.item(&id(STALE)).unwrap();
    assert_eq!(item.version, before + 1, "a refresh is a new version");
    let note = fake
        .versions(&id(STALE))
        .unwrap()
        .last()
        .unwrap()
        .note
        .clone();
    assert_eq!(note.as_deref(), Some(super::stale::DEFAULT_NOTE));
    view.read_with(cx, |v, _| {
        assert!(!v.has_stale_quote(&item), "the ↻ is gone");
        let Some(Sheet::StaleQuotes(s)) = &v.sheet else {
            panic!("the sheet stays open")
        };
        assert_eq!(
            s.list.ready().map(Vec::len),
            Some(0),
            "nothing is stale now"
        );
        let done = s.done.values().next().expect("the outcome is kept");
        assert_eq!(
            done.as_ref().unwrap().words(),
            format!("v{} · 1 quote refreshed · 1 blyg fetched again", before + 1)
        );
    });
}

#[gpui_kit::test]
fn edits_hold_a_refresh_and_refresh_all_skips_them(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    // A second stale thread of the sample's, with an edit not published.
    let other = id("01J9PX1");
    fake.add_stale_quote(&other);
    fake.save(&id(STALE), "Notes, edited and not published")
        .unwrap();
    cx.dispatch_action(super::ShowStaleQuotes);
    cx.run_until_parked();
    let rows = view.read_with(cx, |v, _| match &v.sheet {
        Some(Sheet::StaleQuotes(s)) => s.list.ready().cloned().unwrap(),
        _ => panic!("open"),
    });
    let held = rows.iter().find(|r| r.id == Some(id(STALE))).unwrap();
    assert!(super::stale::why_not(held).is_some_and(|w| w.contains("unpublished edits")));
    let before = fake.item(&other).unwrap().version;
    cx.simulate_keystrokes("shift-enter");
    cx.run_until_parked();
    assert_eq!(fake.item(&other).unwrap().version, before + 1);
    assert_eq!(fake.item(&id(STALE)).unwrap().version, 2, "held back");
}

#[gpui_kit::test]
fn signals_list_likes_dislikes_and_activity(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    let r = fake
        .reading()
        .into_iter()
        .find(|r| r.thumb.is_none() && r.state != "tombstone")
        .unwrap();
    fake.signal(&r.subscription_id, &r.remote_id, Some(-1))
        .unwrap();
    cx.dispatch_action(super::ShowSignals);
    cx.run_until_parked();
    let liked = view.read_with(cx, |v, _| match &v.sheet {
        Some(Sheet::Signals(s)) => s.rows().to_vec(),
        _ => panic!("open"),
    });
    assert!(liked.iter().all(|row| row.what.is_none()));
    cx.simulate_keystrokes("2");
    cx.run_until_parked();
    let disliked = view.read_with(cx, |v, _| match &v.sheet {
        Some(Sheet::Signals(s)) => s.rows().to_vec(),
        _ => panic!("open"),
    });
    assert_eq!(disliked.len(), 1);
    assert_eq!(
        disliked[0].key,
        Some((r.subscription_id.clone(), r.remote_id.clone()))
    );
    cx.simulate_keystrokes("3");
    cx.run_until_parked();
    let activity = view.read_with(cx, |v, _| match &v.sheet {
        Some(Sheet::Signals(s)) => s.rows().to_vec(),
        _ => panic!("open"),
    });
    assert!(
        activity
            .iter()
            .any(|a| a.what.as_deref() == Some("👎 disliked")),
        "{activity:?}"
    );
    // ⏎ on the disliked post opens it in Reading.
    cx.simulate_keystrokes("2");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |v, _| {
        assert!(v.sheet.is_none());
        assert_eq!(
            v.reading.opened.as_ref().map(|o| o.key.clone()),
            Some((r.subscription_id.clone(), r.remote_id.clone()))
        );
    });
}

#[gpui_kit::test]
fn cmd_y_shows_what_changed_between_versions(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    open(&view, FRICTION, cx);
    cx.dispatch_action(crate::app::reading::ShowVersions);
    cx.run_until_parked();
    let at = cx
        .debug_bounds("own-diff")
        .expect("the chip shows")
        .center();
    cx.simulate_click(at, gpui_kit::Modifiers::none());
    cx.run_until_parked();
    let (old, new) = (
        fake.version_text(&id(FRICTION), 2).unwrap(),
        fake.version_text(&id(FRICTION), 3).unwrap(),
    );
    assert_ne!(old, new);
    view.read_with(cx, |v, _| {
        let d = v.upkeep.diff.as_ref().expect("a diff");
        assert_eq!((d.from, d.to), (2, 3));
        let runs = d.runs.ready().expect("both versions read");
        let shown: String = runs.iter().map(|(_, s)| s.as_str()).collect();
        assert!(runs.iter().any(|(op, _)| *op != crate::vm::DiffOp::Same));
        assert!(shown.len() >= new.len().min(old.len()));
    });
    assert!(cx.debug_bounds("own-diff-text").is_some());
    // The chip again hides it.
    let at = cx.debug_bounds("own-diff").unwrap().center();
    cx.simulate_click(at, gpui_kit::Modifiers::none());
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.upkeep.diff.is_none()));
}

#[gpui_kit::test]
fn copy_link_copies_the_open_posts_permalink(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    open(&view, FRICTION, cx);
    cx.dispatch_action(super::CopyLink);
    cx.run_until_parked();
    let link = fake.item(&id(FRICTION)).unwrap().permalink.unwrap();
    let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()));
    assert_eq!(copied, Some(link));
    // A draft has no link: nothing is copied over it.
    open(&view, "01J9QK3", cx);
    cx.update(|_, cx| cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("x".into())));
    cx.dispatch_action(super::CopyLink);
    cx.run_until_parked();
    let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()));
    assert_eq!(copied.as_deref(), Some("x"));
}

#[gpui_kit::test]
fn a_pasted_image_asks_for_its_alt_text(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    open(&view, FRICTION, cx);
    view.update_in(cx, |v, window, cx| {
        v.start_upload(vec![1, 2, 3], "image/png".into(), window, cx)
    });
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| matches!(v.sheet, Some(Sheet::AltText(_)))));
    cx.simulate_input("two crabs at dusk");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.sheet.is_none()));
    let text = fake.item(&id(FRICTION)).unwrap().content_md;
    assert!(text.contains("![two crabs at dusk](https://"), "{text}");
}

#[test]
fn the_new_smart_feeds_filter_by_kind_and_thumb() {
    use crate::app::reading::sources_vm::{Smart, Source, in_source};
    use std::collections::HashMap;
    let now = chrono::Utc::now();
    let rows = crate::fake::reading_seed::seed(now).reading;
    let filed = HashMap::new();
    let mut down = rows[0].clone();
    down.thumb = Some(-1);
    down.state = "current".into();
    assert!(in_source(
        &down,
        &Source::Smart(Smart::Disliked),
        &filed,
        now
    ));
    assert!(!in_source(
        &down,
        &Source::Smart(Smart::Thumbed),
        &filed,
        now
    ));
    for r in rows.iter().filter(|r| r.state != "tombstone") {
        let thread = r.kind == blyg_core::Kind::Thread;
        assert_eq!(
            in_source(r, &Source::Smart(Smart::Threads), &filed, now),
            thread
        );
        assert_eq!(
            in_source(r, &Source::Smart(Smart::Fragments), &filed, now),
            !thread
        );
    }
}
