//! Wire shapes the owner API returns that don't map 1:1 onto `model` types.
//! Field names follow blygger-studio's private API contract 5.0.0
//! (`openapi.json`, studio 0.49.12): camelCase throughout. The app's own
//! types (`model`) keep their snake_case serde names, which are also how the
//! store keeps them, so conversion happens here.

use serde::Deserialize;
use serde_json::Value;

use crate::model::{
    Author, AuthorLink, Kind, Mention, PickerTyping, ReadingItem, ResponsesMode, Settings, Status,
    Subscription, SubscriptionKind, Version,
};

/// `GET /api/items` element and `GET /api/items/:id` body (the contract's
/// `Item`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireItem {
    pub id: String,
    /// The item's private revision: every command on it carries the one it
    /// was based on (`expectedRevision`). 0 = not reported.
    #[serde(default)]
    pub revision: u64,
    /// `fragment` or `thread`, also after withdrawal (`status` says that).
    pub kind: String,
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
    /// Not on the wire: the version history, when it was read too
    /// (`Api::get_item_full`, from `GET /api/items/:id/versions`).
    #[serde(skip)]
    pub versions: Option<Vec<Version>>,
    /// Not on the wire: the client derives it (`{base}/f|t/{id}`) for
    /// published items. `None` keeps whatever the store already holds.
    #[serde(skip)]
    pub permalink: Option<String>,
    /// Not on the wire: whether the page shows responses now, resolved from
    /// `responses` and the blyg's `showResponsesDefault`.
    #[serde(skip)]
    pub showing: Option<bool>,
    /// The working copy's TK provenance, one entry per scope. Raw:
    /// upstream's `model` is optional.
    #[serde(default)]
    pub provenance: Option<Vec<Option<Value>>>,
    /// The item's generated-text highlight choice.
    #[serde(default)]
    pub highlight: Option<ResponsesMode>,
    /// Not on the wire: whether the public page highlights generated text
    /// now, resolved from `highlight` and `highlightGeneratedDefault`.
    #[serde(skip)]
    pub highlighting: Option<bool>,
}

impl WireItem {
    /// `provenance` as the app's shape (`None` when the field is absent).
    /// An entry without a model reads as `"unknown"`; one that doesn't
    /// parse as `null`.
    pub fn server_provenance(&self) -> Option<Vec<Option<crate::model::ScopeProvenance>>> {
        self.provenance.as_ref().map(|all| {
            all.iter()
                .map(|p| {
                    let mut p = p.clone()?;
                    let o = p.as_object_mut()?;
                    if !o.get("model").is_some_and(Value::is_string) {
                        o.insert("model".into(), Value::String("unknown".into()));
                    }
                    serde_json::from_value(p).ok()
                })
                .collect()
        })
    }

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
    /// `showResponsesDefault`, `base` the blyg's public base (with its mount).
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

    /// Whether the public page highlights generated text now.
    pub fn highlights(&self) -> bool {
        self.highlighting
            .unwrap_or(self.highlight == Some(ResponsesMode::Show))
    }

    /// Fill `highlighting`: `default` is `highlightGeneratedDefault`.
    pub fn resolve_highlight(&mut self, default: bool) {
        self.highlighting = Some(match self.highlight {
            Some(ResponsesMode::Show) => true,
            Some(ResponsesMode::Hide) => false,
            Some(ResponsesMode::Default) | None => default,
        });
    }

    pub fn local_kind(&self) -> Kind {
        parse_kind(&self.kind)
    }

    pub fn local_status(&self) -> Status {
        parse_status(&self.status)
    }

    /// `stubOf` may also be `{url}` for an L0 stub, which `RemoteRef` can't hold.
    pub fn stub_ref(&self) -> Option<crate::model::RemoteRef> {
        self.stub_of
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    pub fn fork_ref(&self) -> Option<crate::model::RemoteRef> {
        self.forked_from
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }
}

/// The contract's `Version` (`GET /api/items/:id/versions`): a withdrawal
/// is `kind: "withdrawn"`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireVersion {
    pub version: u32,
    #[serde(default)]
    published_at: String,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    pinned: bool,
    #[serde(default)]
    kind: String,
}

impl WireVersion {
    pub fn into_version(self) -> Version {
        Version {
            version: self.version,
            published_at: self.published_at,
            note: self.note,
            pinned: self.pinned,
            endcap: self.kind == "withdrawn",
        }
    }
}

