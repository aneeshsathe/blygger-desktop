//! --- web-parity --- What the studio does that the web lacked (the
//! studio-parity audit, 2026-11-15), as the studio does it:
//!
//! - **Mark all read** for the chosen Reading source and lens
//!   (`mark_all.rs`).
//! - **Status filters** over Posts (`filters.rs`).
//! - **Updates**: threads whose quotes have newer versions, and Refresh
//!   (`updates.rs`).
//! - **Access tokens**: list, make (shown once) and revoke (`access.rs`).
//!
//! Every server feature is gated on the operation names the blyg
//! advertises in `GET /api/capabilities` (`wr_can`), never on its
//! `contractVersion`: the studio may reuse a contract number without the
//! feature.

use std::collections::{HashMap, HashSet};

use blyg_core::api::offers;
use blyg_core::upkeep::ThreadFreshness;
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

use crate::api::ApiError;
use crate::{Burrow, json};

pub mod access;
pub mod filters;
pub mod mark_all;
pub mod updates;

/// Mark all read in one request (`mark_all.rs`).
pub const MARK_BATCH_OPERATION: &str = "markReadBatch";
/// The Updates screen's list of threads with stale quotes (`updates.rs`).
pub const STALE_OPERATION: &str = "listStaleThreads";
/// Refresh a thread's quotes (`updates.rs`).
pub const REFRESH_OPERATION: &str = "refreshItem";
/// Probe one thread's quotes at their blygs (`updates.rs`).
pub const FRESHNESS_OPERATION: &str = "getItemFreshness";

/// The operations the web gates a feature on that `answer` (`GET
/// /api/capabilities`) lists: each asked of it by name with `offers`, so
/// every gate is a constant the capability checks can see
/// (`blyg-core/tests/it/capability_gates.rs`).
pub(crate) fn offered(answer: &Value) -> HashSet<&'static str> {
    let mut on = HashSet::new();
    let mut ask = |op: &'static str, listed: bool| {
        if listed {
            on.insert(op);
        }
    };
    ask(MARK_BATCH_OPERATION, offers(answer, MARK_BATCH_OPERATION));
    ask(STALE_OPERATION, offers(answer, STALE_OPERATION));
    ask(REFRESH_OPERATION, offers(answer, REFRESH_OPERATION));
    ask(FRESHNESS_OPERATION, offers(answer, FRESHNESS_OPERATION));
    ask(
        blyg_core::shelf::GET_OPERATION,
        offers(answer, blyg_core::shelf::GET_OPERATION),
    );
    ask(
        blyg_core::shelf::PUT_OPERATION,
        offers(answer, blyg_core::shelf::PUT_OPERATION),
    );
    ask(
        blyg_core::background::OPERATION,
        offers(answer, blyg_core::background::OPERATION),
    );
    ask(
        blyg_core::changelog::CHANGELOG_OPERATION,
        offers(answer, blyg_core::changelog::CHANGELOG_OPERATION),
    );
    on
}

/// The web's state for these screens. Nothing here is persisted.
#[derive(Default)]
pub struct Parity {
    /// The Posts list's filter.
    pub filter: filters::PostFilter,
    /// `GET /api/freshness`, stalest first, once read (or why not).
    pub stale: Option<Result<Vec<ThreadFreshness>, String>>,
    /// Each thread's quotes as their blygs answer now, by server id.
    pub probed: HashMap<String, Result<ThreadFreshness, String>>,
    /// Why a thread's last refresh failed, by server id.
    pub failed: HashMap<String, String>,
    /// The grants that can act as you (never a token).
    pub access: access::Access,
}

/// An error's words, as the page shows them.
pub(crate) fn err_text(e: &ApiError) -> String {
    e.to_json()
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("The blyg refused it")
        .to_string()
}

/// `{error, code}` for the page (`code` `unauthorized` asks to sign in).
pub(crate) fn err_out(e: &ApiError) -> String {
    json(&json!({ "error": err_text(e), "code": e.to_json().get("code").cloned() }))
}

impl Burrow {
    pub(crate) fn can(&self, op: &str) -> bool {
        self.wr.ops.as_ref().is_some_and(|o| o.contains(op))
    }
}

#[wasm_bindgen]
impl Burrow {
    /// Whether the blyg advertises operation `op` (`/api/capabilities`):
    /// `false` until capabilities are read, and in the demo (no server).
    pub fn wr_can(&self, op: &str) -> bool {
        self.is_live() && self.can(op)
    }

    /// Whether capabilities have been read, so `wr_can` is final.
    pub fn wr_ops_known(&self) -> bool {
        self.wr.ops.is_some()
    }
}

#[cfg(test)]
mod tests;
