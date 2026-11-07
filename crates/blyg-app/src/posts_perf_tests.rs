//! The Posts screen at scale: typing into a long thread with the preview
//! up, and typing an omnibar search, over thousands of your own posts and
//! a reading list (invented content: `blyg_core::synth`), on the real
//! store through a `LiveBackend` that never syncs. What's timed is the
//! UI thread's work for one keystroke, against a budget in frames.
//!
//!     cargo test -p blyg-app --release posts_perf_tests -- --nocapture
//!
//! Debug builds hold fewer posts and get looser budgets.

use std::sync::Arc;
use std::time::{Duration, Instant};

use blyg_core::config::MemoryTokenStore;
use blyg_core::store::Store;
use blyg_core::{Backend, ConfigStore, CoreEvent, Kind, LiveBackend, LocalId, SyncOptions, synth};
use gpui_kit::{Entity, Keystroke, TestAppContext, VisualTestContext};

use super::MainView;
use super::studio::ViewMode;
use crate::prefs::Prefs;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";

/// One frame at 60 Hz.
const FRAME_MS: f64 = 1000.0 / 60.0;

fn posts() -> usize {
    if cfg!(debug_assertions) { 500 } else { 2_000 }
}

fn reading_rows() -> usize {
    if cfg!(debug_assertions) { 1_000 } else { 4_000 }
}

/// A quarter of a frame in a release build: the rest of the frame is the
/// draw. Debug builds get 15x; Windows 4x on top (its CI runners are
/// several times slower at everything that touches the disk).
fn budget() -> Duration {
    let debug = if cfg!(debug_assertions) { 15.0 } else { 1.0 };
    let os = if cfg!(windows) { 4.0 } else { 1.0 };
    Duration::from_secs_f64(FRAME_MS / 4.0 * debug * os / 1e3)
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

/// Per-keystroke wall-clock times. The worst one is what you feel, and
/// it's printed, but the budget is checked against the median: on a busy
/// machine (a CI runner, a build alongside) the worst of two hundred
/// keystrokes measures the scheduler. The work this guards against (a
/// store transaction, a copy of every post) is in every keystroke, so it
/// moves the median: it was 50 ms here before.
#[derive(Default)]
struct Keys(Vec<Duration>);

impl Keys {
    /// Prints the median and the worst; returns the median.
    fn report(&self, label: &str) -> Duration {
        let mut v = self.0.clone();
        v.sort();
        let worst = v.last().copied().unwrap_or_default();
        let median = v.get(v.len() / 2).copied().unwrap_or_default();
        eprintln!(
            "{label:<40} {} keys · median {:>7.3} ms · worst {:>7.3} ms ({:.2} frames)",
            v.len(),
            ms(median),
            ms(worst),
            ms(worst) / FRAME_MS
        );
        median
    }
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..10 {
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// The thread being typed into: ~20k characters that link and quote held
/// posts (so the preview has something to resolve).
fn long_thread() -> String {
    let mut md =
        String::from("# A long thread\n\n![[r000020]]\n\nSee [[r000010]] and [[r000011]].\n\n");
    for r in synth::reading(40, 9) {
        if md.len() > 20_000 {
            break;
        }
        md.push_str(&r.content_md);
        md.push_str("\n\n");
    }
    md
}

fn setup<'a>(
    dir: &std::path::Path,
    cx: &'a mut TestAppContext,
) -> (
    Entity<MainView>,
    Arc<LiveBackend>,
    LocalId,
    &'a mut VisualTestContext,
) {
    let thread = {
        let store = Store::open(&dir.join("blygger.db")).unwrap();
        store
            .replace_subscriptions(&synth::subscriptions())
            .unwrap();
        store
            .merge_reading(&synth::reading(reading_rows(), 3), true, &synth::kinds())
            .unwrap();
        // Your own posts, each with its create still queued (the worst
        // case for the outbox's status queries).
        for r in synth::reading(posts(), 11) {
            let md = if r.content_md.is_empty() {
                "a withdrawn post's stand-in".to_string()
            } else {
                r.content_md
            };
            store.create_draft(Kind::Fragment, &md, 0).unwrap();
        }
        store.create_draft(Kind::Thread, &long_thread(), 0).unwrap()
    };
    let live = Arc::new(
        LiveBackend::open_with(
            dir,
            // Nothing listens here; nothing is ever sent anyway.
            "http://127.0.0.1:9",
            "perf-test",
            SyncOptions {
                start_worker: false,
                ..SyncOptions::default()
            },
        )
        .unwrap(),
    );
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
    let backend: Arc<dyn Backend> = live.clone();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        MainView::new(backend, None, prefs, std::time::Instant::now(), window, cx)
    });
    settle(cx);
    // The typing writer reports the sync status from its own thread, which
    // GPUI's deterministic test scheduler refuses; nothing here needs it.
    live.set_event_sink(Box::new(|_| {}));
    (view, live, thread, cx)
}

