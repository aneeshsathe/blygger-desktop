//! Domain types shared by the store, the API client and the UI.
//!
//! Wire types mirror the blyg Worker's owner API (see docs/SPEC.md §API). Local
//! types (`Item`, `SyncStatus`, …) are what the UI sees: every item has a
//! `LocalId` from the moment it's created, even offline, and gains a server id
//! once the outbox has pushed its `POST /api/items`.

use serde::{Deserialize, Serialize};

/// Client-generated id, stable for the life of the item on this machine.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LocalId(pub String);

/// Server id: 26-char base32 assigned by the Worker.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ServerId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Fragment,
    Thread,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Draft,
    Public,
    Withdrawn,
    // --- scratch notes ---
    /// A local-only scratch note (docs/SPEC.md § Scratch notes): kept in the
    /// local store, searchable and editable, but never enqueued, pushed or
    /// pulled. `Backend::promote` turns it into a `Draft` (same `LocalId`);
    /// nothing turns an item back into scratch.
    Scratch,
}

/// Upstream caps fragments at 1000 chars (counted after TK stripping; we count
/// `chars()` of the working copy, which is what the user sees).
pub const FRAGMENT_LIMIT: usize = 1000;

/// Strip `[TK]instruction[=]output[/TK]` scopes to their output (an
/// ungenerated scope contributes nothing), mirroring the Worker's linear scan
/// in `tk.ts::parseScopes`: no nesting, and an unterminated scope ends the scan.
pub fn strip_tk(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    let mut rest = md;
    while let Some(open) = rest.find("[TK]") {
        let after = &rest[open + 4..];
        let Some(close) = after.find("[/TK]") else {
            break;
        };
        out.push_str(&rest[..open]);
        let body = &after[..close];
        if let Some(eq) = body.find("[=]") {
            out.push_str(&body[eq + 3..]);
        }
        rest = &after[close + 5..];
    }
    out.push_str(rest);
    out
}

/// Published length in UTF-16 code units, the unit the Worker's 1000 cap uses.
pub fn published_len(md: &str) -> usize {
    strip_tk(md).encode_utf16().count()
}

/// `{origin, id, version}` reference to an item on some blyg (stub_of, forked_from).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteRef {
    pub origin: String,
    pub id: String,
    pub version: u32,
}

/// What the UI renders. Content is the local working copy.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub local_id: LocalId,
    pub server_id: Option<ServerId>,
    /// Authored kind (a thread stays a thread even if withdrawn).
    pub kind: Kind,
    pub status: Status,
    /// Last published version; 0 = never published.
    pub version: u32,
    /// Working copy differs from the last published version.
    pub dirty: bool,
    pub content_md: String,
    pub created: String,
    pub updated: String,
    pub permalink: Option<String>,
    pub stub_of: Option<RemoteRef>,
    pub forked_from: Option<RemoteRef>,
    /// Whether the item's page shows its verified responses right now (the
    /// effective state, after any global default).
    pub show_responses: bool,
    /// The item's own choice about responses (studio 0.8). `None` when the
    /// server doesn't report one (pre-0.8), which offers only show and hide.
    pub responses_mode: Option<ResponsesMode>,
    /// Local edits not yet acknowledged by the server.
    pub pending_sync: bool,
    /// Server changed this item underneath a local edit; see `Backend::resolve_conflict`.
    pub conflict: bool,
}

impl Item {
    /// The first line with readable text, as plain text (see [`plain_title`]).
    pub fn title(&self) -> String {
        plain_title(&self.content_md).unwrap_or_else(|| "Untitled".into())
    }

    /// Length exactly as the Worker measures it at publish: TK markup stripped
    /// to its output, then JavaScript `.length` (UTF-16 code units, so an emoji
    /// counts as 2). See apps/blyg/src/model.rs `FragmentTooLongError`.
    pub fn char_count(&self) -> usize {
        published_len(&self.content_md)
    }

    pub fn over_limit(&self) -> bool {
        self.kind == Kind::Fragment && self.char_count() > FRAGMENT_LIMIT
    }
}

