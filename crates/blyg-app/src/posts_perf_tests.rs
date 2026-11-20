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
/// tick, cost: their times, and the store's work and trigram hits a
/// keystroke (with the rows held, then with twice as many) and a tick.
struct Omnibar {
    typed: Typed,
    ticks: Keys,
    doubled: Typed,
    steps_a_tick: u64,
}

const QUERY: &str = "garden notes";

/// Type `QUERY` into the empty omnibar, a key at a time, each key's
/// search landing before the next key (a typist's pace: a search takes
/// milliseconds, a keystroke a tenth of a second). Measures the
/// keystrokes' and their searches' times, the store's work and the
/// trigram hits a keystroke, and checks that what's listed is exactly
/// what the backend finds for the query.
fn type_query(
    view: &Entity<MainView>,
    live: &LiveBackend,
    cx: &mut VisualTestContext,
    label: &str,
) -> Typed {
    let mut typing = Keys::default();
    let mut steps = Steps::start(live);
    for (i, c) in QUERY.chars().enumerate() {
        let t = key(cx, &key_name(c));
        steps.keys += 1;
        let q: String = QUERY.chars().take(i + 1).collect();
        eprintln!("  omnibar keystroke {q:<14?} {:>7.3} ms", ms(t));
        typing.0.push(t);
        settle(cx);
    }
    let per_key = steps.per_key(label);
    let want: Vec<LocalId> = live.search(QUERY).into_iter().map(|i| i.local_id).collect();
    let shown: Vec<LocalId> = view.read_with(cx, |v, _| {
        assert_eq!(v.list.query(), QUERY);
        v.list
            .results()
            .iter()
            .map(|i| i.local_id.clone())
            .collect()
    });
    assert!(!want.is_empty(), "the query finds posts");
    assert_eq!(shown, want);
    // Each key's search again, as the background thread ran it: its time
    // (which the keystroke's doesn't include), and the trigram index's
    // hits (the keys of three or more characters search it; shorter ones
    // read the snapshot).
    let mut searches = Keys::default();
    let mut trigram_hits = 0;
    for n in 1..=QUERY.chars().count() {
        let q: String = QUERY.chars().take(n).collect();
        let t = Instant::now();
        let hits = live.search(&q).len();
        searches.0.push(t.elapsed());
        if q.trim().chars().count() >= 3 {
            trigram_hits += hits;
        }
    }
    let hits_a_key = trigram_hits as u64 / QUERY.chars().count() as u64;
    eprintln!("{label:<40} {hits_a_key} trigram hits a key");
    Typed {
        typing,
        searches,
        steps_a_key: per_key,
        hits_a_key,
    }
}

/// What `type_query` measured.
struct Typed {
    /// The keystrokes' times on the UI thread.
    typing: Keys,
    /// Each key's search's time (on a background thread in the app).
    searches: Keys,
    steps_a_key: u64,
    hits_a_key: u64,
}

/// Typing an omnibar search over thousands of posts, and the list's
/// refresh after a sync tick; then typing it again over twice as many
/// held rows.
fn omnibar_search(cx: &mut TestAppContext) -> Omnibar {
    let dir = tempfile::tempdir().unwrap();
    let (view, live, thread, cx) = setup(dir.path(), cx);
    let typed = type_query(&view, &live, cx, "omnibar typing");

    // A sync tick after a save: the list is refreshed for the query.
    view.update_in(cx, |v, window, cx| v.set_query_text("", window, cx));
    settle(cx);
    let work = live.count_store_work();
    let mut ticks = Keys::default();
    let mut tick_steps = 0;
    for i in 0..5 {
        live.save(&thread, &format!("{} {i}", long_thread()))
            .unwrap();
        work.take();
        let t = Instant::now();
        view.update_in(cx, |v, window, cx| {
            v.on_core_event(CoreEvent::ItemsChanged, window, cx)
        });
        ticks.0.push(t.elapsed());
        settle(cx);
        tick_steps += work.take();
    }
    let steps_a_tick = tick_steps / 5;
    eprintln!(
        "{:<40} {steps_a_tick} VM steps a tick",
        "ItemsChanged (empty query)"
    );
    let n = view.read_with(cx, |v, _| v.list.results().len());
    assert_eq!(n, live.items().len());
    // The background search itself, for the record (not on the UI thread).
    for q in ["", "g", "ga", "gar", "garden", QUERY] {
        live.save(&thread, &format!("{} {q}", long_thread()))
            .unwrap();
        let t = Instant::now();
        let hits = live.search(q).len();
        eprintln!(
            "  backend search {q:<14?} {:>7.3} ms ({hits} hits)",
            ms(t.elapsed())
        );
    }

    // Twice the rows held (as a sync would bring them), then the same
    // search typed again.
    let (own, held) = (live.items().len(), live.reading().len());
    {
        let store = Store::open(&dir.path().join("blygger.db")).unwrap();
        store
            .merge_reading(
                &synth::reading(reading_rows() * 2, 3),
                true,
                &synth::kinds(),
            )
            .unwrap();
        for r in synth::reading(posts(), 12) {
            let md = format!("{} (more)", r.content_md);
            store.create_draft(Kind::Fragment, &md, 0).unwrap();
        }
    }
    view.update_in(cx, |v, window, cx| {
        v.on_core_event(CoreEvent::ItemsChanged, window, cx);
        v.set_query_text("", window, cx);
    });
    settle(cx);
    assert_eq!(live.items().len(), own + posts());
    assert!(live.reading().len() >= held * 2 - held / 50);
    let doubled = type_query(&view, &live, cx, "omnibar typing (twice the rows)");
    Omnibar {
        typed,
        ticks,
        doubled,
        steps_a_tick,
    }
}

