//! --- agentic feed --- The owner-API reads of the Agentic feed (studio
//! 0.69, contract 6.2.0; shapes in `crate::agentic`). Asked only of a
//! studio whose capabilities list `getAgentic`. --- agentic writer ---
//! Burrow's own AI writes one only where they list `putAgentic`
//! (`crate::agentic_write`).

use super::{Api, enc};
use crate::agentic::{Edition, Feed, parse_edition, parse_feeds};
use crate::agentic_write::{EditionInput, Put, put_body};
use crate::backend::{CoreError, Result};

impl Api {
    /// Whether this studio offers editions (its capabilities list
    /// `getAgentic`). Where wave D's `api::offers(caps, op)` fits.
    pub fn serves_agentic(&self) -> Result<bool> {
        self.offers_all(&[crate::agentic::GET_OPERATION])
    }

    /// `GET /api/agentic`. `Ok(None)`: not offered here.
    pub fn agentic_feeds(&self) -> Result<Option<Vec<Feed>>> {
        if !self.offers_all(&[crate::agentic::LIST_OPERATION])? {
            return Ok(None);
        }
        match Self::optional(self.call("GET", "/api/agentic", None))? {
            Some(v) => parse_feeds(&v)
                .map(Some)
                .ok_or_else(|| unexpected("/api/agentic")),
            None => Ok(None),
        }
    }

    /// `GET /api/agentic/{feed}`. `Ok(None)`: not offered, or no such feed.
    pub fn agentic_edition(&self, feed: &str) -> Result<Option<Edition>> {
        if !self.serves_agentic()? {
            return Ok(None);
        }
        let path = format!("/api/agentic/{}", enc(feed));
        match Self::optional(self.call("GET", &path, None))? {
            Some(v) => parse_edition(&v).map(Some).ok_or_else(|| unexpected(&path)),
            None => Ok(None),
        }
    }
}

impl Api {
    /// --- agentic writer --- Whether this sign-in may write an edition
    /// (the capabilities list `putAgentic`).
    pub fn serves_agentic_write(&self) -> Result<bool> {
        self.offers_all(&[crate::agentic::PUT_OPERATION])
    }

    /// `PUT /api/agentic/{feed} {edition, expectedRevision}`.
    pub fn put_agentic(
        &self,
        feed: &str,
        expected_revision: u64,
        edition: &EditionInput,
    ) -> Result<Put> {
        if !self.serves_agentic_write()? {
            return Ok(Put::NotOffered);
        }
        let path = format!("/api/agentic/{}", enc(feed));
        match self.call("PUT", &path, Some(put_body(edition, expected_revision))) {
            Ok(v) => parse_edition(&v)
                .map(Put::Written)
                .ok_or_else(|| unexpected(&path)),
            Err(CoreError::RevisionConflict) => Ok(Put::Conflict),
            Err(CoreError::NotFound) => Ok(Put::NotOffered),
            Err(e) => Err(e),
        }
    }
}

fn unexpected(path: &str) -> CoreError {
    CoreError::Other(format!("unexpected response from {path}"))
}