/// `GET /api/mentions?direction=inbound` element (the contract's `Mention`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    /// The source's author object (remote data, unvalidated).
    #[serde(default)]
    pub source_author: Option<Value>,
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
            .source_author
            .filter(Value::is_object)
            .and_then(|a| serde_json::from_value::<Author>(a).ok());
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

/// The contract's `Subscription`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireSubscription {
    pub id: String,
    pub kind: SubscriptionKind,
    #[serde(default)]
    pub origin: String,
    #[serde(default)]
    pub feed_url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub in_blogroll: bool,
    #[serde(default)]
    pub title_follows_source: bool,
    // --- contacts --- the poll status (Studio's Inspector).
    #[serde(default)]
    pub last_poll_at: Option<String>,
    #[serde(default)]
    pub fail_count: f64,
    #[serde(default)]
    pub flags: Vec<crate::model::SubscriptionFlag>,
}

impl From<WireSubscription> for Subscription {
    fn from(w: WireSubscription) -> Subscription {
        Subscription {
            id: w.id,
            kind: w.kind,
            origin: w.origin,
            feed_url: w.feed_url,
            title: w.title,
            status: w.status,
            in_blogroll: w.in_blogroll,
            title_follows_source: w.title_follows_source,
            // --- contacts ---
            poll: crate::model::SubscriptionPoll {
                last_poll_at: w.last_poll_at.filter(|t| !t.is_empty()),
                fail_count: w.fail_count.max(0.0) as u32,
                flags: w.flags,
            },
        }
    }
}

/// The contract's `Settings`, the fields the app reads.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WireSettings {
    pub site_title: Option<String>,
    pub author_name: Option<String>,
    pub author_bio: Option<String>,
    pub site_url: Option<String>,
    pub theme: Option<String>,
    pub avatar_media_id: Option<String>,
    pub author_links: Vec<AuthorLink>,
    pub accept_mentions: Option<bool>,
    pub timezone: Option<String>,
    pub show_responses_default: Option<bool>,
    pub highlight_generated_default: Option<bool>,
    #[serde(deserialize_with = "crate::model::lenient")]
    pub picker_typing: Option<PickerTyping>,
}

impl From<WireSettings> for Settings {
    fn from(w: WireSettings) -> Settings {
        Settings {
            site_title: w.site_title,
            author_name: w.author_name,
            author_bio: w.author_bio,
            site_url: w.site_url,
            theme: w.theme,
            avatar_media_id: w.avatar_media_id,
            author_links: w.author_links,
            accept_mentions: w.accept_mentions,
            timezone: w.timezone,
            show_responses_default: w.show_responses_default,
            highlight_generated_default: w.highlight_generated_default,
            picker_typing: w.picker_typing,
        }
    }
}

/// A cursor page: `{items, page: {limit, nextCursor}}`. A `nextCursor` of
/// null means the list is exhausted. Lists carry no totals.
#[derive(Debug, Clone, Deserialize)]
pub struct CursorPage<T> {
    pub items: Vec<T>,
    pub page: PageInfo,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    #[serde(default)]
    pub limit: u32,
    /// Opaque; passed back verbatim as `cursor`.
    pub next_cursor: Option<String>,
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

/// `POST /api/items/:id/generate` → `{text, model, contentMd, revision}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generated {
    /// The scope's new output.
    pub text: String,
    pub model: String,
    /// The working copy with `text` spliced into its scope, as the server
    /// stored it.
    pub content_md: Option<String>,
    /// The item's revision after it.
    pub revision: u64,
}

/// `GET /api/changes`: `{epoch, domains: {items, reading, subscriptions,
/// hoppers, signals, settings, feed, …}}`. A domain's counter moves whenever
/// its data changes; a new epoch means the database was restored or
/// replaced and no stored revision holds.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ChangeState {
    pub epoch: String,
    pub domains: std::collections::BTreeMap<String, u64>,
}

/// The id of a created resource (`POST /api/subscriptions {confirm}` →
/// `201 Subscription`).
#[derive(Debug, Clone, Deserialize)]
pub struct Created {
    pub id: String,
}

/// `POST /api/items/:id/publish` → `{ok, revision, version, warning?}`.
#[derive(Debug, Clone, Deserialize)]
pub struct Published {
    pub version: u32,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub warning: Option<String>,
    /// --- contacts --- `@handle`s the publish couldn't resolve (5.7.0).
    #[serde(default, rename = "unresolvedHandles")]
    pub unresolved_handles: Vec<String>,
}

