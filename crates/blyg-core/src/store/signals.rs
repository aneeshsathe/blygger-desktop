//! Signals (thumbs) on held reading rows, from `GET /api/signals`.
//!
//! The reading list's pages carry each row's `thumb`, but re-reading the
//! whole list because one thumb moved is what issue #71 is about: a pull
//! that sees only the `signals` revision move reads the (short) signals
//! list instead, and lays it over the rows held here.

use std::collections::HashMap;

use rusqlite::params;

use super::Store;
use crate::backend::{CoreError, Result};
use crate::model::ReadingItem;

impl Store {
    /// Make every held reading row's thumb what `signals` says: the listed
    /// rows get theirs, every other row none. `signals` must be the whole
    /// list. Signals on posts not held here are ignored. True when a row
    /// changed.
    pub fn replace_thumbs(&self, signals: &[(String, String, i8)]) -> Result<bool> {
        let want: HashMap<(&str, &str), i8> = signals
            .iter()
            .map(|(s, r, t)| ((s.as_str(), r.as_str()), *t))
            .collect();
        let mut c = self.conn();
        let tx = c.transaction()?;
        let rows: Vec<(String, String, String)> = {
            let mut st = tx.prepare("SELECT subscription_id, remote_id, json FROM reading")?;
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<rusqlite::Result<_>>()?
        };
        let mut changed = false;
        for (sub, rid, json) in rows {
            let Ok(mut it) = serde_json::from_str::<ReadingItem>(&json) else {
                continue;
            };
            let thumb = want.get(&(sub.as_str(), rid.as_str())).copied();
            if it.thumb == thumb {
                continue;
            }
            it.thumb = thumb;
            it.read_version = None; // kept in `read_state`, never in the json
            let json = serde_json::to_string(&it).map_err(|e| CoreError::Storage(e.to_string()))?;
            tx.execute(
                "UPDATE reading SET json = ?3 WHERE subscription_id = ?1 AND remote_id = ?2",
                params![sub, rid, json],
            )?;
            changed = true;
        }
        tx.commit()?;
        Ok(changed)
    }
}
