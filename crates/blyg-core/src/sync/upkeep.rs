//! --- upkeep --- Stale quotes, kept between asks: `GET /freshness`'s last
//! answer lives in `meta` (no migration), so the list's marker and
//! `is:stale-quote` read it instantly. A pull asks again whenever your
//! items or your reading moved (`FRESHNESS_DEPS`); the Stale quotes sheet
//! asks when it opens, and a refresh drops the thread it refreshed.

use super::Engine;
use crate::backend::Result;
use crate::upkeep::{ThreadFreshness, sort_stalest_first};

/// `meta` key: the stale threads as last fetched (JSON).
pub const FRESHNESS_META: &str = "upkeep.freshness";

/// What a stale quote depends on, by `/api/changes` domain: your items (a
/// republish), the imports (a source's new version) and the subscriptions.
pub(super) const FRESHNESS_DEPS: &[&str] = &["items", "reading", "subscriptions"];

impl Engine {
    /// The stale threads as last fetched, stalest first.
    pub fn cached_freshness(&self) -> Vec<ThreadFreshness> {
        self.store
            .meta(FRESHNESS_META)
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default()
    }

    /// Remember `list` (sorted stalest first). True when it changed what
    /// was held, so the list's markers are drawn again.
    pub fn keep_freshness(&self, mut list: Vec<ThreadFreshness>) -> Result<bool> {
        sort_stalest_first(&mut list);
        if self.cached_freshness() == list {
            return Ok(false);
        }
        let json = serde_json::to_string(&list).unwrap_or_else(|_| "[]".into());
        self.store.set_meta(FRESHNESS_META, &json)?;
        Ok(true)
    }

    /// `GET /freshness`, kept. `Ok(None)`: the blyg doesn't serve it.
    pub fn fetch_freshness(&self) -> Result<Option<(Vec<ThreadFreshness>, bool)>> {
        let Some(list) = self.api.freshness()? else {
            return Ok(None);
        };
        let changed = self.keep_freshness(list)?;
        Ok(Some((self.cached_freshness(), changed)))
    }
}