/// What one item does about showing its verified responses
/// (`PUT /api/items/:id/responses {mode}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResponsesMode {
    /// No choice of its own: follows the blyg's `show_responses_default`.
    Default,
    Show,
    Hide,
}

impl ResponsesMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ResponsesMode::Default => "default",
            ResponsesMode::Show => "show",
            ResponsesMode::Hide => "hide",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "default" => Some(ResponsesMode::Default),
            "show" => Some(ResponsesMode::Show),
            "hide" => Some(ResponsesMode::Hide),
            _ => None,
        }
    }

    /// The server's `responses_override` (`1`, `0`, or `null` for none).
    pub fn from_override(o: Option<i64>) -> Self {
        match o {
            None => ResponsesMode::Default,
            Some(0) => ResponsesMode::Hide,
            Some(_) => ResponsesMode::Show,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Version {
    pub version: u32,
    pub published_at: String,
    pub note: Option<String>,
    pub pinned: bool,
    /// The withdraw marker version (no content).
    pub endcap: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubscriptionKind {
    Blyg,
    Rss,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subscription {
    pub id: String,
    pub kind: SubscriptionKind,
    pub origin: String,
    pub feed_url: String,
    pub title: String,
    /// "active" | "paused" | "degraded"
    pub status: String,
    pub in_blogroll: bool,
}

/// Result of `POST /api/subscriptions` without `confirm` — what the URL resolved to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubscribePreview {
    pub kind: SubscriptionKind,
    pub title: String,
    pub origin: Option<String>,
    pub feed_url: Option<String>,
    pub site_mismatch: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Author {
    pub name: Option<String>,
    pub url: Option<String>,
}

/// One item from a subscription (imported_items), for the reading view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadingItem {
    pub subscription_id: String,
    pub remote_id: String,
    pub subscription_title: String,
    pub origin: String,
    pub kind: Kind,
    /// "current" | "tombstone"
    pub state: String,
    pub version: u32,
    pub created: Option<String>,
    pub updated: Option<String>,
    pub observed_at: String,
    pub content_md: String,
    pub content_html: String,
    pub author: Option<Author>,
    /// Canonical page URL, if the source declared one.
    pub page: Option<String>,
    /// 1 = thumbs up, -1 = thumbs down.
    pub thumb: Option<i8>,
    pub hoppers: Vec<String>,
    /// On a tombstone: the pinned version whose content is retained (from the
    /// server's `pinned_version_retained`, or a pin this client cached). The
    /// content fields then hold that pinned version, and the UI attributes it
    /// with a link to the pin (`pin_url`). `None` on a tombstone = no content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_version_retained: Option<u32>,
    /// Read state: the highest version the user has seen, set by
    /// `Backend::mark_read`. `None` = unread. Only the number is kept, never
    /// the text of that version (spec §8.4: unpinned history of other people's
    /// posts is not retained). See `Backend::pinned_diff_base`. On a pulled
    /// reading page it's the server's copy (owner-API extension 5), which the
    /// store merges as `max(local, server)`. Lenient: junk reads as `None`.
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub read_version: Option<u32>,
    // --- profiles ---
    /// `stub_of` from the item document (v0.3, threads only): the post this
    /// one replies to. Tolerant: a shape we don't understand reads as `None`.
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub stub_of: Option<StubOf>,
    /// `forked_from` (v0.3): the pinned version this post descends from.
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub forked_from: Option<RemoteRef>,
    /// `transclusions[]`: the posts this thread quotes (`origin` only for
    /// remote ones). Entries we can't read are skipped.
    #[serde(
        default,
        deserialize_with = "lenient_vec",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub transclusions: Vec<TransclusionRef>,
}

// --- profiles ---

/// `stub_of`: `{origin, id, version}` for a blyg target, or `{url}` for any
/// web page (digest §2.3). Every field is optional so an odd shape still
/// reads.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StubOf {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl StubOf {
    /// Where to go for "↳ stub of …": the blyg origin, else the page URL.
    pub fn target(&self) -> Option<&str> {
        self.origin
            .as_deref()
            .or(self.url.as_deref())
            .filter(|s| !s.trim().is_empty())
    }
}

/// One `transclusions[]` entry: `{id, version, origin?, cited?}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransclusionRef {
    pub id: String,
    #[serde(default)]
    pub version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    /// The citation frozen when the quote was made (protocol 0.3 §16.1;
    /// remote entries only). Remote data: shown as plain text, never markup.
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub cited: Option<Cited>,
}

/// A reference's human half (`cited`): who and what was quoted, as the
/// quoting blyg saw it. Every field is optional; one we can't read is absent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cited {
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub source: Option<String>,
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub author: Option<String>,
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub excerpt: Option<String>,
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub url: Option<String>,
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub retrieved: Option<String>,
}