/// `POST /api/media` → `201 {id, url, mime, revision?}`, or `200 {…,
/// duplicate: true}` when the item already has these bytes in that place
/// (the existing row comes back). `url` is absolute, with the blyg's mount.
#[derive(Debug, Clone, Deserialize)]
pub struct Media {
    pub id: String,
    pub url: String,
    pub mime: String,
    #[serde(default)]
    pub duplicate: bool,
    /// The item's revision after attaching (attachments only).
    #[serde(default)]
    pub revision: Option<u64>,
}

/// `GET /api/reading/imported`, one page, in the app's shape.
#[derive(Debug, Clone)]
pub struct ReadingPage {
    pub items: Vec<ReadingItem>,
    /// The next page's cursor; `None` when this was the last page.
    pub next: Option<String>,
}

/// `GET /api/reading/imported` serves at most this many rows a page.
pub const IMPORTED_READING_MAX: u32 = 100;

/// One `ImportedReadingEntry` (camelCase) in the app's shape. Tolerant:
/// only `subscriptionId` and `remoteId` are required; anything else odd
/// reads as absent. `withdrawn` is the store's `state: "tombstone"`. The
/// row's lineage is the origin's own (null when none), so it is known for
/// every row but an L0 one or a withdrawn one.
pub fn imported_row(row: &Value) -> Result<ReadingItem, String> {
    let o = row
        .as_object()
        .ok_or("a reading row that isn't an object")?;
    let s = |k: &str| o.get(k).and_then(Value::as_str).map(str::to_string);
    let b = |k: &str| o.get(k).and_then(Value::as_bool).unwrap_or(false);
    let n = |k: &str| {
        o.get(k)
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
    };
    let sub = s("subscriptionId").ok_or("a reading row without subscriptionId")?;
    let rid = s("remoteId").ok_or("a reading row without remoteId")?;
    let withdrawn = b("withdrawn");
    let l0 = b("l0");
    let kind = if s("kind").as_deref() == Some("thread") {
        "thread"
    } else {
        "fragment"
    };
    let thumb = o
        .get("thumb")
        .and_then(Value::as_i64)
        .filter(|t| *t == 1 || *t == -1);
    let hoppers: Vec<String> = o
        .get("hoppers")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|h| h.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let obj = |k: &str| o.get(k).filter(|v| v.is_object()).cloned();
    let v = serde_json::json!({
        "subscription_id": sub,
        "remote_id": rid,
        "subscription_title": s("subscriptionTitle").unwrap_or_default(),
        "origin": s("origin").unwrap_or_default(),
        "kind": kind,
        "state": if withdrawn { "tombstone" } else { "current" },
        "version": n("version").unwrap_or(0),
        "created": s("created"),
        "updated": s("updated"),
        "observed_at": s("observedAt").unwrap_or_default(),
        "content_md": s("contentMd").unwrap_or_default(),
        "content_html": s("contentHtml").unwrap_or_default(),
        "author": obj("author"),
        "page": s("page"),
        "thumb": thumb,
        "hoppers": hoppers,
        "pinned_version_retained": n("pinnedVersionRetained"),
        "read_version": n("readVersion"),
        "stub_of": obj("stubOf"),
        "forked_from": obj("forkedFrom"),
        "transclusions": o.get("transclusions").filter(|v| v.is_array()),
        "lineage_known": !l0 && !withdrawn,
        "source_url": s("sourceUrl"),
        "generated": o.get("generated").filter(|v| v.is_array()),
    });
    serde_json::from_value(v).map_err(|e| e.to_string())
}

/// A hopper membership's held item (the contract's `ImportedItem`) as a
/// reading row, with the subscription's title and origin. `None` if it
/// doesn't read.
pub fn imported_item_row(item: &Value, title: &str, origin: &str) -> Option<ReadingItem> {
    let mut o = item.as_object()?.clone();
    let withdrawn = o.get("state").and_then(Value::as_str) == Some("tombstone");
    o.insert("withdrawn".into(), withdrawn.into());
    o.insert("subscriptionTitle".into(), title.into());
    o.insert("origin".into(), origin.into());
    let mut r = imported_row(&Value::Object(o)).ok()?;
    r.page = r
        .page
        .take()
        .and_then(|p| super::absolute_page(&r.origin, &p));
    Some(r)
}

/// `GET /api/reading`'s read-state flags: the server stores read state
/// (`readState`), and can clear it (`readStateClear`). Only the flags are read.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadingFlags {
    #[serde(default, deserialize_with = "crate::model::lenient")]
    pub read_state: Option<bool>,
    #[serde(default, deserialize_with = "crate::model::lenient")]
    pub read_state_clear: Option<bool>,
}

