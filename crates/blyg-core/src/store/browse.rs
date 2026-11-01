//! --- browse --- The local browsing log and link-probe cache (schema v13,
//! mirroring studio 0.49's migration 0033). The log is used only when the
//! blyg doesn't keep one (`crate::browse::reads`); the cache only when the
//! blyg can't classify links itself.

use rusqlite::{OptionalExtension, params};

use super::Store;
use crate::backend::Result;
use crate::browse::{BrowseStep, BrowseVia, ItemRef, NewBrowseStep};

/// What the studio keeps of a title (`browse-log.ts` `TITLE_MAX`).
const TITLE_KEPT: usize = 160;

fn step(r: &rusqlite::Row<'_>) -> rusqlite::Result<Option<BrowseStep>> {
    let via: String = r.get(7)?;
    Ok(BrowseVia::parse(&via).map(|via| BrowseStep {
        id: r.get(0).unwrap_or_default(),
        at: r.get(1).unwrap_or_default(),
        trail: r.get(2).unwrap_or_default(),
        parent_id: r.get(3).unwrap_or_default(),
        origin: r.get(4).unwrap_or_default(),
        remote_id: r.get(5).unwrap_or_default(),
        version: r.get::<_, Option<i64>>(6).ok().flatten().map(|v| v as u32),
        via,
        title: r.get(8).unwrap_or_default(),
    }))
}