impl Cited {
    /// Who to name: the author, else the source blyg, trimmed and non-empty.
    pub fn name(&self) -> Option<&str> {
        [&self.author, &self.source]
            .into_iter()
            .flatten()
            .map(|s| s.trim())
            .find(|s| !s.is_empty())
    }
}

/// A post's lineage: what it stubs, forks and quotes. On a reading item the
/// owner API may leave these out, so the item document's copy (carried on the
/// current changelog row, see [`RemoteVersion::lineage`]) fills the gaps.
/// Every field is lenient: a shape we don't understand reads as absent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lineage {
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub stub_of: Option<StubOf>,
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub forked_from: Option<RemoteRef>,
    #[serde(
        default,
        deserialize_with = "lenient_vec",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub transclusions: Vec<TransclusionRef>,
}

impl Lineage {
    pub fn is_empty(&self) -> bool {
        self.stub_of.is_none() && self.forked_from.is_none() && self.transclusions.is_empty()
    }

    /// Each field from `self` when present, else from `fallback`.
    pub fn or(mut self, fallback: &Lineage) -> Lineage {
        if self.stub_of.is_none() {
            self.stub_of = fallback.stub_of.clone();
        }
        if self.forked_from.is_none() {
            self.forked_from = fallback.forked_from.clone();
        }
        if self.transclusions.is_empty() {
            self.transclusions = fallback.transclusions.clone();
        }
        self
    }

    /// The item document's lineage, from a cached or fetched changelog: the
    /// current row's copy.
    pub fn of_changelog(changelog: &[RemoteVersion]) -> Option<&Lineage> {
        changelog
            .iter()
            .find(|v| v.current)
            .map(|v| &v.lineage)
            .filter(|l| !l.is_empty())
    }
}

/// Deserialize `T`, or `None` when the value is null or doesn't fit
/// (tolerance rule: never reject a whole item over one odd field).
pub(crate) fn lenient<'de, D, T>(d: D) -> std::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let v = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(v).ok())
}

/// A list where entries that don't fit are dropped (and a non-list is empty).
pub(crate) fn lenient_vec<'de, D, T>(d: D) -> std::result::Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let v = serde_json::Value::deserialize(d)?;
    Ok(match v {
        serde_json::Value::Array(a) => a
            .into_iter()
            .filter_map(|x| serde_json::from_value(x).ok())
            .collect(),
        _ => vec![],
    })
}

impl ReadingItem {
    /// `stub_of`, `forked_from` and `transclusions` as this row holds them.
    pub fn lineage(&self) -> Lineage {
        Lineage {
            stub_of: self.stub_of.clone(),
            forked_from: self.forked_from.clone(),
            transclusions: self.transclusions.clone(),
        }
    }

    /// Fill the lineage fields this row lacks from `doc` (the item
    /// document's copy). Fields the row already has win.
    pub fn fill_lineage(&mut self, doc: &Lineage) {
        let l = self.lineage().or(doc);
        self.stub_of = l.stub_of;
        self.forked_from = l.forked_from;
        self.transclusions = l.transclusions;
    }

    pub fn is_unread(&self) -> bool {
        self.read_version.is_none()
    }

