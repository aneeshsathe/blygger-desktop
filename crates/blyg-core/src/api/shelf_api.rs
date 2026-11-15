//! --- shelf sync --- The owner-API calls for the synced shelf (studio
//! 0.61; the shapes and the merge are in `crate::shelf`). Asked only of a
//! studio whose capabilities list `getShelf` and `putShelf`.

use super::{Api, is_transient};
use crate::backend::{CoreError, Result};
use crate::shelf::{Entry, Remote, parse, put_body, serves};

/// What a `PUT /api/shelf` came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PutShelf {
    /// Stored: the list as the blyg keeps it (unresolved entries dropped).
    Stored(Remote),
    /// 409 `revision-conflict`: the shelf moved elsewhere since it was
    /// read. Nothing was stored.
    Conflict,
}

impl Api {
    /// Whether this studio offers the synced shelf: its capabilities'
    /// `operations` list `getShelf` and `putShelf`. A studio without
    /// capabilities (404, or not readable) doesn't; a failed read is an
    /// error.
    pub fn serves_shelf(&self) -> Result<bool> {
        match self.call("GET", "/api/capabilities", None) {
            Ok(v) => Ok(serves(&v)),
            Err(e) if is_transient(&e) || matches!(e, CoreError::Unauthorized) => Err(e),
            Err(_) => Ok(false),
        }
    }

    /// `GET /api/shelf`. `Ok(None)`: not served here (404).
    pub fn get_shelf(&self) -> Result<Option<Remote>> {
        match Self::optional(self.call("GET", "/api/shelf", None))? {
            Some(v) => parse(&v).map(Some).ok_or_else(unexpected),
            None => Ok(None),
        }
    }

    /// `PUT /api/shelf {entries, expectedRevision}` (at most `SHELF_MAX`
    /// entries go).
    pub fn put_shelf(&self, entries: &[Entry], expected_revision: u64) -> Result<PutShelf> {
        match self.call(
            "PUT",
            "/api/shelf",
            Some(put_body(entries, expected_revision)),
        ) {
            Ok(v) => parse(&v).map(PutShelf::Stored).ok_or_else(unexpected),
            Err(CoreError::RevisionConflict) => Ok(PutShelf::Conflict),
            Err(e) => Err(e),
        }
    }
}

fn unexpected() -> CoreError {
    CoreError::Other("unexpected response from /api/shelf".into())
}
