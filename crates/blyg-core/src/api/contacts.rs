//! --- contacts --- studio 0.56's owner address book and mentions lens
//! (contract 5.7.0; `crate::contacts`). Both are asked only of a studio
//! whose capabilities say 5.7 or later; an older one is never sent the
//! route or the parameter.

use serde_json::{Value, json};

use super::{Api, enc};
use crate::backend::{CoreError, Result};
use crate::contacts::{Contact, ContactInput, parse_contacts, parse_reading_keys, serves_contacts};
use crate::outbound::{MentionGroup, MentionSource, MentionSummary, OutboundMention};

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
            // Studio 0.56 answers the whole book, unpaged; a later one may
            // page it.
            let path = match &cursor {
                Some(c) => format!("/api/contacts?cursor={}", enc(c)),
                None => "/api/contacts".to_string(),
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

// --- contacts --- managing the book, and the Mentions screen's other reads
// (`crate::outbound`).
impl Api {
    fn contact_reply(v: Value, path: &str) -> Result<Contact> {
        crate::contacts::parse_contact(&v)
            .ok_or_else(|| CoreError::Other(format!("unexpected response from {path}")))
    }

    /// `POST /api/contacts {handle, name, url}`. A handle already in the
    /// book is a 409 (`Rejected`), named by the studio.
    pub fn create_contact(&self, c: &ContactInput) -> Result<Contact> {
        let c = c.tidy();
        let path = "/api/contacts";
        let v = self.call(
            "POST",
            path,
            Some(json!({ "handle": c.handle, "name": c.name, "url": c.url })),
        )?;
        Self::contact_reply(v, path)
    }

    /// `PATCH /api/contacts/{id} {handle, name, url}`.
    pub fn update_contact(&self, id: &str, c: &ContactInput) -> Result<Contact> {
        let c = c.tidy();
        let path = format!("/api/contacts/{}", enc(id));
        let v = self.call(
            "PATCH",
            &path,
            Some(json!({ "handle": c.handle, "name": c.name, "url": c.url })),
        )?;
        Self::contact_reply(v, &path)
    }

    /// `DELETE /api/contacts/{id}`. One already gone is fine.
    pub fn delete_contact(&self, id: &str) -> Result<()> {
        match self.call("DELETE", &format!("/api/contacts/{}", enc(id)), None) {
            Ok(_) | Err(CoreError::NotFound) => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// `POST /api/contacts/{id}/resolve`: the studio reads the contact's
    /// page and keeps the `rel="me"` identifiers it lists. A page it can't
    /// read is a 502 (`Rejected`) naming why.
    pub fn resolve_contact(&self, id: &str) -> Result<Contact> {
        let path = format!("/api/contacts/{}/resolve", enc(id));
        let v = self.call("POST", &path, None)?;
        Self::contact_reply(v, &path)
    }

    /// `GET /api/mentions?direction=outbound`, every page. `Ok(None)`: not
    /// served.
    pub fn outbound_mentions(&self) -> Result<Option<Vec<OutboundMention>>> {
        Self::optional(self.collect("/api/mentions", "direction=outbound"))
    }

    /// Whether this studio serves `/mention-groups` and `/mentions/summary`.
    fn serves_groups(&self) -> Result<bool> {
        Ok(self
            .contract_version()?
            .as_deref()
            .is_some_and(crate::outbound::serves_groups))
    }

    /// `GET /api/mentions/summary`. `Ok(None)`: not served (before 5.2).
    pub fn mention_summary(&self) -> Result<Option<MentionSummary>> {
        if !self.serves_groups()? {
            return Ok(None);
        }
        Self::optional(self.call_as("GET", "/api/mentions/summary", None))
    }

    /// `GET /api/mention-groups`, every page, newest response first.
    /// `Ok(None)`: not served (before 5.2).
    pub fn mention_groups(&self) -> Result<Option<Vec<MentionGroup>>> {
        if !self.serves_groups()? {
            return Ok(None);
        }
        Self::optional(self.collect("/api/mention-groups", ""))
    }

    /// `GET /api/mentions/{id}/source`. `Ok(None)`: no such mention, or not
    /// served.
    pub fn mention_source(&self, id: &str) -> Result<Option<MentionSource>> {
        let v =
            Self::optional(self.call("GET", &format!("/api/mentions/{}/source", enc(id)), None))?;
        Ok(v.as_ref().map(MentionSource::parse))
    }
}
// --- end contacts ---

fn next_cursor(v: &Value) -> Option<String> {
    v.get("page")?
        .get("nextCursor")?
        .as_str()
        .filter(|c| !c.is_empty())
        .map(str::to_string)
}
