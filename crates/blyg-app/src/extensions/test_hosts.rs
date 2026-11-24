//! --- test children --- In a test, every extension host a window makes
//! is stopped when the test ends, so the processes it started end with it
//! (`children_tests.rs` checks it).
//!
//! GPUI's test contexts never drop the app: `add_window_view` leaks the
//! `VisualTestContext` it returns (which holds the app), and the test
//! platform's quit is a no-op, so no `on_app_quit` handler runs. Neither
//! `Host`'s drop nor the window's quit hook ever runs in a test, and each
//! child lived until the whole test binary exited.
//!
//! The test harness runs each test on a thread of its own, and a window's
//! host is made on that thread (GPUI's test executor runs the window
//! there). So the hosts are kept in a thread-local whose destructor stops
//! them: when the test's thread ends, passed or panicked.

use std::cell::RefCell;

use blyg_ext::Host;

/// The hosts this thread's windows made; stopped when it ends.
#[derive(Default)]
struct StopAtThreadEnd(RefCell<Vec<Host>>);

impl Drop for StopAtThreadEnd {
    fn drop(&mut self) {
        // `shutdown` asks each extension to stop, then kills and reaps it.
        for host in self.0.get_mut().drain(..) {
            host.shutdown();
        }
    }
}

thread_local! {
    static HOSTS: StopAtThreadEnd = StopAtThreadEnd::default();
}

/// Stop `host` when the current test's thread ends.
pub(super) fn stop_at_test_end(host: &Host) {
    HOSTS.with(|h| h.0.borrow_mut().push(host.clone()));
}
