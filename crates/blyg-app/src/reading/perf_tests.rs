//! The reading screens at scale: thousands of posts (`blyg_core::synth`,
//! invented content) on the real store, through a `LiveBackend` that
//! never syncs (no worker, a base URL nothing answers on). Each step is
//! something the UI thread does on a keystroke, an open or a sync tick.
//!
//! What gates a merge is the work: refreshes, and the store's VM steps
//! (`LiveBackend::count_store_work`), which stay under a share of the rows
//! held however many there are. The same steps against the clock are a
//! report (`timing_…`, run by CI's timing job):
//!
//!     cargo test -p blyg-app --release reading::perf_tests -- --ignored --nocapture
//!
//! Debug builds hold fewer posts.

use std::sync::Arc;
use std::time::Instant;

use blyg_core::config::MemoryTokenStore;
use blyg_core::store::{Store, Work};
use blyg_core::wait::Budget;
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

/// How a step is measured: the store's work (the gate), or the clock (a
/// report).
enum Meter {
    Work { work: Work, over: Vec<String> },
    Clock(Budget),
}

/// What a step may cost: its UI-thread budget in a release build (ms),
/// and the store's VM steps per row held (all it starts, settled). The
/// step counts are what this data and these queries take, the same on
/// every machine, with room to spare; a step that starts reading more
/// than it did goes over.
struct Cost {
    ms: f64,
    steps_a_row: f64,
}

const fn cost(ms: f64, steps_a_row: f64) -> Cost {
    Cost { ms, steps_a_row }
}

impl Meter {
    /// Run `f`, a step, and measure it.
    fn step<T>(
        &mut self,
        cx: &mut VisualTestContext,
        what: &str,
        cost: Cost,
        f: impl FnOnce(&mut VisualTestContext) -> T,
    ) -> T {
        match self {
            Meter::Work { work, over } => {
                work.take();
                let out = f(cx);
                settle(cx);
                let n = work.take();
                let limit = (rows() as f64 * cost.steps_a_row) as u64;
                eprintln!("{what:<44} {n:>9} VM steps (limit {limit})");
                if n >= limit {
                    over.push(format!("{what}: {n} VM steps (limit {limit})"));
                }
                out
            }
            Meter::Clock(budget) => budget.time(what, cost.ms, || f(cx)),
        }
    }

    fn done(self) {
        match self {
            Meter::Work { over, .. } => assert!(
                over.is_empty(),
                "{} rows held; work grew with the list:\n{}",
                rows(),
                over.join("\n")
            ),
            Meter::Clock(budget) => budget.done(),
        }
    }
}

use crate::test_wait::{poll, settle};

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
            .take_served_subscriptions(&synth::subscriptions(), blyg_core::api::Prune::Allowed)
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

