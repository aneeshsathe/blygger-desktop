//! --- helpers --- The studio's helpers (blygger-studio 0.69.0, contract
//! 6.2.0; blygger-spec `docs/helpers-plan.md`): simple agents people and
//! agents share through the same operations.
//!
//! Burrow uses one: **catch-up-summary** (`POST
//! /api/helpers/catch-up-summary {sub, remoteId}` → `{summary, model, at,
//! sources, cached}`), a few sentences over the conversation an entry
//! quotes, written by the blyg's text helpers model (`aiModelHelpers`, else
//! the change-notes model), once per set of versions and kept. 409 when the
//! entry quotes nothing (or one is being written); 429/502 when the model
//! can't be asked. Catch up asks it first when the blyg offers it, and
//! falls back to Burrow's own AI (`blyg_ai::prompts::catch_up_summary`).
//!
//! Burrow doesn't ask `catchUp` (the cards) or `listHelpers` (the
//! catalogue): its own Catch up (`crate::catchup`) lays out more than the
//! studio's (stubs, forks and side branches, offline), and the
//! capabilities already say which helpers run.

use serde_json::{Value, json};

use crate::shelf::Entry;

/// The capabilities' operation name for the summary.
pub const SUMMARIZE_OPERATION: &str = "summarizeCatchUp";

/// Whether a capabilities answer's `operations` (an array of ids; a map is
/// read by its keys) list `op`. The one place this crate reads them for
/// studio 0.69's features (wave D's `api::offers` is the same rule).
pub fn lists(capabilities: &Value, op: &str) -> bool {
    match capabilities.get("operations") {
        Some(Value::Array(a)) => a.iter().any(|o| o.as_str() == Some(op)),
        Some(Value::Object(m)) => m.contains_key(op),
        _ => false,
    }
}

/// Whether a capabilities answer offers the catch-up summary.
pub fn serves_summary(capabilities: &Value) -> bool {
    lists(capabilities, SUMMARIZE_OPERATION)
}

/// One post the summary read, in the §5.9 reference shape (your own posts
/// carry no origin).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummarySource {
    pub id: String,
    pub version: Option<u32>,
    pub origin: Option<String>,
}

/// The blyg's summary of a conversation. Generated: shown as the model's,
/// never as anyone's words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatchUpSummary {
    pub summary: String,
    pub model: String,
    /// When it was written.
    pub at: String,
    /// Every post it read, oldest first.
    pub sources: Vec<SummarySource>,
    /// The same versions were summarized before; nothing was spent.
    pub cached: bool,
}

/// The `POST /api/helpers/catch-up-summary` body.
pub fn summary_body(entry: &Entry) -> Value {
    json!({ "sub": entry.sub, "remoteId": entry.remote_id })
}

/// Its answer. `None` when it isn't one.
pub fn parse_summary(v: &Value) -> Option<CatchUpSummary> {
    Some(CatchUpSummary {
        summary: v.get("summary")?.as_str()?.to_string(),
        model: v.get("model")?.as_str()?.to_string(),
        at: v
            .get("at")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        sources: v
            .get("sources")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|s| {
                        Some(SummarySource {
                            id: s.get("id")?.as_str()?.to_string(),
                            version: s
                                .get("version")
                                .and_then(Value::as_u64)
                                .and_then(|v| u32::try_from(v).ok()),
                            origin: s.get("origin").and_then(Value::as_str).map(str::to_string),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        cached: v.get("cached").and_then(Value::as_bool).unwrap_or(false),
    })
}
