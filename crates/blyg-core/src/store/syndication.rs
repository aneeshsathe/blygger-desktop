//! --- syndication --- Copies elsewhere (schema v21, `crate::syndication`).
//!
//! Your own posts: `items.syndication` is the list as you last left it, or
//! as last read from the blyg (`syndication_fetched`: the owner API keeps it
//! as its own resource, read when the Copies sheet opens).
//! `set_syndication` writes it here at once and queues one `syndication`
//! op per post (the last edit replaces a waiting one), which the sync
//! worker sends as an in-place rewrite: no version, nothing published. A
//! pull takes the blyg's list only for a post with no write waiting, so an
//! edit made here never flickers back.
//!
//! Imported reading rows: `reading.syndication` is what the blyg served
//! with the row, written after each reading merge (`set_reading_syndication`).
//! Nothing is fetched from the copies.

use rusqlite::{OptionalExtension, params};

use super::{OpKind, Store};
use crate::api::wire::WireItem;
use crate::backend::{CoreError, Result};
use crate::model::LocalId;
use crate::syndication::{
    SyndicationEntry, SyndicationOp, from_column, normalize, op_key, to_column,
};
use crate::util::now_ms;

/// One reading row's served list: (subscription id, remote id, entries).
pub type ReadingSyndication = (String, String, Vec<SyndicationEntry>);

impl Store {
    /// Your post's copies elsewhere, as last left here. Local, instant.
    pub fn syndication(&self, id: &LocalId) -> Vec<SyndicationEntry> {
        let c = self.rconn();
        let s: Option<Option<String>> = c
            .prepare_cached("SELECT syndication FROM items WHERE local_id = ?1")
            .and_then(|mut st| st.query_row([&id.0], |r| r.get(0)).optional())
            .ok()
            .flatten();
        from_column(s.flatten().as_deref())
    }

    /// Replace your post's list (normalized) and queue its write. Only a
    /// post the blyg holds (it has a server id) has copies to list.
    /// Returns the list as kept.
    pub fn set_syndication(
        &self,
        id: &LocalId,
        entries: &[SyndicationEntry],
    ) -> Result<Vec<SyndicationEntry>> {
        let entries = normalize(entries);
        let mut c = self.conn();
        let tx = c.transaction()?;
        let sid: Option<Option<String>> = tx
            .query_row(
                "SELECT server_id FROM items WHERE local_id = ?1",
                [&id.0],
                |r| r.get(0),
            )
            .optional()?;
        let Some(sid) = sid.flatten() else {
            return Err(CoreError::Other(
                "Publish the post first: only a published post has copies elsewhere".into(),
            ));
        };
        tx.execute(
            "UPDATE items SET syndication = ?2 WHERE local_id = ?1",
            params![id.0, to_column(&entries)],
        )?;
        let key = op_key(id);
        tx.execute(
            "DELETE FROM outbox WHERE op = 'syndication' AND local_id = ?1 AND in_flight = 0",
            [&key.0],
        )?;
        let op = SyndicationOp {
            local_id: id.0.clone(),
            server_id: sid,
            entries: entries.clone(),
        };
        tx.execute(
            "INSERT INTO outbox (local_id, op, payload, not_before) VALUES (?1, ?2, ?3, ?4)",
            params![
                key.0,
                OpKind::Syndication.as_str(),
                serde_json::to_string(&op).unwrap_or_default(),
                now_ms()
            ],
        )?;
        tx.commit()?;
        Ok(entries)
    }

