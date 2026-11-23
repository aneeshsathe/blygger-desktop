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

/// A card seen for a second under "when I see it": its mark waits in the
/// batch, not yet sent.
fn seen_and_waiting(
    view: &Entity<MainView>,
    fake: &FakeBackend,
    cx: &mut VisualTestContext,
) -> Instant {
    stream(view, cx);
    let t0 = Instant::now();
    view.update(cx, |v, cx| {
        v.stream_tick(t0, true, cx);
        v.stream_tick(t0 + Duration::from_millis(1050), true, cx);
    });
    settle(cx);
    view.read_with(cx, |v, _| {
        assert!(!v.reading.count_read.batch.is_empty(), "the batch holds it")
    });
    assert_eq!(read_version(fake, ADA_FINISHED), None, "not sent yet");
    t0
}

#[gpui_kit::test]
fn quitting_sends_the_waiting_marks(app: &mut TestAppContext) {
    let (view, fake, cx) = setup(fake(), app);
    choose(&view, &fake, MarkRead::Seen, cx);
    seen_and_waiting(&view, &fake, cx);
    // The app's quit: its quit handlers run, then it stops.
    app.quit();
    assert!(
        read_version(&fake, ADA_FINISHED).is_some(),
        "written before Burrow quit"
    );
}

#[gpui_kit::test]
fn closing_the_window_sends_the_waiting_marks(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(fake(), cx);
    choose(&view, &fake, MarkRead::Seen, cx);
    seen_and_waiting(&view, &fake, cx);
    drop(view);
    cx.update(|window, _| window.remove_window());
    cx.run_until_parked();
    assert!(
        read_version(&fake, ADA_FINISHED).is_some(),
        "written as the window closed"
    );
}

/// Choosing another rule drops the marks "when I see it" collected: under
/// "when I open it" a card only seen isn't read.
#[gpui_kit::test]
fn choosing_another_rule_drops_the_waiting_marks(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(fake(), cx);
    choose(&view, &fake, MarkRead::Seen, cx);
    let t0 = seen_and_waiting(&view, &fake, cx);
    view.update(cx, |v, cx| v.set_count_read(MarkRead::Opened, cx));
    view.read_with(cx, |v, _| {
        assert!(v.reading.count_read.batch.is_empty(), "dropped")
    });
    view.update(cx, |v, cx| {
        v.stream_tick(t0 + Duration::from_secs(5), true, cx)
    });
    view.update(cx, |v, cx| {
        v.stream_tick(t0 + Duration::from_secs(6), false, cx)
    });
    settle(cx);
    assert_eq!(read_version(&fake, ADA_FINISHED), None, "seen isn't read");
}

/// Offline at launch: the first read of the blyg's choice fails, and the
/// next time Reading opens it's asked again (not only when Settings opens).
#[gpui_kit::test]
fn a_failed_first_read_of_the_choice_is_tried_again(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(fake(), cx);
    fake.set_offline(true);
    view.update(cx, |v, cx| v.count_read_refresh(false, cx));
    settle(cx);
    view.read_with(cx, |v, _| assert_eq!(v.reading.count_read.setting, None));
    fake.set_offline(false);
    view.update(cx, |v, cx| v.count_read_refresh(false, cx));
    until(cx, "the choice, read again", |cx| {
        view.read_with(cx, |v, _| {
            v.reading.count_read.setting == Some(MarkRead::Seen)
        })
    });
}

// ------------------------------------------------- two blygs (the switcher)

const A: &str = "https://blyg.example.com";
const B: &str = "https://notes.example.org";
const TWO: &str = "blyg-url = https://blyg.example.com\nblyg = https://blyg.example.com\nblyg = https://notes.example.org\n";

/// Blygs `a` and `b`, each a `FakeBackend` with the same reading list,
/// behind the app's `SwitchBackend`; `a` is active.
struct Two {
    a: Arc<FakeBackend>,
    b: Arc<FakeBackend>,
    switch: Arc<crate::connection::SwitchBackend>,
    _dir: tempfile::TempDir,
}