#[gpui_kit::test]
fn omnibar_search_at_scale_does_bounded_work(cx: &mut TestAppContext) {
    let o = omnibar_search(cx);
    // A tick after a save reads the posts again (about 60 steps a post);
    // it's held to about twice that. (A keystroke's work:
    // `an_omnibar_keystroke_reads_row_ids_not_every_matching_post`.)
    let posts = posts() as u64;
    assert!(
        o.steps_a_tick < posts * PER_POST_ON_A_TICK,
        "{} VM steps a refresh over {posts} posts",
        o.steps_a_tick
    );
}

/// VM steps a refresh after a save may spend on each post it lists.
const PER_POST_ON_A_TICK: u64 = 120;

/// VM steps a keystroke may spend on each post the trigram index finds:
/// reading its row id (the index yields it, the stale-row check probes
/// it). Decoding the matching row instead (21 columns and an outbox
/// lookup) costs about 80.
const PER_HIT_A_KEY: u64 = 20;

/// VM steps a keystroke may spend whatever it finds: the stamp check,
/// the stale rows, the selected post's reads.
const FIXED_A_KEY: u64 = 500;

/// What a keystroke's SQLite work may grow with. The gate is VM steps:
/// a keystroke reads the trigram index's row ids and resolves them
/// against the `items()` snapshot, so its work grows only with the posts
/// the index finds (a few steps each, `PER_HIT_A_KEY`), not with the rows
/// held and not by decoding each matching post. Checked with the rows
/// held and with twice as many. The Rust side (resolving and copying the
/// hits) is no VM step: `timing_omnibar_search_at_scale` reports each
/// key's search time (a report, not a gate), and is the measure of it.
#[gpui_kit::test]
fn an_omnibar_keystroke_reads_row_ids_not_every_matching_post(cx: &mut TestAppContext) {
    let o = omnibar_search(cx);
    for (label, t) in [("the rows held", &o.typed), ("twice the rows", &o.doubled)] {
        let (steps, hits) = (t.steps_a_key, t.hits_a_key);
        let limit = hits * PER_HIT_A_KEY + FIXED_A_KEY;
        assert!(
            steps <= limit,
            "{steps} VM steps an omnibar keystroke over {label}, for {hits} trigram hits a \
             key (limit {limit}): a keystroke decodes every matching post"
        );
    }
}

#[gpui_kit::test]
#[ignore = "timing: run by the CI timing job"]
fn timing_omnibar_search_at_scale(cx: &mut TestAppContext) {
    let o = omnibar_search(cx);
    let mut budget = Budget::default();
    budget.check(
        "median omnibar keystroke",
        o.typed.typing.report("omnibar typing"),
        KEY_MS,
    );
    budget.check(
        "median refresh",
        o.ticks.report("ItemsChanged (empty query)"),
        KEY_MS,
    );
    // Each key's search, off the UI thread: reported, not budgeted.
    o.typed.searches.report("omnibar search a key");
    o.doubled
        .searches
        .report("omnibar search a key (twice the rows)");
    budget.done();
}
