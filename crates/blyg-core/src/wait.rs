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

#[expect(
    clippy::disallowed_methods,
    reason = "the one sleep these crates may use; see this module's docs"
)]
fn sleep(d: Duration) {
    std::thread::sleep(d);
}