fn setup_two(cx: &mut TestAppContext) -> (Entity<MainView>, Two, &mut VisualTestContext) {
    use crate::connection::{BlygOpener, Connection, Mode, SwitchBackend};
    use blyg_core::config::TokenStore;
    let (a, b) = (Arc::new(fake()), Arc::new(fake()));
    let tokens = Arc::new(MemoryTokenStore::default());
    tokens.set(A, "tok-a").unwrap();
    tokens.set(B, "tok-b").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let switch = SwitchBackend::new(a.clone(), Mode::Fake);
    let backend: Arc<dyn Backend> = switch.clone();
    let prefs = Prefs::from_config(ConfigStore::in_memory(TWO).config());
    {
        let (a, b, sw, data_dir) = (
            a.clone(),
            b.clone(),
            switch.clone(),
            dir.path().to_path_buf(),
        );
        cx.update(move |cx| {
            gpui_kit::init(cx);
            crate::app::bind_keys(cx);
            crate::settings::init(ConfigStore::in_memory(TWO), tokens, None, cx);
            cx.set_global(Connection {
                switch: sw,
                data_dir,
                verify: crate::connection::fake_verifier,
            });
            cx.set_global(BlygOpener(Arc::new(move |url: &str| {
                let f = if url.contains("notes.") {
                    b.clone()
                } else {
                    a.clone()
                };
                Ok(Some((f as Arc<dyn Backend>, Mode::Fake)))
            })));
        });
    }
    let f = a.clone();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        MainView::new(
            backend,
            Some(f),
            prefs,
            std::time::Instant::now(),
            window,
            cx,
        )
    });
    cx.run_until_parked();
    (
        view,
        Two {
            a,
            b,
            switch,
            _dir: dir,
        },
        cx,
    )
}

/// Switching blygs sends the marks waiting in the batch to the blyg they
/// were seen on, never to the one switched to.
#[gpui_kit::test]
fn switching_blygs_sends_the_waiting_marks_to_their_blyg(cx: &mut TestAppContext) {
    let (view, two, cx) = setup_two(cx);
    choose(&view, &two.a, MarkRead::Seen, cx);
    seen_and_waiting(&view, &two.a, cx);
    view.update_in(cx, |v, window, cx| v.switch_blyg(B.into(), window, cx));
    settle(cx);
    assert_eq!(
        cx.update(|_, cx| crate::settings::blyg_url(cx)).as_deref(),
        Some(B)
    );
    assert!(
        read_version(&two.a, ADA_FINISHED).is_some(),
        "the blyg it was seen on"
    );
    assert_eq!(read_version(&two.b, ADA_FINISHED), None, "not the new one");
}

