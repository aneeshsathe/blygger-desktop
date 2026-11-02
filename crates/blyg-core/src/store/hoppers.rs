//! --- hoppers --- The blyg's hoppers, cached for the sources pane (schema
//! v14), and the outbox ops that put posts in them or take them out.
//!
//! A hopper's own fields (create, rename, public, description, delete) are
//! changed on the server first, and written here from its answer. Membership
//! is local first: `set_hopper_member` changes `hopper_items`, the reading
//! row's `hoppers` and the hopper's count at once, and queues a `hopper` op
//! that the sync worker sends (`PUT`/`DELETE /api/hoppers/{id}/items/…`).
//! Offline, the op waits like any other. A pull that reads the hoppers
//! (`replace_hoppers`) takes the server's memberships and lays the ops still
//! waiting on top, so a change made here never flickers back.
//!
//! `hopper_items` is the one place that knows every membership, your own
//! posts' (`~own`) included; the reading rows' `hoppers` follow it once the
//! hoppers have been read (`HOPPERS_KNOWN`).

use std::collections::{BTreeSet, HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension, params};

use super::{OpKind, Store};
use crate::backend::Result;
use crate::model::{Hopper, LocalId, ReadingItem};
use crate::util::{now_iso, now_ms};

/// `meta` key: the hoppers have been read from this blyg at least once, so
/// `hopper_items` is the truth about memberships.
pub const HOPPERS_KNOWN: &str = "hoppers_known";

/// A queued membership change.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HopperOp {
    pub hopper: String,
    pub sub: String,
    pub remote_id: String,
    /// In (`PUT`) or out (`DELETE`).
    pub on: bool,
}

/// Outbox key for one (hopper, post) membership.
pub fn hopper_key(hopper: &str, sub: &str, remote_id: &str) -> LocalId {
    LocalId(format!("hopper\u{1f}{hopper}\u{1f}{sub}\u{1f}{remote_id}"))
}

/// One hopper read from the server: the hopper, its memberships
/// `(sub, remote_id, added_at)`, and the imported items it returned.
pub struct FetchedHopper {
    pub hopper: Hopper,
    pub members: Vec<(String, String, String)>,
    pub items: Vec<ReadingItem>,
}

fn waiting_ops(c: &Connection) -> rusqlite::Result<Vec<HopperOp>> {
    let mut st = c.prepare_cached("SELECT payload FROM outbox WHERE op = 'hopper' ORDER BY seq")?;
    let v = st
        .query_map([], |r| r.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .filter_map(|p| serde_json::from_str::<HopperOp>(&p).ok())
        .collect();
    Ok(v)
}

/// Apply one membership change to `hopper_items` and the hopper's count.
/// True when it changed anything.
fn apply(c: &Connection, op: &HopperOp) -> rusqlite::Result<bool> {
    let known: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM hoppers WHERE id = ?1)",
        [&op.hopper],
        |r| r.get(0),
    )?;
    if !known {
        return Ok(false);
    }
    let n = if op.on {
        c.execute(
            "INSERT OR IGNORE INTO hopper_items (hopper_id, subscription_id, remote_id, added_at) \
             VALUES (?1, ?2, ?3, ?4)",
            params![op.hopper, op.sub, op.remote_id, now_iso()],
        )?
    } else {
        c.execute(
            "DELETE FROM hopper_items WHERE hopper_id = ?1 AND subscription_id = ?2 AND remote_id = ?3",
            params![op.hopper, op.sub, op.remote_id],
        )?
    };
    if n > 0 {
        bump_count(c, &op.hopper, if op.on { 1 } else { -1 })?;
    }
    Ok(n > 0)
}

fn bump_count(c: &Connection, id: &str, by: i64) -> rusqlite::Result<()> {
    let json: Option<String> = c
        .query_row("SELECT json FROM hoppers WHERE id = ?1", [id], |r| r.get(0))
        .optional()?;
    if let Some(mut h) = json.and_then(|j| serde_json::from_str::<Hopper>(&j).ok()) {
        h.count = (h.count as i64 + by).max(0) as u32;
        c.execute(
            "UPDATE hoppers SET json = ?2 WHERE id = ?1",
            params![id, serde_json::to_string(&h).unwrap_or_default()],
        )?;
    }
    Ok(())
}