impl ReadingFlags {
    pub fn read_sync(&self) -> bool {
        self.read_state == Some(true)
    }

    pub fn read_clear(&self) -> bool {
        self.read_sync() && self.read_state_clear == Some(true)
    }
}

/// One row's read state as sent to the server: `(sub, remote_id)` is a
/// reading row, `version` the highest version read. `read_at` is when it
/// was read on this Mac (ISO-8601, UTC). It's also the payload of a queued
/// `read` op, so the snake_case names an older Burrow queued still read.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadMark {
    pub sub: String,
    #[serde(alias = "remote_id")]
    pub remote_id: String,
    pub version: u32,
    #[serde(default, alias = "read_at", skip_serializing_if = "Option::is_none")]
    pub read_at: Option<String>,
}

/// One row to mark unread (`POST /api/reading/unread`); also a queued
/// `unread` op's payload.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnreadMark {
    pub sub: String,
    #[serde(alias = "remote_id")]
    pub remote_id: String,
}

/// `PUT /api/reading/{sub}/{remoteId}/read` → whether the read was stored
/// (false: the server holds no such row, or the read predates a later
/// "mark unread"), and the version it holds now (null: unread).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadAck {
    pub stored: bool,
    #[serde(default)]
    pub read_version: Option<u32>,
}

/// At most this many entries per `POST /api/reading/read`.
pub const READ_BATCH_MAX: usize = 500;

#[cfg(test)]
mod responses_tests {
    use super::*;
    use serde_json::json;

    fn item(extra: Value) -> WireItem {
        let mut v = json!({ "id": "A", "revision": 1, "kind": "fragment", "status": "public", "version": 1 });
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
        // Contract 5: a withdrawn thread is still `kind: "thread"`.
        let mut w = item(json!({ "kind": "thread", "status": "withdrawn" }));
        w.resolve(false, "https://blyg.example.com/b");
        assert_eq!(
            w.permalink.as_deref(),
            Some("https://blyg.example.com/b/t/A")
        );
        assert_eq!(w.local_status(), Status::Withdrawn);
        let mut d = item(json!({ "version": 0 }));
        d.resolve(false, "https://blyg.example.com");
        assert_eq!(d.permalink, None, "a draft has no page");
    }

    #[test]
    fn an_item_reads_in_camel_case_with_its_revision() {
        let w = item(json!({ "revision": 7, "contentMd": "Hi", "stubOf": null, "dirty": true }));
        assert_eq!(w.revision, 7);
        assert_eq!(w.content_md, "Hi");
        assert!(w.dirty);
    }

    #[test]
    fn a_withdrawn_version_is_the_endcap() {
        let v: Vec<WireVersion> = serde_json::from_value(json!([
            { "version": 1, "publishedAt": "t1", "note": null, "pinned": true, "kind": "fragment" },
            { "version": 2, "publishedAt": "t2", "note": null, "pinned": false, "kind": "withdrawn" },
        ]))
        .unwrap();
        let v: Vec<Version> = v.into_iter().map(WireVersion::into_version).collect();
        assert!(v[0].pinned && !v[0].endcap && v[0].published_at == "t1");
        assert!(v[1].endcap);
    }

    #[test]
    fn a_mention_author_is_an_object() {
        let w: WireMention = serde_json::from_value(json!({
            "id": "m", "targetItemId": "A", "status": "verified", "relation": "stub",
            "source": "https://other.example.com/t/x", "sourceAuthor": {"name": "Ana", "url": null},
            "firstSeen": "t", "hidden": false,
        }))
        .unwrap();
        let m: Mention = w.into();
        assert_eq!(m.source_author.unwrap().name.as_deref(), Some("Ana"));
        assert_eq!(m.target_item_id, "A");
    }

    #[test]
    fn a_cursor_page_ends_at_a_null_cursor() {
        let p: CursorPage<Value> = serde_json::from_value(
            json!({ "items": [1, 2], "page": { "limit": 2, "nextCursor": "c2" } }),
        )
        .unwrap();
        assert_eq!(p.page.next_cursor.as_deref(), Some("c2"));
        let last: CursorPage<Value> = serde_json::from_value(
            json!({ "items": [], "page": { "limit": 2, "nextCursor": null } }),
        )
        .unwrap();
        assert!(last.page.next_cursor.is_none());
        // The old `{items, total, offset, limit}` shape isn't a page.
        assert!(
            serde_json::from_value::<CursorPage<Value>>(
                json!({ "items": [], "total": 0, "offset": 0, "limit": 100 })
            )
            .is_err()
        );
    }

