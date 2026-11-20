//! Store performance at scale: a reading list of 10,000 synthetic posts
//! (`blyg_core::synth`), the shape a blyg importing its archive at once
//! leaves behind. The UI thread calls these on every keystroke, open and
//! sync tick.
//!
//! What gates a merge is the store's work, counted in SQLite VM steps
//! (`Store::count_work`): a point read or write that starts walking the
//! table shows on any machine. The wall-clock budgets are a report
//! (`timing_…`, run by CI's timing job):
//!
//!     cargo test -p blyg-core --release --test perf -- --ignored --nocapture
//!
//! The renderer's timing reports are here too (`timing_render_…`).
//!
//! Debug builds seed 3,000 posts.
//! `BLYGGER_PERF_DB=<copy of a blygger.db>` runs `report` against that copy
//! instead (never point it at a live data dir: the store writes to it).

use std::path::Path;
use std::time::{Duration, Instant};

use blyg_core::store::Store;
use blyg_core::synth;
use blyg_core::wait::Budget;

/// 10,000 posts in a release build; debug builds (the everyday test run)
/// seed fewer, the slow part being SQLite's own indexing.
const ROWS: usize = if cfg!(debug_assertions) {
    3_000
} else {
    10_000
};
/// One reading pull: 20 pages of 100.
const PULL: usize = 2_000;

fn time<T>(label: &str, f: impl FnOnce() -> T) -> (T, Duration) {
    let t = Instant::now();
    let out = f();
    let e = t.elapsed();
    eprintln!("{label:<40} {:>9.2} ms", e.as_secs_f64() * 1e3);
    (out, e)
}

fn wal_size(db: &Path) -> u64 {
    let mut p = db.as_os_str().to_owned();
    p.push("-wal");
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

fn seeded(dir: &Path) -> (Store, std::path::PathBuf) {
    let path = dir.join("blygger.db");
    let store = Store::open(&path).unwrap();
    store
        .replace_subscriptions(&synth::subscriptions())
        .unwrap();
    let rows = synth::reading(ROWS, 1);
    let kinds = synth::kinds();
    time("seed the rows", || {
        store.merge_reading(&rows, true, &kinds).unwrap()
    });
    (store, path)
}

/// A pull that brings nothing new: the first 2,000 rows as already held.
fn unchanged_pull() -> Vec<blyg_core::ReadingItem> {
    synth::reading(ROWS, 1).into_iter().take(PULL).collect()
}

/// VM steps an unchanged pulled row may cost (seeking it, comparing it).
const PER_PULLED_ROW: u64 = 200;

/// What each step costs the store, in VM steps: a point read or write
/// stays far under one step per held row (a walk of the table costs
/// several per row), and a pull costs what it pulls, not what's held.
#[test]
fn reading_list_at_scale_does_bounded_work() {
    let dir = tempfile::tempdir().unwrap();
    let (store, path) = seeded(dir.path());
    let kinds = synth::kinds();
    let all = store.reading();
    assert!(all.len() > ROWS * 9 / 10);
    let work = store.count_work();
    let mut counts: Vec<(&str, u64, u64)> = vec![];
    let held = ROWS as u64;
    let mut step = |what: &'static str, limit: u64, f: &mut dyn FnMut()| {
        f();
        let n = work.take();
        eprintln!("{what:<40} {n:>9} VM steps (limit {limit})");
        counts.push((what, n, limit));
    };

    step("reading() warm", held / 4, &mut || {
        store.reading();
    });
    let first = &all[10];
    step("mark_read one post", held / 4, &mut || {
        store
            .mark_read(&first.subscription_id, &first.remote_id)
            .unwrap();
    });
    step("reading() after a mark", held / 4, &mut || {
        store.reading();
    });
    step("responses_to", held / 4, &mut || {
        store.responses_to(&first.origin, &first.remote_id);
    });
    let pull = unchanged_pull();
    let wal_before = wal_size(&path);
    let mut changed = true;
    step(
        "merge an unchanged pull of 2k",
        PULL as u64 * PER_PULLED_ROW,
        &mut || {
            changed = store.merge_reading(&pull, false, &kinds).unwrap();
        },
    );
    assert!(!changed, "nothing changed");
    step("reading() after the pull", held / 4, &mut || {
        store.reading();
    });
    let grew = wal_size(&path).saturating_sub(wal_before);
    assert!(
        grew < 4 * 1024 * 1024,
        "an unchanged pull rewrote {grew} bytes of WAL"
    );

    let over: Vec<String> = counts
        .iter()
        .filter(|(_, n, limit)| n >= limit)
        .map(|(what, n, limit)| format!("{what}: {n} VM steps (limit {limit})"))
        .collect();
    assert!(
        over.is_empty(),
        "{held} rows held; work grew with the table:\n{}",
        over.join("\n")
    );
}

