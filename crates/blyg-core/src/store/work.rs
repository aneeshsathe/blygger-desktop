//! Counting the store's work, for tests that guard an algorithm: a query
//! that walks every row where it should seek one shows as a count that
//! grows with the table, on any machine, where a wall-clock budget only
//! shows it on a quiet one.
//!
//! The count is SQLite's virtual-machine steps (its progress handler, on
//! every step), on both connections. Nothing counts until a test asks,
//! and nothing outside tests can ask: this module is only built for
//! blyg-core's own tests and under its `test-support` feature, which
//! crates enable only in `[dev-dependencies]` (`tests/it/crate_graph.rs`).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::Connection;

use super::Store;

/// The store's work since [`Store::count_work`] (or the last
/// [`Work::take`]): SQLite VM steps.
#[derive(Clone)]
pub struct Work(Arc<AtomicU64>);

impl Work {
    /// The steps counted since the last take, and start again from zero.
    pub fn take(&self) -> u64 {
        self.0.swap(0, Ordering::Relaxed)
    }
}

fn count_on(c: &Connection, n: &Arc<AtomicU64>) {
    let n = n.clone();
    // Can't fail on an owned connection; a test whose count stayed at
    // zero would show it.
    let _ = c.progress_handler(
        1,
        Some(move || {
            n.fetch_add(1, Ordering::Relaxed);
            false
        }),
    );
}

impl Store {
    /// Start counting the store's work (tests only, the `test-support`
    /// feature: a progress handler on every VM step slows every query).
    pub fn count_work(&self) -> Work {
        let n = Arc::new(AtomicU64::new(0));
        count_on(&self.conn(), &n);
        if self.read.is_some() {
            count_on(&self.rconn(), &n);
        }
        Work(n)
    }
}