impl Store {
    /// Record a step at `at` (RFC 3339); returns its id. A parent from
    /// another trail (or one that isn't here) is dropped, as the studio does.
    pub fn record_browse(&self, s: &NewBrowseStep, at: &str) -> Result<i64> {
        let c = self.conn();
        let parent: Option<i64> = match s.parent_id {
            Some(p) => c
                .query_row(
                    "SELECT id FROM browse_log WHERE id = ?1 AND trail = ?2",
                    params![p, s.trail],
                    |r| r.get(0),
                )
                .optional()?,
            None => None,
        };
        let title = s
            .title
            .as_deref()
            .filter(|t| !t.is_empty())
            .map(|t| t.chars().take(TITLE_KEPT).collect::<String>());
        c.execute(
            "INSERT INTO browse_log (at, trail, parent_id, origin, remote_id, version, via, title) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                at,
                s.trail,
                parent,
                s.origin,
                s.remote_id,
                s.version.filter(|v| *v > 0),
                s.via.as_str(),
                title
            ],
        )?;
        Ok(c.last_insert_rowid())
    }

    /// The local log, newest first: one trail, or every trail.
    pub fn browse_log(
        &self,
        trail: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<BrowseStep>> {
        let c = self.conn();
        let cols = "id, at, trail, parent_id, origin, remote_id, version, via, title";
        let rows = match trail {
            Some(t) => {
                let mut st = c.prepare_cached(&format!(
                    "SELECT {cols} FROM browse_log WHERE trail = ?1 ORDER BY id DESC LIMIT ?2 OFFSET ?3"
                ))?;
                st.query_map(params![t, limit as i64, offset as i64], step)?
                    .collect::<rusqlite::Result<Vec<_>>>()?
            }
            None => {
                let mut st = c.prepare_cached(&format!(
                    "SELECT {cols} FROM browse_log ORDER BY id DESC LIMIT ?1 OFFSET ?2"
                ))?;
                st.query_map(params![limit as i64, offset as i64], step)?
                    .collect::<rusqlite::Result<Vec<_>>>()?
            }
        };
        Ok(rows.into_iter().flatten().collect())
    }

    /// Forget one trail, or the whole local log.
    pub fn clear_browse_log(&self, trail: Option<&str>) -> Result<()> {
        let c = self.conn();
        match trail {
            Some(t) => c.execute("DELETE FROM browse_log WHERE trail = ?1", [t])?,
            None => c.execute("DELETE FROM browse_log", [])?,
        };
        Ok(())
    }

    /// A cached probe of `url` checked at or after `since` (unix ms):
    /// `Some(None)` = not a blyg item; `None` = not known (or too old).
    pub fn cached_link_probe(&self, url: &str, since: i64) -> Option<Option<ItemRef>> {
        let row: Option<(Option<String>, Option<String>)> = self
            .conn()
            .query_row(
                "SELECT origin, remote_id FROM link_probe WHERE url = ?1 AND checked_at >= ?2",
                params![url, since],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .ok()
            .flatten();
        row.map(|(o, id)| match (o, id) {
            (Some(origin), Some(id)) => Some(ItemRef { origin, id }),
            _ => None,
        })
    }

    /// Remember what a probe of `url` found at `now` (unix ms).
    pub fn put_link_probe(&self, url: &str, target: Option<&ItemRef>, now: i64) -> Result<()> {
        self.conn().execute(
            "INSERT INTO link_probe (url, origin, remote_id, checked_at) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(url) DO UPDATE SET origin = excluded.origin, \
             remote_id = excluded.remote_id, checked_at = excluded.checked_at",
            params![
                url,
                target.map(|t| t.origin.as_str()),
                target.map(|t| t.id.as_str()),
                now
            ],
        )?;
        Ok(())
    }

    /// A held post of a blyg subscription whose page is `url` (its own
    /// address, as the studio's `classifyLinks` looks it up first).
    pub fn held_item_at(&self, url: &str) -> Option<ItemRef> {
        let c = self.conn();
        let mut st = c
            .prepare_cached(
                "SELECT r.origin, r.remote_id, r.sub_kind, s.json FROM reading r \
                 LEFT JOIN subscriptions s ON s.id = r.subscription_id \
                 WHERE r.page_url = ?1",
            )
            .ok()?;
        let rows = st
            .query_map([url], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            })
            .ok()?
            .filter_map(|r| r.ok())
            .collect::<Vec<_>>();
        rows.into_iter()
            .find(|(_, _, kind, sub)| match kind.as_deref() {
                Some(k) => k == "blyg",
                None => sub
                    .as_deref()
                    .and_then(|j| serde_json::from_str::<serde_json::Value>(j).ok())
                    .and_then(|v| v.get("kind").and_then(|k| k.as_str()).map(|k| k == "blyg"))
                    .unwrap_or(false),
            })
            .map(|(origin, id, _, _)| ItemRef { origin, id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_step(trail: &str, parent: Option<i64>, id: &str) -> NewBrowseStep {
        NewBrowseStep {
            trail: trail.into(),
            parent_id: parent,
            origin: "https://blyg.example.com/".into(),
            remote_id: id.into(),
            version: Some(2),
            via: BrowseVia::Link,
            title: Some("x".repeat(500)),
        }
    }

    #[test]
    fn the_log_is_a_tree_per_trail_newest_first() {
        let s = Store::open_in_memory().unwrap();
        let a = s
            .record_browse(&new_step("trail-aaaa", None, "a"), "2026-10-31T00:00:00Z")
            .unwrap();
        let b = s
            .record_browse(
                &new_step("trail-aaaa", Some(a), "b"),
                "2026-10-31T00:00:01Z",
            )
            .unwrap();
        // A parent from another trail is a stale client, not an edge.
        let c = s
            .record_browse(
                &new_step("trail-bbbb", Some(b), "c"),
                "2026-10-31T00:00:02Z",
            )
            .unwrap();
        let all = s.browse_log(None, 0, 50).unwrap();
        assert_eq!(all.iter().map(|x| x.id).collect::<Vec<_>>(), [c, b, a]);
        assert_eq!(all[0].parent_id, None);
        assert_eq!(all[1].parent_id, Some(a));
        assert_eq!(
            all[1].title.as_deref().map(|t| t.chars().count()),
            Some(160)
        );
        assert_eq!(all[1].version, Some(2));
        let one = s.browse_log(Some("trail-aaaa"), 0, 50).unwrap();
        assert_eq!(one.len(), 2);
        assert_eq!(s.browse_log(None, 1, 1).unwrap()[0].id, b);
        s.clear_browse_log(Some("trail-aaaa")).unwrap();
        assert_eq!(s.browse_log(None, 0, 50).unwrap().len(), 1);
        s.clear_browse_log(None).unwrap();
        assert!(s.browse_log(None, 0, 50).unwrap().is_empty());
    }

    #[test]
    fn probes_keep_hits_and_misses_until_they_age() {
        let s = Store::open_in_memory().unwrap();
        let hit = ItemRef {
            origin: "https://blyg.example.com/".into(),
            id: "abc".into(),
        };
        s.put_link_probe("https://blyg.example.com/f/abc/", Some(&hit), 1_000)
            .unwrap();
        s.put_link_probe("https://example.org/wiki", None, 1_000)
            .unwrap();
        assert_eq!(
            s.cached_link_probe("https://blyg.example.com/f/abc/", 500),
            Some(Some(hit))
        );
        assert_eq!(
            s.cached_link_probe("https://example.org/wiki", 500),
            Some(None)
        );
        assert_eq!(s.cached_link_probe("https://example.org/wiki", 2_000), None);
        assert_eq!(s.cached_link_probe("https://example.org/other", 0), None);
    }
}