/// The same steps against the clock: a report, not a gate.
#[test]
#[ignore = "timing: run by the CI timing job"]
fn timing_reading_list_at_scale() {
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = seeded(dir.path());
    let kinds = synth::kinds();
    let mut budget = Budget::default();

    // Warm (statement cache, first parse).
    // Budgets from CI's macOS runner, three runs (`Budget` says how): warm
    // 75-126 ms, after a mark 20-23, after the pull 16-28, the merge
    // 195-273. mark_read (3-8, varying 3x) and responses_to (under 1) keep
    // theirs: 1.5x is under the 5 ms floor or at what they had.
    let (all, _) = time("reading() cold", || store.reading());
    budget.time("reading() warm", 190.0, || store.reading());
    let first = &all[10];
    budget.time("mark_read one post", 15.0, || {
        store
            .mark_read(&first.subscription_id, &first.remote_id)
            .unwrap()
    });
    budget.time("reading() after a mark", 40.0, || store.reading());
    budget.time("responses_to", 5.0, || {
        store.responses_to(&first.origin, &first.remote_id)
    });
    let pull = unchanged_pull();
    budget.time("merge an unchanged pull of 2k", 410.0, || {
        store.merge_reading(&pull, false, &kinds).unwrap()
    });
    budget.time("reading() after the pull", 45.0, || store.reading());
    budget.done();
}

/// A realistic mixed document of at least `target` characters: quotes
/// (unresolved here), TK, links, fuzzy links, lists, a table, code, videos.
fn document(target: usize) -> String {
    let section = "## A section about small tools\n\n\
Small tools, used daily, beat big tools used rarely. See https://blyg.example.com/notes/tools \
or www.example.org/guide for more, and write to someone@example.com if you disagree. \
Some *emphasis*, some **strong text**, a `code span`, and a [titled link](https://example.org/a \"A\").\n\n\
- first item with example.com in it\n- second item with a [link](/local)\n  - nested item\n\n\
> A plain quote that runs\n> over two lines.\n\n\
The weather was [TK]describe the weather[=]*grey and still*[/TK] that morning.\n\n\
[TK]write a bridging paragraph[=]Which brings us to the point: the tool should disappear.[/TK]\n\n\
![a picture](https://images.example.org/p.jpg)\n\n\
![[01j9zq3k4m5n6p7q8r9s0t1v2w]]\n\n\
```rust\nfn main() { println!(\"hello\"); }\n```\n\n\
| a | b |\n|---|---|\n| 1 | 2 |\n\n\
https://www.youtube.com/watch?v=Zx9_abc-123\n\n\
![[03zzzzzzzzzzzzzzzzzzzzzzzz]]\n\n\
A closing paragraph with a trailing link https://example.org/end.\n\n";
    let mut doc = String::from("# Notes\n\n");
    while doc.chars().count() < target {
        doc.push_str(section);
    }
    doc
}

/// blyg-render's preview of a 20,000-character thread, against a 30 ms
/// budget (the studio re-renders ~100 ms after typing pauses): its median
/// on CI's macOS runner was 12-17 ms (`Budget` says how). Here, not
/// in blyg-render, because the one budget helper is blyg-core's and
/// blyg-render is a leaf crate.
#[test]
#[ignore = "timing: run by the CI timing job"]
fn timing_render_20k_chars() {
    use blyg_render::{Kind, NoResolver, RenderOpts, render_preview};
    let doc = document(20_000);
    let opts = RenderOpts::default();
    // Warm up (regex compilation, lazy statics).
    let first = render_preview(&doc, Kind::Thread, &NoResolver, &opts);
    let runs = 20;
    let mut times: Vec<Duration> = (0..runs)
        .map(|_| {
            let t = Instant::now();
            let out = render_preview(&doc, Kind::Thread, &NoResolver, &opts);
            let e = t.elapsed();
            assert_eq!(out.html.len(), first.html.len());
            e
        })
        .collect();
    times.sort();
    let median = times[runs / 2];
    eprintln!(
        "render_preview: {} chars, median {median:?}, worst {:?}",
        doc.chars().count(),
        times[runs - 1]
    );
    let mut budget = Budget::default();
    budget.check("render_preview of 20k chars (median)", median, 30.0);
    budget.done();
}