/// The backend swapped without the reading state rebuilt (as connecting a
/// blyg from sample data does): the batch still goes to the blyg its marks
/// were seen on.
#[gpui_kit::test]
fn a_swapped_backend_never_gets_the_marks_seen_before(cx: &mut TestAppContext) {
    let (view, two, cx) = setup_two(cx);
    choose(&view, &two.a, MarkRead::Seen, cx);
    let t0 = seen_and_waiting(&view, &two.a, cx);
    two.switch
        .switch(two.b.clone(), crate::connection::Mode::Fake, || {});
    view.update(cx, |v, cx| {
        v.stream_tick(t0 + Duration::from_secs(5), true, cx)
    });
    let a = two.a.clone();
    until(cx, "the marks, on the blyg they were seen on", move |_| {
        read_version(&a, ADA_FINISHED).is_some()
    });
    assert_eq!(read_version(&two.b, ADA_FINISHED), None, "not the new one");
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

/// Connecting a blyg (here from blyg `a`, as from the sample data) puts the
/// new blyg's "Count an entry as read" in force at once, without Settings
/// opening. Before 0.28.0's fix the backend was swapped and synced but the
/// reading state kept the old backend's rule (read already, so never asked
/// again), and entries were marked read by it until a restart.
#[gpui_kit::test]
fn connecting_a_blyg_puts_its_rule_in_force(cx: &mut TestAppContext) {
    use blyg_core::api::auth::Credential;
    let (view, two, cx) = setup_two(cx);
    choose(&view, &two.a, MarkRead::Seen, cx);
    two.b.load_reader_mark_read().unwrap();
    two.b.save_reader_mark_read(MarkRead::Never).unwrap();
    view.update_in(cx, |v, window, cx| {
        v.finish_connect(B, &Credential::Token("tok-b".into()), window, cx)
    });
    settle(cx);
    assert_eq!(
        cx.update(|_, cx| crate::settings::blyg_url(cx)).as_deref(),
        Some(B)
    );
    assert!(
        cx.debug_bounds("count-read").is_none(),
        "Settings never opened"
    );
    until(cx, "the new blyg's rule", |cx| {
        view.read_with(cx, |v, _| {
            v.reading.count_read.setting == Some(MarkRead::Never)
        })
    });
    view.read_with(cx, |v, _| {
        assert!(!v.reading.count_read.on_open(), "opening doesn't count");
        assert_eq!(v.reading.count_read.seen_rule(), None, "nor seeing");
    });
}

/// Every place the backend under the window changes (switching blygs,
/// connecting, signing out, the sample data, the tour) rebuilds the reading
/// state through `MainView::reading_for_new_backend`, which writes the seen
/// marks waiting, builds the state from the new backend and reads its rule.
/// A site that builds `reading::State` itself can forget one of those, as
/// connecting did.
#[test]
fn the_reading_state_is_rebuilt_in_one_place() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    let mut dirs = vec![src.clone()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if !name.ends_with(".rs") || name.ends_with("_tests.rs") || name == "tests.rs" {
                continue;
            }
            let rel = path.strip_prefix(&src).unwrap().to_string_lossy();
            let text = std::fs::read_to_string(&path).unwrap();
            files.push((rel.replace('\\', "/"), text));
        }
    }
    let stray = stray_reading_builds(&files);
    assert!(
        stray.is_empty(),
        "build the reading state only in MainView::new (app.rs) and \
         reading_for_new_backend (reading/count_read.rs); call \
         reading_for_new_backend instead at: {stray:?}"
    );
    // The scan sees the two it allows (so it still finds the calls at all).
    let all = reading_builds(&files);
    assert!(all.iter().any(|f| f.starts_with("app.rs:")), "{all:?}");
    assert!(
        all.iter().any(|f| f.starts_with("reading/count_read.rs:")),
        "{all:?}"
    );
}

/// The files allowed to build `reading::State`.
const READING_BUILT_IN: [&str; 2] = ["app.rs", "reading/count_read.rs"];