    /// A write was answered (`seq` done): keep the blyg's normalized list,
    /// unless a newer edit is already waiting.
    pub fn syndication_sent(
        &self,
        seq: i64,
        id: &LocalId,
        answered: &[SyndicationEntry],
    ) -> Result<()> {
        let mut c = self.conn();
        let tx = c.transaction()?;
        tx.execute("DELETE FROM outbox WHERE seq = ?1", [seq])?;
        if !waiting(&tx, id)? {
            tx.execute(
                "UPDATE items SET syndication = ?2 WHERE local_id = ?1",
                params![id.0, to_column(&normalize(answered))],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The blyg's list as just read (`GET …/syndication`): kept unless an
    /// edit waits.
    pub fn syndication_fetched(&self, id: &LocalId, entries: &[SyndicationEntry]) -> Result<bool> {
        let mut c = self.conn();
        let tx = c.transaction()?;
        if waiting(&tx, id)? {
            return Ok(false);
        }
        let n = tx.execute(
            "UPDATE items SET syndication = ?2 WHERE local_id = ?1 AND syndication IS NOT ?2",
            params![id.0, to_column(&normalize(entries))],
        )?;
        tx.commit()?;
        Ok(n > 0)
    }

    /// After a pull's merge of your items: take each item's served list
    /// (an item that doesn't carry the member keeps what's held; a
    /// withdrawn one has none), except where an edit waits. True when
    /// anything changed.
    pub fn merge_wire_syndication(&self, wires: &[WireItem]) -> Result<bool> {
        let mut c = self.conn();
        let tx = c.transaction()?;
        let mut changed = false;
        for w in wires {
            let served = if w.status == "withdrawn" {
                Some(vec![])
            } else {
                w.syndication_entries()
            };
            let Some(served) = served else { continue };
            let row: Option<(String, Option<String>)> = tx
                .prepare_cached("SELECT local_id, syndication FROM items WHERE server_id = ?1")?
                .query_row([&w.id], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?;
            let Some((local, held)) = row else { continue };
            let local = LocalId(local);
            if waiting(&tx, &local)? {
                continue;
            }
            let new = to_column(&served);
            if new != held {
                tx.execute(
                    "UPDATE items SET syndication = ?2 WHERE local_id = ?1",
                    params![local.0, new],
                )?;
                changed = true;
            }
        }
        tx.commit()?;
        Ok(changed)
    }

    /// An imported post's copies elsewhere, as its blyg served them.
    pub fn reading_syndication(&self, sub: &str, remote_id: &str) -> Vec<SyndicationEntry> {
        let c = self.rconn();
        let s: Option<Option<String>> = c
            .prepare_cached(
                "SELECT syndication FROM reading WHERE subscription_id = ?1 AND remote_id = ?2",
            )
            .and_then(|mut st| st.query_row([sub, remote_id], |r| r.get(0)).optional())
            .ok()
            .flatten();
        from_column(s.flatten().as_deref())
    }

    /// After a reading merge: each pulled row's served list (`None`: the
    /// row didn't carry the member, so it has none). A tombstone keeps
    /// none (§16.14 rule 4). True when anything changed.
    pub fn set_reading_syndication(&self, rows: &[ReadingSyndication]) -> Result<bool> {
        if rows.is_empty() {
            return Ok(false);
        }
        let mut c = self.conn();
        let tx = c.transaction()?;
        let mut changed = 0;
        for (sub, rid, entries) in rows {
            changed += tx
                .prepare_cached(
                    "UPDATE reading SET syndication = ?3 WHERE subscription_id = ?1 \
                     AND remote_id = ?2 AND syndication IS NOT ?3",
                )?
                .execute(params![sub, rid, to_column(entries)])?;
        }
        tx.commit()?;
        Ok(changed > 0)
    }
}

/// A write of `id`'s list is waiting (not yet answered).
fn waiting(c: &rusqlite::Connection, id: &LocalId) -> rusqlite::Result<bool> {
    c.prepare_cached(
        "SELECT EXISTS(SELECT 1 FROM outbox WHERE op = 'syndication' AND local_id = ?1)",
    )?
    .query_row([&op_key(id).0], |r| r.get(0))
}

#[cfg(test)]
#[path = "syndication_tests.rs"]
mod tests;
