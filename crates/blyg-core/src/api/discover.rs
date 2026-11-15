//! --- discover --- `Settings.discoverSeenAt` (studio 0.60, private contract
//! 5.9.0; `crate::discover`). A settings field, not an operation, so it is
//! gated on the field itself: a blyg whose `GET /api/settings` has no
//! `discoverSeenAt` doesn't keep it, and is never sent it.

use serde_json::{Value, json};

use super::Api;
use crate::backend::{CoreError, Result};

impl Api {
    /// `discoverSeenAt` from `GET /api/settings` (`""`: never looked).
    /// `Ok(None)`: not kept by this blyg.
    pub fn discover_seen_at(&self) -> Result<Option<String>> {
        let v: Option<Value> = Self::optional(self.call("GET", "/api/settings", None))?;
        Ok(v.and_then(|v| v.get("discoverSeenAt")?.as_str().map(str::to_string)))
    }

    /// `PATCH /api/settings {discoverSeenAt}`: that key only, so nothing
    /// else in the settings is written back.
    pub fn put_discover_seen_at(&self, at: &str) -> Result<()> {
        let v = self.call(
            "PATCH",
            "/api/settings",
            Some(json!({ "discoverSeenAt": at })),
        )?;
        match v.get("discoverSeenAt").and_then(Value::as_str) {
            Some(saved) if saved == at => Ok(()),
            _ => Err(CoreError::Other(
                "your blyg didn't keep the Discover mark".into(),
            )),
        }
    }
}