/// The reader at scale, a step at a time, each measured by `meter`.
fn reading_screen_steps(cx: &mut TestAppContext, meter: impl FnOnce(&LiveBackend) -> Meter) {
    let dir = tempfile::tempdir().unwrap();
    let (view, live, cx) = setup(dir.path(), cx);
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

    // The budgets (ms) are from CI's macOS runner, over four runs
    // (`blyg_core::wait::Budget` says how). A refresh of the whole list
    // varied about 2x there: 90-156 ms a tick, 102-215 a burst, 81-218
    // with a post open.
    let mut meter = meter(&live);
    meter.step(
        cx,
        "reading_changed (a sync tick)",
        cost(240.0, 4.0),
        |cx| view.update(cx, |v, cx| v.reading_changed(cx)),
    );
    // A sync burst: the queued events are one batch, and one refresh.
    let before = view.read_with(cx, |v, _| v.reading.refreshes);
    meter.step(
        cx,
        "a burst of 20 ReadingChanged + ItemsChanged",
        cost(330.0, 12.0),
        |cx| {
            view.update_in(cx, |v, window, cx| {
                let mut burst = vec![blyg_core::CoreEvent::ReadingChanged; 20];
                burst.push(blyg_core::CoreEvent::ItemsChanged);
                v.on_core_events(burst, window, cx)
            })
        },
    );
    let after = view.read_with(cx, |v, _| v.reading.refreshes);
    assert_eq!(after - before, 1, "one refresh for the burst");
    // Let the burst's follow-up work (ItemsChanged's requery) finish, so it
    // isn't measured as part of opening a post below.
    settle(cx);
    meter.step(cx, "unread count", cost(10.0, 0.25), |_| {
        live.unread_count()
    });
    // CI: 5.6, 7.1, 9.7, 10.3 ms; 1.5x 10.3 is 15.4, up to 20.
    meter.step(cx, "sources pane entries", cost(20.0, 0.25), |cx| {
        view.read_with(cx, |v, _| v.source_entries().len())
    });

    // Open a post: read mark, the event, the list catching up. An unread
    // one, so the read mark is stored and the list refreshes.
    let key = view.read_with(cx, |v, _| {
        v.reading
            .shown_rows()
            .filter(|r| r.read_version.is_none())
            .nth(5)
            .map(super::vm::key)
            .unwrap()
    });
    let before = view.read_with(cx, |v, _| v.reading.refreshes);
    meter.step(
        cx,
        "open a post (mark read)",
        // CI's macOS runner: 77.7 ms (PR #98, the first run with the read
        // mark in place: 1 poll round, all of it outside the pauses), so
        // 1.5x is 116.6, rounded up to 120.
        cost(120.0, 4.0),
        |cx| {
            let t = Instant::now();
            view.update_in(cx, |v, window, cx| v.open_reading(key.clone(), window, cx));
            let mut awake = t.elapsed();
            // Until the post's versions are in and its row shows the read
            // mark (in place: no list refresh); not `settle`, whose twenty fixed pauses
            // (60 ms here, several hundred on a CI runner) would be timed.
            // Polling still pauses 2 ms (plus the timer's overshoot) after
            // each round that finds it unfinished, so the rounds and the
            // time this thread spent outside those pauses are printed too:
            // they tell work from sleep. (The read mark is written on
            // another thread, partly during the pauses.)
            let mut rounds = 0;
            poll("the opened post's read mark and versions", || {
                let t = Instant::now();
                rounds += 1;
                cx.run_until_parked();
                let done = view.read_with(cx, |v, _| {
                    let r = &v.reading;
                    let read = r
                        .rows
                        .iter()
                        .any(|row| super::vm::key(row) == key && row.read_version.is_some());
                    let loaded = r
                        .opened
                        .as_ref()
                        .is_some_and(|o| !matches!(o.changelog, super::Load::Loading));
                    read && loaded
                });
                awake += t.elapsed();
                done
            });
            eprintln!(
                "  {rounds} poll rounds; {:.2} ms on this thread outside the pauses",
                awake.as_secs_f64() * 1e3
            );
            assert_eq!(
                view.read_with(cx, |v, _| v.reading.refreshes),
                before,
                "opening a post refreshed the whole list"
            );
        },
    );
    meter.step(
        cx,
        "reading_changed with a post open",
        cost(330.0, 4.0),
        |cx| view.update(cx, |v, cx| v.reading_changed(cx)),
    );

    // Typing a search, a keystroke at a time: the list in memory, filtered.
    // CI: 28-49 ms for the short queries, 54-64 for "author 0", 45-81 for
    // "author 07"; 1.5x 80.7 is 121, up to 130.
    for q in ["a", "au", "aut", "author", "author 0", "author 07"] {
        let what = format!("search keystroke {q:?}");
        meter.step(cx, &what, cost(130.0, 0.25), |cx| {
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

    // --- studio 0.71 --- "Count an entry as read: when I see it" in the
    // Stream. A tick measures only the cards from the first drawn down to
    // the viewport's bottom, and the batch it sends costs the posts seen,
    // not the list.
    view.update_in(cx, |v, window, cx| {
        v.set_read_mode(super::stream_vm::ReadMode::Stream, window, cx);
        v.reading.count_read.setting = Some(blyg_core::mark_read::MarkRead::Seen);
    });
    crate::test_wait::settle_drawn(cx);
    let unread = live.unread_count();
    let t0 = Instant::now();
    meter.step(
        cx,
        "stream ticks: seen marks sent in a batch",
        cost(30.0, 0.25),
        |cx| {
            view.update(cx, |v, cx| {
                v.stream_tick(t0, true, cx);
                v.stream_tick(t0 + std::time::Duration::from_millis(1100), true, cx);
                v.stream_tick(t0 + std::time::Duration::from_millis(4200), true, cx);
            })
        },
    );
    poll("the seen marks", || {
        cx.run_until_parked();
        live.unread_count() < unread
    });
    meter.done();
}

#[gpui_kit::test]
fn reading_screen_at_scale_does_bounded_work(cx: &mut TestAppContext) {
    reading_screen_steps(cx, |live| Meter::Work {
        work: live.count_store_work(),
        over: vec![],
    });
}

#[gpui_kit::test]
#[ignore = "timing: run by the CI timing job"]
fn timing_reading_screen_at_scale(cx: &mut TestAppContext) {
    reading_screen_steps(cx, |_| Meter::Clock(Budget::default()));
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
#[ignore = "timing: run by the CI timing job"]
fn timing_keep_order_at_scale() {
    let all = synth::reading(rows(), 5);
    let held = all.clone();
    let mut budget = Budget::default();
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
    // CI's macOS runner: 24 and 49 ms (about 2x apart) in the runs that
    // went over the old 20; under 20 in the others.
    let got = budget.time("reorder, keep_order", 75.0, || {
        super::keep_order(&held, fresh)
    });
    assert_eq!(got.len(), held.len());
    budget.done();
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
