//! How a test waits. This is the only place a test in this crate may sleep:
//! `crates/blyg-app/clippy.toml` bans `std::thread::sleep` everywhere else
//! and names this module.
//!
//! The GPUI tests mix a fake clock (`run_until_parked`, `advance_clock`) with
//! real threads (the fake backend's, off-thread store reads) and animations
//! that follow the wall clock. Fixed sleeps and counted retry loops pass on an
//! idle machine and fail under load (the notes drawer, the pacing and reading
//! perf tests, and clicks on sliding elements all did). So a test waits for
//! what it is about, with a deadline long enough for a loaded machine:
//!
//! - [`until`]: pump until a condition holds (the usual one); [`poll`] when
//!   the test pumps more than the window itself (extensions, two backends).
//! - [`still`]: an element's bounds once it has stopped moving.
//! - [`settle`] / [`settle_drawn`]: let already-started background work land.
//! - [`wall_clock`]: let real time pass when the code under test measures it
//!   (a debounce, a process that must still be waiting); never to "let things
//!   finish".

use std::time::{Duration, Instant};

use gpui_kit::{Bounds, Pixels, VisualTestContext};

/// How long [`until`] and [`still`] wait before failing.
pub const DEADLINE: Duration = Duration::from_secs(30);

/// Let real threads run between rounds of the foreground executor.
const POLL: Duration = Duration::from_millis(2);

/// About one frame: how far apart [`still`] compares bounds.
const FRAME: Duration = Duration::from_millis(16);

/// Let work already started on other threads land: twenty rounds of running
/// the foreground tasks, each followed by a moment for real threads.
pub fn settle(cx: &mut VisualTestContext) {
    for _ in 0..20 {
        cx.run_until_parked();
        pause(POLL);
    }
}

/// [`settle`], then draw the window again (for tests that read bounds).
pub fn settle_drawn(cx: &mut VisualTestContext) {
    settle(cx);
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

/// Pump until `done` holds; fail naming `what` after [`DEADLINE`].
pub fn until(
    cx: &mut VisualTestContext,
    what: &str,
    mut done: impl FnMut(&mut VisualTestContext) -> bool,
) {
    poll(what, || {
        cx.run_until_parked();
        done(cx)
    });
}

/// Call `step` (which pumps whatever the test needs, then answers whether
/// it's done) until it says so; fail naming `what` after [`DEADLINE`].
pub fn poll(what: &str, step: impl FnMut() -> bool) {
    poll_within(DEADLINE, what, step);
}

/// [`poll`] with a longer limit, for tests that wait on real servers.
pub fn poll_within(limit: Duration, what: &str, mut step: impl FnMut() -> bool) {
    let start = Instant::now();
    loop {
        if step() {
            return;
        }
        assert!(
            start.elapsed() < limit,
            "timed out after {limit:?} waiting for {what}"
        );
        pause(POLL);
    }
}

/// The bounds of the element with debug selector `selector` once it has
/// stopped moving: the same in three draws a frame apart. Fails if it never
/// appears or keeps moving past [`DEADLINE`].
pub fn still(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    rest(cx, selector, true).expect("present")
}

/// Like [`still`], for an element that may be on its way out: `None` once
/// it has stayed gone for three draws.
pub fn at_rest(cx: &mut VisualTestContext, selector: &'static str) -> Option<Bounds<Pixels>> {
    rest(cx, selector, false)
}

fn rest(
    cx: &mut VisualTestContext,
    selector: &'static str,
    present: bool,
) -> Option<Bounds<Pixels>> {
    let start = Instant::now();
    // `None` until the first draw is measured.
    let mut last: Option<Option<Bounds<Pixels>>> = None;
    let mut same = 0;
    loop {
        settle_drawn(cx);
        let now = cx.debug_bounds(selector);
        if last == Some(now) && (now.is_some() || !present) {
            same += 1;
        } else {
            same = 0;
        }
        if same == 2 {
            return now;
        }
        assert!(
            start.elapsed() < DEADLINE,
            "{selector} {} after {DEADLINE:?}",
            if now.is_none() {
                "never appeared"
            } else {
                "kept moving"
            }
        );
        last = Some(now);
        pause(FRAME);
    }
}

/// Let `d` of real time pass, for code under test that measures it.
pub fn wall_clock(d: Duration) {
    pause(d);
}

#[expect(
    clippy::disallowed_methods,
    reason = "the one sleep a test may use; see this module's docs"
)]
fn pause(d: Duration) {
    std::thread::sleep(d);
}
