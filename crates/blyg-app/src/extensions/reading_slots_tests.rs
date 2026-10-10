//! --- reading slots --- The window driving a real `Host` running the real
//! bundled reading-time and inspect over the fake's invented reading rows:
//! the byline marker (asked off the main thread, cached, drawn in the
//! stream), nothing without `reading.read`, and the ⋯ chip, sheet, inspect's
//! answer and Copy.
//!
//! As in `tests.rs`, the extensions run as child processes of this test
//! binary, each running only its `…_child` test below (which serves the
//! same `run_stdio` that `blygger +ext <name>` calls).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore, ReadingItem};
use blyg_ext::reading::ReadingEntry;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::super::{Launch, host_config};
use super::Phase;
use crate::app::MainView;
use crate::fake::reading_seed::*;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

const RT: &str = blyg_ext_reading_time::NAME;
const INSPECT: &str = blyg_ext_inspect::NAME;

// ------------------------------------------------------------ the children

fn child_test(name: &str) -> String {
    let m = module_path!();
    let m = m.split_once("::").map(|(_, rest)| rest).unwrap_or(m);
    format!("{m}::{name}")
}

fn child_args(name: &str) -> Vec<String> {
    vec![
        "--exact".into(),
        child_test(name),
        "--ignored".into(),
        "--quiet".into(),
        "--test-threads=1".into(),
    ]
}

/// Not a test: the bundled reading-time's process for the tests below.
#[test]
#[ignore = "the reading-time child process of the reading-slot tests"]
fn reading_time_child() {
    if std::env::args().any(|a| a == child_test("reading_time_child")) {
        let _ = blyg_ext_reading_time::run_stdio();
    }
}

/// Not a test: the bundled inspect's process for the tests below.
#[test]
#[ignore = "the inspect child process of the reading-slot tests"]
fn inspect_child() {
    if std::env::args().any(|a| a == child_test("inspect_child")) {
        let _ = blyg_ext_inspect::run_stdio();
    }
}

fn launch(data_dir: PathBuf) -> Launch {
    Launch {
        program: std::env::current_exe().expect("the test binary"),
        args: vec![],
        crosspost_args: vec![],
        reading_time_args: child_args("reading_time_child"),
        inspect_args: child_args("inspect_child"),
        data_dir,
        timing: blyg_ext::Timing::default(),
    }
}

// ------------------------------------------------------------ the window

struct Env<'a> {
    _dir: tempfile::TempDir,
    view: Entity<MainView>,
    fake: Arc<FakeBackend>,
    cx: &'a mut VisualTestContext,
}

fn setup<'a>(cx: &'a mut TestAppContext, config: &str) -> Env<'a> {
    let dir = tempfile::tempdir().unwrap();
    let config = format!("blyg-url = https://blyg.example.com\n{config}");
    let prefs = Prefs::from_config(ConfigStore::in_memory(&config).config());
    let l = launch(dir.path().join("data"));
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::bind_keys(cx);
        crate::settings::init(
            ConfigStore::in_memory(&config),
            Arc::new(MemoryTokenStore::default()),
            None,
            cx,
        );
        cx.set_global(l);
    });
    let fake = Arc::new(FakeBackend::with_timing(Timing::instant()).without_media_cache());
    let backend: Arc<dyn Backend> = fake.clone();
    let f2 = fake.clone();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        MainView::new(backend, Some(f2), prefs, Instant::now(), window, cx)
    });
    cx.run_until_parked();
    Env {
        _dir: dir,
        view,
        fake,
        cx,
    }
}