/// Each `file:line` that builds the reading state: `State::new(&*…)` under
/// any path (`reading::`, `super::`, `crate::app::reading::`, or none),
/// the only `State::new` taking a backend. `InputState::new(` and the
/// like don't count.
fn reading_builds(files: &[(String, String)]) -> Vec<String> {
    let mut found = Vec::new();
    for (path, text) in files {
        for (i, line) in text.lines().enumerate() {
            let builds = line.match_indices("State::new(&*").any(|(at, _)| {
                !line[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_')
            });
            if builds {
                found.push(format!("{path}:{}", i + 1));
            }
        }
    }
    found
}

/// [`reading_builds`] outside [`READING_BUILT_IN`].
fn stray_reading_builds(files: &[(String, String)]) -> Vec<String> {
    reading_builds(files)
        .into_iter()
        .filter(|f| !READING_BUILT_IN.contains(&f.split(':').next().unwrap()))
        .collect()
}

/// The scan fails on the shape that went wrong: `finish_connect` swapping
/// the backend and building the state itself (as the switcher did, and the
/// sample data with `super::State::new`), and passes the allowed files.
#[test]
fn the_reading_state_scan_catches_a_swap_that_builds_it_itself() {
    let file = |path: &str, text: &str| (path.to_string(), text.to_string());
    let old_connect = file(
        "app/connect.rs",
        "match crate::connection::go_live(&url, cx) {\n    Ok(true) => {\n        \
         self.reading = reading::State::new(&*self.backend, cx);\n",
    );
    assert_eq!(
        stray_reading_builds(&[old_connect]),
        vec!["app/connect.rs:3"]
    );
    let sample = file(
        "onboarding/mod.rs",
        "self.reading = super::reading::State::new(&*self.backend, cx);",
    );
    let in_reading = file("reading/mentions.rs", "let s = State::new(&*backend, cx);");
    let via_super = file("reading/feed.rs", "x = super::State::new(&*b, cx);");
    assert_eq!(
        stray_reading_builds(&[sample, in_reading, via_super]),
        vec![
            "onboarding/mod.rs:1",
            "reading/mentions.rs:1",
            "reading/feed.rs:1"
        ]
    );
    let fine = [
        file(
            "app.rs",
            "let reading = reading::State::new(&*backend, cx);",
        ),
        file(
            "reading/count_read.rs",
            "self.reading = crate::app::reading::State::new(&*self.backend, cx);",
        ),
        file("palette.rs", "cx.new(|cx| InputState::new(window, cx));"),
        file("app.rs", "onboarding: onboarding::State::new(cx),"),
    ];
    assert!(stray_reading_builds(&fine).is_empty());
}

/// The rule chosen in Studio on the web while Burrow was in the background
/// is in force once its window comes back to the front, without Settings
/// or a restart. Before 0.28.0's fix it was read only when Reading first
/// opened and when Settings opened, so cards went on being marked read.
#[gpui_kit::test]
fn the_window_coming_to_the_front_reads_the_rule_again(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(fake(), cx);
    choose(&view, &fake, MarkRead::Seen, cx);
    cx.deactivate_window();
    // Chosen in Studio meanwhile.
    fake.save_reader_mark_read(MarkRead::Never).unwrap();
    cx.update(|window, _| window.activate_window());
    until(cx, "the rule chosen on the web", |cx| {
        view.read_with(cx, |v, _| {
            v.reading.count_read.setting == Some(MarkRead::Never)
        })
    });
    assert!(
        cx.debug_bounds("count-read").is_none(),
        "Settings never opened"
    );
}

/// A blyg whose rule was never asked for (Reading and Settings unopened)
/// isn't asked each time the window comes to the front.
#[gpui_kit::test]
fn the_front_window_asks_only_once_the_rule_is_in_use(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(fake(), cx);
    cx.deactivate_window();
    cx.update(|window, _| window.activate_window());
    settle(cx);
    view.read_with(cx, |v, _| assert_eq!(v.reading.count_read.setting, None));
}

/// Coming to the front reads the rule again at most once a minute
/// (`FRONT_REREAD_GAP`): switching windows back and forth doesn't send a
/// settings read each time. Before the fix every activation sent one.
#[gpui_kit::test]
fn the_front_window_asks_at_most_once_a_minute(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(fake(), cx);
    choose(&view, &fake, MarkRead::Seen, cx);
    let front = |cx: &mut VisualTestContext| {
        cx.deactivate_window();
        cx.update(|window, _| window.activate_window());
        settle(cx);
    };
    let rule = |cx: &mut VisualTestContext| view.read_with(cx, |v, _| v.reading.count_read.setting);
    fake.save_reader_mark_read(MarkRead::Never).unwrap();
    front(cx);
    assert_eq!(
        rule(cx),
        Some(MarkRead::Never),
        "read on coming to the front"
    );
    fake.save_reader_mark_read(MarkRead::Opened).unwrap();
    cx.executor().advance_clock(Duration::from_secs(20));
    front(cx);
    assert_eq!(
        rule(cx),
        Some(MarkRead::Never),
        "not again within the minute"
    );
    cx.executor().advance_clock(Duration::from_secs(45));
    front(cx);
    assert_eq!(rule(cx), Some(MarkRead::Opened), "again after it");

// --- studio 0.71 user testing ---

/// Settings, open, with the blyg's choice read.
fn settings(view: &Entity<MainView>, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes(&crate::keymap::keys("cmd-,"));
    until(cx, "Settings to read the blyg's choice", |cx| {
        view.read_with(cx, |v, _| v.reading.count_read.setting.is_some())
    });
    settle(cx);
}

/// Finding 4: ⇥ never reached "Count an entry as read". It's a stop after
/// THEME now: ←/→ choose, space or ⏎ save, as on the other rows.
#[gpui_kit::test]
fn tab_reaches_read_marks_and_space_or_enter_saves(cx: &mut TestAppContext) {
    let (view, fake, cx) = setup(fake(), cx);
    settings(&view, cx);
    assert_eq!(fake.reader_mark_read(), Some(MarkRead::Seen));
    // WRITING FONT, INTERFACE FONT, SIZE, THEME, then READ MARKS.
    for _ in 0..5 {
        cx.simulate_keystrokes("tab");
    }
    settle(cx);
    assert!(
        cx.debug_bounds("count-read-focused").is_some(),
        "a focus ring on the choice"
    );
    cx.simulate_keystrokes("right");
    settle(cx);
    assert_eq!(
        fake.reader_mark_read(),
        Some(MarkRead::Seen),
        "→ only moves the ring"
    );
    cx.simulate_keystrokes("space");
    let f = fake.clone();
    until(cx, "space to save “when I open it”", move |_| {
        f.reader_mark_read() == Some(MarkRead::Opened)
    });
    cx.simulate_keystrokes("right");
    cx.simulate_keystrokes("enter");
    let f = fake.clone();
    until(cx, "⏎ to save “only when I mark it”", move |_| {
        f.reader_mark_read() == Some(MarkRead::Never)
    });
    // ⇧⇥ goes back to THEME.
    cx.simulate_keystrokes("shift-tab");
    view.read_with(cx, |v, _| {
        assert_eq!(
            v.preview.cursor.as_ref().map(|c| c.0),
            Some(crate::app::pref_preview::Row::Theme)
        )
    });
}

/// Finding 4: its own row, and what every choice means beside it.
#[gpui_kit::test]
fn read_marks_is_its_own_row_and_says_what_each_choice_means(cx: &mut TestAppContext) {
    let (view, _fake, cx) = setup(fake(), cx);
    settings(&view, cx);
    assert!(cx.debug_bounds("settings-row-READ MARKS").is_some());
    for m in MarkRead::ALL {
        let id = format!("count-read-hint-{}", m.as_str());
        assert!(
            cx.debug_bounds(Box::leak(id.into_boxed_str())).is_some(),
            "{m:?}"
        );
    }
    let opened = super::count_read::hint(MarkRead::Opened);
    assert!(
        opened.contains("short posts stay unread until opened or marked"),
        "{opened}"
    );
    let seen = super::count_read::hint(MarkRead::Seen);
    assert!(!seen.contains("60%"), "the detail is in the guide: {seen}");
    for (p, key) in [
        (crate::platform_text::Platform::Mac, "⇧R"),
        (crate::platform_text::Platform::Windows, "Shift+R"),
    ] {
        let never =
            crate::platform_text::spelled_as(p, || super::count_read::hint(MarkRead::Never));
        assert!(never.contains(key), "{p:?}: {never}");
        assert!(never.contains("Mark All Read"), "{never}");
    }
}
