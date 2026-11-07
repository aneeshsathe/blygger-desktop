//! --- upkeep --- Stale quotes, your signals and activity log, and one
//! version's text (`crate::upkeep`). Every route here is in contract 5.0
//! (studio 0.49.12, Burrow's floor), so nothing is gated on capabilities;
//! a 404 still reads as "not served" where that makes sense.

use serde_json::json;

use super::{Api, enc};
use crate::backend::Result;
use crate::upkeep::{Interaction, Page, Refreshed, ThreadFreshness, Thumb};

/// How many rows a signals page asks for (Studio's page size).
pub const SIGNALS_PAGE: u32 = 50;

/// One version's words, as `GET /items/{id}/versions/{v}` serves them.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionText {
    pub version: u32,
    /// "fragment", "thread" or "withdrawn" (an endcap: no words).
    pub kind: String,
    #[serde(default)]
    pub content_md: String,
}

impl Api {
    /// `GET /api/freshness`: every published thread with a stale or
    /// blocking quote, stalest first. `None` on 404.
    pub fn freshness(&self) -> Result<Option<Vec<ThreadFreshness>>> {
        #[derive(serde::Deserialize)]
        struct R {
            items: Vec<ThreadFreshness>,
        }
        Ok(Self::optional(self.call_as::<R>("GET", "/api/freshness", None))?.map(|r| r.items))
    }

    /// `GET /api/items/{id}/freshness[?probe=false]`: one thread's quotes,
    /// asking their origins for their newest version unless `probe` is false.
    pub fn item_freshness(&self, id: &str, probe: bool) -> Result<ThreadFreshness> {
        let q = if probe { "" } else { "?probe=false" };
        self.call_as("GET", &format!("/api/items/{}/freshness{q}", enc(id)), None)
    }

    /// `POST /api/items/{id}/refresh {expectedRevision, note?}`: republish
    /// the thread's published words with its stale quotes re-baked.
    pub fn refresh_quotes(&self, id: &str, revision: u64, note: Option<&str>) -> Result<Refreshed> {
        let mut body = json!({ "expectedRevision": revision });
        if let Some(n) = note.map(str::trim).filter(|n| !n.is_empty()) {
            body["note"] = json!(n);
        }
        self.call_as(
            "POST",
            &format!("/api/items/{}/refresh", enc(id)),
            Some(body),
        )
    }

    /// `GET /api/items/{id}/versions/{v}`: one of your versions, with its words.
    pub fn item_version(&self, id: &str, version: u32) -> Result<VersionText> {
        self.call_as(
            "GET",
            &format!("/api/items/{}/versions/{version}", enc(id)),
            None,
        )
    }

    /// `GET /api/thumbs?thumb=1|-1`, one page: your likes or dislikes.
    pub fn thumbs(&self, thumb: i8, cursor: Option<&str>) -> Result<Page<Thumb>> {
        let p = self.page::<Thumb>(
            "/api/thumbs",
            &format!("thumb={thumb}"),
            SIGNALS_PAGE,
            cursor,
        )?;
        Ok(Page {
            items: p.items,
            next: p.page.next_cursor,
        })
    }

    /// `GET /api/interactions`, one page: your private activity log.
    pub fn interactions(&self, cursor: Option<&str>) -> Result<Page<Interaction>> {
        let p = self.page::<Interaction>("/api/interactions", "", SIGNALS_PAGE, cursor)?;
        Ok(Page {
            items: p.items,
            next: p.page.next_cursor,
        })
    }
}
