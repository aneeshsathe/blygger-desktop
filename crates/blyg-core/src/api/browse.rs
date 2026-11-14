//! --- browse --- The studio 0.49 owner routes for following links out of an
//! entry (`crate::browse`): `peekItem`, `classifyLinks`, `listBrowse`,
//! `logBrowse`, `clearBrowse`.
//!
//! Every path and envelope these use is in this file. The routes are under
//! the blyg's mount (`{blyg}/api/…`), and the log is cursor-paged (studio
//! contract 5). A 404 is `Ok(None)` here, which the caller remembers as
//! "not served".

use serde::Deserialize;
use serde_json::{Value, json};

use super::{Api, enc};
use crate::backend::{CoreError, Result};
use crate::browse::{BrowseStep, BrowseVia, ItemRef, LinkTarget, NewBrowseStep};

/// The routes, relative to the API root.
mod path {
    pub const PEEK: &str = "/api/peek";
    pub const PEEK_LINKS: &str = "/api/peek/links";
    pub const BROWSE: &str = "/api/browse";
}

/// The server's limit on one `POST /api/peek/links` (upstream `CLASSIFY_MAX`);
/// the caller's longer lists are sent in chunks.
pub const PEEK_LINKS_MAX: usize = crate::browse::CLASSIFY_MAX;

/// The server's limit on one `GET /api/browse` page (contract 5's
/// maximum for every paged list but Reading).
pub const BROWSE_PAGE_MAX: usize = 100;

/// The server's limit on a step's title (`logBrowse`'s schema).
pub const BROWSE_TITLE_MAX: usize = 400;

/// `GET /api/peek`: an item read from its own blyg without subscribing
/// (upstream `PeekItem`). `content_html` is sanitized by the server.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PeekItem {
    pub origin: String,
    pub id: String,
    /// `fragment` | `thread` | `withdrawn`.
    pub kind: String,
    pub version: u32,
    #[serde(default)]
    pub content_html: String,
    #[serde(default)]
    pub page: Option<String>,
    /// Where the item reads on its own site.
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub stub_of: Option<Value>,
    #[serde(default)]
    pub forked_from: Option<Value>,
    /// The subscription holding this origin, when there is one.
    #[serde(default)]
    pub subscription_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireTarget {
    origin: String,
    id: String,
}

#[derive(Debug, Deserialize)]
struct WireLink {
    url: String,
    #[serde(default)]
    target: Option<WireTarget>,
}

#[derive(Debug, Deserialize)]
struct WireLinks {
    links: Vec<WireLink>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireStep {
    id: i64,
    at: String,
    trail: String,
    #[serde(default)]
    parent_id: Option<i64>,
    origin: String,
    remote_id: String,
    #[serde(default)]
    version: Option<u32>,
    via: String,
    #[serde(default)]
    title: Option<String>,
}

impl WireStep {
    fn into_step(self) -> Option<BrowseStep> {
        Some(BrowseStep {
            id: self.id,
            at: self.at,
            trail: self.trail,
            parent_id: self.parent_id,
            origin: self.origin,
            remote_id: self.remote_id,
            version: self.version,
            via: BrowseVia::parse(&self.via)?,
            title: self.title,
        })
    }
}

/// One page of the browsing log, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowsePage {
    pub items: Vec<BrowseStep>,
    /// The next page's cursor; `None` on the last page.
    pub next: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Created {
    id: i64,
}

/// A title cut to the server's limit (code points), on a grapheme cluster
/// boundary (§5.3).
pub fn clip_title(t: &str) -> String {
    crate::text::prefix_graphemes(t, BROWSE_TITLE_MAX).to_string()
}

impl Api {
    /// `GET /api/peek?origin&id`. `None` on 404, which is both "a studio
    /// before 0.49" and "that blyg doesn't serve this item": callers that
    /// need to tell them apart don't use this (LiveBackend reads item
    /// documents itself, `public_item`).
    pub fn peek_item(&self, origin: &str, id: &str) -> Result<Option<PeekItem>> {
        let p = format!("{}?origin={}&id={}", path::PEEK, enc(origin), enc(id));
        Self::optional(self.call_as("GET", &p, None))
    }

    /// `POST /api/peek/links {urls}` → which name a blyg item, at most
    /// [`PEEK_LINKS_MAX`] per call (more are cut). The server answers only
    /// the distinct http(s) URLs; the rest aren't in the answer. `None` on
    /// 404 (a studio before 0.49).
    pub fn peek_links(&self, urls: &[String]) -> Result<Option<Vec<LinkTarget>>> {
        let urls = &urls[..urls.len().min(PEEK_LINKS_MAX)];
        let got: Option<WireLinks> =
            Self::optional(self.call_as("POST", path::PEEK_LINKS, Some(json!({ "urls": urls }))))?;
        Ok(got.map(|g| {
            g.links
                .into_iter()
                .map(|l| LinkTarget {
                    url: l.url,
                    target: l.target.map(|t| ItemRef {
                        origin: t.origin,
                        id: t.id,
                    }),
                })
                .collect()
        }))
    }

    /// `GET /api/browse?limit[&trail][&cursor]`: one page of the browsing
    /// log, newest first; pass the page's `next` back as `cursor` for the
    /// one after. A step with a `via` this client doesn't know is skipped.
    /// `None` on 404.
    pub fn list_browse(
        &self,
        trail: Option<&str>,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<Option<BrowsePage>> {
        let limit = limit.clamp(1, BROWSE_PAGE_MAX) as u32;
        let query = trail.map_or_else(String::new, |t| format!("trail={}", enc(t)));
        let got: Option<super::wire::CursorPage<WireStep>> =
            Self::optional(self.page(path::BROWSE, &query, limit, cursor))?;
        Ok(got.map(|g| BrowsePage {
            items: g
                .items
                .into_iter()
                .filter_map(WireStep::into_step)
                .collect(),
            next: g.page.next_cursor,
        }))
    }

    /// `POST /api/browse` → 201 `{id}`. Needs `reading:state` (403
    /// otherwise). `None` on 404.
    pub fn log_browse(&self, s: &NewBrowseStep) -> Result<Option<i64>> {
        let body = json!({
            "trail": s.trail,
            "parentId": s.parent_id,
            "origin": s.origin,
            "remoteId": s.remote_id,
            "version": s.version.filter(|v| *v > 0),
            "via": s.via.as_str(),
            "title": s.title.as_deref().map(clip_title),
        });
        let got: Option<Created> = Self::optional(self.call_as("POST", path::BROWSE, Some(body)))?;
        Ok(got.map(|c| c.id))
    }

    /// `DELETE /api/browse[?trail]`: one trail, or the whole log. Needs
    /// `reading:state`. `None` on 404.
    pub fn clear_browse(&self, trail: Option<&str>) -> Result<Option<()>> {
        let p = match trail {
            Some(t) => format!("{}?trail={}", path::BROWSE, enc(t)),
            None => path::BROWSE.to_string(),
        };
        Self::optional(self.call("DELETE", &p, None).map(|_| ()))
    }
}

/// A 403: the credential lacks the scope (`reading:state` for browse writes).
pub fn is_forbidden(e: &CoreError) -> bool {
    matches!(e, CoreError::Rejected { status: 403, .. })
}
