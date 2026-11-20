//! Upkeep of the trigram text indexes (`items_fts`, `reading_fts`).
//!
//! An FTS5 index is a stack of segments. Every write adds a small one, and
//! FTS5 merges them as it goes (`automerge`), but an edit or a delete only
//! adds a *delete marker*: the old text's trigrams stay in whichever older,
//! bigger segment holds them until a merge happens to reach that segment,
//! which for the biggest ones may be never. A reading list that has been
//! re-pulled and pruned for months ends up with an index several times the
//! size of its text (one copy was 182 MB for 17.5 MB of text; rebuilt, 43 MB).
//!
//! **Compaction** ([`Store::fts_upkeep`], on the typing writer's idle tick,
//! `crate::live`, never on the UI thread) keeps it in check: now and then
//! each index's size is weighed against its text's, and when it has grown
//! past [`BLOAT`] times what it was after the last compaction (or
//! [`FIRST_BLOAT`] times its text, before the first), it is merged into one
//! segment, [`STEP`] pages a step, each step its own short transaction, so
//! a pull or a save never waits long behind it. That also drops the dead
//! entries a withdrawn post's text left in the index (`PRAGMA secure_delete`
//! covers the pages SQLite frees; spec §13.4).
//!
//! FTS5's own `secure-delete` option (deletes edit the old segments in
//! place, so nothing dead piles up) was measured and left off: on a
//! 6,000-post list of 11 KB posts (`fts_tests::fts_bench`) an edited or
//! deleted post cost ~140 ms of writing instead of ~4 ms, as a trigram index
//! touches a page for nearly every trigram. Without it, 8,000 such writes
//! took that index from 2.6 to 5.1 times its text, and compaction brought
//! it back to 2.6 in 4.4 s of steps (searches 5.9 → 3.6 ms).

use std::time::Instant;

use rusqlite::Connection;

use super::Store;
use crate::backend::Result;

/// The indexes, with the text each one indexes (for the size ratio).
const TABLES: [(&str, &str); 2] = [
    (
        "reading_fts",
        "SELECT coalesce(sum(length(c0)), 0) FROM reading_fts_content",
    ),
    (
        "items_fts",
        "SELECT coalesce(sum(length(content_md)), 0) FROM items",
    ),
];

/// Pages of index one compaction step writes (`'merge', -STEP`): about
/// 4 KB each.
pub(super) const STEP: i64 = 64;

/// How long one upkeep call may keep stepping before it hands the store back.
pub(super) const BUDGET: std::time::Duration = std::time::Duration::from_millis(150);

/// How often (ms) the index's size is weighed.
pub(super) const WEIGH_EVERY_MS: i64 = 6 * 60 * 60 * 1000;

/// Compact when the index has grown to this many times its size right after
/// the last compaction…
pub(super) const BLOAT: f64 = 1.5;

/// …or, before any compaction, to this many times its text. A compact
/// trigram index is about 2.5 times its text.
pub(super) const FIRST_BLOAT: f64 = 3.5;

/// An index smaller than this (bytes) is never worth compacting: a few
/// short posts have more overhead than text.
pub(super) const MIN_INDEX: i64 = 1024 * 1024;

/// An index's size and its text's, in bytes.
pub(super) fn weigh(c: &Connection, table: &str) -> rusqlite::Result<(i64, i64)> {
    let text_sql = TABLES
        .iter()
        .find(|(t, _)| *t == table)
        .map(|(_, q)| *q)
        .expect("a known index");
    let index: i64 = c.query_row(
        &format!("SELECT coalesce(sum(length(block)), 0) FROM {table}_data"),
        [],
        |r| r.get(0),
    )?;
    let text: i64 = c.query_row(text_sql, [], |r| r.get(0))?;
    Ok((index, text))
}

/// One compaction step: merge up to `pages` pages of `table` towards a
/// single segment. Returns whether there's more to do.
fn step(c: &Connection, table: &str, pages: i64) -> rusqlite::Result<bool> {
    let before = c.total_changes();
    c.execute(
        &format!("INSERT INTO {table}({table}, rank) VALUES ('merge', ?1)"),
        [-pages],
    )?;
    // SQLite's documented test: fewer than two changes, nothing left.
    Ok(c.total_changes() - before >= 2)
}

/// What one [`Store::fts_upkeep`] did.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct FtsUpkeep {
    /// Indexes weighed: (index, index bytes, text bytes).
    pub weighed: Vec<(&'static str, i64, i64)>,
    /// Indexes that started compacting.
    pub started: Vec<&'static str>,
    /// Compaction steps run.
    pub steps: usize,
    /// Indexes that finished compacting.
    pub finished: Vec<&'static str>,
}

fn meta_key(table: &str, what: &str) -> String {
    format!("fts.{table}.{what}")
}

impl Store {
    /// A bounded slice of text-index upkeep (see the module docs): weigh
    /// the indexes when due, and step any compaction that's under way. At
    /// most [`BUDGET`] of work, then the store is free again. `now` is unix
    /// ms. For the typing writer's idle tick; never on the UI thread.
    pub fn fts_upkeep(&self, now: i64) -> Result<FtsUpkeep> {
        // Where it ran, for the test that it runs on the typing writer.
        #[cfg(test)]
        if let Some(name) = std::thread::current().name() {
            self.set_meta("fts.test.thread", name)?;
        }
        let mut done = FtsUpkeep::default();
        let weighed_at = self
            .meta("fts.weighed_at")
            .and_then(|v| v.parse::<i64>().ok());
        if weighed_at.is_none_or(|t| now - t >= WEIGH_EVERY_MS || t > now) {
            for (t, _) in TABLES {
                let (index, text) = weigh(&self.conn(), t)?;
                done.weighed.push((t, index, text));
                if self.meta(&meta_key(t, "compacting")).is_some() || text == 0 || index < MIN_INDEX
                {
                    continue;
                }
                let base = self
                    .meta(&meta_key(t, "compact"))
                    .and_then(|v| v.parse::<f64>().ok());
                let ratio = index as f64 / text as f64;
                let bloated = match base {
                    Some(b) if b > 0.0 => ratio > b * BLOAT,
                    _ => ratio > FIRST_BLOAT,
                };
                if bloated {
                    self.set_meta(&meta_key(t, "compacting"), "1")?;
                    done.started.push(t);
                }
            }
            self.set_meta("fts.weighed_at", &now.to_string())?;
        }
        let start = Instant::now();
        for (t, _) in TABLES {
            if self.meta(&meta_key(t, "compacting")).is_none() {
                continue;
            }
            loop {
                if start.elapsed() >= BUDGET {
                    return Ok(done);
                }
                let more = step(&self.conn(), t, STEP)?;
                done.steps += 1;
                if !more {
                    let (index, text) = weigh(&self.conn(), t)?;
                    if text > 0 {
                        let ratio = index as f64 / text as f64;
                        self.set_meta(&meta_key(t, "compact"), &format!("{ratio:.3}"))?;
                    }
                    self.delete_meta(&meta_key(t, "compacting"))?;
                    done.finished.push(t);
                    break;
                }
            }
        }
        Ok(done)
    }

    /// Whether an index compaction is under way (`fts_upkeep` has steps
    /// left), so the caller can come back sooner than its usual tick.
    pub fn fts_compacting(&self) -> bool {
        TABLES
            .iter()
            .any(|(t, _)| self.meta(&meta_key(t, "compacting")).is_some())
    }
}