fn key(cx: &mut VisualTestContext, k: &str) -> Duration {
    let ks = Keystroke::parse(k).unwrap();
    let t = Instant::now();
    cx.update(|window, cx| window.dispatch_keystroke(ks, cx));
    t.elapsed()
}

/// Typing into a 20k-character thread with the preview up: every
/// keystroke saves, re-titles the list row and schedules a render.
#[gpui_kit::test]
fn typing_at_scale_within_budget(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, live, thread, cx) = setup(dir.path(), cx);
    eprintln!(
        "{} own posts, {} reading rows held, thread {} chars",
        live.items().len(),
        live.reading().len(),
        live.item(&thread).unwrap().content_md.chars().count()
    );
    view.update_in(cx, |v, window, cx| {
        v.studio_set_view(ViewMode::Split, cx);
        v.open(&thread, window, cx);
    });
    settle(cx);
    let before = live.item(&thread).unwrap().content_md;

    let mut keys = Keys::default();
    let typed = "the quiet harbour keeps its lanterns lit ";
    for round in 0..5 {
        for c in typed.chars() {
            let k = if c == ' ' {
                "space".to_string()
            } else {
                c.to_string()
            };
            keys.0.push(key(cx, &k));
        }
        // Background work (the writer, a render) runs between bursts, as
        // it would while you type.
        if round % 2 == 1 {
            settle(cx);
        }
    }
    let median = keys.report("typing (20k thread, preview up)");

    // Every keystroke landed, in order, and the list row shows it.
    settle(cx);
    let want = format!("{before}{}", typed.repeat(5));
    let editor = view.read_with(cx, |v, cx| v.editor.read(cx).value().to_string());
    assert_eq!(editor, want);
    let stored = live.item(&thread).unwrap();
    assert_eq!(stored.content_md, want);
    view.read_with(cx, |v, _| {
        let row = v
            .list
            .results()
            .iter()
            .find(|i| i.local_id == thread)
            .unwrap();
        assert_eq!(row.content_md, want);
    });
    assert!(
        median < budget(),
        "median keystroke {median:?} (budget {:?})",
        budget()
    );
}

/// Typing an omnibar search over thousands of posts, and the list's
/// refresh after a sync tick.
#[gpui_kit::test]
fn omnibar_search_at_scale_within_budget(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, live, thread, cx) = setup(dir.path(), cx);
    let mut keys = Keys::default();
    for (i, c) in "garden notes".chars().enumerate() {
        let k = if c == ' ' {
            "space".to_string()
        } else {
            c.to_string()
        };
        let t = key(cx, &k);
        let q: String = "garden notes".chars().take(i + 1).collect();
        eprintln!("  omnibar keystroke {q:<14?} {:>7.3} ms", ms(t));
        keys.0.push(t);
    }
    let typing = keys.report("omnibar typing");
    settle(cx);
    // What's listed is exactly what the backend finds for the query.
    let want: Vec<LocalId> = live
        .search("garden notes")
        .into_iter()
        .map(|i| i.local_id)
        .collect();
    let shown: Vec<LocalId> = view.read_with(cx, |v, _| {
        assert_eq!(v.list.query(), "garden notes");
        v.list
            .results()
            .iter()
            .map(|i| i.local_id.clone())
            .collect()
    });
    assert_eq!(shown, want);

    // A sync tick after a save: the list is refreshed for the query.
    view.update_in(cx, |v, window, cx| v.set_query_text("", window, cx));
    settle(cx);
    let mut ticks = Keys::default();
    for i in 0..5 {
        live.save(&thread, &format!("{} {i}", long_thread()))
            .unwrap();
        let t = Instant::now();
        view.update_in(cx, |v, window, cx| {
            v.on_core_event(CoreEvent::ItemsChanged, window, cx)
        });
        ticks.0.push(t.elapsed());
        settle(cx);
    }
    let tick = ticks.report("ItemsChanged (empty query)");
    let n = view.read_with(cx, |v, _| v.list.results().len());
    assert_eq!(n, live.items().len());
    // The background search itself, for the record (not on the UI thread).
    for q in ["", "g", "ga", "gar", "garden", "garden notes"] {
        live.save(&thread, &format!("{} {q}", long_thread()))
            .unwrap();
        let t = Instant::now();
        let hits = live.search(q).len();
        eprintln!(
            "  backend search {q:<14?} {:>7.3} ms ({hits} hits)",
            ms(t.elapsed())
        );
    }
    assert!(
        typing < budget() && tick < budget(),
        "median omnibar keystroke {typing:?}, median refresh {tick:?} (budget {:?})",
        budget()
    );
}
