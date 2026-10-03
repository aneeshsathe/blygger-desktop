//! Wire shapes the owner API returns that don't map 1:1 onto `model` types.
//! Field names follow upstream blygger-studio's OpenAPI contract
//! (`openapi.json`, studio 0.10), plus the extensions in docs/SERVER.md.

use serde::Deserialize;
use serde_json::Value;

use crate::model::{Author, Kind, Mention, ReadingItem, RemoteRef, ResponsesMode, Status, Version};

/// `GET /api/items` element and `GET /api/items/:id` body (the contract's
/// `Item`, plus `authored_kind` and `versions` on the detail read).
#[derive(Debug, Clone, Default, Deserialize)]
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
    /// The item's response policy. `None` only on a hand-built placeholder.
    #[serde(default)]
    pub responses: Option<ResponsesMode>,
    /// Only on `GET /api/items/:id`.
    #[serde(default, deserialize_with = "wire_versions")]
    pub versions: Option<Vec<Version>>,
    /// Not on the wire: the client derives it (`{base}/f|t/{id}`) for
    /// published items. `None` keeps whatever the store already holds.
    #[serde(skip)]
    pub permalink: Option<String>,
    /// Not on the wire: whether the page shows responses now, resolved from
    /// `responses` and the blyg's `show_responses_default`.
    #[serde(skip)]
    pub showing: Option<bool>,
}

impl WireItem {
    /// Whether the page shows responses now.
    pub fn shows_responses(&self) -> bool {
        self.showing
            .unwrap_or(self.responses == Some(ResponsesMode::Show))
    }

    /// The item's own choice, when the server reports one.
    pub fn responses_mode(&self) -> Option<ResponsesMode> {
        self.responses
    }

    /// Fill the derived fields: `default` is the blyg's
    /// `show_responses_default`, `base` the API base (it carries the mount).
    pub fn resolve(&mut self, default: bool, base: &str) {
        self.showing = Some(match self.responses {
            Some(ResponsesMode::Show) => true,
            Some(ResponsesMode::Hide) => false,
            Some(ResponsesMode::Default) | None => default,
        });
        if self.version > 0 {
            let p = if self.local_kind() == Kind::Thread {
                "t"
            } else {
                "f"
            };
            self.permalink = Some(format!("{base}/{p}/{}", self.id));
        }
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

/// The contract's `Version`: a withdraw marker is `kind: "withdrawn"`.
#[derive(Debug, Clone, Deserialize)]
struct WireVersion {
    version: u32,
    #[serde(default)]
    published_at: String,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    pinned: bool,
    #[serde(default)]
    kind: String,
}

fn wire_versions<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Vec<Version>>, D::Error> {
    let v = Option::<Vec<WireVersion>>::deserialize(d)?;
    Ok(v.map(|v| {
        v.into_iter()
            .map(|w| Version {
                version: w.version,
                published_at: w.published_at,
                note: w.note,
                pinned: w.pinned,
                endcap: w.kind == "withdrawn",
            })
            .collect()
    }))
}

/// `GET /api/mentions?direction=inbound` element. The source's author is a
/// serialized JSON string (`source_author_json`).
#[derive(Debug, Clone, Deserialize)]
pub struct WireMention {
    pub id: String,
    pub target_item_id: String,
    pub status: String,
    #[serde(default)]
    pub relation: Option<String>,
    pub source: String,
    #[serde(default)]
    pub source_origin: Option<String>,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub source_kind: Option<String>,
    #[serde(default)]
    pub source_version: Option<u32>,
    #[serde(default)]
    pub source_author_json: Option<String>,
    #[serde(default)]
    pub first_seen: String,
    #[serde(default)]
    pub verified_at: Option<String>,
    #[serde(default)]
    pub hidden: bool,
}

impl From<WireMention> for Mention {
    fn from(w: WireMention) -> Mention {
        // Tolerant: an author we can't read is no author.
        let source_author = w
            .source_author_json
            .as_deref()
            .and_then(|j| serde_json::from_str::<Author>(j).ok());
        Mention {
            id: w.id,
            target_item_id: w.target_item_id,
            status: w.status,
            relation: w.relation,
            source: w.source,
            source_origin: w.source_origin,
            source_id: w.source_id,
            source_kind: w.source_kind,
            source_version: w.source_version,
            source_author,
            first_seen: w.first_seen,
            verified_at: w.verified_at,
            hidden: w.hidden,
        }
    }
}

/// Collection reads: `{items, total, offset, limit}`.
#[derive(Debug, Clone, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    #[serde(default)]
    pub total: u64,
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

/// The id of a created resource (`POST /api/items` → `201 Item`,
/// `POST /api/subscriptions {confirm}` → `201 Subscription`).
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

/// `POST /api/media` → `201 {id, url, mime}`. The maintainer's Worker fork
/// also answers `200 {…, duplicate: true}` when the item already has an
/// attachment with identical bytes (the existing row comes back).
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
        let mut v = json!({ "id": "A", "kind": "fragment", "status": "public", "version": 1 });
        v.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn the_policy_resolves_against_the_site_default() {
        let mut w = item(json!({ "responses": "default" }));
        assert_eq!(w.responses_mode(), Some(ResponsesMode::Default));
        w.resolve(true, "https://blyg.example.com/b");
        assert!(w.shows_responses());
        w.resolve(false, "https://blyg.example.com/b");
        assert!(!w.shows_responses());
        let mut w = item(json!({ "responses": "show" }));
        w.resolve(false, "https://blyg.example.com/b");
        assert!(w.shows_responses());
        let mut w = item(json!({ "responses": "hide" }));
        w.resolve(true, "https://blyg.example.com/b");
        assert!(!w.shows_responses());
    }

    #[test]
    fn permalinks_carry_the_mount_and_the_kind() {
        let mut w = item(json!({ "kind": "withdrawn", "authored_kind": "thread" }));
        w.resolve(false, "https://blyg.example.com/b");
        assert_eq!(
            w.permalink.as_deref(),
            Some("https://blyg.example.com/b/t/A")
        );
        let mut d = item(json!({ "version": 0 }));
        d.resolve(false, "https://blyg.example.com");
        assert_eq!(d.permalink, None, "a draft has no page");
    }

    #[test]
    fn a_withdrawn_version_is_the_endcap() {
        let w = item(json!({ "versions": [
            { "version": 1, "published_at": "t1", "note": null, "pinned": true, "kind": "fragment" },
            { "version": 2, "published_at": "t2", "note": null, "pinned": false, "kind": "withdrawn" },
        ] }));
        let v = w.versions.unwrap();
        assert!(v[0].pinned && !v[0].endcap);
        assert!(v[1].endcap);
    }

    #[test]
    fn a_mention_author_is_parsed_from_its_json_string() {
        let w: WireMention = serde_json::from_value(json!({
            "id": "m", "target_item_id": "A", "status": "verified", "relation": "stub",
            "source": "https://other.example.com/t/x", "source_author_json": "{\"name\":\"Ana\",\"url\":null}",
            "first_seen": "t", "hidden": false,
        }))
        .unwrap();
        let m: Mention = w.into();
        assert_eq!(m.source_author.unwrap().name.as_deref(), Some("Ana"));
    }
}
