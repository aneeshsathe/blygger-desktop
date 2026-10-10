//! --- lineage counts --- The `lineage-glyph` extension's answers, cached
//! (`crate::lineage`): glyph summaries keyed by reading entry key, graphs
//! keyed by the lineage route's query, each with when it was fetched; and
//! whether the node serves the extension at all (meta `lineage_glyph`:
//! `on`, or `off:<unix ms>` after a 404).
//!
//! `lineage_cache` is a cache, not data: it's created on first use outside
//! the numbered migrations (so it never takes a schema version another
//! change needs), and dropping it loses nothing but a few requests.

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, params};

use super::Store;
use crate::backend::Result;
use crate::lineage::{LineageGraph, LineageSummary};

const META: &str = "lineage_glyph";

const TABLE: &str = "CREATE TABLE IF NOT EXISTS lineage_cache (
    kind        TEXT NOT NULL,      -- 'summary' | 'graph'
    key         TEXT NOT NULL,      -- reading entry key | the lineage query
    json        TEXT NOT NULL,      -- the answer; 'null' = the node didn't know it
    fetched_at  INTEGER NOT NULL,   -- unix ms
    PRIMARY KEY (kind, key)
)";

/// Whether the node serves the extension, as last found out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Served {
    /// Never asked.
    Unknown,
    /// It answered.
    On,
    /// It answered 404 at this time (unix ms).
    Off(i64),
}

fn table(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch(TABLE)
}

impl Store {
    pub fn lineage_served(&self) -> Served {
        match self.meta(META).as_deref() {
            Some("on") => Served::On,
            Some(v) => v
                .strip_prefix("off:")
                .and_then(|t| t.parse().ok())
                .map_or(Served::Unknown, Served::Off),
            None => Served::Unknown,
        }
    }

    /// Record what the node said. Turning off drops everything it served:
    /// the counts then come from this Mac.
    pub fn set_lineage_served(&self, on: bool, now: i64) -> Result<()> {
        if on {
            return self.set_meta(META, "on");
        }
        {
            let c = self.conn();
            table(&c)?;
            c.execute("DELETE FROM lineage_cache", [])?;
        }
        self.set_meta(META, &format!("off:{now}"))
    }

