//! --- syndication --- The owner-API calls for copies elsewhere (decision
//! #98; the wire shapes are in `crate::syndication`). Asked only of a
//! studio whose capabilities list `putSyndication`.

use super::{Api, is_transient};
use crate::backend::{CoreError, Result};
use crate::syndication::{Answer, SyndicationEntry, parse_answer, put_body, route, serves};

impl Api {
    /// Whether this studio serves syndication writes: its capabilities'
    /// `operations` list `putSyndication`. A studio without capabilities
    /// (404, or not readable) doesn't; a failed read is an error.
    pub fn serves_syndication(&self) -> Result<bool> {
        match self.call("GET", "/api/capabilities", None) {
            Ok(v) => Ok(serves(&v)),
            Err(e) if is_transient(&e) || matches!(e, CoreError::Unauthorized) => Err(e),
            Err(_) => Ok(false),
        }
    }

    /// `GET /api/items/:id/syndication`: the item's list as the blyg keeps
    /// it, with the item's revision.
    pub fn get_syndication(&self, server_id: &str) -> Result<Answer> {
        let v = self.call("GET", &route(server_id), None)?;
        parse_answer(&v).ok_or_else(unexpected)
    }

    /// `PUT /api/items/:id/syndication {entries, expectedRevision}`: rewrite
    /// the item's list in place (no version, no changelog or feed entry).
    /// The answer is the list as kept and the item's new revision.
    pub fn put_syndication(
        &self,
        server_id: &str,
        revision: u64,
        entries: &[SyndicationEntry],
    ) -> Result<Answer> {
        let v = self.call("PUT", &route(server_id), Some(put_body(entries, revision)))?;
        parse_answer(&v).ok_or_else(unexpected)
    }
}

fn unexpected() -> CoreError {
    CoreError::Other("unexpected response from the syndication route".into())
}
