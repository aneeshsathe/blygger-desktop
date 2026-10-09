//! The `burrow/*` item and reading methods, answered from a [`Backend`].
//! Every method checks its capability first. Nothing here publishes,
//! deletes or touches the network: the methods map to `Backend`'s local
//! calls only (`items`, `search`, `item`, `create_draft`, `create_scratch`,
//! `save`, `save_if_base`, `reading`).
//!
//! `burrow/toast`, `burrow/openItem` and `burrow/requestCapability` are UI
//! hops and are answered by the host itself ([`crate::host`]).

use std::sync::Arc;

use blyg_core::{
    Backend, CoreError, Kind, LocalId, STALE_MESSAGE, Status, content_hash, promotion_kind,
};
use serde_json::{Value, json};

use crate::capability::Capability;
use crate::protocol::*;
use crate::rpc::{params, to_value};

/// What the host answers extension requests with. [`BackendApi`] is the
/// real one; [`NoBlyg`] answers when no blyg is connected.
pub trait HostApi: Send + Sync {
    /// Answer `method` for extension `ext`, which holds `granted`.
    fn call(
        &self,
        ext: &str,
        granted: &[Capability],
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError>;

    /// The blyg's origin, for `blyg.identity`.
    fn blyg_origin(&self) -> Option<String> {
        None
    }
}

/// The capability `method` needs, or `None` for a method the host answers
/// without one (or doesn't know). `burrow/browser.open` needs one that
/// depends on its URL: see [`required_capability_for`].
pub fn required_capability(method: &str) -> Option<Capability> {
    Some(match method {
        methods::LIST_ITEMS | methods::GET_ITEM | methods::SEARCH_ITEMS => Capability::ItemsRead,
        methods::CREATE_DRAFT | methods::SAVE_ITEM => Capability::ItemsWrite,
        methods::LIST_READING => Capability::ReadingRead,
        methods::TOAST | methods::OPEN_ITEM => Capability::Ui,
        methods::BROWSER_PAGE => Capability::BrowserCapture,
        _ => return None,
    })
}

/// The capability this call needs, looking at its params where that
/// matters: `burrow/browser.open {url}` needs `browser.automate:<the URL's
/// origin>` (a URL that isn't http(s) with a host is invalid params).
pub fn required_capability_for(method: &str, p: &Value) -> Result<Option<Capability>, RpcError> {
    if method == methods::BROWSER_OPEN {
        let open: BrowserOpenParams = params(p.clone())?;
        return Capability::automate_for_url(&open.url)
            .map(Some)
            .ok_or_else(|| {
                RpcError::invalid_params(format!("{:?} isn't an http(s) URL", open.url))
            });
    }
    Ok(required_capability(method))
}

/// Refuse unless `granted` covers what `method` (with params `p`) needs.
pub fn check(granted: &[Capability], method: &str, p: &Value) -> Result<(), RpcError> {
    match required_capability_for(method, p)? {
        Some(c) if !granted.iter().any(|g| c.covered_by(g)) => {
            Err(RpcError::permission_denied(&c.as_string()))
        }
        _ => Ok(()),
    }
}

/// Whether `method` is one [`HostApi`] answers from the backend (item and
/// reading calls), as opposed to a UI hop the app answers.
fn backend_method(method: &str) -> bool {
    matches!(
        method,
        methods::LIST_ITEMS
            | methods::GET_ITEM
            | methods::SEARCH_ITEMS
            | methods::CREATE_DRAFT
            | methods::SAVE_ITEM
            | methods::LIST_READING
    )
}

/// The real host API over the app's backend.
pub struct BackendApi {
    backend: Arc<dyn Backend>,
}

impl BackendApi {
    pub fn new(backend: Arc<dyn Backend>) -> Self {
        BackendApi { backend }
    }
}

/// The default cap on `searchItems` and `listReading`.
pub const DEFAULT_LIMIT: usize = 200;

fn refused(e: CoreError) -> RpcError {
    match e {
        CoreError::Rejected {
            status: 409,
            message,
            details,
        } if message == STALE_MESSAGE => {
            let current = details.into_iter().next().unwrap_or_default();
            RpcError::new(codes::STALE, "the item changed since it was read")
                .with_data(json!({ "currentHash": current }))
        }
        CoreError::NotFound => RpcError::new(codes::REFUSED, "not found"),
        other => RpcError::new(codes::REFUSED, other.to_string()),
    }
}

fn status_matches(filter: &[String], s: Status) -> bool {
    filter.is_empty() || filter.iter().any(|f| f == status_str(s))
}

impl HostApi for BackendApi {
    fn blyg_origin(&self) -> Option<String> {
        self.backend.base_url()
    }

