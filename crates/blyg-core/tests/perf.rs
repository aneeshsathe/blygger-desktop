//! Store performance at scale: a reading list of 10,000 synthetic posts
//! (`blyg_core::synth`), the shape a blyg importing its archive at once
//! leaves behind. The UI thread calls these on every keystroke, open and
//! sync tick, so each has a budget.
//!
//!     cargo test -p blyg-core --release --test perf -- --nocapture
//!
//! Debug builds seed 3,000 posts and only enforce loose bounds.
//! `BLYGGER_PERF_DB=<copy of a blygger.db>` runs `report` against that copy
//! instead (never point it at a live data dir: the store writes to it).

use std::path::Path;
use std::time::{Duration, Instant};

use blyg_core::store::Store;
use blyg_core::synth;

/// 10,000 posts in a release build; debug builds (the everyday test run)
/// seed fewer, the slow part being SQLite's own indexing.
const ROWS: usize = if cfg!(debug_assertions) {
    3_000
} else {
    10_000
};
/// One reading pull: 20 pages of 100.
const PULL: usize = 2_000;

fn release() -> bool {
    !cfg!(debug_assertions)
}

/// Budget in a release build; debug builds get 20x.
fn budget(ms: u64) -> Duration {
    Duration::from_millis(if release() { ms } else { ms * 20 })
}

fn time<T>(label: &str, f: impl FnOnce() -> T) -> (T, Duration) {
    let t = Instant::now();
    let out = f();
    let e = t.elapsed();
    eprintln!("{label:<40} {:>9.2} ms", e.as_secs_f64() * 1e3);
    (out, e)
}

/// Every budget overrun, reported together at the end.
#[derive(Default)]
struct Over(Vec<String>);

impl Over {
    fn check(&mut self, t: Duration, budget: Duration, what: &str) {
        if t >= budget {
            self.0.push(format!("{what}: {t:?} (budget {budget:?})"));
        }
    }
    fn done(self) {
        assert!(self.0.is_empty(), "over budget:\n{}", self.0.join("\n"));
    }
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

#[test]
fn reading_list_at_scale_within_budget() {
    let dir = tempfile::tempdir().unwrap();
    let (store, path) = seeded(dir.path());
    let kinds = synth::kinds();
    let mut over = Over::default();

    // Warm (statement cache, first parse).
    let (all, _) = time("reading() cold", || store.reading());
    assert!(all.len() > ROWS * 9 / 10);
    let (_, t) = time("reading() warm", || store.reading());
    over.check(t, budget(60), "reading() took ");

    let first = &all[10];
    let (_, t) = time("mark_read one post", || {
        store
            .mark_read(&first.subscription_id, &first.remote_id)
            .unwrap()
    });
    over.check(t, budget(15), "mark_read took ");
    let (_, t) = time("reading() after a mark", || store.reading());
    over.check(t, budget(60), "reading() after a write took ");

    let (_, t) = time("responses_to", || {
        store.responses_to(&first.origin, &first.remote_id)
    });
    over.check(t, budget(5), "responses_to took ");

    // A pull that brings nothing new: 2,000 rows as already held.
    let pull = synth::reading(ROWS, 1)
        .into_iter()
        .take(PULL)
        .collect::<Vec<_>>();
    let wal_before = wal_size(&path);
    let (changed, t) = time("merge an unchanged pull of 2k", || {
        store.merge_reading(&pull, false, &kinds).unwrap()
    });
    let grew = wal_size(&path).saturating_sub(wal_before);
    eprintln!("{:<40} {:>9} KB", "WAL growth for it", grew / 1024);
    assert!(!changed, "nothing changed");
    over.check(t, budget(400), "unchanged merge took ");
    if grew >= 4 * 1024 * 1024 {
        over.0
            .push(format!("an unchanged pull rewrote {grew} bytes of WAL"));
    }
    let (_, t) = time("reading() after the pull", || store.reading());
    over.check(t, budget(60), "reading() after a no-op pull took ");
    over.done();
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
