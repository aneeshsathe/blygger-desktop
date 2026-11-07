//! --- shelf --- your own posts kept to hand at the top of the Write list
//! (schema v17), for the living posts you keep updating. Local only: nothing
//! here is queued in the outbox or sent to the blyg (studio has no owner-side
//! field for it yet). Unrelated to pinning a version, which is public and
//! permanent. Positions are dense (0, 1, 2, …) and rewritten on every change.

use rusqlite::{Transaction, params};

use super::Store;
use crate::backend::{CoreError, Result};
use crate::model::LocalId;

/// Every post on the shelf in order, withdrawn ones included (they're filtered
/// when read, so withdrawing and nothing else can't lose a position).
fn ordered_ids(tx: &Transaction) -> Result<Vec<String>> {
    let mut st = tx.prepare_cached("SELECT local_id FROM shelf ORDER BY position, rowid")?;
    let ids = st
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(ids)
}

fn renumber(tx: &Transaction, ids: &[String]) -> Result<()> {
    let mut st = tx.prepare_cached("UPDATE shelf SET position = ?1 WHERE local_id = ?2")?;
    for (i, id) in ids.iter().enumerate() {
        st.execute(params![i as i64, id])?;
    }
    Ok(())
}

impl Store {
    /// The shelf, in your order. A withdrawn post is left out: it can't be
    /// updated any more, so it has no reason to be kept to hand.
    pub fn shelf(&self) -> Vec<LocalId> {
        let c = self.conn();
        let Ok(mut st) = c.prepare_cached(
            "SELECT p.local_id FROM shelf p JOIN items i ON i.local_id = p.local_id \
             WHERE i.status <> 'withdrawn' ORDER BY p.position, p.rowid",
        ) else {
            return vec![];
        };
        st.query_map([], |r| r.get::<_, String>(0).map(LocalId))
            .and_then(|it| it.collect())
            .unwrap_or_default()
    }

    /// Put a post on the shelf (at the end) or take it off. Either, when it's
    /// already so, changes nothing. `NotFound` for a post this store doesn't
    /// hold.
    pub fn set_on_shelf(&self, id: &LocalId, on: bool) -> Result<()> {
        let mut c = self.conn();
        let tx = c.transaction()?;
        let mut ids = ordered_ids(&tx)?;
        let at = ids.iter().position(|x| x == &id.0);
        match (on, at) {
            (true, None) => {
                let exists: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM items WHERE local_id = ?1)",
                    params![id.0],
                    |r| r.get(0),
                )?;
                if !exists {
                    return Err(CoreError::NotFound);
                }
                tx.execute(
                    "INSERT INTO shelf (local_id, position) VALUES (?1, ?2)",
                    params![id.0, ids.len() as i64],
                )?;
            }
            (false, Some(ix)) => {
                tx.execute("DELETE FROM shelf WHERE local_id = ?1", params![id.0])?;
                ids.remove(ix);
                renumber(&tx, &ids)?;
            }
            _ => {}
        }
        tx.commit()?;
        Ok(())
    }

    /// Move a post to `index` on the shelf as `shelf()` lists it (clamped to
    /// the end). `NotFound` if it isn't on the shelf.
    pub fn move_on_shelf(&self, id: &LocalId, index: usize) -> Result<()> {
        let visible = self.shelf();
        let mut c = self.conn();
        let tx = c.transaction()?;
        let mut ids = ordered_ids(&tx)?;
        let Some(from) = ids.iter().position(|x| x == &id.0) else {
            return Err(CoreError::NotFound);
        };
        // `index` counts visible (not withdrawn) posts: land just before the
        // visible post now at that index, else at the very end.
        let anchor = visible
            .iter()
            .filter(|v| v.0 != id.0)
            .nth(index)
            .map(|v| v.0.clone());
        let f = ids.remove(from);
        let to = anchor
            .and_then(|a| ids.iter().position(|x| x == &a))
            .unwrap_or(ids.len());
        ids.insert(to, f);
        renumber(&tx, &ids)?;
        tx.commit()?;
        Ok(())
    }
}