impl Env<'_> {
    /// Run the window until `done` (the extensions answer from other
    /// processes, in real time).
    fn wait(&mut self, what: &str, mut done: impl FnMut(&MainView) -> bool) {
        let t = Instant::now();
        loop {
            self.cx.run_until_parked();
            self.view
                .update_in(self.cx, |v, window, cx| v.ext_pump(window, cx));
            self.cx.update(|window, _| window.refresh());
            self.cx.run_until_parked();
            if self.view.read_with(self.cx, |v, _| done(v)) {
                return;
            }
            assert!(
                t.elapsed() < Duration::from_secs(20),
                "timed out waiting for {what}"
            );
            std::thread::sleep(Duration::from_millis(15));
        }
    }

    fn start(&mut self) {
        self.view
            .update_in(self.cx, |v, window, cx| v.ext_start(window, cx));
    }

    /// The reading screen, in the stream.
    fn reading(&mut self) {
        self.cx.simulate_keystrokes(&crate::keymap::keys("cmd-r"));
        self.cx.run_until_parked();
    }

    fn row(&self, id: &str) -> ReadingItem {
        self.fake
            .reading()
            .into_iter()
            .find(|r| r.remote_id == id)
            .expect("a seeded row")
    }
}

fn running(v: &MainView, name: &str) -> bool {
    v.ext
        .host
        .as_ref()
        .is_some_and(|h| h.entry_slots().iter().any(|s| s.ext == name))
}

const BOTH: &str = "extension = reading-time\nextension-allow = reading-time reading.read\n\
                    extension = inspect\nextension-allow = inspect reading.read\n";

#[test]
fn the_host_config_carries_both_bundled_slot_extensions() {
    let store = ConfigStore::in_memory(BOTH);
    let l = launch(PathBuf::from("/data"));
    let hc = host_config(&store, &l);
    assert_eq!(hc.enabled, [RT, INSPECT]);
    for (name, args) in [(RT, &l.reading_time_args), (INSPECT, &l.inspect_args)] {
        let b = hc
            .bundled
            .iter()
            .find(|b| b.manifest.name == name)
            .unwrap_or_else(|| panic!("{name} is bundled"));
        match &b.origin {
            blyg_ext::Origin::Bundled { program, args: a } => {
                assert_eq!(program, &l.program);
                assert_eq!(a, args);
            }
            o => panic!("{o:?}"),
        }
    }
    // The app runs them as its own `+ext <name>`.
    let app = crate::cli::host_config(&store);
    for name in [RT, INSPECT] {
        let b = app
            .bundled
            .iter()
            .find(|b| b.manifest.name == name)
            .unwrap();
        let blyg_ext::Origin::Bundled { args, .. } = &b.origin else {
            panic!()
        };
        assert_eq!(args, &["+ext".to_string(), name.to_string()]);
    }
}

#[gpui_kit::test]
fn the_stream_byline_shows_reading_time_once_it_answers(cx: &mut TestAppContext) {
    let mut env = setup(cx, BOTH);
    env.reading();
    // Nothing runs before the start, and nothing is drawn or asked.
    assert!(env.cx.debug_bounds("slot-byline-reading-time").is_none());
    env.view
        .read_with(env.cx, |v, _| assert_eq!(v.ext.slots.asked(), 0));
    env.start();
    env.wait("reading-time to run", |v| running(v, RT));
    let row = env.row(RUE_TRUST);
    let want = blyg_ext_reading_time::byline(&ReadingEntry::of(&row)).expect("it has text");
    let key = (
        RT.to_string(),
        row.subscription_id.clone(),
        row.remote_id.clone(),
        row.version,
    );
    env.wait("the marker", |v| v.ext.slots.marker(&key).is_some());
    env.view.read_with(env.cx, |v, _| {
        assert_eq!(v.ext.slots.marker(&key), Some(Some(want.clone())));
    });
    env.cx.update(|window, _| window.refresh());
    env.cx.run_until_parked();
    assert!(
        env.cx.debug_bounds("slot-byline-reading-time").is_some(),
        "drawn at the end of the byline"
    );
    // Asked once per entry and version: drawing again asks nothing new.
    let asked = env.view.read_with(env.cx, |v, _| v.ext.slots.asked());
    for _ in 0..3 {
        env.cx.update(|window, _| window.refresh());
        env.cx.run_until_parked();
    }
    assert_eq!(
        env.view.read_with(env.cx, |v, _| v.ext.slots.asked()),
        asked
    );
}

