//! Wire shapes the owner API returns that don't map 1:1 onto `model` types.
//! Field names follow `apps/blyg/src/owner-read-api.ts` and `api.ts`.

use serde::Deserialize;
use serde_json::Value;

use crate::model::{Kind, ReadingItem, RemoteRef, ResponsesMode, Status, Version};

/// `GET /api/items` element and `GET /api/items/:id` body.
#[derive(Debug, Clone, Deserialize)]
pub struct WireItem {
    pub id: String,
    /// Current kind; `"withdrawn"` for withdrawn items, so prefer `authored_kind`.
    pub kind: String,
    #[serde(default)]
    pub authored_kind: Option<String>,
    pub status: String,
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub dirty: bool,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub updated: String,
    #[serde(default)]
    pub content_md: String,
    #[serde(default)]
    pub stub_of: Option<Value>,
    #[serde(default)]
    pub forked_from: Option<Value>,
    #[serde(default)]
    pub permalink: Option<String>,
    /// Patch 3; absent on older servers.
    #[serde(default)]
    pub show_responses: bool,
    /// Studio 0.8: the item's own choice, `1`/`0`, or `null` to follow the
    /// global default. Outer `None` = the server didn't send the key.
    #[serde(default, deserialize_with = "present")]
    pub responses_override: Option<Option<i64>>,
    /// Studio 0.8: the effective state, when the server reports it; it wins
    /// over `show_responses`, which a 0.8 store no longer updates.
    #[serde(default)]
    pub showing: Option<bool>,
    /// Only on `GET /api/items/:id`.
    #[serde(default)]
    pub versions: Option<Vec<Version>>,
}

/// Tells a present `null` (`Some(None)`) from an absent key (`None`).
fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<i64>>, D::Error> {
    Option::<i64>::deserialize(d).map(Some)
}

impl WireItem {
    /// Whether the page shows responses now.
    pub fn shows_responses(&self) -> bool {
        self.showing.unwrap_or(self.show_responses)
    }

    /// The item's own choice, when the server reports one.
    pub fn responses_mode(&self) -> Option<ResponsesMode> {
        self.responses_override.map(ResponsesMode::from_override)
    }

    pub fn local_kind(&self) -> Kind {
        let k = self.authored_kind.as_deref().unwrap_or(&self.kind);
        parse_kind(k)
    }

    pub fn local_status(&self) -> Status {
        parse_status(&self.status)
    }

    /// `stub_of` may also be `{url}` for an L0 stub, which `RemoteRef` can't hold.
    pub fn stub_ref(&self) -> Option<RemoteRef> {
        self.stub_of
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    pub fn fork_ref(&self) -> Option<RemoteRef> {
        self.forked_from
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }
}

pub fn parse_kind(s: &str) -> Kind {
    if s == "thread" {
        Kind::Thread
    } else {
        Kind::Fragment
    }
}

pub fn kind_str(k: Kind) -> &'static str {
    match k {
        Kind::Fragment => "fragment",
        Kind::Thread => "thread",
    }
}

pub fn parse_status(s: &str) -> Status {
    match s {
        "public" => Status::Public,
        "withdrawn" => Status::Withdrawn,
        // Local-only (never on the wire): stored in `items.status`.
        "scratch" => Status::Scratch,
        _ => Status::Draft,
    }
}

pub fn status_str(s: Status) -> &'static str {
    match s {
        Status::Draft => "draft",
        Status::Public => "public",
        Status::Withdrawn => "withdrawn",
        Status::Scratch => "scratch",
    }
}

/// `POST /api/items` and `POST /api/fork` → `201 {id, kind, status}`.
#[derive(Debug, Clone, Deserialize)]
pub struct Created {
    pub id: String,
}

/// `POST /api/items/:id/publish` → `{ok, version, warning?}`.
#[derive(Debug, Clone, Deserialize)]
pub struct Published {
    pub version: u32,
    #[serde(default)]
    pub warning: Option<String>,
}

/// `POST /api/media` → `201 {id, url, mime}`, or `200 {…, duplicate: true}`
/// when the item already has an attachment with identical bytes (patch 8:
/// the existing row comes back and nothing new is stored).
#[derive(Debug, Clone, Deserialize)]
pub struct Media {
    pub id: String,
    pub url: String,
    pub mime: String,
    #[serde(default)]
    pub duplicate: bool,
}

/// `GET /api/reading` (patch 3) → `{items, next}`, plus `read_state: true`
/// from a server that stores read state (extension 5; each item then carries
/// its `read_version`).
#[derive(Debug, Clone, Deserialize)]
pub struct ReadingPage {
    pub items: Vec<ReadingItem>,
    #[serde(default)]
    pub next: Option<String>,
    #[serde(default, deserialize_with = "crate::model::lenient")]
    pub read_state: Option<bool>,
}

impl ReadingPage {
    /// The server advertises read-state sync (extension 5).
    pub fn read_sync(&self) -> bool {
        self.read_state == Some(true)
    }
}

/// One row's read state as sent to the server: `(sub, remote_id)` is a
/// reading row, `version` the highest version read.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, Deserialize)]
pub struct ReadMark {
    pub sub: String,
    pub remote_id: String,
    pub version: u32,
}

/// At most this many entries per `POST /api/reading/read`.
pub const READ_BATCH_MAX: usize = 500;

#[cfg(test)]
mod responses_tests {
    use super::*;
    use serde_json::json;

    fn item(extra: Value) -> WireItem {
        let mut v = json!({ "id": "A", "kind": "fragment", "status": "public" });
        v.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn override_tells_null_from_absent() {
        assert_eq!(item(json!({})).responses_mode(), None);
        assert_eq!(
            item(json!({ "responses_override": null })).responses_mode(),
            Some(ResponsesMode::Default)
        );
        assert_eq!(
            item(json!({ "responses_override": 0 })).responses_mode(),
            Some(ResponsesMode::Hide)
        );
        assert_eq!(
            item(json!({ "responses_override": 1 })).responses_mode(),
            Some(ResponsesMode::Show)
        );
    }

    #[test]
    fn showing_wins_over_the_legacy_column() {
        // A 0.8 store no longer updates `show_responses`.
        let w = item(json!({ "show_responses": false, "showing": true }));
        assert!(w.shows_responses());
        assert!(item(json!({ "show_responses": true })).shows_responses());
    }
}
