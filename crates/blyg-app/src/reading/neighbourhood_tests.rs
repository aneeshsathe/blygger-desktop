//! The neighbourhood map inside the ⌘J sheet, in a headless window: real
//! keys through GPUI's dispatch, against a zero-latency FakeBackend.

use std::sync::Arc;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore, TransclusionRef};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use crate::app::MainView;
use crate::fake::reading_seed::{ADA, ADA_TIDES, LIN_FORK};
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";
/// A post that quotes Ada's tide tables: two hops from Lin's fork of them.
const ECHO: &str = "01k2ada0ech000000000000001";

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
    fake.with_reading(|rows| {
        let mut echo = rows
            .iter()
            .find(|r| r.remote_id == ADA_TIDES)
            .expect("Ada's tide tables")
            .clone();
        echo.remote_id = ECHO.into();
        echo.content_md = "Echoes of the tide tables\n\nThey keep coming back.".into();
        echo.content_html.clear();
        echo.stub_of = None;
        echo.forked_from = None;
        echo.transclusions = vec![TransclusionRef {
            id: ADA_TIDES.into(),
            version: Some(1),
            origin: Some(ADA.into()),
            cited: None,
            selector: None,
        }];
        rows.push(echo);
    });
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

fn key(cx: &mut VisualTestContext, k: &str) {
    cx.simulate_keystrokes(k);
    cx.run_until_parked();
}

fn selected(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |v, _| v.lineage_map_selected())
}

fn centre(view: &Entity<MainView>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |v, _| {
        v.lineage_map_rows().and_then(|r| r[2].first().cloned())
    })
}

/// Lin's fork, open in the lineage sheet on the Reading screen.
fn sheet_on_the_fork(view: &Entity<MainView>, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-r"));
    cx.run_until_parked();
    view.update_in(cx, |v, window, cx| {
        let origin = v
            .reading
            .rows
            .iter()
            .find(|r| r.remote_id == LIN_FORK)
            .map(|r| r.origin.clone())
            .expect("Lin's fork is held");
        v.open_lineage(origin, LIN_FORK.into(), window, cx);
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn minus_zooms_out_to_two_hops_and_the_keyboard_walks_it(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(cx);
    sheet_on_the_fork(&view, cx);
    assert!(view.read_with(cx, |v, _| v.lineage_open() && !v.lineage_map_open()));

    // − : the map, centred on the fork.
    key(cx, "-");
    assert!(view.read_with(cx, |v, _| v.lineage_map_open()));
    assert!(cx.debug_bounds("lineage-map").is_some(), "drawn");
    let rows = view.read_with(cx, |v, _| v.lineage_map_rows()).unwrap();
    assert_eq!(rows[2], ["Tides, revised"]);
    assert_eq!(
        rows[1].len(),
        2,
        "forked from Ada's tables, quotes Kit: {rows:?}"
    );
    assert!(
        rows[0].iter().any(|t| t == "Echoes of the tide tables"),
        "two hops up: another post drawing on Ada's tables: {rows:?}"
    );
    assert_eq!(selected(&view, cx).as_deref(), Some("Tides, revised"));

    // ↑ ↑ reaches the second hop; ⏎ makes it the centre.
    key(cx, "up");
    key(cx, "up");
    let far = selected(&view, cx).expect("a node");
    assert_eq!(far, "Echoes of the tide tables");
    key(cx, "enter");
    assert_eq!(
        centre(&view, cx).as_deref(),
        Some("Echoes of the tide tables")
    );
    assert!(
        view.read_with(cx, |v, _| v.lineage_map_open()),
        "still the map"
    );
    // Re-centred: Ada's tables are one hop up now, Lin's fork two.
    let rows = view.read_with(cx, |v, _| v.lineage_map_rows()).unwrap();
    assert!(rows[0].iter().any(|t| t == "Tides, revised"), "{rows:?}");

    // ⌫ walks back.
    key(cx, "backspace");
    assert_eq!(centre(&view, cx).as_deref(), Some("Tides, revised"));

    // + is the list again; esc closes.
    key(cx, "+");
    assert!(view.read_with(cx, |v, _| v.lineage_open() && !v.lineage_map_open()));
    key(cx, "escape");
    assert!(view.read_with(cx, |v, _| !v.lineage_open()));
}

#[gpui_kit::test]
fn o_on_the_map_opens_the_highlighted_post_in_the_reader(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(cx);
    sheet_on_the_fork(&view, cx);
    key(cx, "-");
    // Up to the row it draws on, then the first: Ada's tables (held).
    key(cx, "up");
    key(cx, "left");
    assert_ne!(selected(&view, cx).as_deref(), Some("Tides, revised"));
    key(cx, "o");
    view.read_with(cx, |v, _| {
        assert!(!v.lineage_open(), "the sheet closes");
        let o = v.reading.opened.as_ref().expect("opened in the reader");
        assert_eq!(o.item.remote_id, ADA_TIDES);
    });
}