    #[test]
    fn read_marks_go_out_in_camel_case_and_old_queued_ones_still_read() {
        let m = ReadMark {
            sub: "S".into(),
            remote_id: "R".into(),
            version: 2,
            read_at: Some("2030-01-01T00:00:00Z".into()),
        };
        assert_eq!(
            serde_json::to_value(&m).unwrap(),
            json!({ "sub": "S", "remoteId": "R", "version": 2, "readAt": "2030-01-01T00:00:00Z" })
        );
        let old: ReadMark =
            serde_json::from_str(r#"{"sub":"S","remote_id":"R","version":2,"read_at":"t"}"#)
                .unwrap();
        assert_eq!(old.remote_id, "R");
        assert_eq!(old.read_at.as_deref(), Some("t"));
        let u: UnreadMark = serde_json::from_str(r#"{"sub":"S","remote_id":"R"}"#).unwrap();
        assert_eq!(
            serde_json::to_value(&u).unwrap(),
            json!({ "sub": "S", "remoteId": "R" })
        );
    }

    #[test]
    fn the_read_flags_are_camel_case() {
        let f: ReadingFlags = serde_json::from_value(
            json!({ "items": [], "readState": true, "readStateClear": true }),
        )
        .unwrap();
        assert!(f.read_sync() && f.read_clear());
        // The 4.x snake_case names aren't read.
        let old: ReadingFlags =
            serde_json::from_value(json!({ "read_state": true, "read_state_clear": true }))
                .unwrap();
        assert!(!old.read_sync());
    }

    #[test]
    fn settings_and_subscriptions_read_in_camel_case() {
        let s: Settings = serde_json::from_value::<WireSettings>(json!({
            "siteTitle": "T", "siteUrl": "https://blyg.example.com", "showResponsesDefault": true,
            "pickerTyping": "panel", "authorLinks": [{"label": "L", "url": "https://blyg.example.com/l"}],
            "customTheme": null,
        }))
        .unwrap()
        .into();
        assert_eq!(s.site_title.as_deref(), Some("T"));
        assert_eq!(s.show_responses_default, Some(true));
        assert_eq!(s.picker_typing, Some(PickerTyping::Panel));
        assert_eq!(s.author_links.len(), 1);
        let sub: Subscription = serde_json::from_value::<WireSubscription>(json!({
            "id": "S", "kind": "blyg", "origin": "https://ada.example.com/",
            "feedUrl": "https://ada.example.com/feed.xml", "title": "Ada", "status": "active",
            "inBlogroll": true, "titleFollowsSource": true, "lastPollAt": null,
        }))
        .unwrap()
        .into();
        assert!(sub.in_blogroll && sub.title_follows_source);
        assert_eq!(sub.feed_url, "https://ada.example.com/feed.xml");
    }
}

#[cfg(test)]
mod reading_page_tests {
    use super::*;

    /// A row as blygger-studio 0.49.12's `listImportedReading` serves it,
    /// with invented content.
    const ROWS: &str = r#"[
      {"subscriptionId":"S1","subscriptionTitle":"Ada","origin":"https://ada.example.com/",
       "remoteId":"R2","kind":"thread","withdrawn":false,"l0":false,"version":3,"readVersion":2,
       "created":"2030-01-01T00:00:00Z","updated":"2030-01-02T00:00:00Z","observedAt":"2030-01-02T00:00:05Z",
       "contentMd":"Re: ![[X]]","contentHtml":"<p>Re</p>","author":{"name":"Ada","url":"https://ada.example.com/"},
       "page":"t/R2/","sourceUrl":"https://ada.example.com/t/R2/","pinnedVersionRetained":null,"thumb":1,
       "hoppers":["H1"],"transclusions":[{"id":"X","version":2,"origin":"https://bo.example.com/",
       "cited":{"author":"Bo","source":"Bo's blyg"}}],
       "stubOf":{"origin":"https://bo.example.com/","id":"X","version":2,"cited":{"author":"Bo"}},
       "forkedFrom":null,"generated":[{"sources":[],"model":"m","at":"2030-01-01T00:00:00Z"}]},
      {"subscriptionId":"S1","subscriptionTitle":"Ada","origin":"https://ada.example.com/",
       "remoteId":"R1","kind":"fragment","withdrawn":true,"l0":false,"version":2,"readVersion":null,
       "created":null,"updated":null,"observedAt":"2030-01-01T00:00:05Z","contentMd":"","contentHtml":"",
       "author":{"name":null,"url":null},"page":null,"sourceUrl":null,"pinnedVersionRetained":1,"thumb":null,"hoppers":[],
       "transclusions":null,"stubOf":null,"forkedFrom":null,"generated":null}
    ]"#;

    #[test]
    fn imported_rows_read_in_the_apps_shape() {
        let rows: Vec<Value> = serde_json::from_str(ROWS).unwrap();
        let a = imported_row(&rows[0]).unwrap();
        assert_eq!(
            (a.subscription_id.as_str(), a.remote_id.as_str()),
            ("S1", "R2")
        );
        assert_eq!(a.kind, Kind::Thread);
        assert_eq!(a.state, "current");
        assert_eq!(a.read_version, Some(2));
        assert_eq!(a.thumb, Some(1));
        assert_eq!(a.hoppers, vec!["H1".to_string()]);
        assert_eq!(a.author.as_ref().unwrap().name.as_deref(), Some("Ada"));
        assert_eq!(a.stub_of.as_ref().unwrap().id.as_deref(), Some("X"));
        assert_eq!(a.transclusions.len(), 1);
        assert!(a.lineage_known);
        assert_eq!(a.created.as_deref(), Some("2030-01-01T00:00:00Z"));
        assert_eq!(
            a.source_url.as_deref(),
            Some("https://ada.example.com/t/R2/")
        );
        assert_eq!(a.generated.as_ref().map(Vec::len), Some(1));
        let w = imported_row(&rows[1]).unwrap();
        assert_eq!(w.state, "tombstone");
        assert_eq!(w.pinned_version_retained, Some(1));
        assert_eq!(w.read_version, None);
        assert_eq!(
            w.created, None,
            "a date the studio couldn't normalize is null"
        );
        assert!(!w.lineage_known, "a withdrawn row says nothing of lineage");
    }

    #[test]
    fn a_row_without_its_identity_fails() {
        assert!(imported_row(&serde_json::json!({ "subscriptionId": "S" })).is_err());
    }

    #[test]
    fn a_hopper_item_becomes_a_row() {
        let item = serde_json::json!({
            "subscriptionId": "S", "remoteId": "R", "kind": "fragment", "state": "current",
            "version": 1, "created": "2030-01-01T00:00:00Z", "updated": null,
            "observedAt": "2030-01-01T00:00:00Z", "contentMd": "x", "contentHtml": "<p>x</p>",
            "contentHash": null, "l0": false, "pinnedVersionRetained": null, "page": "f/R/",
            "author": {"name": "Ada", "url": "https://ada.example.com/", "bio": "Writes."},
            "media": null, "transclusions": null, "stubOf": null, "forkedFrom": null, "generated": null,
        });
        let r = imported_item_row(&item, "Ada", "https://ada.example.com/").unwrap();
        assert_eq!(r.subscription_title, "Ada");
        assert_eq!(r.page.as_deref(), Some("https://ada.example.com/f/R/"));
        assert!(r.lineage_known);
        // The whole author object is kept (protocol §5.5).
        assert_eq!(r.author.unwrap().extra["bio"], "Writes.");
        let mut gone = item.clone();
        gone["state"] = "tombstone".into();
        let r = imported_item_row(&gone, "Ada", "https://ada.example.com/").unwrap();
        assert_eq!(r.state, "tombstone");
    }

    #[test]
    fn a_stub_of_keeps_its_citation() {
        let it = imported_row(&serde_json::json!({
            "subscriptionId": "S", "remoteId": "R", "kind": "thread",
            "stubOf": {"origin": "https://bo.example.com/", "id": "X", "version": 2,
              "cited": {"source": "Bo", "url": "https://bo.example.com/t/X/", "retrieved": "2030-01-01T00:00:00Z"}},
            "forkedFrom": {"origin": "https://bo.example.com/", "id": "Y", "version": 1,
              "cited": {"source": "Bo", "url": "https://bo.example.com/f/Y/", "retrieved": "2030-01-01T00:00:00Z"}},
        }))
        .unwrap();
        let c = it.stub_of.as_ref().unwrap().cited.as_ref().unwrap();
        assert!(c.complete());
        assert_eq!(c.source.as_deref(), Some("Bo"));
        let f = it.forked_from.as_ref().unwrap().cited.as_ref().unwrap();
        assert_eq!(f.url.as_deref(), Some("https://bo.example.com/f/Y/"));
    }
}
