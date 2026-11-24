//! --- test children --- A test's extension processes end when the test
//! does. The tests that run extensions run them as this test binary's
//! children (`extensions::test_launch`); one `cargo test -p blyg-app` once
//! left about 130 of them alive at once, each living until the whole test
//! binary exited, and the machine's load went past 100.
//!
//! Each child holds a lock on [`ALIVE`] in its working folder (its storage
//! folder) while it runs, so a test can see from the outside, on every
//! platform, whether a child is still alive: the system drops the lock when
//! the process ends.

use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use blyg_core::config::MemoryTokenStore;
use blyg_core::{Backend, ConfigStore};
use blyg_ext::ExtState;
use gpui_kit::TestAppContext;

use crate::app::MainView;
use crate::fake::{FakeBackend, Timing};
use crate::prefs::Prefs;

/// The file a child extension process locks while it runs.
const ALIVE: &str = "alive.lock";

/// In a child extension process (a child test, before it serves): lock
/// [`ALIVE`] in the working folder for as long as the process lives.
pub(crate) fn hold_alive_lock() {
    let f = File::create(ALIVE).expect("the child's alive lock");
    f.lock().expect("lock the child's alive lock");
    // Held until the process exits: the system releases it then.
    std::mem::forget(f);
}

/// Some process (a child) holds `path`'s lock.
fn held(path: &Path) -> bool {
    let f = File::open(path).expect("an alive lock");
    match f.try_lock() {
        Ok(()) => false,
        Err(TryLockError::WouldBlock) => true,
        Err(TryLockError::Error(e)) => panic!("{}: {e}", path.display()),
    }
}

/// Every [`ALIVE`] lock under `dir`.
fn locks(dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(locks(&p));
        } else if p.file_name().is_some_and(|n| n == ALIVE) {
            out.push(p);
        }
    }
    out
}

/// What a test does: a window whose tour runs the bundled extensions (as
/// `test_launch`'s children over `data`), until all four are running. The
/// locks they hold come back.
fn tour_running(cx: &mut TestAppContext, data: PathBuf) -> Vec<PathBuf> {
    let config = "blyg-url = https://blyg.example.com\n";
    let prefs = Prefs::from_config(ConfigStore::in_memory(config).config());
    let launch = super::test_launch(data.clone());
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::bind_keys(cx);
        crate::settings::init(
            ConfigStore::in_memory(config),
            Arc::new(MemoryTokenStore::default()),
            None,
            cx,
        );
        cx.set_global(launch);
    });
    let fake = Arc::new(FakeBackend::with_timing(Timing::instant()).without_media_cache());
    let backend: Arc<dyn Backend> = fake.clone();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        MainView::new(backend, Some(fake), prefs, Instant::now(), window, cx)
    });
    cx.run_until_parked();
    view.update_in(cx, |v, window, cx| v.ext_tour_start(window, cx));
    crate::test_wait::until(cx, "the tour's four extensions running", |cx| {
        view.update_in(cx, |v, window, cx| v.ext_tour_pump(window, cx));
        cx.run_until_parked();
        let running = view.read_with(cx, |v, _| {
            v.ext.host.as_ref().map_or(0, |h| {
                h.status()
                    .iter()
                    .filter(|s| s.state == ExtState::Running)
                    .count()
            })
        });
        running == 4 && locks(&data).len() == 4
    });
    let held_now = locks(&data);
    for l in &held_now {
        assert!(held(l), "{}: the running child holds it", l.display());
    }
    held_now
}

/// A test that runs extensions (here the tour's four, as the tour's tests
/// do) leaves none of its child processes alive once it has ended. The
/// test runs on a thread of its own, as the test harness runs each test.
#[test]
fn a_tests_extension_children_end_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let d = data.clone();
    let alive = std::thread::Builder::new()
        .name("a test running extensions".into())
        .spawn(move || tour_running(&mut TestAppContext::single(), d))
        .unwrap()
        .join()
        .expect("the test that runs extensions passed");
    assert_eq!(alive.len(), 4, "{alive:?}");
    for l in &alive {
        crate::test_wait::poll(
            &format!("the child holding {} to have exited", l.display()),
            || !held(l),
        );
    }
}