/// Inputs aimed at blyg-render's linkify regexes' backtracking, and its
/// formula finder (lone dollars that close nothing). None may be
/// pathological.
#[test]
#[ignore = "timing: run by the CI timing job"]
fn timing_render_adversarial_inputs() {
    use blyg_render::{Kind, NoResolver, RenderOpts, render_markdown, render_preview};
    let cases: Vec<(&str, String)> = vec![
        ("dotted words", "foo.bar.baz.qux ".repeat(1300)),
        ("one long label", format!("{}.com", "a".repeat(20_000))),
        ("hyphen run", format!("x{}y.org", "-".repeat(20_000))),
        ("many schemes", "http:// ".repeat(2500)),
        ("many ats", "a@b@c@d@ ".repeat(2200)),
        (
            "path punctuation",
            format!("https://example.org/{}", "a.,;!?".repeat(3300)),
        ),
        (
            "unclosed parens",
            format!("https://example.org/{}", "(".repeat(20_000)),
        ),
        ("www chain", "www.".repeat(5000)),
        ("emphasis soup", "*_".repeat(10_000)),
        ("brackets", "[".repeat(20_000)),
        ("tk tokens", "[TK]a[=]b[/TK] ".repeat(1400)),
        ("comments", "/* note */ word\n".repeat(1300)),
        ("stray closers", "*/ ".repeat(2000)),
        ("attached openers", "a/*b ".repeat(4000)),
    ];
    let mut budget = Budget::default();
    for (name, doc) in &cases {
        let out = budget.time(name, 250.0, || {
            render_preview(doc, Kind::Thread, &NoResolver, &RenderOpts::default())
        });
        assert!(!out.html.is_empty());
    }
    let dollars = "$a ".repeat(200_000);
    let html = budget.time("200k lone dollars", 350.0, || render_markdown(&dollars));
    assert!(!html.contains("<math"));
    budget.done();
}

/// The WAL is truncated back after a big write instead of staying at its
/// high-water mark (the 2k-post import left one of 240 MB).
#[test]
fn wal_does_not_keep_its_high_water_mark() {
    let dir = tempfile::tempdir().unwrap();
    let (store, path) = seeded(dir.path());
    drop(store);
    // Reopening checkpoints; the next big write must not leave the WAL
    // at the size of the seed.
    let store = Store::open(&path).unwrap();
    let pull: Vec<_> = synth::reading(ROWS, 2).into_iter().take(PULL).collect();
    store.merge_reading(&pull, false, &synth::kinds()).unwrap();
    let big = wal_size(&path);
    // The first commit after the automatic checkpoint starts the WAL over.
    let r = store.reading();
    store
        .set_thumb(&r[0].subscription_id, &r[0].remote_id, Some(1))
        .unwrap();
    let wal = wal_size(&path);
    eprintln!(
        "WAL after a 2k-row rewrite: {} KB, after one more write: {} KB",
        big / 1024,
        wal / 1024
    );
    assert!(wal <= 16 * 1024 * 1024, "WAL is {wal} bytes");
}

/// Timings against a copy of a real database (never a live data dir).
#[test]
#[ignore]
fn report() {
    let Some(src) = std::env::var_os("BLYGGER_PERF_DB") else {
        eprintln!("set BLYGGER_PERF_DB to a copy of a blygger.db");
        return;
    };
    let (store, _) = time("open (migrating, if older)", || {
        Store::open(Path::new(&src)).unwrap()
    });
    let kinds = store
        .subscriptions()
        .into_iter()
        .map(|s| (s.id, s.kind))
        .collect();
    let (all, _) = time("reading() cold", || store.reading());
    eprintln!("{} rows shown", all.len());
    time("reading() warm", || store.reading());
    time("background_reading()", || store.background_reading());
    let first = &all[all.len() / 2];
    time("responses_to", || {
        store.responses_to(&first.origin, &first.remote_id)
    });
    time("mark_read one post", || {
        store
            .mark_read(&first.subscription_id, &first.remote_id)
            .unwrap()
    });
    time("reading() after a mark", || store.reading());
    let pull: Vec<_> = all.iter().take(PULL).cloned().collect();
    let wal = wal_size(Path::new(&src));
    let (changed, _) = time("merge an unchanged pull of 2k", || {
        store.merge_reading(&pull, false, &kinds).unwrap()
    });
    eprintln!(
        "changed: {changed}; WAL grew {} KB",
        wal_size(Path::new(&src)).saturating_sub(wal) / 1024
    );
    time("reading() after the pull", || store.reading());
    let (changed, _) = time("merge the same pull again", || {
        store.merge_reading(&pull, false, &kinds).unwrap()
    });
    eprintln!("changed: {changed}");
    time("sync_reading_hoppers (every pull)", || {
        store.sync_reading_hoppers().unwrap()
    });
}
