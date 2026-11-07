//! --- contacts --- studio 0.56's owner address book and mentions lens
//! (contract 5.7.0; `crate::contacts`). Both are asked only of a studio
//! whose capabilities say 5.7 or later; an older one is never sent the
//! route or the parameter.

use serde_json::Value;

use super::{Api, enc};
use crate::backend::{CoreError, Result};
use crate::contacts::{Contact, parse_contacts, parse_reading_keys, serves_contacts};

/// Pages read at most, so a misbehaving cursor can't loop forever.
const MAX_PAGES: usize = 50;

impl Api {
    /// Whether this studio serves contacts (asks its capabilities).
    pub fn serves_contacts(&self) -> Result<bool> {
        Ok(self
            .contract_version()?
            .as_deref()
            .is_some_and(serves_contacts))
    }

    /// `GET /api/contacts`, every page. `Ok(None)`: not served (a contract
    /// before 5.7, or 404).
    pub fn contacts(&self) -> Result<Option<Vec<Contact>>> {
        if !self.serves_contacts()? {
            return Ok(None);
        }
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let path = match &cursor {
                Some(c) => format!("/api/contacts?limit=100&cursor={}", enc(c)),
                None => "/api/contacts?limit=100".to_string(),
            };
            let v = match Self::optional(self.call("GET", &path, None))? {
                Some(v) => v,
                None => return Ok(None),
            };
            out.extend(parse_contacts(&v));
            cursor = next_cursor(&v);
            if cursor.is_none() {
                break;
            }
        }
        Ok(Some(out))
    }

    /// `GET /api/reading?mentions=true`: `(subscription id, remote id)` of
    /// the held posts that mention you. `Ok(None)`: not served.
    pub fn reading_mentions(&self) -> Result<Option<Vec<(String, String)>>> {
        if !self.serves_contacts()? {
            return Ok(None);
        }
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut path = "/api/reading?mentions=true&limit=50".to_string();
            if let Some(c) = &cursor {
                path.push_str(&format!("&cursor={}", enc(c)));
            }
            let v = match self.call("GET", &path, None) {
                Ok(v) => v,
                Err(CoreError::NotFound | CoreError::Rejected { status: 400, .. }) => {
                    return Ok(None);
                }
                Err(e) => return Err(e),
            };
            out.extend(parse_reading_keys(&v));
            cursor = next_cursor(&v);
            if cursor.is_none() {
                break;
            }
        }
        Ok(Some(out))
    }
}

fn next_cursor(v: &Value) -> Option<String> {
    v.get("page")?
        .get("nextCursor")?
        .as_str()
        .filter(|c| !c.is_empty())
        .map(str::to_string)
}
