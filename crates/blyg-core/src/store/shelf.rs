//! --- shelf --- posts kept to hand: your own at the top of the Write list,
//! and (with the synced shelf, schema v22) anyone's in Reading's Shelf lens.
//! For the living posts you keep updating, and what you keep coming back
//! to. Unrelated to pinning a version, which is public and permanent.
//! Positions are dense (0, 1, 2, …) and rewritten on every change.
//!
//! A row is one of your own posts (`local_id`) or a post named as the wire
//! names it (`subscription_id`, `remote_id`, crate::shelf::Entry). Your
//! posts' wire names are their server ids, looked up when the list is sent:
//! a scratch note or a draft the blyg doesn't have yet has none and stays
//! on this computer only, in its place. When the blyg offers the synced
//! shelf (`crate::shelf`), sync sends `shelf_wire` and lays the blyg's list
//! over the rows with `apply_shelf`; otherwise nothing here is sent.

use rusqlite::{OptionalExtension, Transaction, params};

use super::Store;
use crate::api::Prune;
use crate::backend::{CoreError, Result};
use crate::model::LocalId;
use crate::shelf::{Entry, OWN, SERVED_META, TAKEN_OFF_MAX, TAKEN_OFF_META};

pub use crate::shelf::SHELF_MAX;

/// One row of the shelf, with its wire name when it has one. Two rows are
/// the same post when their wire names match, else their local ids.
#[derive(Debug, Clone)]
pub struct ShelfRow {
    rowid: i64,
    /// One of your posts this computer holds.
    pub local_id: Option<LocalId>,
    /// What the blyg calls it; `None` for a post of yours the blyg doesn't
    /// have yet.
    pub wire: Option<Entry>,
    /// One of your posts, withdrawn: kept in its place, but neither shown
    /// nor sent (the blyg leaves withdrawn posts off).
    pub withdrawn: bool,
}

impl PartialEq for ShelfRow {
    fn eq(&self, o: &ShelfRow) -> bool {
        match (&self.wire, &o.wire) {
            (Some(a), Some(b)) => a == b,
            _ => self.local_id.is_some() && self.local_id == o.local_id,
        }
    }
}

