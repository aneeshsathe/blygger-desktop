//! --- studio 0.71 --- Generation on the blyg (`POST /api/items/:id/generate
//! {scope, expectedRevision, proceed?}`): the studio fills a `[TK]` scope
//! with its own model, records the provenance and stores the spliced text.
//!
//! Since studio 0.70 (decision #117, spec 0.4 §16.16) it first reads every
//! link the author placed, in the instruction or anywhere in the draft. If
//! any can't be read it stops at a **checkpoint**: the answer is
//! `{checkpoint: {read, total, failed: [{link, reason}]}}` and nothing is
//! saved. The author decides; `proceed: true` generates without them.
//! An older studio never answers a checkpoint and ignores `proceed`.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::backend::{CoreError, Result};

/// The text the blyg generated (`{text, model, contentMd, revision}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generated {
    /// The scope's new output.
    pub text: String,
    /// The model that wrote it; `"unknown"` when the studio didn't say.
    pub model: String,
    /// The working copy with `text` spliced into its scope, as the server
    /// stored it.
    pub content_md: Option<String>,
    /// The item's revision after it.
    pub revision: u64,
}

/// One link the blyg couldn't read: the link as written (a URL, or
/// `[[id]]`) and why, in the studio's words.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UnreadLink {
    pub link: String,
    pub reason: String,
}

/// Generation stopped before the model: `read` of `total` links were read,
/// `failed` weren't. Nothing was saved.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Checkpoint {
    pub read: u32,
    pub total: u32,
    #[serde(default)]
    pub failed: Vec<UnreadLink>,
}

/// What `POST /api/items/:id/generate` answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenerateOutcome {
    Generated(Generated),
    /// Some links couldn't be read: ask the author, then send again with
    /// `proceed` to generate without them.
    Checkpoint(Checkpoint),
}

impl GenerateOutcome {
    /// The 200's body, either shape.
    pub fn from_value(v: Value) -> Result<GenerateOutcome> {
        let bad = |e: serde_json::Error| CoreError::Other(format!("generate answered oddly: {e}"));
        if let Some(c) = v.get("checkpoint") {
            return Ok(GenerateOutcome::Checkpoint(
                Checkpoint::deserialize(c).map_err(bad)?,
            ));
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct R {
            text: String,
            #[serde(default)]
            model: Option<String>,
            #[serde(default)]
            content_md: Option<String>,
            #[serde(default)]
            revision: u64,
        }
        let r = R::deserialize(&v).map_err(bad)?;
        Ok(GenerateOutcome::Generated(Generated {
            text: r.text,
            model: r.model.unwrap_or_else(|| "unknown".into()),
            content_md: r.content_md,
            revision: r.revision,
        }))
    }
}

/// The request's body: `proceed` only when the author said so (an older
/// studio knows nothing of it).
pub fn request_body(scope: u32, revision: u64, proceed: bool) -> Value {
    let mut b = json!({ "scope": scope, "expectedRevision": revision });
    if proceed {
        b["proceed"] = json!(true);
    }
    b
}
