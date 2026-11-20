//! The Posts screen at scale: typing into a long thread with the preview
//! up, and typing an omnibar search, over thousands of your own posts and
//! a reading list (invented content: `blyg_core::synth`), on the real
//! store through a `LiveBackend` that never syncs.
//!
//! What gates a merge is the store's work per keystroke, in SQLite VM
//! steps (`LiveBackend::count_store_work`): a keystroke that reads every
//! post shows on any machine. The UI thread's time per keystroke, against
//! a budget in frames, is a report (`timing_…`, run by CI's timing job):
//!
//!     cargo test -p blyg-app --release posts_perf_tests -- --ignored --nocapture
//!
//! Debug builds hold fewer posts.

use std::sync::Arc;
use std::time::{Duration, Instant};

use blyg_core::config::MemoryTokenStore;
use blyg_core::store::{Store, Work};
use blyg_core::wait::Budget;
use blyg_core::{Backend, ConfigStore, CoreEvent, Kind, LiveBackend, LocalId, SyncOptions, synth};
use gpui_kit::{Entity, Keystroke, TestAppContext, VisualTestContext};

use super::MainView;
use super::studio::ViewMode;
use crate::prefs::Prefs;

const CONNECTED: &str = "# test config\nblyg-url = https://blyg.example.com\n";

/// One frame at 60 Hz.
const FRAME_MS: f64 = 1000.0 / 60.0;

/// The UI thread's budget for one keystroke in a release build: a quarter
/// of a frame (the rest is the draw).
const KEY_MS: f64 = FRAME_MS / 4.0;

fn posts() -> usize {
    if cfg!(debug_assertions) { 500 } else { 2_000 }
}

