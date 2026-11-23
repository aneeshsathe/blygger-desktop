//! --- studio 0.71 --- "Count an entry as read" (`readerMarkRead`): offered
//! in Settings › READER ORDER only by a blyg that has it, and applied in the
//! Stream and the Reader. The Stream's tick is driven with explicit times;
//! the marks themselves are written on another thread, so each check waits
//! for them (`crate::test_wait`), never a sleep.

use std::sync::Arc;
use std::time::{Duration, Instant};

use blyg_core::config::MemoryTokenStore;
use blyg_core::mark_read::MarkRead;
use blyg_core::{Backend, ConfigStore};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::stream_vm::ReadMode;
use super::vm::Key;
use crate::app::MainView;
use crate::fake::reading_seed::*;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;
use crate::test_wait::{settle_drawn as settle, until};

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";

fn setup(
    fake: FakeBackend,
    cx: &mut TestAppContext,
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
    let fake = Arc::new(fake);
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

fn fake() -> FakeBackend {
    FakeBackend::with_timing(Timing::instant())
        .chronological()
        .without_media_cache()
}

fn read_version(fake: &FakeBackend, id: &str) -> Option<u32> {
    fake.reading()
        .into_iter()
        .find(|r| r.remote_id == id)
        .and_then(|r| r.read_version)
}

fn key_of(fake: &FakeBackend, id: &str) -> Key {
    fake.reading()
        .into_iter()
        .find(|r| r.remote_id == id)
        .map(|r| super::vm::key(&r))
        .unwrap()
}

/// The blyg's choice is `m`, and this window has read it.
fn choose(view: &Entity<MainView>, fake: &FakeBackend, m: MarkRead, cx: &mut VisualTestContext) {
    fake.load_reader_mark_read().unwrap();
    fake.save_reader_mark_read(m).unwrap();
    view.update(cx, |v, cx| v.count_read_refresh(true, cx));
    until(cx, "the blyg's choice to arrive", |cx| {
        view.read_with(cx, |v, _| v.reading.count_read.setting == Some(m))
    });
}

/// The Stream, drawn.
fn stream(view: &Entity<MainView>, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-r"));
    settle(cx);
    view.update_in(cx, |v, window, cx| {
        v.set_read_mode(ReadMode::Stream, window, cx)
    });
    settle(cx);
}

fn wait_read(fake: &Arc<FakeBackend>, id: &str, cx: &mut VisualTestContext) {
    let f = fake.clone();
    until(cx, "the read mark", move |_| read_version(&f, id).is_some());
}

#[gpui_kit::test]
fn settings_offers_the_choice_once_the_blyg_has_it(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(fake(), cx);
    // Not read yet: no choice, and Burrow's earlier marks.
    view.read_with(cx, |v, _| {
        assert_eq!(v.reading.count_read.setting, None);
        assert!(v.reading.count_read.on_open());
    });
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-,"));
    until(cx, "Settings to read the blyg's choice", |cx| {
        view.read_with(cx, |v, _| {
            v.reading.count_read.setting == Some(MarkRead::Seen)
        })
    });
    settle(cx);
    for m in MarkRead::ALL {
        let id = format!("count-read-{}", m.as_str());
        assert!(cx.debug_bounds(Box::leak(id.into_boxed_str())).is_some());
    }
    view.update(cx, |v, cx| v.set_count_read(MarkRead::Never, cx));
    let f = fake.clone();
    until(cx, "the choice to reach the blyg", move |_| {
        f.reader_mark_read() == Some(MarkRead::Never)
    });
    view.read_with(cx, |v, _| assert!(!v.reading.count_read.on_open()));
}

#[gpui_kit::test]
fn a_blyg_without_the_setting_shows_no_choice(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(fake().without_reader_mark_read(), cx);
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-,"));
    settle(cx);
    assert!(
        cx.debug_bounds("reader-order-newest").is_some(),
        "Settings is up"
    );
    assert!(cx.debug_bounds("count-read").is_none());
    view.read_with(cx, |v, _| {
        assert_eq!(v.reading.count_read.setting, None);
        assert_eq!(
            v.reading.count_read.seen_rule(),
            Some(super::count_read::SeenRule::Half),
            "Burrow's earlier rule"
        );
    });
}

#[gpui_kit::test]
fn when_i_see_it_marks_a_card_on_screen_for_a_second_in_a_batch(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(fake(), cx);
    choose(&view, &fake, MarkRead::Seen, cx);
    stream(&view, cx);
    assert_eq!(read_version(&fake, ADA_FINISHED), None);
    let t0 = Instant::now();
    view.update(cx, |v, cx| v.stream_tick(t0, true, cx));
    view.update(cx, |v, cx| {
        v.stream_tick(t0 + Duration::from_millis(1050), true, cx)
    });
    settle(cx);
    // Seen, and waiting to go with the others.
    view.read_with(cx, |v, _| {
        assert!(!v.reading.count_read.batch.is_empty(), "the batch holds it")
    });
    assert_eq!(read_version(&fake, ADA_FINISHED), None, "not sent yet");
    // The batch goes once its first mark has waited three seconds.
    view.update(cx, |v, cx| {
        v.stream_tick(t0 + Duration::from_millis(3100), true, cx)
    });
    settle(cx);
    assert_eq!(read_version(&fake, ADA_FINISHED), None, "still waiting");
    view.update(cx, |v, cx| {
        v.stream_tick(t0 + Duration::from_millis(4100), true, cx)
    });
    wait_read(&fake, ADA_FINISHED, cx);
    view.read_with(cx, |v, _| assert!(v.reading.count_read.batch.is_empty()));
}

#[gpui_kit::test]
fn leaving_the_stream_sends_the_waiting_marks(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(fake(), cx);
    choose(&view, &fake, MarkRead::Seen, cx);
    stream(&view, cx);
    let t0 = Instant::now();
    view.update(cx, |v, cx| {
        v.stream_tick(t0, true, cx);
        v.stream_tick(t0 + Duration::from_millis(1050), true, cx);
    });
    settle(cx);
    assert_eq!(read_version(&fake, ADA_FINISHED), None, "not sent yet");
    // The window goes to the background: what was seen goes at once.
    view.update(cx, |v, cx| {
        v.stream_tick(t0 + Duration::from_millis(1300), false, cx)
    });
    wait_read(&fake, ADA_FINISHED, cx);
}

#[gpui_kit::test]
fn when_i_open_it_only_opening_counts(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(fake(), cx);
    choose(&view, &fake, MarkRead::Opened, cx);
    stream(&view, cx);
    let t0 = Instant::now();
    view.update(cx, |v, cx| {
        v.stream_tick(t0, true, cx);
        v.stream_tick(t0 + Duration::from_secs(2), true, cx);
        v.stream_tick(t0 + Duration::from_secs(6), true, cx);
    });
    settle(cx);
    assert_eq!(read_version(&fake, ADA_FINISHED), None, "seen isn't read");
    let key = key_of(&fake, ADA_FINISHED);
    view.update_in(cx, |v, window, cx| v.open_reading(key, window, cx));
    wait_read(&fake, ADA_FINISHED, cx);
}

#[gpui_kit::test]
fn only_when_i_mark_it_nothing_counts_by_itself(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(fake(), cx);
    choose(&view, &fake, MarkRead::Never, cx);
    stream(&view, cx);
    let t0 = Instant::now();
    view.update(cx, |v, cx| {
        v.stream_tick(t0, true, cx);
        v.stream_tick(t0 + Duration::from_secs(2), true, cx);
        v.stream_tick(t0 + Duration::from_secs(6), true, cx);
    });
    let key = key_of(&fake, ADA_FINISHED);
    view.update_in(cx, |v, window, cx| v.open_reading(key.clone(), window, cx));
    settle(cx);
    view.read_with(cx, |v, _| {
        assert_eq!(
            v.reading.opened.as_ref().map(|o| o.key.clone()),
            Some(key.clone()),
            "it opened"
        )
    });
    assert_eq!(
        read_version(&fake, ADA_FINISHED),
        None,
        "opening isn't read"
    );
    view.update(cx, |v, cx| v.mark_posts(vec![key], true, cx));
    wait_read(&fake, ADA_FINISHED, cx);
}

#[gpui_kit::test]
fn a_cards_date_opens_its_full_view(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(fake(), cx);
    choose(&view, &fake, MarkRead::Opened, cx);
    stream(&view, cx);
    let first = view.read_with(cx, |v, _| {
        v.reading
            .shown_rows()
            .next()
            .map(|r| r.remote_id.to_string())
    });
    let at = crate::test_wait::still(cx, "stream-date-0");
    cx.simulate_click(at.center(), gpui_kit::Modifiers::none());
    settle(cx);
    let opened = view.read_with(cx, |v, _| {
        v.reading
            .opened
            .as_ref()
            .map(|o| o.item.remote_id.to_string())
    });
    assert_eq!(opened, first, "the date opened its post in the pane");
    let id = first.unwrap();
    wait_read(&fake, &id, cx);
}
