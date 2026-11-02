//! The reading screens at scale: thousands of posts (`blyg_core::synth`,
//! invented content) on the real store, through a `LiveBackend` that
//! never syncs (no worker, a base URL nothing answers on). Each step is
//! something the UI thread does on a keystroke, an open or a sync tick,
//! so each has a budget.
//!
//!     cargo test -p blyg-app --release reading::perf_tests -- --nocapture
//!
//! Debug builds hold fewer posts and get looser budgets.

use std::sync::Arc;
use std::time::{Duration, Instant};

use blyg_core::config::MemoryTokenStore;
use blyg_core::store::Store;
use blyg_core::{Backend, ConfigStore, LiveBackend, SyncOptions, synth};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::View;
use crate::app::MainView;
use crate::prefs::Prefs;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";

fn rows() -> usize {
    if cfg!(debug_assertions) {
        2_000
    } else {
        10_000
    }
}

/// Budget in a release build; debug builds get 15x. Windows gets 4x on
/// top: its CI runners sync SQLite writes to disk several times slower
/// (opening a post, which marks it read, took 4.3 s there against 1.8 s),
/// and the budgets are there to catch a regression of an order of
/// magnitude, not the runner's disk.
fn budget(ms: u64) -> Duration {
    let debug = if cfg!(debug_assertions) { 15 } else { 1 };
    let os = if cfg!(windows) { 4 } else { 1 };
    Duration::from_millis(ms * debug * os)
}

#[derive(Default)]
struct Over(Vec<String>);

impl Over {
    fn time<T>(&mut self, label: &str, ms: u64, f: impl FnOnce() -> T) -> T {
        let t = Instant::now();
        let out = f();
        let e = t.elapsed();
        eprintln!("{label:<44} {:>9.2} ms", e.as_secs_f64() * 1e3);
        if e >= budget(ms) {
            self.0
                .push(format!("{label}: {e:?} (budget {:?})", budget(ms)));
        }
        out
    }
    fn done(self) {
        assert!(self.0.is_empty(), "over budget:\n{}", self.0.join("\n"));
    }
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..10 {
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn setup<'a>(
    dir: &std::path::Path,
    cx: &'a mut TestAppContext,
) -> (
    Entity<MainView>,
    Arc<LiveBackend>,
    &'a mut VisualTestContext,
) {
    {
        let store = Store::open(&dir.join("blygger.db")).unwrap();
        store
            .replace_subscriptions(&synth::subscriptions())
            .unwrap();
        store
            .merge_reading(&synth::reading(rows(), 3), true, &synth::kinds())
            .unwrap();
    }
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
    (view, live, cx)
}

#[gpui_kit::test]
fn reading_screen_at_scale_within_budget(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, live, cx) = setup(dir.path(), cx);
    let mut over = Over::default();
    let n = view.read_with(cx, |v, _| v.reading.rows.len());
    eprintln!("{n} rows held");
    assert!(n > rows() * 9 / 10);

    // The reader (three panes), with the list and the sources pane drawn.
    view.update_in(cx, |v, window, cx| {
        v.set_read_mode(super::stream_vm::ReadMode::Reader, window, cx);
    });
    settle(cx);
    // On the reading screen, a refresh keeps the order under the cursor.
    assert_eq!(view.read_with(cx, |v, _| v.reading.view), View::Reading);

    over.time("reading_changed (a sync tick)", 60, || {
        view.update(cx, |v, cx| v.reading_changed(cx))
    });
    over.time("unread count", 10, || live.unread_count());
    over.time("sources pane entries", 10, || {
        view.read_with(cx, |v, _| v.source_entries().len())
    });

    // Open a post: read mark, the event, the list catching up.
    let key = view.read_with(cx, |v, _| {
        v.reading.shown_rows().nth(5).map(super::vm::key).unwrap()
    });
    over.time("open a post (mark read, refresh)", 120, || {
        view.update_in(cx, |v, window, cx| v.open_reading(key, window, cx));
        settle(cx);
    });
    over.time("reading_changed with a post open", 60, || {
        view.update(cx, |v, cx| v.reading_changed(cx))
    });

    // Typing a search, a keystroke at a time.
    for q in ["a", "au", "aut", "author", "author 0", "author 07"] {
        over.time(&format!("search keystroke {q:?}"), 60, || {
            view.update_in(cx, |v, window, cx| v.set_reading_query(q, window, cx))
        });
    }
    // Narrowing as you type finds what a search from scratch finds.
    view.read_with(cx, |v, _| {
        let r = &v.reading;
        assert_eq!(r.searched.1, super::vm::filter(&r.rows, &r.query));
    });
    let found = view.read_with(cx, |v, _| v.reading.shown.len());
    assert!(found > 0 && found < n, "{found} of {n}");
    view.update_in(cx, |v, window, cx| v.set_reading_query("", window, cx));
    over.done();
}

/// The refresh's reorder as it was: a pairwise scan, copying every row.
fn pairwise(
    held: &[blyg_core::ReadingItem],
    order: &[blyg_core::ReadingItem],
) -> Vec<blyg_core::ReadingItem> {
    let key = super::vm::key;
    let mut next: Vec<_> = order
        .iter()
        .filter(|n| !held.iter().any(|o| key(o) == key(n)))
        .cloned()
        .collect();
    for old in held {
        if let Some(n) = order.iter().find(|n| key(n) == key(old)) {
            next.push(n.clone());
        }
    }
    next
}

/// The reorder at scale, old against new (the old one only in release:
/// it's quadratic).
#[test]
fn keep_order_at_scale() {
    let all = synth::reading(rows(), 5);
    let held = all.clone();
    let mut over = Over::default();
    if !cfg!(debug_assertions) {
        let fresh = all.clone();
        let t = Instant::now();
        let old = pairwise(&held, &fresh);
        eprintln!(
            "{:<44} {:>9.2} ms",
            "reorder, pairwise (before)",
            t.elapsed().as_secs_f64() * 1e3
        );
        assert_eq!(old.len(), all.len());
    }
    let fresh = all.clone();
    let got = over.time("reorder, keep_order", 20, || {
        super::keep_order(&held, fresh)
    });
    assert_eq!(got.len(), held.len());
    over.done();
}

/// `keep_order` moves rows where the pairwise scan it replaced copied
/// them; the order comes out the same, duplicates and all.
#[test]
fn keep_order_matches_the_pairwise_scan() {
    use blyg_core::ReadingItem;
    let all = synth::reading(40, 9);
    let pick = |ids: &[usize], v: u32| -> Vec<ReadingItem> {
        ids.iter()
            .map(|&i| {
                let mut r = all[i].clone();
                r.version = v;
                r
            })
            .collect()
    };
    let cases = [
        (pick(&[0, 1, 2, 3], 1), pick(&[4, 0, 2, 5, 3], 2)),
        (pick(&[3, 2, 1], 1), pick(&[1, 2, 3], 2)),
        (pick(&[], 1), pick(&[1, 2], 2)),
        (pick(&[1, 2], 1), pick(&[], 2)),
        // A key twice over on either side.
        (pick(&[1, 2, 1], 1), pick(&[2, 1, 6, 1], 2)),
    ];
    for (held, fresh) in cases {
        let want = pairwise(&held, &fresh);
        let got = super::keep_order(&held, fresh);
        assert_eq!(got, want);
    }
}
