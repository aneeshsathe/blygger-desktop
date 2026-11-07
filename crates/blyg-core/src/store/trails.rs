//! --- trails --- paths through posts you chose to keep (schema v19). Local
//! only: nothing here is queued in the outbox or sent to the blyg. Positions
//! are dense (0, 1, 2, …) and rewritten on every change, like the shelf's.
//!
//! A stop names a post the way a reading row does, `(subscription_id,
//! remote_id)` (`OWN_SUBSCRIPTION` and the server id for yours). Reading a
//! trail checks each stop against what this Mac holds: a withdrawn post
//! (your item withdrawn, or a reading tombstone) makes the stop a tombstone,
//! and the title kept for it is dropped for good.

use rusqlite::{OptionalExtension, Transaction, params};

use super::Store;
use crate::backend::{CoreError, Result};
use crate::model::OWN_SUBSCRIPTION;
use crate::trails::{TRAIL_MAX_STOPS, Trail, TrailStop};
use crate::util::{new_local_id, now_iso};

fn bad(message: impl Into<String>) -> CoreError {
    CoreError::Rejected {
        status: 400,
        message: message.into(),
        details: vec![],
    }
}

fn stop_keys(tx: &Transaction, trail: &str) -> Result<Vec<(String, String)>> {
    let mut st = tx.prepare_cached(
        "SELECT subscription_id, remote_id FROM trail_stops WHERE trail_id = ?1 \
         ORDER BY position, rowid",
    )?;
    let v = st
        .query_map(params![trail], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

fn renumber(tx: &Transaction, trail: &str, keys: &[(String, String)]) -> Result<()> {
    let mut st = tx.prepare_cached(
        "UPDATE trail_stops SET position = ?1 \
         WHERE trail_id = ?2 AND subscription_id = ?3 AND remote_id = ?4",
    )?;
    for (i, (s, r)) in keys.iter().enumerate() {
        st.execute(params![i as i64, trail, s, r])?;
    }
    Ok(())
}

fn trail_exists(tx: &Transaction, trail: &str) -> Result<()> {
    let n: i64 = tx.query_row(
        "SELECT COUNT(*) FROM trails WHERE id = ?1",
        params![trail],
        |r| r.get(0),
    )?;
    if n == 0 {
        return Err(CoreError::NotFound);
    }
    Ok(())
}

/// Is the post a stop names withdrawn, as far as this Mac knows?
fn is_withdrawn(tx: &Transaction, sub: &str, remote: &str) -> Result<bool> {
    let w = if sub == OWN_SUBSCRIPTION {
        tx.query_row(
            "SELECT status = 'withdrawn' FROM items WHERE server_id = ?1",
            params![remote],
            |r| r.get::<_, bool>(0),
        )
        .optional()?
    } else {
        tx.query_row(
            "SELECT state = 'tombstone' FROM reading \
             WHERE subscription_id = ?1 AND remote_id = ?2",
            params![sub, remote],
            |r| r.get::<_, bool>(0),
        )
        .optional()?
    };
    Ok(w.unwrap_or(false))
}

impl Store {
    /// Every trail, in order, with its stop count.
    pub fn trails(&self) -> Vec<Trail> {
        let c = self.conn();
        let Ok(mut st) = c.prepare_cached(
            "SELECT t.id, t.name, (SELECT COUNT(*) FROM trail_stops s WHERE s.trail_id = t.id) \
             FROM trails t ORDER BY t.position, t.rowid",
        ) else {
            return vec![];
        };
        st.query_map([], |r| {
            Ok(Trail {
                id: r.get(0)?,
                name: r.get(1)?,
                stops: r.get::<_, i64>(2)? as usize,
            })
        })
        .and_then(|it| it.collect())
        .unwrap_or_default()
    }

    /// A new, empty trail at the end. `Rejected{400}` for an empty name.
    pub fn create_trail(&self, name: &str) -> Result<Trail> {
        let name = name.trim();
        if name.is_empty() {
            return Err(bad("A trail needs a name."));
        }
        let id = format!("trail-{}", new_local_id().0);
        let c = self.conn();
        c.execute(
            "INSERT INTO trails (id, name, position, created) \
             VALUES (?1, ?2, (SELECT COALESCE(MAX(position) + 1, 0) FROM trails), ?3)",
            params![id, name, now_iso()],
        )?;
        Ok(Trail {
            id,
            name: name.to_string(),
            stops: 0,
        })
    }

    pub fn rename_trail(&self, id: &str, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(bad("A trail needs a name."));
        }
        let n = self.conn().execute(
            "UPDATE trails SET name = ?2 WHERE id = ?1",
            params![id, name],
        )?;
        if n == 0 {
            return Err(CoreError::NotFound);
        }
        Ok(())
    }

    /// Delete a trail and its stops. The posts are untouched.
    pub fn delete_trail(&self, id: &str) -> Result<()> {
        let mut c = self.conn();
        let tx = c.transaction()?;
        tx.execute("DELETE FROM trail_stops WHERE trail_id = ?1", params![id])?;
        tx.execute("DELETE FROM trails WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(())
    }

    /// A trail's stops in order. A stop whose post is withdrawn comes back
    /// as a tombstone, and the title kept for it is dropped here for good.
    pub fn trail_stops(&self, trail: &str) -> Vec<TrailStop> {
        self.trail_stops_inner(trail).unwrap_or_default()
    }

    fn trail_stops_inner(&self, trail: &str) -> Result<Vec<TrailStop>> {
        let mut c = self.conn();
        let tx = c.transaction()?;
        let rows: Vec<(String, String, String, String, Option<String>)> = {
            let mut st = tx.prepare_cached(
                "SELECT subscription_id, remote_id, origin, why, title FROM trail_stops \
                 WHERE trail_id = ?1 ORDER BY position, rowid",
            )?;
            st.query_map(params![trail], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut out = Vec::with_capacity(rows.len());
        for (sub, remote, origin, why, title) in rows {
            let withdrawn = is_withdrawn(&tx, &sub, &remote)?;
            if withdrawn && title.is_some() {
                tx.execute(
                    "UPDATE trail_stops SET title = NULL \
                     WHERE trail_id = ?1 AND subscription_id = ?2 AND remote_id = ?3",
                    params![trail, sub, remote],
                )?;
            }
            out.push(TrailStop {
                subscription_id: sub,
                remote_id: remote,
                origin,
                why,
                title: if withdrawn { None } else { title },
                withdrawn,
            });
        }
        tx.commit()?;
        Ok(out)
    }

    /// Add a post to the end of a trail, with why it's there; its index.
    /// A post already on the trail keeps its place (a non-empty `why`
    /// replaces the old one). A withdrawn post can't be added.
    pub fn add_trail_stop(&self, trail: &str, stop: &TrailStop) -> Result<usize> {
        let mut c = self.conn();
        let tx = c.transaction()?;
        trail_exists(&tx, trail)?;
        if is_withdrawn(&tx, &stop.subscription_id, &stop.remote_id)? {
            return Err(bad("That post was withdrawn."));
        }
        let keys = stop_keys(&tx, trail)?;
        let key = stop.key();
        let why = stop.why.trim();
        let at = if let Some(i) = keys.iter().position(|k| *k == key) {
            if !why.is_empty() {
                tx.execute(
                    "UPDATE trail_stops SET why = ?4 \
                     WHERE trail_id = ?1 AND subscription_id = ?2 AND remote_id = ?3",
                    params![trail, key.0, key.1, why],
                )?;
            }
            i
        } else {
            if keys.len() >= TRAIL_MAX_STOPS {
                return Err(bad(format!(
                    "A trail holds at most {TRAIL_MAX_STOPS} stops."
                )));
            }
            tx.execute(
                "INSERT INTO trail_stops \
                 (trail_id, position, subscription_id, remote_id, origin, why, title) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    trail,
                    keys.len() as i64,
                    key.0,
                    key.1,
                    stop.origin,
                    why,
                    stop.title
                ],
            )?;
            keys.len()
        };
        tx.commit()?;
        Ok(at)
    }

    /// Change why the stop at `index` is on the trail.
    pub fn set_trail_why(&self, trail: &str, index: usize, why: &str) -> Result<()> {
        let mut c = self.conn();
        let tx = c.transaction()?;
        let keys = stop_keys(&tx, trail)?;
        let (s, r) = keys.get(index).ok_or(CoreError::NotFound)?;
        tx.execute(
            "UPDATE trail_stops SET why = ?4 \
             WHERE trail_id = ?1 AND subscription_id = ?2 AND remote_id = ?3",
            params![trail, s, r, why.trim()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Take the stop at `index` off the trail.
    pub fn remove_trail_stop(&self, trail: &str, index: usize) -> Result<()> {
        let mut c = self.conn();
        let tx = c.transaction()?;
        let mut keys = stop_keys(&tx, trail)?;
        if index >= keys.len() {
            return Err(CoreError::NotFound);
        }
        let (s, r) = keys.remove(index);
        tx.execute(
            "DELETE FROM trail_stops \
             WHERE trail_id = ?1 AND subscription_id = ?2 AND remote_id = ?3",
            params![trail, s, r],
        )?;
        renumber(&tx, trail, &keys)?;
        tx.commit()?;
        Ok(())
    }

    /// Move the stop at `from` to `to` (clamped to the end).
    pub fn move_trail_stop(&self, trail: &str, from: usize, to: usize) -> Result<()> {
        let mut c = self.conn();
        let tx = c.transaction()?;
        let mut keys = stop_keys(&tx, trail)?;
        if from >= keys.len() {
            return Err(CoreError::NotFound);
        }
        let k = keys.remove(from);
        keys.insert(to.min(keys.len()), k);
        renumber(&tx, trail, &keys)?;
        tx.commit()?;
        Ok(())
    }
}
