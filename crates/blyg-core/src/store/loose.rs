//! --- loose ends --- what `is:orphan` and `quotes:<host>` need from the
//! held reading list (`crate::loose`): which of your posts a held post
//! points at, and where each held post comes from. Two index reads.

use std::collections::{HashMap, HashSet};

use super::Store;
use crate::loose::Facts;

impl Store {
    /// The loose-ends facts for a blyg at `own_origin` (your public origin,
    /// as `public_origin` reads it). `inbound` holds the ids (lowercase) of
    /// your posts a held post quotes, stubs or forks (`reading_refs`).
    pub fn loose_facts(&self, own_origin: Option<&str>) -> Facts {
        let c = self.rconn();
        let mut inbound = HashSet::new();
        if let Some(o) = own_origin {
            let origin = crate::model::post_key(o, "").0;
            if let Ok(mut st) = c.prepare_cached(
                "SELECT DISTINCT target_id FROM reading_refs WHERE target_origin = ?1",
            ) {
                inbound = st
                    .query_map([&origin], |r| r.get::<_, String>(0))
                    .map(|it| it.filter_map(|r| r.ok()).collect())
                    .unwrap_or_default();
            }
        }
        let mut held_origins = HashMap::new();
        if let Ok(mut st) = c.prepare_cached("SELECT remote_id, origin FROM reading") {
            held_origins = st
                .query_map([], |r| {
                    Ok((r.get::<_, String>(0)?.to_ascii_lowercase(), r.get(1)?))
                })
                .map(|it| it.filter_map(|r| r.ok()).collect())
                .unwrap_or_default();
        }
        Facts {
            inbound,
            held_origins,
        }
    }
}
