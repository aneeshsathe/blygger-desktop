//! The `lineage-glyph` studio extension's owner reads (blygger-studio PR #53,
//! `extensions/lineage-glyph/contract.ts` at dc632c5): `GET
//! /api/ext/lineage-glyph/summaries` and `/lineage`, scope `owner:read`.
//! Both answer 404 on a node that doesn't compile the extension in or hasn't
//! enabled it: `Ok(None)` here, which the caller remembers as "not served".

use std::collections::HashMap;

use super::{Api, enc};
use crate::backend::Result;
use crate::lineage::{Centre, LineageGraph, LineageSummary, MAX_SUMMARY_KEYS, SummariesPage};

impl Api {
    /// Glyph counts for up to [`MAX_SUMMARY_KEYS`] reading entry keys
    /// (`crate::lineage::imported_key`). Unknown keys are left out of the
    /// answer. `None` on 404: the extension isn't enabled there.
    pub fn lineage_summaries(
        &self,
        keys: &[String],
    ) -> Result<Option<HashMap<String, LineageSummary>>> {
        let keys = &keys[..keys.len().min(MAX_SUMMARY_KEYS)];
        let json = serde_json::to_string(keys).unwrap_or_else(|_| "[]".into());
        let path = format!("/api/ext/lineage-glyph/summaries?keys={}", enc(&json));
        let page: Option<SummariesPage> = Self::optional(self.call_as("GET", &path, None))?;
        Ok(page.map(|p| p.summaries))
    }

    /// One hop of lineage around `centre`. `None` on 404: the extension
    /// isn't enabled there (or, for an imported post, its subscription is
    /// unknown).
    pub fn lineage_graph(&self, centre: &Centre) -> Result<Option<LineageGraph>> {
        let path = format!("/api/ext/lineage-glyph/lineage?{}", centre.query());
        Self::optional(self.call_as("GET", &path, None))
    }
}