    /// The post's own date for the reading list (issue #6): when the author
    /// last wrote it (`updated`, else `created`), falling back to when your
    /// blyg first imported it (`observed_at`) only when neither exists or
    /// parses. `edited` = `updated` is meaningfully later than `created`
    /// (more than [`EDIT_GRACE_MS`]), so the UI says "edited …".
    pub fn post_time(&self) -> PostTime {
        let ms = |s: &Option<String>| s.as_deref().and_then(crate::util::parse_ts_ms);
        let (created, updated) = (ms(&self.created), ms(&self.updated));
        let edited = matches!((created, updated), (Some(c), Some(u)) if u - c > EDIT_GRACE_MS);
        match updated.or(created) {
            Some(t) => PostTime {
                at: crate::util::iso_from_ms(t),
                edited,
                observed: false,
            },
            None => PostTime {
                at: crate::util::parse_ts_ms(&self.observed_at)
                    .map(crate::util::iso_from_ms)
                    .unwrap_or_else(|| self.observed_at.clone()),
                edited: false,
                observed: true,
            },
        }
    }

    /// The reading list's sort key, newest first: [`Self::post_time`] as a
    /// normalized UTC timestamp (`2026-09-24T12:34:56.789Z`), so an edit by
    /// the author moves the post up. Stored in `reading.sort_at`.
    pub fn sort_at(&self) -> String {
        self.post_time().at
    }

    /// Read before, and the author has published a newer version since.
    pub fn edited_since_read(&self) -> bool {
        self.read_version.is_some_and(|v| v < self.version)
    }

    /// The public pin document backing a retained tombstone, for attribution.
    pub fn pin_url(&self) -> Option<String> {
        self.pinned_version_retained
            .and_then(|v| pin_doc_url(&self.origin, &self.remote_id, v))
    }
}

// --- responses --- (issue #7) who quoted, stubbed or forked a post.

/// One post pointing at another: its `stub_of`, `forked_from`, or a
/// `transclusions[]` entry. `origin` is normalized
/// ([`crate::profile::normalize_origin`]) and `id` lowercased, so two
/// spellings of the same post compare equal.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PostRef {
    pub origin: String,
    pub id: String,
    pub relation: crate::profile::Relation,
    pub version: Option<u32>,
}

/// A post in your reading list that quotes, stubs or forks another post
/// ("seen in your network"). A list entry, never a count.
#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    /// The responding post.
    pub item: ReadingItem,
    pub relation: crate::profile::Relation,
    /// The version of the target it responds to, when it says.
    pub version: Option<u32>,
}

/// `(origin, id)` as [`PostRef`] keys them.
pub fn post_key(origin: &str, id: &str) -> (String, String) {
    (
        crate::profile::normalize_origin(origin)
            .unwrap_or_else(|| origin.trim().trim_end_matches('/').to_lowercase()),
        id.trim().to_lowercase(),
    )
}

impl ReadingItem {
    /// The posts this one points at. A quote without an origin is from the
    /// quoting post's own blyg; a `stub_of` that's only a URL is left out.
    pub fn references(&self) -> Vec<PostRef> {
        use crate::profile::Relation;
        let mut out = Vec::new();
        let mut push = |origin: &str, id: &str, relation, version| {
            if id.trim().is_empty() || origin.trim().is_empty() {
                return;
            }
            let (origin, id) = post_key(origin, id);
            let r = PostRef {
                origin,
                id,
                relation,
                version,
            };
            if !out.contains(&r) {
                out.push(r);
            }
        };
        if let Some(s) = &self.stub_of
            && let (Some(o), Some(id)) = (&s.origin, &s.id)
        {
            push(o, id, Relation::Stubs, s.version);
        }
        if let Some(f) = &self.forked_from {
            push(&f.origin, &f.id, Relation::Forks, Some(f.version));
        }
        for t in &self.transclusions {
            let o = t.origin.as_deref().unwrap_or(&self.origin);
            push(o, &t.id, Relation::Quotes, t.version);
        }
        out
    }
}

