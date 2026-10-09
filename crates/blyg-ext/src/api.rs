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
/// without one (or doesn't know).
pub fn required_capability(method: &str) -> Option<Capability> {
    Some(match method {
        methods::LIST_ITEMS | methods::GET_ITEM | methods::SEARCH_ITEMS => Capability::ItemsRead,
        methods::CREATE_DRAFT | methods::SAVE_ITEM => Capability::ItemsWrite,
        methods::LIST_READING => Capability::ReadingRead,
        methods::TOAST | methods::OPEN_ITEM => Capability::Ui,
        _ => return None,
    })
}

/// Refuse unless `granted` covers what `method` needs.
pub fn check(granted: &[Capability], method: &str) -> Result<(), RpcError> {
    match required_capability(method) {
        Some(c) if !granted.iter().any(|g| c.covered_by(g)) => {
            Err(RpcError::permission_denied(&c.as_string()))
        }
        _ => Ok(()),
    }
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
        check(granted, method)?;
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
        _p: Value,
    ) -> Result<Value, RpcError> {
        check(granted, method)?;
        match required_capability(method) {
            Some(_) => Err(RpcError::new(codes::REFUSED, "no blyg is connected")),
            None => Err(RpcError::method_not_found(method)),
        }
    }
}