#[gpui_kit::test]
fn without_reading_read_nothing_is_asked_or_drawn(cx: &mut TestAppContext) {
    // Enabled, and allowed nothing: consent first, no slots meanwhile.
    let mut env = setup(cx, "extension = reading-time\n");
    env.reading();
    env.start();
    env.wait("the consent sheet", |v| v.ext.overlay.is_some());
    env.view.read_with(env.cx, |v, _| {
        assert!(!running(v, RT));
        assert_eq!(v.ext.slots.asked(), 0, "nothing sent");
    });
    assert!(env.cx.debug_bounds("slot-byline-reading-time").is_none());
}

#[gpui_kit::test]
fn the_more_chip_opens_inspect_and_copies_the_json(cx: &mut TestAppContext) {
    let mut env = setup(cx, BOTH);
    env.reading();
    env.start();
    env.wait("inspect to run", |v| running(v, INSPECT));
    // The selected post's actions end with "⋯".
    env.cx.simulate_keystrokes("j");
    env.cx.run_until_parked();
    env.cx.update(|window, _| window.refresh());
    env.cx.run_until_parked();
    let chip = env.cx.debug_bounds("slot-more").expect("the ⋯ chip");
    env.cx
        .simulate_click(chip.center(), gpui_kit::Modifiers::none());
    env.cx.run_until_parked();
    env.view.read_with(env.cx, |v, _| {
        let s = v.ext.slots.sheet.as_ref().expect("the ⋯ sheet");
        assert_eq!(s.item.remote_id, RUE_TRUST);
        assert_eq!(s.phase, Phase::Choose);
        let rows: Vec<(&str, &str)> = s
            .rows
            .iter()
            .map(|(e, a)| (e.as_str(), a.title.as_str()))
            .collect();
        assert_eq!(rows, [(INSPECT, "inspect")]);
        assert!(v.ext.has_overlay(), "web views hide under it");
    });
    assert!(env.cx.debug_bounds("slot-row-0").is_some());
    // ⏎ runs the row; inspect answers with the record.
    env.cx.simulate_keystrokes("enter");
    env.wait("inspect's sheet", |v| {
        matches!(
            v.ext.slots.sheet.as_ref().map(|s| &s.phase),
            Some(Phase::Shown(..))
        )
    });
    let row = env.row(RUE_TRUST);
    env.view.read_with(env.cx, |v, _| {
        let Some(Phase::Shown(_, sheet)) = v.ext.slots.sheet.as_ref().map(|s| &s.phase) else {
            unreachable!()
        };
        assert_eq!(sheet.title, "inspect");
        let field = |l: &str| {
            sheet
                .fields
                .iter()
                .find(|f| f.label == l)
                .map(|f| f.value.clone())
        };
        assert_eq!(field("id").as_deref(), Some(RUE_TRUST));
        assert_eq!(field("version"), Some(row.version.to_string()));
        assert_eq!(
            field("content hash"),
            Some(blyg_core::content_hash(&row.content_md))
        );
        let code = sheet.code.as_deref().unwrap();
        assert!(code.contains("\"remote_id\""), "{code}");
        assert!(
            !code.contains(row.content_md.lines().next().unwrap_or("x")),
            "the body is elided"
        );
    });
    env.cx.update(|window, _| window.refresh());
    env.cx.run_until_parked();
    assert!(env.cx.debug_bounds("slot-fields").is_some());
    assert!(env.cx.debug_bounds("slot-code").is_some());
    let copy = env.cx.debug_bounds("slot-copy").expect("Copy JSON");
    env.cx
        .simulate_click(copy.center(), gpui_kit::Modifiers::none());
    env.cx.run_until_parked();
    let clip = env
        .cx
        .update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()))
        .unwrap_or_default();
    let json: serde_json::Value = serde_json::from_str(&clip).expect("the JSON");
    assert_eq!(json["remote_id"], RUE_TRUST);
    assert!(
        json["content_md"]
            .as_str()
            .is_some_and(|s| s.ends_with("characters, not shown›"))
    );
    // esc closes it.
    env.cx.simulate_keystrokes("escape");
    env.cx.run_until_parked();
    env.view
        .read_with(env.cx, |v, _| assert!(v.ext.slots.sheet.is_none()));
}
