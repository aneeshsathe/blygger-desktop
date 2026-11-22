//! --- agentic feed --- The owner-API reads of the Agentic feed (studio
//! 0.69, contract 6.2.0; shapes in `crate::agentic`). Asked only of a
//! studio whose capabilities list `getAgentic`. Burrow never writes an
//! edition (`putAgentic` is the assistant's).

use super::{Api, enc};
use crate::agentic::{Edition, Feed, parse_edition, parse_feeds};
use crate::backend::{CoreError, Result};

impl Api {
    /// Whether this studio offers editions (its capabilities list
    /// `getAgentic`). Where wave D's `api::offers(caps, op)` fits.
    pub fn serves_agentic(&self) -> Result<bool> {
        self.offers_op(crate::agentic::GET_OPERATION)
    }

    /// `GET /api/agentic`. `Ok(None)`: not offered here.
    pub fn agentic_feeds(&self) -> Result<Option<Vec<Feed>>> {
        if !self.serves_agentic()? {
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

fn unexpected(path: &str) -> CoreError {
    CoreError::Other(format!("unexpected response from {path}"))
}
