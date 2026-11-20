//! How blyg-core, blyg-ai and the extension crates wait. This is the only
//! place they may call `std::thread::sleep`: each crate's `clippy.toml`
//! bans it everywhere else and names this module.
//!
//! A fixed sleep that "lets something finish" passes on an idle machine and
//! fails under load, so code waits for what it's about:
//!
//! - **Production** waits are deliberate pauses: [`pace`] (pacing a busy
//!   loop for others, a poll step of something that can't notify: a child
//!   process, a non-blocking socket), [`nap`] (a pause the user can cancel:
//!   backoff, a device-flow interval), [`poll_until`] (a bounded wait for a
//!   condition, false when it times out).
//! - **Tests** wait for a condition with [`until`] (a deadline long enough
//!   for a loaded machine), and let real time pass with [`wall_clock`] only
//!   when the code under test measures it (second-resolution stamps, a
//!   debounce, a timeout that must fire).
//! - **Timing reports** hold a wall-clock [`Budget`], and never gate a
//!   merge.

use std::time::{Duration, Instant};

/// How long [`until`] waits before failing.
pub const DEADLINE: Duration = Duration::from_secs(30);

/// How often [`until`] and [`poll_until`] look again, unless told.
const STEP: Duration = Duration::from_millis(5);

/// A deliberate pause in production code: pacing, or one step of polling
/// something that can't notify. Never to let other work finish.
pub fn pace(d: Duration) {
    sleep(d);
}

/// Pause `d`, looking at `cancelled` at least every 50 ms. False when it
/// was cancelled.
pub fn nap(d: Duration, mut cancelled: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + d;
    loop {
        if cancelled() {
            return false;
        }
        let now = Instant::now();
        if now >= end {
            return true;
        }
        sleep((end - now).min(Duration::from_millis(50)));
    }
}

/// Look at `done` every `step` until it holds (true) or `limit` passes
/// (false).
pub fn poll_until(limit: Duration, step: Duration, mut done: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + limit;
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= end {
            return false;
        }
        sleep(step);
    }
}

/// Tests: wait until `done` holds; fail naming `what` after [`DEADLINE`].
pub fn until(what: &str, done: impl FnMut() -> bool) {
    until_within(DEADLINE, what, done);
}

/// [`until`] with another limit.
pub fn until_within(limit: Duration, what: &str, done: impl FnMut() -> bool) {
    assert!(
        poll_until(limit, STEP, done),
        "timed out after {limit:?} waiting for {what}"
    );
}

/// Tests: let `d` of real time pass, for code under test that measures it.
/// Never to let other work finish: wait for it with [`until`].
pub fn wall_clock(d: Duration) {
    sleep(d);
}

/// Timing reports: the wall-clock budgets left once a test guarding an
/// algorithm counts work instead (queries, VM steps, renders, calls).
///
/// A budget measures the machine as much as the code: every one that gated
/// a merge failed on a slow, loaded or Windows runner sooner or later. So a
/// test that uses one is `#[ignore = "timing: run by the CI timing job"]`
/// and named `timing_…`: CI's `timing` job runs them (`scripts/check.sh
/// timing`) and reports without blocking. This is the one place a budget is
/// scaled; `blyg-core/tests/it/timing_scan.rs` refuses any other.
///
/// ```ignore
/// let mut budget = Budget::default();
/// let rows = budget.time("reading()", 60.0, || store.reading());
/// budget.check("median keystroke", median, 4.0);
/// budget.done();
/// ```
#[derive(Default)]
#[must_use = "call done() to fail on an overrun"]
pub struct Budget {
    over: Vec<String>,
}

impl Budget {
    /// `ms`, what a release build takes on a Mac, scaled once: a debug build
    /// gets 15x, Windows 4x on top (its runners are several times slower at
    /// anything that touches the disk).
    pub fn limit(ms: f64) -> Duration {
        let debug = if cfg!(debug_assertions) { 15.0 } else { 1.0 };
        let os = if cfg!(windows) { 4.0 } else { 1.0 };
        Duration::from_secs_f64(ms * debug * os / 1e3)
    }

    /// Time `f`, print it, and note it if it took longer than `ms` (scaled).
    pub fn time<T>(&mut self, what: &str, ms: f64, f: impl FnOnce() -> T) -> T {
        let t = Instant::now();
        let out = f();
        let took = t.elapsed();
        eprintln!("{what:<44} {:>9.2} ms", took.as_secs_f64() * 1e3);
        self.check(what, took, ms);
        out
    }

    /// Note `took` (measured by the test: a median, say) if it's longer
    /// than `ms` (scaled).
    pub fn check(&mut self, what: &str, took: Duration, ms: f64) {
        let limit = Self::limit(ms);
        if took >= limit {
            self.over
                .push(format!("{what}: {took:?} (budget {limit:?})"));
        }
    }

    /// Fail, listing every overrun, if there was one.
    pub fn done(self) {
        assert!(
            self.over.is_empty(),
            "over budget:\n{}",
            self.over.join("\n")
        );
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "the one sleep these crates may use; see this module's docs"
)]
fn sleep(d: Duration) {
    std::thread::sleep(d);
}