/// How much later than `created` an `updated` must be to count as an edit
/// ("edited 3d ago" in the reading list): ten minutes, so a quick fix right
/// after publishing doesn't.
pub const EDIT_GRACE_MS: i64 = 10 * 60 * 1000;

/// A reading item's date as the UI shows it (see [`ReadingItem::post_time`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostTime {
    /// Normalized UTC RFC 3339 (`…T…Z`), or `observed_at` as given when even
    /// that doesn't parse.
    pub at: String,
    /// The author edited it after publishing: show "edited …".
    pub edited: bool,
    /// The post carried no usable date: this is when it was imported.
    pub observed: bool,
}

/// `"sha256:" + hex(SHA-256(content_md as UTF-8))`, the protocol's
/// `content_hash` (digest §1.3; upstream `util.ts::contentHash`). It covers
/// `content_md` only: not author, media or HTML.
pub fn content_hash(content_md: &str) -> String {
    format!(
        "sha256:{}",
        crate::util::hex(&crate::util::sha256(content_md.as_bytes()))
    )
}

/// Resolve an origin-relative path of the public surface against `origin`,
/// which may be mounted anywhere (`https://x.example/`, `https://x.example/blyg/`).
/// A missing trailing slash on the origin is tolerated. `None` unless the
/// origin is an absolute http(s) URL.
pub fn origin_url(origin: &str, path: &str) -> Option<String> {
    let base = if origin.ends_with('/') {
        origin.to_string()
    } else {
        format!("{origin}/")
    };
    let base = url::Url::parse(&base).ok()?;
    if !matches!(base.scheme(), "http" | "https") || base.cannot_be_a_base() {
        return None;
    }
    base.join(path).ok().map(|u| u.to_string())
}

/// One path segment, percent-encoded so an id can't climb out of `items/`.
fn path_segment(id: &str) -> Option<String> {
    if id.is_empty() || id == "." || id == ".." {
        return None;
    }
    Some(
        url::form_urlencoded::byte_serialize(id.as_bytes())
            .collect::<String>()
            .replace('+', "%20"),
    )
}

/// `{origin}items/{id}.json`, the public item document.
pub fn item_doc_url(origin: &str, id: &str) -> Option<String> {
    origin_url(origin, &format!("items/{}.json", path_segment(id)?))
}

/// `{origin}items/{id}/v{n}.json`, a pinned version.
pub fn pin_doc_url(origin: &str, id: &str, version: u32) -> Option<String> {
    origin_url(
        origin,
        &format!("items/{}/v{version}.json", path_segment(id)?),
    )
}

/// One row of someone else's post's public `changelog` (spec §5.2). Metadata
/// only: an unpinned, non-current row has no content anywhere, and the UI must
/// not offer, imply or hint at any (§8.4). Only `current` and `pinned` rows
/// can be opened (`Backend::remote_pinned` for pins).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteVersion {
    pub version: u32,
    pub at: String,
    pub note: Option<String>,
    pub pinned: bool,
    /// The version the item document currently serves.
    pub current: bool,
    /// On the current row only: the images the item document attaches
    /// (`media[]`, §5.4), which public pages show after the text but which
    /// are not part of `content_html`. Resolved against the origin the
    /// document was fetched from; only http(s) URLs are kept.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media: Vec<RemoteMedia>,
    /// On the current row only: the item document's `stub_of`,
    /// `forked_from` and `transclusions`, which the owner reading API
    /// doesn't send. Cached with the changelog.
    #[serde(default, skip_serializing_if = "Lineage::is_empty")]
    pub lineage: Lineage,
}

/// An image attached to someone else's post (an item document's `media[]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteMedia {
    /// Absolute http(s) URL.
    pub url: String,
    #[serde(default)]
    pub alt: Option<String>,
}

impl RemoteVersion {
    /// Has public content: the current version or a pinned one.
    pub fn openable(&self) -> bool {
        self.current || self.pinned
    }
}

/// What the version browser lists for someone else's post: only the current
/// and pinned versions, oldest first. Unpinned history doesn't appear at all
/// (the full changelog stays available for the reading list's notes).
pub fn shown_versions(changelog: &[RemoteVersion]) -> Vec<RemoteVersion> {
    changelog.iter().filter(|v| v.openable()).cloned().collect()
}

