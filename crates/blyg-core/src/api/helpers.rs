//! --- helpers --- The studio's helpers that Burrow asks (studio 0.69,
//! contract 6.2.0; `crate::helpers`): the catch-up summary, gated on
//! `summarizeCatchUp` ([`Api::offers_all`]).

use super::Api;
use crate::backend::{CoreError, Result};
use crate::helpers::{CatchUpSummary, SUMMARIZE_OPERATION, parse_summary, summary_body};
use crate::shelf::Entry;

impl Api {
    /// `POST /api/helpers/catch-up-summary {sub, remoteId}`. `Ok(None)`: not
    /// offered here. 409 when the entry quotes nothing or one is being
    /// written; 429/502 when the model can't be asked.
    pub fn summarize_catch_up(&self, entry: &Entry) -> Result<Option<CatchUpSummary>> {
        if !self.offers_all(&[SUMMARIZE_OPERATION])? {
            return Ok(None);
        }
        let path = "/api/helpers/catch-up-summary";
        match Self::optional(self.call("POST", path, Some(summary_body(entry))))? {
            Some(v) => parse_summary(&v)
                .map(Some)
                .ok_or_else(|| CoreError::Other(format!("unexpected response from {path}"))),
            None => Ok(None),
        }
    }
}
