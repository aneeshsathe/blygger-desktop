//! --- trails --- in a headless window: real keystrokes through GPUI's
//! dispatch, against a zero-latency FakeBackend.

use std::sync::Arc;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore, Kind, Status};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use crate::app::MainView;
use crate::app::reading::View;
use crate::app::reading::sources_vm::{Entry, Source};
use crate::app::reading::stream_vm::ReadMode;
use crate::fake::reading_seed::*;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";

pub(crate) fn setup(
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

pub(crate) fn settle(cx: &mut VisualTestContext) {
    for _ in 0..10 {
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

/// The reading screen, `mode`, with the post `(sub, id)` open.
pub(crate) fn reading_on(
    view: &Entity<MainView>,
    mode: ReadMode,
    key: (&str, &str),
    cx: &mut VisualTestContext,
) {
    view.update_in(cx, |v, window, cx| {
        v.set_read_mode(mode, window, cx);
        if v.reading.view != View::Reading {
            v.show_view(View::Reading, window, cx);
        }
        let k = (key.0.to_string(), key.1.to_string());
        v.reading.sel = Some(k.clone());
        v.open_reading(k, window, cx);
        window.focus(&v.reading.focus, cx);
    });
    settle(cx);
}

fn keys(cx: &mut VisualTestContext, k: &str) {
    cx.simulate_keystrokes(&crate::keymap::keys(k));
    settle(cx);
}

fn type_in(cx: &mut VisualTestContext, text: &str) {
    cx.simulate_input(text);
    settle(cx);
}

fn stops(fake: &FakeBackend) -> Vec<(String, String)> {
    let t = &fake.trails()[0];
    fake.trail_stops(&t.id)
        .into_iter()
        .map(|s| (s.remote_id, s.why))
        .collect()
}

#[gpui_kit::test]
fn t_names_a_first_trail_then_asks_why(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    reading_on(&view, ReadMode::Stream, ("sub-lin", LIN_GARDENS), cx);
    keys(cx, "t");
    // No trail yet: a name first.
    assert!(view.read_with(cx, |v, _| v.reading.trails.field.is_some()));
    type_in(cx, "weeding");
    keys(cx, "enter");
    assert_eq!(fake.trails().len(), 1);
    assert_eq!(fake.trails()[0].name, "weeding");
    // Then why it's there.
    type_in(cx, "gardens get weeded");
    keys(cx, "enter");
    assert_eq!(
        stops(&fake),
        [(LIN_GARDENS.to_string(), "gardens get weeded".to_string())]
    );
    assert!(view.read_with(cx, |v, _| v.reading.trails.field.is_none()));

    // A second stop; esc skips its why. The reading keys stood back while
    // the field had the keyboard (`j` typed there, not a move).
    reading_on(&view, ReadMode::Stream, ("sub-ada", ADA_TIDES), cx);
    keys(cx, "t");
    type_in(cx, "j");
    keys(cx, "escape");
    assert_eq!(
        stops(&fake)
            .into_iter()
            .map(|(id, why)| format!("{id}:{why}"))
            .collect::<Vec<_>>(),
        [
            format!("{LIN_GARDENS}:gardens get weeded"),
            format!("{ADA_TIDES}:")
        ]
    );
    let at = view.read_with(cx, |v, _| {
        let key = v.reading.opened.as_ref().unwrap().key.clone();
        super::trails::stop_label(v.reading.trails.current_stops(), &key)
    });
    assert_eq!(at.as_deref(), Some("stop 2 of 2"));
}

#[gpui_kit::test]
fn a_trail_is_a_source_listing_its_stops_in_order(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    let t = fake.create_trail("tides").unwrap();
    for id in [ADA_TIDES, LIN_GARDENS] {
        let r = fake
            .reading()
            .into_iter()
            .find(|r| r.remote_id == id)
            .unwrap();
        fake.add_trail_stop(&t.id, &super::trails::stop_for(&r, ""))
            .unwrap();
    }
    reading_on(&view, ReadMode::Reader, ("sub-ada", ADA_TIDES), cx);
    view.update(cx, |v, cx| v.reload_trails(cx));
    let listed = view.read_with(cx, |v, _| {
        v.source_entries()
            .iter()
            .any(|e| matches!(e, Entry::Trail(x) if x.name == "tides" && x.stops == 2))
    });
    assert!(listed, "the sources pane lists the trail");
    view.update(cx, |v, cx| v.select_trail(&t.id, cx));
    settle(cx);
    assert_eq!(
        view.read_with(cx, |v, _| v.reading_shown_ids()),
        [ADA_TIDES, LIN_GARDENS]
    );
    assert_eq!(
        view.read_with(cx, |v, _| v.reading.source.clone()),
        Source::Trail(t.id.clone())
    );
    // j walks it.
    view.update_in(cx, |v, window, cx| window.focus(&v.reading.focus, cx));
    keys(cx, "j");
    let sel = view.read_with(cx, |v, _| v.reading.sel.clone().map(|k| k.1));
    assert_eq!(sel.as_deref(), Some(LIN_GARDENS));
}

#[gpui_kit::test]
fn shift_t_starts_another_trail(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    fake.create_trail("old").unwrap();
    view.update(cx, |v, cx| v.reload_trails(cx));
    reading_on(&view, ReadMode::Stream, ("sub-lin", LIN_GARDENS), cx);
    keys(cx, "shift-t");
    type_in(cx, "new one");
    keys(cx, "enter");
    keys(cx, "escape");
    let names: Vec<_> = fake
        .trails()
        .into_iter()
        .map(|t| (t.name, t.stops))
        .collect();
    assert_eq!(names, [("old".to_string(), 0), ("new one".to_string(), 1)]);
}

#[gpui_kit::test]
fn feed_posts_cant_be_stops(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    fake.create_trail("t").unwrap();
    view.update(cx, |v, cx| {
        v.reload_trails(cx);
        v.reading.trails.current = Some(fake.trails()[0].id.clone());
    });
    reading_on(&view, ReadMode::Stream, ("sub-omar", OMAR_YEAR), cx);
    keys(cx, "t");
    assert_eq!(fake.trails()[0].stops, 0);
}

#[gpui_kit::test]
fn publish_writes_a_thread_draft_and_publishes_nothing(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    let t = fake.create_trail("weeding").unwrap();
    for (id, why) in [(LIN_GARDENS, "tended"), (ADA_TIDES, "")] {
        let r = fake
            .reading()
            .into_iter()
            .find(|r| r.remote_id == id)
            .unwrap();
        fake.add_trail_stop(&t.id, &super::trails::stop_for(&r, why))
            .unwrap();
    }
    reading_on(&view, ReadMode::Stream, ("sub-lin", LIN_GARDENS), cx);
    view.update(cx, |v, cx| {
        v.reload_trails(cx);
        v.reading.trails.current = Some(t.id.clone());
    });
    keys(cx, "alt-cmd-t");
    assert!(view.read_with(cx, |v, _| v.reading.trails.publish.is_some()));
    keys(cx, "enter");
    let (cur, view_now) = view.read_with(cx, |v, _| (v.current.clone(), v.reading.view));
    let cur = cur.expect("a draft is open");
    assert_eq!(view_now, View::Posts);
    assert_eq!(cur.kind, Kind::Thread);
    assert_eq!(cur.status, Status::Draft);
    assert!(cur.content_md.starts_with("# weeding\n\n"));
    assert!(
        cur.content_md
            .contains(&format!("![[{LIN_GARDENS} | tended]]\n\n"))
    );
    assert!(cur.content_md.contains(&format!("![[{ADA_TIDES} | ")));
    assert!(
        fake.items()
            .iter()
            .all(|i| i.local_id != cur.local_id || i.status == Status::Draft)
    );
}

#[gpui_kit::test]
fn h_on_the_publish_sheet_makes_a_hopper_of_the_stops(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(cx);
    let t = fake.create_trail("slow gardens").unwrap();
    let r = fake
        .reading()
        .into_iter()
        .find(|r| r.remote_id == LIN_GARDENS)
        .unwrap();
    fake.add_trail_stop(&t.id, &super::trails::stop_for(&r, ""))
        .unwrap();
    reading_on(&view, ReadMode::Stream, ("sub-lin", LIN_GARDENS), cx);
    view.update(cx, |v, cx| {
        v.reload_trails(cx);
        v.reading.trails.current = Some(t.id.clone());
    });
    keys(cx, "alt-cmd-t");
    keys(cx, "h");
    settle(cx);
    let h = fake
        .hoppers()
        .into_iter()
        .find(|h| h.name == "slow gardens")
        .expect("a hopper");
    assert!(!h.public, "private, like every new hopper");
    let members = fake
        .hopper_members()
        .get(&("sub-lin".to_string(), LIN_GARDENS.to_string()))
        .cloned()
        .unwrap_or_default();
    assert!(members.contains(&h.id));
    assert!(view.read_with(cx, |v, _| v.reading.trails.publish.is_none()));
}