fn reading_rows() -> usize {
    if cfg!(debug_assertions) { 1_000 } else { 4_000 }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

/// Per-keystroke wall-clock times. The worst one is what you feel, and
/// it's printed, but the budget is checked against the median: on a busy
/// machine the worst of two hundred keystrokes measures the scheduler.
/// The work this guards against (a store transaction, a copy of every
/// post) is in every keystroke, so it moves the median: it was 50 ms here
/// before.
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

/// The store's work over a run of keystrokes, per keystroke.
struct Steps {
    work: Work,
    keys: u64,
}

impl Steps {
    fn start(live: &LiveBackend) -> Steps {
        Steps {
            work: live.count_store_work(),
            keys: 0,
        }
    }

    /// The VM steps per keystroke since the start (or the last `per_key`).
    fn per_key(&mut self, label: &str) -> u64 {
        let per = self.work.take() / self.keys.max(1);
        eprintln!("{label:<40} {} keys · {per} VM steps a key", self.keys);
        self.keys = 0;
        per
    }
}

use crate::test_wait::settle;

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

fn key_name(c: char) -> String {
    if c == ' ' {
        "space".to_string()
    } else {
        c.to_string()
    }
}

/// Typing into a 20k-character thread with the preview up: every
/// keystroke saves, re-titles the list row and schedules a render. Returns
/// the keystrokes' times, and the store's work a keystroke.
fn type_into_long_thread(cx: &mut TestAppContext) -> (Keys, u64) {
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
    let mut steps = Steps::start(&live);
    let typed = "the quiet harbour keeps its lanterns lit ";
    for round in 0..5 {
        for c in typed.chars() {
            keys.0.push(key(cx, &key_name(c)));
            steps.keys += 1;
        }
        // Background work (the writer, a render) runs between bursts, as
        // it would while you type.
        if round % 2 == 1 {
            settle(cx);
        }
    }
    // Every keystroke landed, in order, and the list row shows it.
    settle(cx);
    let per_key = steps.per_key("typing (20k thread, preview up)");
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
    (keys, per_key)
}

#[gpui_kit::test]
fn typing_at_scale_does_bounded_work(cx: &mut TestAppContext) {
    let (_, per_key) = type_into_long_thread(cx);
    // A keystroke writes one row (about 270 steps at 500 posts); reading
    // every post costs several steps a post.
    let limit = posts() as u64 * 2;
    assert!(
        per_key < limit,
        "{per_key} VM steps a keystroke over {} posts (limit {limit})",
        posts()
    );
}

#[gpui_kit::test]
#[ignore = "timing: run by the CI timing job"]
fn timing_typing_at_scale(cx: &mut TestAppContext) {
    let (keys, _) = type_into_long_thread(cx);
    let median = keys.report("typing (20k thread, preview up)");
    let mut budget = Budget::default();
    // KEY_MS (a quarter frame) is the aim; CI's macOS runner's median
    // reached 4.7 ms, so the budget is 1.5x that (`Budget` says how).
    eprintln!(
        "median keystroke {:.3} ms · aim {KEY_MS:.2} ms{} · budget 7.5 ms",
        ms(median),
        if ms(median) > KEY_MS { " (missed)" } else { "" }
    );
    budget.check("median keystroke", median, 7.5);
    budget.done();
}

/// What typing an omnibar search, and the list's refresh after a sync
/// tick, cost: their times, and the store's work a keystroke and a tick.
struct Omnibar {
    typing: Keys,
    ticks: Keys,
    steps_a_key: u64,
    steps_a_tick: u64,
}

/// Typing an omnibar search over thousands of posts, and the list's
/// refresh after a sync tick.
fn omnibar_search(cx: &mut TestAppContext) -> Omnibar {
    let dir = tempfile::tempdir().unwrap();
    let (view, live, thread, cx) = setup(dir.path(), cx);
    let mut typing = Keys::default();
    let mut steps = Steps::start(&live);
    for (i, c) in "garden notes".chars().enumerate() {
        let t = key(cx, &key_name(c));
        steps.keys += 1;
        let q: String = "garden notes".chars().take(i + 1).collect();
        eprintln!("  omnibar keystroke {q:<14?} {:>7.3} ms", ms(t));
        typing.0.push(t);
    }
    settle(cx);
    let steps_a_key = steps.per_key("omnibar typing");
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
    let mut tick_steps = 0;
    for i in 0..5 {
        live.save(&thread, &format!("{} {i}", long_thread()))
            .unwrap();
        steps.work.take();
        let t = Instant::now();
        view.update_in(cx, |v, window, cx| {
            v.on_core_event(CoreEvent::ItemsChanged, window, cx)
        });
        ticks.0.push(t.elapsed());
        settle(cx);
        tick_steps += steps.work.take();
    }
    let steps_a_tick = tick_steps / 5;
    eprintln!(
        "{:<40} {steps_a_tick} VM steps a tick",
        "ItemsChanged (empty query)"
    );
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
    Omnibar {
        typing,
        ticks,
        steps_a_key,
        steps_a_tick,
    }
}

#[gpui_kit::test]
fn omnibar_search_at_scale_does_bounded_work(cx: &mut TestAppContext) {
    let o = omnibar_search(cx);
    // Both read what's held (a keystroke about 17 steps a held row today; a
    // tick after a save, the posts again, about 60 a post): each is held
    // to about twice that, so a step that starts reading more goes over.
    let posts = posts() as u64;
    let held = posts + reading_rows() as u64;
    assert!(
        o.steps_a_key < held * PER_HELD_ROW_A_KEY,
        "{} VM steps an omnibar keystroke over {held} held rows",
        o.steps_a_key
    );
    assert!(
        o.steps_a_tick < posts * PER_POST_ON_A_TICK,
        "{} VM steps a refresh over {posts} posts",
        o.steps_a_tick
    );
}

/// VM steps a refresh after a save may spend on each post it lists.
const PER_POST_ON_A_TICK: u64 = 120;

/// VM steps an omnibar keystroke may spend on each held row.
const PER_HELD_ROW_A_KEY: u64 = 35;

#[gpui_kit::test]
#[ignore = "timing: run by the CI timing job"]
fn timing_omnibar_search_at_scale(cx: &mut TestAppContext) {
    let o = omnibar_search(cx);
    let mut budget = Budget::default();
    budget.check(
        "median omnibar keystroke",
        o.typing.report("omnibar typing"),
        KEY_MS,
    );
    budget.check(
        "median refresh",
        o.ticks.report("ItemsChanged (empty query)"),
        KEY_MS,
    );
    budget.done();
}