/// (sub, remote id) → the hoppers holding it, sorted.
fn members(c: &Connection) -> rusqlite::Result<HashMap<(String, String), Vec<String>>> {
    let mut st = c.prepare_cached(
        "SELECT subscription_id, remote_id, hopper_id FROM hopper_items ORDER BY hopper_id",
    )?;
    let mut out: HashMap<(String, String), Vec<String>> = HashMap::new();
    for r in st.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (s, rid, h) = r?;
        out.entry((s, rid)).or_default().push(h);
    }
    Ok(out)
}

/// Make each reading row's `hoppers` what `hopper_items` says (only once
/// the hoppers are known). `only` limits it to one row. True when a row
/// changed.
/// `gone`: hoppers just removed here, which leave the rows too.
fn sync_rows(
    c: &Connection,
    only: Option<(&str, &str)>,
    gone: &[String],
) -> rusqlite::Result<bool> {
    let known: bool = c
        .query_row(
            "SELECT value FROM meta WHERE key = ?1",
            [HOPPERS_KNOWN],
            |r| r.get::<_, String>(0),
        )
        .optional()?
        .is_some();
    if !known {
        return Ok(false);
    }
    let map = members(c)?;
    // A hopper this Mac hasn't read yet (made elsewhere since) keeps the
    // pulled row's word for it: only known hoppers are set from the table.
    let ids: std::collections::HashSet<String> = {
        let mut st = c.prepare("SELECT id FROM hoppers")?;
        st.query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    let rows: Vec<(String, String, String)> = {
        let (sql, args): (&str, Vec<&str>) = match only {
            Some((s, r)) => (
                "SELECT subscription_id, remote_id, json FROM reading WHERE subscription_id = ?1 AND remote_id = ?2",
                vec![s, r],
            ),
            None => (
                "SELECT subscription_id, remote_id, json FROM reading",
                vec![],
            ),
        };
        let mut st = c.prepare(sql)?;
        st.query_map(rusqlite::params_from_iter(args), |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut changed = false;
    for (s, rid, json) in rows {
        let Ok(mut it) = serde_json::from_str::<ReadingItem>(&json) else {
            continue;
        };
        let mut want = map
            .get(&(s.clone(), rid.clone()))
            .cloned()
            .unwrap_or_default();
        want.extend(
            it.hoppers
                .iter()
                .filter(|h| !ids.contains(*h) && !gone.contains(*h))
                .cloned(),
        );
        want.sort();
        want.dedup();
        let mut have = it.hoppers.clone();
        have.sort();
        if have == want {
            continue;
        }
        it.hoppers = want;
        c.execute(
            "UPDATE reading SET json = ?3 WHERE subscription_id = ?1 AND remote_id = ?2",
            params![s, rid, serde_json::to_string(&it).unwrap_or(json)],
        )?;
        changed = true;
    }
    Ok(changed)
}

impl Store {
    /// Every hopper, in the server's order (oldest first).
    pub fn hoppers(&self) -> Vec<Hopper> {
        let c = self.conn();
        let Ok(mut st) = c.prepare_cached("SELECT json FROM hoppers ORDER BY position, rowid")
        else {
            return vec![];
        };
        st.query_map([], |r| r.get::<_, String>(0))
            .map(|it| {
                it.filter_map(|r| r.ok())
                    .filter_map(|j| serde_json::from_str(&j).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// (sub, remote id) → the ids of the hoppers holding that post, yours
    /// (`~own`) included.
    pub fn hopper_members(&self) -> HashMap<(String, String), Vec<String>> {
        members(&self.conn()).unwrap_or_default()
    }

    /// Hopper id → its posts (sub, remote id) in the hopper's own order:
    /// `added_at` newest first, then subscription and remote id (the
    /// studio's membership order). A membership made here carries the time
    /// it was made until the next read of the hoppers brings the blyg's.
    /// The studio keeps whole seconds (`…:05Z`) where Burrow keeps
    /// milliseconds (`…:05.123Z`), so times compare to the second and
    /// posts added in the same second follow the studio's tie order.
    pub fn hopper_order(&self) -> HashMap<String, Vec<(String, String)>> {
        let c = self.conn();
        let Ok(mut st) = c.prepare_cached(
            "SELECT hopper_id, subscription_id, remote_id FROM hopper_items \
             ORDER BY hopper_id, substr(added_at, 1, 19) DESC, subscription_id, remote_id",
        ) else {
            return HashMap::new();
        };
        let mut out: HashMap<String, Vec<(String, String)>> = HashMap::new();
        if let Ok(rows) = st.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        }) {
            for (h, s, rid) in rows.filter_map(|r| r.ok()) {
                out.entry(h).or_default().push((s, rid));
            }
        }
        out
    }

    /// Posts in a hopper that this Mac doesn't hold as reading rows, as the
    /// hopper read returned them, each with every hopper it's in.
    pub fn hopper_unheld(&self) -> Vec<ReadingItem> {
        let c = self.conn();
        let map = members(&c).unwrap_or_default();
        let Ok(mut st) = c.prepare_cached(
            "SELECT h.subscription_id, h.remote_id, h.item_json FROM hopper_items h \
             WHERE h.item_json IS NOT NULL AND NOT EXISTS (SELECT 1 FROM reading r \
             WHERE r.subscription_id = h.subscription_id AND r.remote_id = h.remote_id)",
        ) else {
            return vec![];
        };
        let mut seen = HashSet::new();
        st.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map(|it| {
            it.filter_map(|r| r.ok())
                .filter(|(s, rid, _)| seen.insert((s.clone(), rid.clone())))
                .filter_map(|(s, rid, j)| {
                    let mut item: ReadingItem = serde_json::from_str(&j).ok()?;
                    item.hoppers = map.get(&(s, rid)).cloned().unwrap_or_default();
                    Some(item)
                })
                .collect()
        })
        .unwrap_or_default()
    }

    /// Take a fresh read of every hopper: the list, the memberships and the
    /// unheld items, with the ops still waiting laid on top. The reading
    /// rows' `hoppers` follow. True when anything shown changed.
    pub fn replace_hoppers(&self, fetched: &[FetchedHopper]) -> Result<bool> {
        let mut c = self.conn();
        let tx = c.transaction()?;
        let before: (Vec<String>, BTreeSet<(String, String, String)>) = {
            let mut st = tx.prepare("SELECT json FROM hoppers ORDER BY position, rowid")?;
            let hs = st
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let mut st =
                tx.prepare("SELECT hopper_id, subscription_id, remote_id FROM hopper_items")?;
            let ms = st
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<rusqlite::Result<BTreeSet<_>>>()?;
            (hs, ms)
        };
        let gone: Vec<String> = {
            let mut st = tx.prepare("SELECT id FROM hoppers")?;
            st.query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
                .into_iter()
                .filter(|id| !fetched.iter().any(|f| &f.hopper.id == id))
                .collect()
        };
        tx.execute("DELETE FROM hoppers", [])?;
        tx.execute("DELETE FROM hopper_items", [])?;
        for (i, f) in fetched.iter().enumerate() {
            tx.execute(
                "INSERT OR REPLACE INTO hoppers (id, json, position) VALUES (?1, ?2, ?3)",
                params![
                    f.hopper.id,
                    serde_json::to_string(&f.hopper).unwrap_or_default(),
                    i as i64
                ],
            )?;
            let items: HashMap<(&str, &str), &ReadingItem> = f
                .items
                .iter()
                .map(|it| ((it.subscription_id.as_str(), it.remote_id.as_str()), it))
                .collect();
            for (s, rid, at) in &f.members {
                let item = items
                    .get(&(s.as_str(), rid.as_str()))
                    .and_then(|it| serde_json::to_string(it).ok());
                tx.execute(
                    "INSERT OR REPLACE INTO hopper_items \
                     (hopper_id, subscription_id, remote_id, added_at, item_json) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![f.hopper.id, s, rid, at, item],
                )?;
            }
        }
        // Drop the ops for hoppers that are gone; lay the rest on top.
        let ids: HashSet<&str> = fetched.iter().map(|f| f.hopper.id.as_str()).collect();
        for op in waiting_ops(&tx)? {
            if ids.contains(op.hopper.as_str()) {
                apply(&tx, &op)?;
            } else {
                tx.execute(
                    "DELETE FROM outbox WHERE op = 'hopper' AND local_id = ?1 AND in_flight = 0",
                    [hopper_key(&op.hopper, &op.sub, &op.remote_id).0],
                )?;
            }
        }
        tx.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, '1')",
            [HOPPERS_KNOWN],
        )?;
        let after: (Vec<String>, BTreeSet<(String, String, String)>) = {
            let mut st = tx.prepare("SELECT json FROM hoppers ORDER BY position, rowid")?;
            let hs = st
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let mut st =
                tx.prepare("SELECT hopper_id, subscription_id, remote_id FROM hopper_items")?;
            let ms = st
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<rusqlite::Result<BTreeSet<_>>>()?;
            (hs, ms)
        };
        let rows = sync_rows(&tx, None, &gone)?;
        tx.commit()?;
        Ok(rows || before != after)
    }

    /// After a reading pull: the rows take the memberships held here (the
    /// server's, with the waiting ops on top). True when a row changed.
    pub fn sync_reading_hoppers(&self) -> Result<bool> {
        let mut c = self.conn();
        let tx = c.transaction()?;
        let changed = sync_rows(&tx, None, &[])?;
        tx.commit()?;
        Ok(changed)
    }

    /// A hopper as the server now has it (made, renamed, made public…):
    /// stored at the end when new, in place (and with its count) otherwise.
    pub fn put_hopper(&self, h: &Hopper) -> Result<Hopper> {
        let c = self.conn();
        let old: Option<(String, i64)> = c
            .query_row(
                "SELECT json, position FROM hoppers WHERE id = ?1",
                [&h.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let mut h = h.clone();
        let position = match old {
            Some((json, pos)) => {
                if let Ok(o) = serde_json::from_str::<Hopper>(&json) {
                    h.count = o.count;
                }
                pos
            }
            None => c.query_row(
                "SELECT COALESCE(MAX(position) + 1, 0) FROM hoppers",
                [],
                |r| r.get(0),
            )?,
        };
        c.execute(
            "INSERT OR REPLACE INTO hoppers (id, json, position) VALUES (?1, ?2, ?3)",
            params![
                h.id,
                serde_json::to_string(&h).unwrap_or_default(),
                position
            ],
        )?;
        Ok(h)
    }

    /// A hopper deleted on the server: gone here too, with its memberships
    /// and waiting ops. The posts stay. True when it was held.
    pub fn remove_hopper(&self, id: &str) -> Result<bool> {
        let mut c = self.conn();
        let tx = c.transaction()?;
        let n = tx.execute("DELETE FROM hoppers WHERE id = ?1", [id])?;
        tx.execute("DELETE FROM hopper_items WHERE hopper_id = ?1", [id])?;
        tx.execute(
            "DELETE FROM outbox WHERE op = 'hopper' AND in_flight = 0 AND substr(local_id, 1, ?2) = ?1",
            params![
                format!("hopper\u{1f}{id}\u{1f}"),
                format!("hopper\u{1f}{id}\u{1f}").len() as i64
            ],
        )?;
        sync_rows(&tx, None, &[id.to_string()])?;
        tx.commit()?;
        Ok(n > 0)
    }

    /// Put a post in a hopper (`on`) or take it out, here at once, and queue
    /// the change for the blyg (one op waits per membership: the last
    /// action replaces it). False when nothing changed (it was already so,
    /// or the hopper isn't known).
    pub fn set_hopper_member(
        &self,
        hopper: &str,
        sub: &str,
        remote_id: &str,
        on: bool,
    ) -> Result<bool> {
        let mut c = self.conn();
        let tx = c.transaction()?;
        let op = HopperOp {
            hopper: hopper.into(),
            sub: sub.into(),
            remote_id: remote_id.into(),
            on,
        };
        if !apply(&tx, &op)? {
            return Ok(false);
        }
        let key = hopper_key(hopper, sub, remote_id);
        tx.execute(
            "DELETE FROM outbox WHERE op = 'hopper' AND local_id = ?1 AND in_flight = 0",
            [&key.0],
        )?;
        tx.execute(
            "INSERT INTO outbox (local_id, op, payload, not_before) VALUES (?1, ?2, ?3, ?4)",
            params![
                key.0,
                OpKind::Hopper.as_str(),
                serde_json::to_string(&op).unwrap_or_default(),
                now_ms()
            ],
        )?;
        // A row this Mac holds follows now, even before the hoppers were
        // ever read in full.
        let json: Option<String> = tx
            .query_row(
                "SELECT json FROM reading WHERE subscription_id = ?1 AND remote_id = ?2",
                [sub, remote_id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(mut it) = json.and_then(|j| serde_json::from_str::<ReadingItem>(&j).ok()) {
            it.hoppers.retain(|h| h != hopper);
            if on {
                it.hoppers.push(hopper.into());
                it.hoppers.sort();
            }
            tx.execute(
                "UPDATE reading SET json = ?3 WHERE subscription_id = ?1 AND remote_id = ?2",
                params![
                    sub,
                    remote_id,
                    serde_json::to_string(&it).unwrap_or_default()
                ],
            )?;
        }
        tx.commit()?;
        Ok(true)
    }
}
