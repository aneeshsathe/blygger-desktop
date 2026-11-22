//! --- helpers --- The studio's helpers that Burrow asks (studio 0.69,
//! contract 6.2.0; `crate::helpers`): the catch-up summary. Also the one
//! capability check studio 0.69's features share ([`Api::offers_op`]).

use super::{Api, is_transient};
use crate::backend::{CoreError, Result};
use crate::helpers::{CatchUpSummary, SUMMARIZE_OPERATION, lists, parse_summary, summary_body};
use crate::shelf::Entry;

impl Api {
    /// Whether this studio's capabilities list `op`. A studio without
    /// capabilities (404, or not readable) doesn't; refused credentials or a
    /// failed read are errors, so nothing passes on one. (Wave D's
    /// `api::offers` is the same rule; this goes once it lands.)
    pub fn offers_op(&self, op: &str) -> Result<bool> {
        match self.call("GET", "/api/capabilities", None) {
            Ok(v) => Ok(lists(&v, op)),
            Err(e) if is_transient(&e) || matches!(e, CoreError::Unauthorized) => Err(e),
            Err(
                e @ CoreError::Rejected {
                    status: 401 | 403, ..
                },
            ) => Err(e),
            Err(_) => Ok(false),
        }
    }

    /// `POST /api/helpers/catch-up-summary {sub, remoteId}`. `Ok(None)`: not
    /// offered here. 409 when the entry quotes nothing or one is being
    /// written; 429/502 when the model can't be asked.
    pub fn summarize_catch_up(&self, entry: &Entry) -> Result<Option<CatchUpSummary>> {
        if !self.offers_op(SUMMARIZE_OPERATION)? {
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