    /// The cached summaries of `keys`, whatever their age: key → (summary,
    /// or `None` when the node didn't know it; fetched at).
    pub fn cached_lineage_summaries(
        &self,
        keys: &[String],
    ) -> Result<HashMap<String, (Option<LineageSummary>, i64)>> {
        let c = self.conn();
        table(&c)?;
        let mut q = c.prepare_cached(
            "SELECT json, fetched_at FROM lineage_cache WHERE kind = 'summary' AND key = ?1",
        )?;
        let mut out = HashMap::new();
        for k in keys {
            let row: Option<(String, i64)> = q
                .query_row([k], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?;
            if let Some((json, at)) = row {
                out.insert(k.clone(), (serde_json::from_str(&json).ok().flatten(), at));
            }
        }
        Ok(out)
    }

    /// Store one summaries answer: every key asked gets a row, `null` for
    /// those the node left out.
    pub fn put_lineage_summaries(
        &self,
        asked: &[String],
        got: &HashMap<String, LineageSummary>,
        now: i64,
    ) -> Result<()> {
        let mut c = self.conn();
        table(&c)?;
        let tx = c.transaction()?;
        {
            let mut q = tx.prepare_cached(
                "INSERT INTO lineage_cache (kind, key, json, fetched_at) VALUES ('summary', ?1, ?2, ?3)
                 ON CONFLICT(kind, key) DO UPDATE SET json = excluded.json, fetched_at = excluded.fetched_at",
            )?;
            for k in asked {
                let json = serde_json::to_string(&got.get(k)).unwrap_or_else(|_| "null".into());
                q.execute(params![k, json, now])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The cached graph for a lineage query, whatever its age: (graph, or
    /// `None` when the node didn't know it; fetched at).
    pub fn cached_lineage_graph(&self, query: &str) -> Result<Option<(Option<LineageGraph>, i64)>> {
        let c = self.conn();
        table(&c)?;
        let row: Option<(String, i64)> = c
            .query_row(
                "SELECT json, fetched_at FROM lineage_cache WHERE kind = 'graph' AND key = ?1",
                [query],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        Ok(row.map(|(json, at)| (serde_json::from_str(&json).ok().flatten(), at)))
    }

    pub fn put_lineage_graph(
        &self,
        query: &str,
        graph: Option<&LineageGraph>,
        now: i64,
    ) -> Result<()> {
        let c = self.conn();
        table(&c)?;
        let json = serde_json::to_string(&graph).unwrap_or_else(|_| "null".into());
        c.execute(
            "INSERT INTO lineage_cache (kind, key, json, fetched_at) VALUES ('graph', ?1, ?2, ?3)
             ON CONFLICT(kind, key) DO UPDATE SET json = excluded.json, fetched_at = excluded.fetched_at",
            params![query, json, now],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lineage::{RelationCounts, imported_key};

    fn summary(up: u32, down: u32) -> LineageSummary {
        LineageSummary {
            up: RelationCounts {
                transclusion: up,
                ..Default::default()
            },
            down: RelationCounts {
                stub: down,
                ..Default::default()
            },
        }
    }

    #[test]
    fn summaries_round_trip_on_disk_with_their_age() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("blygger.db");
        let (a, b) = (imported_key("s1", "A"), imported_key("s1", "B"));
        {
            let s = Store::open(&path).unwrap();
            assert_eq!(s.lineage_served(), Served::Unknown);
            assert!(
                s.cached_lineage_summaries(std::slice::from_ref(&a))
                    .unwrap()
                    .is_empty()
            );
            let got = HashMap::from([(a.clone(), summary(1, 2))]);
            s.put_lineage_summaries(&[a.clone(), b.clone()], &got, 1_000)
                .unwrap();
            s.set_lineage_served(true, 1_000).unwrap();
        }
        // A fresh connection reads what the last one wrote.
        let s = Store::open(&path).unwrap();
        assert_eq!(s.lineage_served(), Served::On);
        let c = s.cached_lineage_summaries(&[a.clone(), b.clone()]).unwrap();
        assert_eq!(c[&a], (Some(summary(1, 2)), 1_000));
        assert_eq!(
            c[&b],
            (None, 1_000),
            "left out by the node: known as unknown"
        );
        // A newer answer replaces the row.
        s.put_lineage_summaries(
            std::slice::from_ref(&a),
            &HashMap::from([(a.clone(), summary(0, 5))]),
            2_000,
        )
        .unwrap();
        assert_eq!(
            s.cached_lineage_summaries(std::slice::from_ref(&a))
                .unwrap()[&a],
            (Some(summary(0, 5)), 2_000)
        );
    }

    #[test]
    fn a_404_turns_it_off_and_drops_what_it_served() {
        let s = Store::open_in_memory().unwrap();
        let a = imported_key("s1", "A");
        s.put_lineage_summaries(
            std::slice::from_ref(&a),
            &HashMap::from([(a.clone(), summary(1, 1))]),
            5,
        )
        .unwrap();
        s.put_lineage_graph("id=A&sub=s1", None, 5).unwrap();
        assert_eq!(
            s.cached_lineage_graph("id=A&sub=s1").unwrap(),
            Some((None, 5))
        );
        s.set_lineage_served(false, 99).unwrap();
        assert_eq!(s.lineage_served(), Served::Off(99));
        assert!(s.cached_lineage_summaries(&[a]).unwrap().is_empty());
        assert_eq!(s.cached_lineage_graph("id=A&sub=s1").unwrap(), None);
        s.set_lineage_served(true, 100).unwrap();
        assert_eq!(s.lineage_served(), Served::On);
    }
}