/// A pinned version of someone else's post, from `{origin}items/{id}/v{n}.json`.
/// Pins are immutable, so these are cached forever.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PinnedVersion {
    pub version: u32,
    pub at: String,
    pub note: Option<String>,
    pub content_md: String,
    pub content_html: String,
    /// As served (`"sha256:<hex>"`).
    pub content_hash: String,
    pub origin: String,
    pub id: String,
    /// Carried verbatim (spec §5.5).
    #[serde(default)]
    pub author: Option<Author>,
    /// The served `content_hash` doesn't match `content_hash(content_md)`.
    /// A flag, not a failure (spec §13: adopt the content anyway).
    #[serde(default)]
    pub hash_mismatch: bool,
}

impl PinnedVersion {
    /// The pin document URL, for "retained via a pinned version" attribution.
    pub fn url(&self) -> Option<String> {
        pin_doc_url(&self.origin, &self.id, self.version)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mention {
    pub id: String,
    pub target_item_id: String,
    /// "pending" | "verified" | "failed" | "gone"
    pub status: String,
    /// "stub" | "transclusion" | "fork"
    pub relation: Option<String>,
    pub source: String,
    pub source_origin: Option<String>,
    pub source_id: Option<String>,
    pub source_kind: Option<String>,
    pub source_version: Option<u32>,
    pub source_author: Option<Author>,
    pub first_seen: String,
    pub verified_at: Option<String>,
    pub hidden: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthorLink {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub site_title: Option<String>,
    pub author_name: Option<String>,
    pub author_bio: Option<String>,
    pub site_url: Option<String>,
    pub theme: Option<String>,
    pub avatar_media_id: Option<String>,
    #[serde(default)]
    pub author_links: Vec<AuthorLink>,
    /// Whether the blyg receives webmentions (protocol 0.3; on by default).
    /// `None` when the server doesn't report it, and then never sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accept_mentions: Option<bool>,
    /// IANA zone the blyg renders dates in (studio 0.8); rendering only.
    /// `None` when unset or unreported, and then never sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    /// Whether items without a choice of their own show their responses
    /// (studio 0.8). `None` when the server doesn't report it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_responses_default: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hopper {
    pub id: String,
    pub name: String,
    pub slug: Option<String>,
    pub public: bool,
    pub count: u32,
}

// --- reader folders ---

/// A folder of subscriptions in the Reader's sources pane. Local only: kept
/// in this Mac's store, never sent to the blyg (unlike the server's
/// hoppers, which are for posts). A subscription is in at most one folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folder {
    pub id: String,
    pub name: String,
    /// Display order, 0-based, dense.
    pub position: u32,
}

// --- end reader folders ---

/// A published fragment a generated TK scope drew on (`![[id]]` inside it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceSource {
    pub id: String,
    /// The version used; `None` lets the server record the current one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
}

/// Who generated one TK scope's output, recorded when the app generates it
/// (docs/SPEC.md § Client-recorded provenance). Never holds the instruction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeProvenance {
    /// The model that answered (`GenResult::model`).
    pub model: String,
    #[serde(default)]
    pub sources: Vec<ProvenanceSource>,
    /// ISO-8601 generation time; `None` = now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStatus {
    /// Everything local is on the server.
    Synced,
    /// Local changes waiting for the debounce.
    Saving,
    /// A push to the server is in flight right now. (Additive: the live
    /// backend emits it while an outbox op runs.)
    Syncing,
    /// Network unreachable; `pending` outbox entries are waiting.
    Offline { pending: usize },
    /// Last attempt failed for a non-network reason (auth, 5xx).
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    KeepMine,
    TakeServer,
    /// Keep the server copy on the item; put mine in a new local draft.
    KeepBoth,
}

