//! --- changelog display --- studio 0.58's changelog settings and summaries
//! (private contract 5.8.0; `crate::changelog`). Asked only of a studio
//! whose capabilities say 5.8 or later; an older one is never sent the
//! route, the keys or the `history` flag.

use serde_json::{Value, json};

use super::{Api, enc};
use crate::access_ai::DraftedNote;
use crate::backend::{CoreError, Result};
use crate::changelog::{ChangelogSettings, ChangelogSummary, SummaryOf, serves_changelog};

impl Api {
    /// Whether this studio has changelog display and summaries (asks its
    /// capabilities).
    pub fn serves_changelog(&self) -> Result<bool> {
        Ok(self
            .contract_version()?
            .as_deref()
            .is_some_and(serves_changelog))
    }

    /// The changelog keys of `GET /api/settings`. `Ok(None)`: not served.
    pub fn changelog_settings(&self) -> Result<Option<ChangelogSettings>> {
        if !self.serves_changelog()? {
            return Ok(None);
        }
        let v: Option<Value> = Self::optional(self.call("GET", "/api/settings", None))?;
        Ok(v.map(|v| ChangelogSettings::from_json(&v))
            .filter(ChangelogSettings::served))
    }

    /// `PATCH /api/settings` with the changelog keys only; the stored values
    /// back.
    pub fn put_changelog_settings(&self, s: &ChangelogSettings) -> Result<ChangelogSettings> {
        if !self.serves_changelog()? {
            return Err(CoreError::Other(NEEDS_0_58.into()));
        }
        let v = self.call("PATCH", "/api/settings", Some(s.to_json()))?;
        Ok(ChangelogSettings::from_json(&v))
    }

    /// `POST /api/changelog-summary`: the blyg's summary of an item's
    /// changelog notes, written once per version and kept there. 409 when
    /// there's nothing to summarize or one is being written; 429/502 when
    /// the model can't be asked.
    pub fn summarize_changelog(&self, of: &SummaryOf) -> Result<ChangelogSummary> {
        if !self.serves_changelog()? {
            return Err(CoreError::Other(NEEDS_0_58.into()));
        }
        let v = self.call("POST", "/api/changelog-summary", Some(of.body()))?;
        ChangelogSummary::from_json(&v).ok_or_else(|| {
            CoreError::Other("unexpected response from /api/changelog-summary".into())
        })
    }

    /// `POST /api/items/{id}/note-draft {expectedRevision, history: true}`:
    /// a note summarizing the whole history so far, ending with this
    /// version (up to 400 characters).
    pub fn draft_note_history(&self, id: &str, revision: u64) -> Result<DraftedNote> {
        if !self.serves_changelog()? {
            return Err(CoreError::Other(NEEDS_0_58.into()));
        }
        let v = self.call(
            "POST",
            &format!("/api/items/{}/note-draft", enc(id)),
            Some(json!({ "expectedRevision": revision, "history": true })),
        )?;
        let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        Ok(DraftedNote {
            note: s("note").trim().to_string(),
            model: s("model"),
        })
    }
}

/// What an older studio is told.
pub const NEEDS_0_58: &str = "your blyg needs blygger-studio 0.58 or later for changelog summaries";
// --- end changelog display ---