/// Every row in order, withdrawn posts included.
fn rows(tx: &rusqlite::Connection) -> Result<Vec<ShelfRow>> {
    let mut st = tx.prepare_cached(
        "SELECT s.rowid, s.local_id, i.server_id, i.status, s.subscription_id, s.remote_id \
         FROM shelf s LEFT JOIN items i ON i.local_id = s.local_id \
         ORDER BY s.position, s.rowid",
    )?;
    let out = st
        .query_map([], |r| {
            let local: Option<String> = r.get(1)?;
            let server: Option<String> = r.get(2)?;
            let status: Option<String> = r.get(3)?;
            let sub: Option<String> = r.get(4)?;
            let rid: Option<String> = r.get(5)?;
            let wire = match (&local, sub, rid) {
                (Some(_), _, _) => server.filter(|s| !s.is_empty()).map(Entry::own),
                (None, Some(s), Some(r)) => Some(Entry::new(s, r)),
                _ => None,
            };
            Ok(ShelfRow {
                rowid: r.get(0)?,
                local_id: local.map(LocalId),
                wire,
                withdrawn: status.as_deref() == Some("withdrawn"),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(out)
}

fn renumber(tx: &Transaction, rowids: &[i64]) -> Result<()> {
    let mut st = tx.prepare_cached("UPDATE shelf SET position = ?1 WHERE rowid = ?2")?;
    for (i, id) in rowids.iter().enumerate() {
        st.execute(params![i as i64, id])?;
    }
    Ok(())
}

fn full() -> CoreError {
    CoreError::Rejected {
        status: 400,
        message: format!("Your shelf holds at most {SHELF_MAX} posts. Take one off first."),
        details: vec![],
    }
}

/// Your post with server id `sid`, when this computer holds it.
fn local_of(tx: &rusqlite::Connection, sid: &str) -> Result<Option<String>> {
    Ok(tx
        .query_row(
            "SELECT local_id FROM items WHERE server_id = ?1",
            params![sid],
            |r| r.get(0),
        )
        .optional()?)
}

impl Store {
    /// Your own posts on the shelf, in order. A withdrawn post is left out:
    /// it can't be updated any more, so it has no reason to be kept to hand.
    /// A post of yours the shelf names by its server id (synced from
    /// elsewhere before this computer held it) counts once it's here.
    pub fn shelf(&self) -> Vec<LocalId> {
        let c = self.conn();
        let Ok(mut st) = c.prepare_cached(
            "SELECT i.local_id FROM shelf s JOIN items i ON i.local_id = s.local_id \
                 OR (s.local_id IS NULL AND s.subscription_id = ?1 AND i.server_id = s.remote_id) \
             WHERE i.status <> 'withdrawn' ORDER BY s.position, s.rowid",
        ) else {
            return vec![];
        };
        st.query_map([OWN], |r| r.get::<_, String>(0).map(LocalId))
            .and_then(|it| it.collect())
            .unwrap_or_default()
    }

    /// Every row of the shelf, in order (withdrawn posts included).
    pub fn shelf_rows(&self) -> Vec<ShelfRow> {
        rows(&self.conn()).unwrap_or_default()
    }

    /// The shelf as the blyg would name it, in order: every row with a wire
    /// name, but not your withdrawn posts (the blyg leaves those off). Your
    /// posts the blyg doesn't have yet aren't in it.
    pub fn shelf_wire(&self) -> Vec<Entry> {
        self.shelf_rows()
            .into_iter()
            .filter(|r| !r.withdrawn)
            .filter_map(|r| r.wire)
            .collect()
    }

    /// Put a post of yours on the shelf (at the end) or take it off. Either,
    /// when it's already so, changes nothing. `NotFound` for a post this
    /// store doesn't hold.
    pub fn set_on_shelf(&self, id: &LocalId, on: bool) -> Result<()> {
        let mut c = self.conn();
        let tx = c.transaction()?;
        let all = rows(&tx)?;
        let sid: Option<Option<String>> = tx
            .query_row(
                "SELECT server_id FROM items WHERE local_id = ?1",
                params![id.0],
                |r| r.get(0),
            )
            .optional()?;
        let Some(sid) = sid else {
            if on {
                return Err(CoreError::NotFound);
            }
            return Ok(());
        };
        let wire = sid.map(Entry::own);
        let at = all
            .iter()
            .position(|r| r.local_id.as_ref() == Some(id) || (wire.is_some() && r.wire == wire));
        match (on, at) {
            (true, None) => {
                if all.len() >= SHELF_MAX {
                    return Err(full());
                }
                tx.execute(
                    "INSERT INTO shelf (local_id, position) VALUES (?1, ?2)",
                    params![id.0, all.len() as i64],
                )?;
                note_taken_off(&tx, wire.as_ref(), false)?;
            }
            (true, Some(ix)) if all[ix].local_id.is_none() => {
                note_taken_off(&tx, wire.as_ref(), false)?;
                // Named by its server id: now held here, so by its local id.
                tx.execute(
                    "UPDATE shelf SET local_id = ?1, subscription_id = NULL, remote_id = NULL \
                     WHERE rowid = ?2",
                    params![id.0, all[ix].rowid],
                )?;
            }
            (false, Some(ix)) => remove_at(&tx, &all, ix)?,
            _ => {}
        }
        tx.commit()?;
        Ok(())
    }

    /// Put anyone's post on the shelf (at the end) or take it off, by its
    /// wire name: a reading row's `(subscription id, remote id)`. Your own
    /// (`~own`) goes through [`Store::set_on_shelf`] when this computer
    /// holds it.
    pub fn set_entry_on_shelf(&self, entry: &Entry, on: bool) -> Result<()> {
        if entry.is_own() {
            let local = local_of(&self.conn(), &entry.remote_id)?;
            if let Some(local) = local {
                return self.set_on_shelf(&LocalId(local), on);
            }
        }
        let mut c = self.conn();
        let tx = c.transaction()?;
        let all = rows(&tx)?;
        let at = all.iter().position(|r| r.wire.as_ref() == Some(entry));
        match (on, at) {
            (true, None) => {
                if all.len() >= SHELF_MAX {
                    return Err(full());
                }
                tx.execute(
                    "INSERT INTO shelf (subscription_id, remote_id, position) VALUES (?1, ?2, ?3)",
                    params![entry.sub, entry.remote_id, all.len() as i64],
                )?;
                note_taken_off(&tx, Some(entry), false)?;
            }
            (false, Some(ix)) => remove_at(&tx, &all, ix)?,
            _ => {}
        }
        tx.commit()?;
        Ok(())
    }

    /// Move a post of yours to `index` on the shelf as `shelf()` lists it
    /// (clamped to the end). Everything else on the shelf keeps its order.
    /// `NotFound` if it isn't on the shelf.
    pub fn move_on_shelf(&self, id: &LocalId, index: usize) -> Result<()> {
        let visible = self.shelf();
        let mut c = self.conn();
        let tx = c.transaction()?;
        let mut all = rows(&tx)?;
        let sid: Option<String> = tx
            .query_row(
                "SELECT server_id FROM items WHERE local_id = ?1",
                params![id.0],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        let own_wire = |sid: &Option<String>| sid.clone().map(Entry::own);
        let is = |r: &ShelfRow, lid: &LocalId, w: &Option<Entry>| {
            r.local_id.as_ref() == Some(lid) || (w.is_some() && &r.wire == w)
        };
        let wire = own_wire(&sid);
        let Some(from) = all.iter().position(|r| is(r, id, &wire)) else {
            return Err(CoreError::NotFound);
        };
        // `index` counts visible posts of yours: land just before the one
        // now at that index, else just after the last of them.
        let anchor = visible.iter().filter(|v| *v != id).nth(index).cloned();
        let last = visible.iter().rfind(|v| *v != id).cloned();
        let f = all.remove(from);
        let find = |all: &[ShelfRow], lid: &LocalId| -> Result<Option<usize>> {
            let sid: Option<String> = tx
                .query_row(
                    "SELECT server_id FROM items WHERE local_id = ?1",
                    params![lid.0],
                    |r| r.get(0),
                )
                .optional()?
                .flatten();
            let w = own_wire(&sid);
            Ok(all.iter().position(|r| is(r, lid, &w)))
        };
        let to = match (&anchor, &last) {
            (Some(a), _) => find(&all, a)?.unwrap_or(all.len()),
            (None, Some(l)) => find(&all, l)?.map_or(all.len(), |p| p + 1),
            (None, None) => all.len(),
        };
        all.insert(to, f);
        renumber(&tx, &all.iter().map(|r| r.rowid).collect::<Vec<_>>())?;
        tx.commit()?;
        Ok(())
    }

    /// Lay the blyg's shelf (`server`, its order) over the rows: what it
    /// names is what's on the shelf, in its order, and what it doesn't name
    /// comes off when `prune` allows (on a newer contract, or before any
    /// check, it stays, after the blyg's). Posts of yours the blyg can't
    /// name (no server id yet, or withdrawn) keep their places among them.
    /// True when anything changed.
    pub fn apply_shelf(&self, server: &[Entry], prune: Prune) -> Result<bool> {
        let mut c = self.conn();
        let tx = c.transaction()?;
        let old = rows(&tx)?;
        let held: Vec<Entry>;
        let server = match prune {
            Prune::Allowed => server,
            Prune::Withheld => {
                held = server
                    .iter()
                    .cloned()
                    .chain(
                        old.iter()
                            .filter_map(|r| r.wire.clone())
                            .filter(|w| !server.contains(w)),
                    )
                    .collect();
                &held
            }
        };
        // Each entry as a row: an existing one, or a new one.
        let mut next: Vec<ShelfRow> = Vec::with_capacity(server.len());
        for e in server {
            if next.iter().any(|r| r.wire.as_ref() == Some(e)) {
                continue;
            }
            let local = if e.is_own() {
                local_of(&tx, &e.remote_id)?.map(LocalId)
            } else {
                None
            };
            let found = old
                .iter()
                .find(|r| r.wire.as_ref() == Some(e) || (local.is_some() && r.local_id == local));
            next.push(ShelfRow {
                rowid: found.map_or(0, |r| r.rowid),
                local_id: local,
                wire: Some(e.clone()),
                withdrawn: false,
            });
        }
        // This computer's own: kept in place.
        let kept = |r: &ShelfRow| r.wire.is_none() || r.withdrawn;
        crate::shelf::place_after(&old, &mut next, kept);
        let same = next.len() == old.len()
            && next
                .iter()
                .zip(&old)
                .all(|(a, b)| a.rowid != 0 && a.rowid == b.rowid && a.local_id == b.local_id);
        if same {
            return Ok(false);
        }
        // Rewrite: the simplest correct thing for at most fifty rows.
        tx.execute("DELETE FROM shelf", [])?;
        for (i, r) in next.iter().enumerate() {
            match (&r.local_id, &r.wire) {
                (Some(l), _) => tx.execute(
                    "INSERT INTO shelf (local_id, position) VALUES (?1, ?2)",
                    params![l.0, i as i64],
                )?,
                (None, Some(w)) => tx.execute(
                    "INSERT INTO shelf (subscription_id, remote_id, position) VALUES (?1, ?2, ?3)",
                    params![w.sub, w.remote_id, i as i64],
                )?,
                (None, None) => 0,
            };
        }
        tx.commit()?;
        Ok(true)
    }
}

/// The outbox key of the one waiting shelf op (`OpKind::Shelf`).
pub const SHELF_OP_KEY: &str = "shelf:";

impl Store {
    /// Ask sync to send the shelf: one `shelf` op waits at a time (one not
    /// yet on the wire is replaced, so it sends the shelf as it is then).
    pub fn queue_shelf(&self) -> Result<()> {
        let mut c = self.conn();
        let tx = c.transaction()?;
        tx.execute(
            "DELETE FROM outbox WHERE op = 'shelf' AND in_flight = 0",
            [],
        )?;
        tx.execute(
            "INSERT INTO outbox (local_id, op, payload, not_before) VALUES (?1, ?2, '{}', ?3)",
            params![
                SHELF_OP_KEY,
                super::OpKind::Shelf.as_str(),
                crate::util::now_ms()
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Whether a shelf op waits to be sent.
    pub fn shelf_waiting(&self) -> bool {
        self.conn()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM outbox WHERE op = 'shelf')",
                [],
                |r| r.get(0),
            )
            .unwrap_or(false)
    }

    /// Drop every waiting shelf op (the blyg doesn't offer the shelf).
    pub fn drop_shelf_ops(&self) -> Result<()> {
        self.conn()
            .execute("DELETE FROM outbox WHERE op = 'shelf'", [])?;
        Ok(())
    }
}

/// Note that this side took `entry` off the shelf (`off`), or put it back
/// ([`TAKEN_OFF_META`]). A post the blyg can't name yet needs no note, and
/// nothing is noted while the blyg doesn't serve Saved: such a removal
/// isn't one from a list the blyg agreed.
fn note_taken_off(tx: &Transaction, entry: Option<&Entry>, off: bool) -> Result<()> {
    let Some(entry) = entry else {
        return Ok(());
    };
    let served: Option<String> = tx
        .query_row("SELECT value FROM meta WHERE key = ?1", [SERVED_META], |r| {
            r.get(0)
        })
        .optional()?;
    if served.as_deref() != Some("1") {
        return Ok(());
    }
    let held: Option<String> = tx
        .query_row(
            "SELECT value FROM meta WHERE key = ?1",
            [TAKEN_OFF_META],
            |r| r.get(0),
        )
        .optional()?;
    let mut list: Vec<Entry> = held
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let was = list.len();
    list.retain(|e| e != entry);
    if off {
        list.push(entry.clone());
        let over = list.len().saturating_sub(TAKEN_OFF_MAX);
        list.drain(..over);
    } else if list.len() == was {
        return Ok(());
    }
    tx.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
        params![
            TAKEN_OFF_META,
            serde_json::to_string(&list).unwrap_or_default()
        ],
    )?;
    Ok(())
}

fn remove_at(tx: &Transaction, all: &[ShelfRow], ix: usize) -> Result<()> {
    note_taken_off(tx, all[ix].wire.as_ref(), true)?;
    tx.execute("DELETE FROM shelf WHERE rowid = ?1", params![all[ix].rowid])?;
    let rest: Vec<i64> = all
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != ix)
        .map(|(_, r)| r.rowid)
        .collect();
    renumber(tx, &rest)
}