    fn call(
        &self,
        _ext: &str,
        granted: &[Capability],
        method: &str,
        p: Value,
    ) -> Result<Value, RpcError> {
        check(granted, method, &p)?;
        let b = &self.backend;
        match method {
            methods::LIST_ITEMS => {
                let p: ListItemsParams = params(p)?;
                let out: Vec<ItemSummary> = b
                    .items()
                    .iter()
                    .filter(|i| status_matches(&p.status, i.status))
                    .filter(|i| p.since.as_deref().is_none_or(|s| i.updated.as_str() >= s))
                    .map(|i| ItemSummary::of(i, p.include_content))
                    .collect();
                to_value(&out)
            }
            methods::GET_ITEM => {
                let p: GetItemParams = params(p)?;
                let item = b
                    .item(&LocalId(p.id))
                    .ok_or_else(|| refused(CoreError::NotFound))?;
                to_value(&ItemSummary::of(&item, true))
            }
            methods::SEARCH_ITEMS => {
                let p: SearchItemsParams = params(p)?;
                let out: Vec<ItemSummary> = b
                    .search(&p.query)
                    .iter()
                    .take(p.limit.unwrap_or(DEFAULT_LIMIT))
                    .map(|i| ItemSummary::of(i, false))
                    .collect();
                to_value(&out)
            }
            methods::CREATE_DRAFT => {
                let p: CreateDraftParams = params(p)?;
                let kind = match p.kind.as_deref() {
                    Some(k) => parse_kind(k)
                        .ok_or_else(|| RpcError::invalid_params(format!("unknown kind {k:?}")))?,
                    None => promotion_kind(Kind::Fragment, &p.content_md),
                };
                let id = if p.scratch {
                    b.create_scratch(kind, &p.content_md)
                } else {
                    b.create_draft(kind, &p.content_md)
                }
                .map_err(refused)?;
                to_value(&CreatedDraft {
                    id: id.0,
                    content_hash: content_hash(&p.content_md),
                })
            }
            methods::SAVE_ITEM => {
                let p: SaveItemParams = params(p)?;
                let id = LocalId(p.id);
                match &p.base_hash {
                    Some(base) => b.save_if_base(&id, &p.content_md, base),
                    None => b.save(&id, &p.content_md),
                }
                .map_err(refused)?;
                to_value(&SavedItem {
                    ok: true,
                    content_hash: content_hash(&p.content_md),
                })
            }
            methods::LIST_READING => {
                let p: ListReadingParams = params(p)?;
                let out: Vec<ReadingSummary> = b
                    .reading()
                    .iter()
                    .filter(|r| r.state != "tombstone")
                    .filter(|r| {
                        p.since
                            .as_deref()
                            .is_none_or(|s| r.observed_at.as_str() >= s)
                    })
                    .take(p.limit.unwrap_or(DEFAULT_LIMIT))
                    .map(ReadingSummary::of)
                    .collect();
                to_value(&out)
            }
            m => Err(RpcError::method_not_found(m)),
        }
    }
}

/// No blyg connected: every item and reading method is refused (after the
/// capability check, so a missing grant still reads as one). Library
/// extensions such as markdown-notes don't need a blyg at all.
pub struct NoBlyg;

impl HostApi for NoBlyg {
    fn call(
        &self,
        _ext: &str,
        granted: &[Capability],
        method: &str,
        p: Value,
    ) -> Result<Value, RpcError> {
        check(granted, method, &p)?;
        if backend_method(method) {
            Err(RpcError::new(codes::REFUSED, "no blyg is connected"))
        } else {
            Err(RpcError::method_not_found(method))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_page_needs_capture() {
        assert_eq!(
            required_capability(methods::BROWSER_PAGE),
            Some(Capability::BrowserCapture)
        );
        let e = check(&[Capability::Ui], methods::BROWSER_PAGE, &json!({})).unwrap_err();
        assert_eq!(e.code, codes::PERMISSION_DENIED);
        assert_eq!(e.data.unwrap()["capability"], "browser.capture");
        assert!(
            check(
                &[Capability::BrowserCapture],
                methods::BROWSER_PAGE,
                &json!({})
            )
            .is_ok()
        );
    }

    #[test]
    fn browser_open_needs_the_urls_own_origin() {
        let granted = [Capability::BrowserAutomate(
            "https://social.example.com".into(),
        )];
        let open = |url: &str| json!({ "url": url, "futureField": 1 });
        let ok = |url: &str| check(&granted, methods::BROWSER_OPEN, &open(url));
        assert!(ok("https://social.example.com/notes").is_ok());
        assert!(ok("HTTPS://Social.Example.com:443/x?y#z").is_ok());
        for other in [
            "https://other.example.com/",
            "http://social.example.com/",
            "https://social.example.com:8443/",
            "https://evil.social.example.com/",
        ] {
            assert_eq!(
                ok(other).unwrap_err().code,
                codes::PERMISSION_DENIED,
                "{other}"
            );
        }
        let e = check(
            &[Capability::BrowserCapture],
            methods::BROWSER_OPEN,
            &open("https://social.example.com/"),
        )
        .unwrap_err();
        assert_eq!(
            e.data.unwrap()["capability"],
            "browser.automate:https://social.example.com"
        );
        for bad in [
            json!({ "url": "file:///etc/hosts" }),
            json!({ "url": "javascript:1" }),
            json!({}),
        ] {
            let e = check(&granted, methods::BROWSER_OPEN, &bad).unwrap_err();
            assert_eq!(e.code, codes::INVALID_PARAMS, "{bad}");
        }
        assert_eq!(required_capability(methods::BROWSER_OPEN), None);
    }

    #[test]
    fn no_blyg_leaves_browser_calls_to_the_app() {
        let call = |granted: &[Capability], method: &str| {
            NoBlyg
                .call("x", granted, method, json!({}))
                .unwrap_err()
                .code
        };
        assert_eq!(
            call(&[Capability::BrowserCapture], methods::BROWSER_PAGE),
            codes::METHOD_NOT_FOUND
        );
        assert_eq!(
            call(&[Capability::ItemsRead], methods::LIST_ITEMS),
            codes::REFUSED
        );
        assert_eq!(call(&[], methods::LIST_ITEMS), codes::PERMISSION_DENIED);
    }
}