/// Pushed from the core to the UI (from a background thread).
#[derive(Debug, Clone, PartialEq)]
pub enum CoreEvent {
    /// Items list changed (sync pulled something, an id was assigned, …).
    ItemsChanged,
    SyncStatus(SyncStatus),
    Conflict {
        local_id: LocalId,
        mine: String,
        theirs: String,
    },
    ReadingChanged,
    /// A background operation failed; human-readable.
    Error(String),
    /// The server turned out to be older than blygger-studio 0.9 (sent once
    /// per session, when first noticed). See `Backend::server_outdated`.
    ServerOutdated,
}

#[cfg(test)]
mod len_tests {
    use super::*;

    #[test]
    fn strips_tk_to_output() {
        assert_eq!(strip_tk("a [TK]say hi[=]hello[/TK] b"), "a hello b");
        assert_eq!(strip_tk("a [TK]not yet[/TK] b"), "a  b");
        assert_eq!(strip_tk("a [TK]unterminated"), "a [TK]unterminated");
    }

    #[test]
    fn content_hash_is_sha256_of_content_md() {
        assert_eq!(
            content_hash("abc"),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn public_urls_resolve_against_any_mount() {
        assert_eq!(
            item_doc_url("https://x.example/", "abc").as_deref(),
            Some("https://x.example/items/abc.json")
        );
        assert_eq!(
            item_doc_url("https://x.example/blyg/", "abc").as_deref(),
            Some("https://x.example/blyg/items/abc.json")
        );
        assert_eq!(
            pin_doc_url("https://x.example/deep/mount", "abc", 2).as_deref(),
            Some("https://x.example/deep/mount/items/abc/v2.json"),
            "missing trailing slash tolerated"
        );
        assert_eq!(
            item_doc_url("https://x.example/b/", "../up").as_deref(),
            Some("https://x.example/b/items/..%2Fup.json"),
            "an id can't climb out of items/"
        );
        assert_eq!(item_doc_url("https://x.example/", ".."), None);
        assert_eq!(item_doc_url("file:///tmp/", "abc"), None);
        assert_eq!(item_doc_url("not a url", "abc"), None);
    }

    #[test]
    fn counts_like_javascript() {
        assert_eq!(published_len("🎺"), 2);
        assert_eq!(published_len("é"), 1);
        assert_eq!(published_len("x [TK]long instruction here[=]yz[/TK]"), 4);
    }
}

/// A list-row title from Markdown: the first line with readable text, TK
/// markup reduced to its output, and Markdown syntax removed. A leading
/// heading is the title (studio 0.8: items stay titleless on the wire; this
/// is derivation). The title is the author's own words, never quoted text,
/// so transclusion lines (`![[id]]`), the `>` quote run attached to one (a
/// partial transclusion) and blockquote lines generally are skipped, as are
/// image-only lines; links keep their text, and an internal link (`[[id]]`)
/// shows as "↗"; emphasis and code marks go. `None` when no line has any
/// text of the author's own.
pub fn plain_title(md: &str) -> Option<String> {
    strip_tk(md)
        .lines()
        .filter(|l| !l.trim_start().starts_with('>'))
        .map(plain_line)
        .find(|l| !l.is_empty())
}

fn plain_line(line: &str) -> String {
    let t = line.trim();
    let t = t.trim_start_matches('#').trim_start();
    let t = t
        .strip_prefix("- ")
        .or_else(|| t.strip_prefix("* "))
        .unwrap_or(t);
    let mut out = String::new();
    let c: Vec<char> = t.chars().collect();
    let mut i = 0;
    while i < c.len() {
        // ![[id]] (a transclusion): dropped.
        if c[i] == '!'
            && c.get(i + 1) == Some(&'[')
            && c.get(i + 2) == Some(&'[')
            && let Some(end) = find(&c, i + 3, &[']', ']'])
        {
            i = end + 2;
            continue;
        }
        // [[id]] (an internal link): an arrow, as the list has no titles.
        if c[i] == '['
            && c.get(i + 1) == Some(&'[')
            && let Some(end) = find(&c, i + 2, &[']', ']'])
            && end > i + 2
            && c[i + 2..end].iter().all(char::is_ascii_alphanumeric)
        {
            out.push('↗');
            i = end + 2;
            continue;
        }
        // ![alt](url) → alt; [text](url) → text.
        let img = c[i] == '!' && c.get(i + 1) == Some(&'[');
        let open = if img { i + 1 } else { i };
        if c[open] == '['
            && let Some(close) = find(&c, open + 1, &[']', '('])
            && let Some(paren) = find(&c, close + 2, &[')'])
        {
            out.extend(&c[open + 1..close]);
            i = paren + 1;
            continue;
        }
        match c[i] {
            '*' | '`' => {}
            // `_` marks emphasis at a word edge; inside a word it's a character.
            '_' => {
                let prev = i.checked_sub(1).map(|j| c[j]);
                let next = c.get(i + 1).copied();
                let word = |ch: Option<char>| ch.is_some_and(char::is_alphanumeric);
                if word(prev) && word(next) {
                    out.push('_');
                }
            }
            ch => out.push(ch),
        }
        i += 1;
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Index of the first occurrence of `pat` in `c` at or after `from`.
fn find(c: &[char], from: usize, pat: &[char]) -> Option<usize> {
    (from..c.len().saturating_sub(pat.len() - 1)).find(|&j| c[j..j + pat.len()] == *pat)
}

#[cfg(test)]
mod title_tests {
    use super::plain_title;

    #[test]
    fn titles_are_plain_text() {
        let t = |s: &str| plain_title(s).unwrap_or_default();
        assert_eq!(
            t("![[0a1b2c3d4e5f6g7h8j9k0m1n2p]]\n_tap tap_ is this on?"),
            "tap tap is this on?"
        );
        assert_eq!(
            t("Mira's [talk](https://example.com/t) was **great**"),
            "Mira's talk was great"
        );
        assert_eq!(t("# Heading"), "Heading");
        assert_eq!(
            t("![](https://example.com/a.png)\n\nAfter the image"),
            "After the image"
        );
        assert_eq!(t("![A harbour](x.png)"), "A harbour");
        assert_eq!(
            t("[TK]write an opener[=]The opener.[/TK] more"),
            "The opener. more"
        );
        assert_eq!(
            t("snake_case stays, `code` loses ticks"),
            "snake_case stays, code loses ticks"
        );
        assert_eq!(plain_title("![[0a1b2c3d4e5f6g7h8j9k0m1n2p]]\n\n"), None);
        assert_eq!(t("See [[0a1b2c3d4e5f6g7h8j9k0m1n2p]] too"), "See ↗ too");
        assert_eq!(t("[[not an id]]"), "[[not an id]]");
    }

    /// Studio 0.8.2: a thread is named in its author's own words, never in
    /// the words it quotes.
    #[test]
    fn titles_are_never_quoted_text() {
        let t = |s: &str| plain_title(s).unwrap_or_default();
        // A partial transclusion: the directive and its attached quote run,
        // a paragraph break inside the selection included.
        assert_eq!(
            t(
                "![[0a1b2c3d4e5f6g7h8j9k0m1n2p]]\n> Their sentence.\n>\n> More of theirs.\n\nMy reply."
            ),
            "My reply."
        );
        // The empty quote line a long-target stub starts with.
        assert_eq!(t("![[0a1b2c3d4e5f6g7h8j9k0m1n2p]]\n> \n\nMine"), "Mine");
        // A detached blockquote is still someone's quote.
        assert_eq!(
            t("![[0a1b2c3d4e5f6g7h8j9k0m1n2p]]\n\n> a quote\n\n# My heading\nbody"),
            "My heading"
        );
        assert_eq!(t("  > indented quote\nOwn words"), "Own words");
        // Quotes and nothing of one's own: no title (the caller names it).
        assert_eq!(plain_title("> quoted line"), None);
        assert_eq!(
            plain_title("![[0a1b2c3d4e5f6g7h8j9k0m1n2p]]\n> theirs\n"),
            None
        );
        // A leading heading is the title, whatever follows.
        assert_eq!(t("## On Protocols\n\nProtocols are thin."), "On Protocols");
    }
}
