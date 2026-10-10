//! BXP, the Burrow Extension Protocol, version 1: the messages, as serde
//! types. JSON-RPC 2.0, one message per line over the child's stdin/stdout
//! (the framing is in [`crate::rpc`]). Field names are camelCase on the
//! wire. Both sides ignore fields they don't know (tolerate the unknown), so
//! an extension written against protocol 1 keeps working when the host adds
//! fields, and the other way round.
//!
//! Requests go both ways on the one channel. The host calls `initialize`,
//! `extension/command`, `extension/source.search`, `extension/source.read`,
//! `extension/library.{list,search,read,write}` and `shutdown`, and sends the `burrow/item*` and `burrow/settingsChanged`
//! notifications. The extension calls the `burrow/*` methods in
//! [`methods`], each behind a capability ([`crate::Capability`]). There is
//! no method that publishes, withdraws, pins, deletes, forks or changes the
//! blyg's settings, and none that hands over a token.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The protocol version this host speaks.
pub const PROTOCOL_VERSION: u32 = 1;

/// Method names.
pub mod methods {
    // host -> extension (requests)
    pub const INITIALIZE: &str = "initialize";
    pub const COMMAND: &str = "extension/command";
    pub const SOURCE_SEARCH: &str = "extension/source.search";
    pub const SOURCE_READ: &str = "extension/source.read";
    pub const SHUTDOWN: &str = "shutdown";
    pub const LIBRARY_LIST: &str = "extension/library.list";
    pub const LIBRARY_SEARCH: &str = "extension/library.search";
    pub const LIBRARY_READ: &str = "extension/library.read";
    pub const LIBRARY_WRITE: &str = "extension/library.write";
    /// Optional: shape a macro's text before the preview sheet
    /// ([`super::MacroPrepareParams`]). Method-not-found = use the template.
    pub const MACRO_PREPARE: &str = "extension/macro.prepare";
    /// Optional, reading slots ([`crate::reading`]): a reading entry's
    /// byline marker, and one of its ⋯ rows.
    pub use crate::reading::{ENTRY_ACTION, ENTRY_BYLINE};
    // host -> extension (notifications)
    pub const ITEM_PUBLISHED: &str = "burrow/itemPublished";
    pub const ITEM_SAVED: &str = "burrow/itemSaved";
    pub const ITEM_CREATED: &str = "burrow/itemCreated";
    pub const SETTINGS_CHANGED: &str = "burrow/settingsChanged";
    // extension -> host (requests)
    pub const LIST_ITEMS: &str = "burrow/listItems";
    pub const GET_ITEM: &str = "burrow/getItem";
    pub const SEARCH_ITEMS: &str = "burrow/searchItems";
    pub const CREATE_DRAFT: &str = "burrow/createDraft";
    pub const SAVE_ITEM: &str = "burrow/saveItem";
    pub const LIST_READING: &str = "burrow/listReading";
    pub const OPEN_ITEM: &str = "burrow/openItem";
    pub const TOAST: &str = "burrow/toast";
    pub const REQUEST_CAPABILITY: &str = "burrow/requestCapability";
    /// The page open in the browser pane ([`super::PageCapture`]); needs
    /// `browser.capture`, and only while one of the extension's commands
    /// runs. `-32004` when the pane has no page.
    pub const BROWSER_PAGE: &str = "burrow/browser.page";
    /// Load a URL in the pane ([`super::BrowserOpenParams`]); needs
    /// `browser.automate:<the URL's origin>`.
    pub const BROWSER_OPEN: &str = "burrow/browser.open";
}

/// JSON-RPC error codes. The `-326xx` ones are JSON-RPC's own.
pub mod codes {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;
    /// The request wasn't answered in time (or the other side went away).
    pub const TIMEOUT: i64 = -32000;
    /// The extension lacks the capability the method needs. `data` is
    /// `{"capability": "<name>"}`.
    pub const PERMISSION_DENIED: i64 = -32001;
    /// `burrow/saveItem` with a `baseHash` that is no longer the item's
    /// content hash. `data` is `{"currentHash": "sha256:…"}`.
    pub const STALE: i64 = -32002;
    /// Burrow refused (the backend's own error, e.g. not found).
    pub const REFUSED: i64 = -32003;
    /// `burrow/browser.page` with no page open in the browser pane (or not
    /// while one of the extension's commands runs).
    pub const NO_PAGE: i64 = -32004;
}

/// A JSON-RPC error object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        RpcError {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    pub fn timeout() -> Self {
        RpcError::new(codes::TIMEOUT, "timeout")
    }

    pub fn closed() -> Self {
        RpcError::new(codes::TIMEOUT, "the extension isn't running")
    }

    pub fn permission_denied(capability: &str) -> Self {
        RpcError::new(
            codes::PERMISSION_DENIED,
            format!("permission denied: {capability}"),
        )
        .with_data(serde_json::json!({ "capability": capability }))
    }

    pub fn invalid_params(e: impl std::fmt::Display) -> Self {
        RpcError::new(codes::INVALID_PARAMS, format!("invalid params: {e}"))
    }

    pub fn method_not_found(method: &str) -> Self {
        RpcError::new(codes::METHOD_NOT_FOUND, format!("no method {method}"))
    }
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl std::error::Error for RpcError {}

// ------------------------------------------------------------ initialize

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostInfo {
    pub name: String,
    pub version: String,
    /// `macos`, `windows`, `linux` (`std::env::consts::OS`).
    pub platform: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub protocol_version: u32,
    pub host: HostInfo,
    /// The capabilities the user granted, as written in the manifest.
    #[serde(default)]
    pub granted: Vec<String>,
    /// This extension's `extension-setting` lines, `key -> value`.
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
    /// A directory of the extension's own (`<data dir>/extensions/<name>/`).
    pub storage_dir: PathBuf,
    /// The blyg's origin, only with `blyg.identity`. Never a token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blyg_origin: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    /// When non-empty, replaces the manifest's commands (it never adds
    /// capabilities).
    #[serde(default)]
    pub commands: Vec<CommandSpec>,
    /// When non-empty, replaces the manifest's sources.
    #[serde(default)]
    pub sources: Vec<SourceSpec>,
    /// When non-empty, replaces the manifest's libraries.
    #[serde(default)]
    pub libraries: Vec<LibrarySpec>,
}

// --------------------------------------------------------------- commands

/// Where a command is offered in the Extensions palette.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum When {
    /// Everywhere.
    #[default]
    Always,
    /// The posts screen with an item open in the editor.
    Editor,
    /// The reading screen.
    Reading,
    /// The notes drawer.
    Notes,
    /// The posts screen with a published item open (cross-posting
    /// macros). The palette checks the item's status: see
    /// [`CommandSpec::applies_to`].
    Published,
}

/// The screen a command runs from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Screen {
    #[default]
    Posts,
    Reading,
    Notes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSpec {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub when: When,
    /// Advisory only: shown in the palette, never bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

impl CommandSpec {
    /// Whether the palette offers it on `screen` (`has_item`: an item is
    /// open in the editor). Without the item's status, `published` is
    /// treated like `editor`; [`CommandSpec::applies_to`] knows better.
    pub fn applies(&self, screen: Screen, has_item: bool) -> bool {
        self.when.applies(screen, has_item, has_item)
    }

    /// [`CommandSpec::applies`] knowing whether the open item is published.
    pub fn applies_to(&self, screen: Screen, has_item: bool, published: bool) -> bool {
        self.when.applies(screen, has_item, published)
    }
}

impl When {
    /// Whether something filed under this applies on `screen`, with an item
    /// open (`has_item`) that is published (`published`; implies
    /// `has_item`).
    pub fn applies(self, screen: Screen, has_item: bool, published: bool) -> bool {
        match self {
            When::Always => true,
            When::Editor => screen == Screen::Posts && has_item,
            When::Reading => screen == Screen::Reading,
            When::Notes => screen == Screen::Notes,
            When::Published => screen == Screen::Posts && has_item && published,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSpec {
    pub id: String,
    pub title: String,
}

/// One of the user's own items, as a command's context or a hook's subject.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemRef {
    /// The local id: what every `burrow/*` method takes.
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    pub kind: String,
    pub status: String,
    #[serde(default)]
    pub title: String,
}

impl ItemRef {
    pub fn of(item: &blyg_core::Item) -> Self {
        ItemRef {
            id: item.local_id.0.clone(),
            server_id: item.server_id.as_ref().map(|s| s.0.clone()),
            kind: kind_str(item.kind).into(),
            status: status_str(item.status).into(),
            title: item.title(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadingRef {
    pub subscription_id: String,
    pub remote_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<ItemRef>,
    /// The selected text, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reading: Option<ReadingRef>,
    #[serde(default)]
    pub screen: Screen,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandParams {
    pub id: String,
    #[serde(default)]
    pub context: CommandContext,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandResult {
    /// A message for the status bar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toast: Option<String>,
    /// A local id to select in the list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open: Option<String>,
}

// ---------------------------------------------------------------- sources

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSearchParams {
    pub source: String,
    #[serde(default)]
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    20
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceEntry {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub excerpt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    /// A public URL for attribution, if there is one (never `file://`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceReadParams {
    pub source: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDocument {
    pub title: String,
    pub markdown: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

// ---------------------------------------------------------------- library
//
// A library is a collection of documents an extension owns (a folder of
// Markdown notes): listed, searched, read and written by the host's notes
// panel. It knows nothing about blyg items or ids. Writes carry the hash the
// edit was based on and are refused (`STALE`) if the document changed since;
// nothing is ever deleted.

/// A library the extension contributes (manifest `[[libraries]]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySpec {
    pub id: String,
    pub title: String,
    /// The panel offers create/edit (`library.write`).
    #[serde(default)]
    pub writable: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryListParams {
    /// The library's id; omitted = the extension's first library.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library: Option<String>,
    /// A folder inside the library (`/`-separated, relative); omitted = the
    /// top. Lists one level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// A document or folder in a library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryEntry {
    /// Stable while the document isn't moved: what `read`/`write` take.
    pub id: String,
    pub title: String,
    /// `/`-separated, relative to the library.
    pub path: String,
    #[serde(default)]
    pub is_dir: bool,
    /// RFC 3339, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySearchParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library: Option<String>,
    #[serde(default)]
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryReadParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library: Option<String>,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryDocument {
    pub id: String,
    pub title: String,
    /// The whole document as stored (frontmatter included, untouched).
    pub markdown: String,
    /// The body without frontmatter, for display and "copy into post".
    #[serde(default)]
    pub body: String,
    /// `sha256:<hex>` of the stored bytes: the `baseHash` for a write.
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryWriteParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library: Option<String>,
    /// The document to overwrite; omitted = create a new one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// For a new document: names the file. Omitted = from its first line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// For a new document: the folder to create it in (relative).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    pub markdown: String,
    /// Required to overwrite: the `hash` from the read the edit started at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryWritten {
    pub id: String,
    pub hash: String,
}

// ------------------------------------------------------------------ hooks

/// `burrow/itemPublished` (needs `hooks:itemPublished`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemPublished {
    pub item: ItemRef,
    pub version: u32,
    pub permalink: String,
    pub content_md: String,
    pub content_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// When (RFC 3339): the item's `updated`.
    #[serde(default)]
    pub at: String,
}

/// `burrow/itemSaved` and `burrow/itemCreated` (each behind its hook).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemChanged {
    pub item: ItemRef,
    pub content_md: String,
    pub content_hash: String,
}

/// `burrow/settingsChanged`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsChanged {
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
    #[serde(default)]
    pub granted: Vec<String>,
}

// ------------------------------------------------- extension -> host calls

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListItemsParams {
    /// `draft`, `public`, `withdrawn`, `scratch`; empty = all.
    #[serde(default)]
    pub status: Vec<String>,
    /// Only items updated at or after this RFC 3339 time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    #[serde(default)]
    pub include_content: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetItemParams {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchItemsParams {
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// One of the user's own items. `content_md` is present from `getItem`, and
/// from `listItems`/`searchItems` with `includeContent`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemSummary {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    pub kind: String,
    pub status: String,
    pub version: u32,
    pub dirty: bool,
    pub created: String,
    pub updated: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permalink: Option<String>,
    pub title: String,
    pub content_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_md: Option<String>,
}

impl ItemSummary {
    pub fn of(item: &blyg_core::Item, include_content: bool) -> Self {
        ItemSummary {
            id: item.local_id.0.clone(),
            server_id: item.server_id.as_ref().map(|s| s.0.clone()),
            kind: kind_str(item.kind).into(),
            status: status_str(item.status).into(),
            version: item.version,
            dirty: item.dirty,
            created: item.created.clone(),
            updated: item.updated.clone(),
            permalink: item.permalink.clone(),
            title: item.title(),
            content_hash: blyg_core::content_hash(&item.content_md),
            content_md: include_content.then(|| item.content_md.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDraftParams {
    /// `fragment` or `thread`; omitted = by length, as a scratch note is
    /// promoted (`blyg_core::promotion_kind`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub content_md: String,
    /// A local-only scratch note: nothing reaches the server.
    #[serde(default)]
    pub scratch: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedDraft {
    pub id: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveItemParams {
    pub id: String,
    pub content_md: String,
    /// The `contentHash` the edit was based on; refused (`STALE`) when the
    /// item changed since. Omitted = save regardless.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedItem {
    pub ok: bool,
    pub content_hash: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListReadingParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// A post held locally from a subscription.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadingSummary {
    pub subscription_id: String,
    pub remote_id: String,
    pub subscription_title: String,
    pub origin: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    pub kind: String,
    pub version: u32,
    pub observed_at: String,
    pub content_md: String,
}

impl ReadingSummary {
    pub fn of(r: &blyg_core::ReadingItem) -> Self {
        ReadingSummary {
            subscription_id: r.subscription_id.clone(),
            remote_id: r.remote_id.clone(),
            subscription_title: r.subscription_title.clone(),
            origin: r.origin.clone(),
            title: blyg_core::plain_title(&r.content_md).unwrap_or_default(),
            author: r.author.as_ref().and_then(|a| a.name.clone()),
            page: r.page.clone(),
            kind: kind_str(r.kind).into(),
            version: r.version,
            observed_at: r.observed_at.clone(),
            content_md: r.content_md.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenItemParams {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToastParams {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestCapabilityParams {
    pub capability: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityAnswer {
    pub granted: bool,
}

// ---------------------------------------------------------------- browser
//
// Sites and macros are manifest contributions (`[[sites]]`, `[[macros]]`,
// kebab-case keys as in the rest of `extension.toml`; the camelCase
// spellings are accepted too). A macro is a declarative recipe the host
// runs in the browser pane ([`crate::recipe`]); the extension never sees
// the page of a signed-in site, and only `extension/macro.prepare` (the
// text) goes over the wire.

/// A website an extension's macros run on (manifest `[[sites]]`). Its
/// `origin` must be declared as `browser.automate:<origin>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SiteSpec {
    pub id: String,
    pub title: String,
    /// Normalised by the manifest loader (`https://social.example.com`).
    pub origin: String,
    /// Where the user signs in by hand; on `origin`.
    pub home: String,
    /// A selector that, when it matches, means "signed out": the run stops
    /// and asks the user to sign in in the pane.
    #[serde(default, alias = "signedOut", skip_serializing_if = "Option::is_none")]
    pub signed_out: Option<String>,
    /// `false`: the browser pane's content blocking is off for this origin
    /// during a run only (never saved).
    #[serde(default = "yes", alias = "contentBlocking")]
    pub content_blocking: bool,
    /// The shortest gap between two runs against this site, as a duration
    /// (`"60s"`, `"2m"`); validated by the manifest loader.
    #[serde(
        default,
        alias = "minInterval",
        skip_serializing_if = "Option::is_none"
    )]
    pub min_interval: Option<String>,
}

fn yes() -> bool {
    true
}

impl SiteSpec {
    /// The parsed `min-interval` (zero when absent or invalid; the
    /// manifest loader refuses an invalid one).
    pub fn min_interval(&self) -> std::time::Duration {
        self.min_interval
            .as_deref()
            .and_then(|s| crate::recipe::parse_duration(s).ok())
            .unwrap_or_default()
    }
}

/// A macro (manifest `[[macros]]`): an ⇧⌘P row that fills in a post on a
/// site and, after the user confirms, submits it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct MacroSpec {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub detail: String,
    /// A [`SiteSpec::id`] of the same manifest.
    pub site: String,
    /// Where the palette offers it; `published` by default.
    #[serde(default = "published")]
    pub when: When,
    /// The text to post, before `extension/macro.prepare`:
    /// `{{title}}`, `{{excerpt}}` and `{{permalink}}` are filled in
    /// ([`crate::recipe::expand`]).
    #[serde(default = "default_template")]
    pub template: String,
    /// When the maintainer last checked the selectors by hand, as free
    /// text (`"2026-10-01"`); shown on the preview sheet and in errors.
    #[serde(default = "unverified")]
    pub tested: String,
    pub steps: Vec<crate::recipe::Step>,
}

fn published() -> When {
    When::Published
}

fn default_template() -> String {
    crate::recipe::DEFAULT_TEMPLATE.into()
}

fn unverified() -> String {
    "unverified".into()
}

impl MacroSpec {
    /// Whether the palette offers it (see [`When::applies`]).
    pub fn applies(&self, screen: Screen, has_item: bool, published: bool) -> bool {
        self.when.applies(screen, has_item, published)
    }
}

/// `extension/macro.prepare` (host -> extension, 10 s): shape the text
/// before the preview sheet. The answer's `text` replaces the template's;
/// the user still sees and edits it. Method-not-found = use `text`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroPrepareParams {
    /// The [`MacroSpec::id`].
    #[serde(rename = "macro")]
    pub macro_id: String,
    /// The item, with `contentMd`.
    pub item: ItemSummary,
    pub permalink: String,
    /// The macro's template, expanded.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroPrepareResult {
    pub text: String,
    /// A line for the preview sheet ("cut to 280 characters").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// `burrow/browser.page` (needs `browser.capture`): the page open in the
/// browser pane, and what Clip Page puts in a draft. Never cookies, storage
/// or headers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageCapture {
    pub url: String,
    /// `<link rel=canonical>`, when the page names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_url: Option<String>,
    #[serde(default)]
    pub title: String,
    /// The selected text; empty when nothing is selected.
    #[serde(default)]
    pub selection: String,
    /// The readable main content as Markdown (capped).
    #[serde(default)]
    pub markdown: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

/// `burrow/browser.open` (needs `browser.automate:<the URL's origin>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserOpenParams {
    pub url: String,
}

/// The answer to `burrow/browser.open`: the URL the pane is loading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserOpened {
    pub url: String,
}

// ------------------------------------------------------------------ names

pub fn kind_str(k: blyg_core::Kind) -> &'static str {
    match k {
        blyg_core::Kind::Fragment => "fragment",
        blyg_core::Kind::Thread => "thread",
    }
}

pub fn parse_kind(s: &str) -> Option<blyg_core::Kind> {
    match s {
        "fragment" => Some(blyg_core::Kind::Fragment),
        "thread" => Some(blyg_core::Kind::Thread),
        _ => None,
    }
}

pub fn status_str(s: blyg_core::Status) -> &'static str {
    match s {
        blyg_core::Status::Draft => "draft",
        blyg_core::Status::Public => "public",
        blyg_core::Status::Withdrawn => "withdrawn",
        blyg_core::Status::Scratch => "scratch",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn camel_case_on_the_wire_and_round_trips() {
        let p = InitializeParams {
            protocol_version: 1,
            host: HostInfo {
                name: "Burrow".into(),
                version: "0.9.0".into(),
                platform: "macos".into(),
            },
            granted: vec!["items.read".into()],
            settings: [("vault".to_string(), "~/Notes".to_string())].into(),
            storage_dir: PathBuf::from("/tmp/x"),
            blyg_origin: None,
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["protocolVersion"], 1);
        assert_eq!(v["storageDir"], "/tmp/x");
        assert!(v.get("blygOrigin").is_none(), "absent without the grant");
        let back: InitializeParams = serde_json::from_value(v).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn unknown_fields_are_ignored_both_ways() {
        let r: InitializeResult = serde_json::from_value(json!({
            "protocolVersion": 1, "name": "x", "version": "1",
            "someFutureField": {"a": 1}
        }))
        .unwrap();
        assert_eq!(r.protocol_version, 1);
        assert!(r.commands.is_empty());
        let s: SaveItemParams = serde_json::from_value(json!({
            "id": "L1", "contentMd": "x", "baseHash": "sha256:00", "mergeStrategy": "future"
        }))
        .unwrap();
        assert_eq!(s.base_hash.as_deref(), Some("sha256:00"));
        let c: CommandParams = serde_json::from_value(json!({"id": "go"})).unwrap();
        assert_eq!(c.context.screen, Screen::Posts);
    }

    #[test]
    fn when_filters_the_palette() {
        let mut c = CommandSpec {
            id: "x".into(),
            title: "X".into(),
            detail: String::new(),
            when: When::Always,
            key: None,
        };
        assert!(c.applies(Screen::Reading, false));
        c.when = When::Editor;
        assert!(c.applies(Screen::Posts, true));
        assert!(!c.applies(Screen::Posts, false));
        assert!(!c.applies(Screen::Reading, true));
        c.when = When::Notes;
        assert!(c.applies(Screen::Notes, false));
        assert!(!c.applies(Screen::Posts, true));
    }

    #[test]
    fn rpc_errors_carry_their_data() {
        let e = RpcError::permission_denied("items.write");
        assert_eq!(e.code, codes::PERMISSION_DENIED);
        assert_eq!(e.data.unwrap()["capability"], "items.write");
    }

    #[test]
    fn published_is_editor_with_a_public_item() {
        let c = CommandSpec {
            id: "x".into(),
            title: "X".into(),
            detail: String::new(),
            when: When::Published,
            key: None,
        };
        assert!(c.applies_to(Screen::Posts, true, true));
        assert!(!c.applies_to(Screen::Posts, true, false));
        assert!(!c.applies_to(Screen::Posts, false, true));
        assert!(!c.applies_to(Screen::Reading, true, true));
        assert!(
            c.applies(Screen::Posts, true),
            "without status: like editor"
        );
        assert_eq!(
            serde_json::to_value(When::Published).unwrap(),
            json!("published")
        );
    }

    #[test]
    fn browser_messages_round_trip_and_ignore_the_unknown() {
        let page: PageCapture = serde_json::from_value(json!({
            "url": "https://news.example.com/a",
            "canonicalUrl": "https://news.example.com/a?ref=x",
            "title": "A story",
            "selection": "",
            "markdown": "Text",
            "author": "Someone",
            "cookies": "never read",
        }))
        .unwrap();
        assert_eq!(
            page.canonical_url.as_deref(),
            Some("https://news.example.com/a?ref=x")
        );
        let v = serde_json::to_value(&page).unwrap();
        assert!(v.get("cookies").is_none());
        assert_eq!(v["canonicalUrl"], "https://news.example.com/a?ref=x");
        assert_eq!(serde_json::from_value::<PageCapture>(v).unwrap(), page);
        let bare: PageCapture =
            serde_json::from_value(json!({ "url": "https://news.example.com/" })).unwrap();
        assert!(bare.canonical_url.is_none() && bare.author.is_none());
        let v = serde_json::to_value(&bare).unwrap();
        assert!(v.get("canonicalUrl").is_none() && v.get("author").is_none());

        let item = ItemSummary {
            id: "L1".into(),
            server_id: Some("S1".into()),
            kind: "fragment".into(),
            status: "public".into(),
            version: 2,
            dirty: false,
            created: "2026-10-01T00:00:00Z".into(),
            updated: "2026-10-02T00:00:00Z".into(),
            permalink: Some("https://blyg.example.com/p/1".into()),
            title: "Hello".into(),
            content_hash: "sha256:00".into(),
            content_md: Some("Hello".into()),
        };
        let p = MacroPrepareParams {
            macro_id: "cross-post-note".into(),
            item,
            permalink: "https://blyg.example.com/p/1".into(),
            text: "Hello\n\nhttps://blyg.example.com/p/1".into(),
        };
        let mut v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["macro"], "cross-post-note");
        assert_eq!(v["item"]["contentMd"], "Hello");
        v["somethingNew"] = json!(true);
        assert_eq!(serde_json::from_value::<MacroPrepareParams>(v).unwrap(), p);
        let r: MacroPrepareResult =
            serde_json::from_value(json!({ "text": "t", "later": [1] })).unwrap();
        assert_eq!(r.note, None);
        let v = serde_json::to_value(&r).unwrap();
        assert!(v.get("note").is_none());
        let o: BrowserOpenParams =
            serde_json::from_value(json!({ "url": "https://social.example.com/", "tab": 2 }))
                .unwrap();
        assert_eq!(o.url, "https://social.example.com/");
        assert_eq!(methods::BROWSER_PAGE, "burrow/browser.page");
        assert_eq!(methods::BROWSER_OPEN, "burrow/browser.open");
        assert_eq!(methods::MACRO_PREPARE, "extension/macro.prepare");
        assert_eq!(codes::NO_PAGE, -32004);
    }

    #[test]
    fn sites_and_macros_round_trip() {
        let site: SiteSpec = toml::from_str(
            "id = \"s\"\ntitle = \"S\"\norigin = \"https://social.example.com\"\n\
             home = \"https://social.example.com/\"\nfuture = 1\n",
        )
        .unwrap();
        assert!(site.content_blocking && site.signed_out.is_none());
        assert_eq!(site.min_interval(), std::time::Duration::ZERO);
        let v = serde_json::to_value(&site).unwrap();
        assert_eq!(v["content-blocking"], true);
        assert_eq!(serde_json::from_value::<SiteSpec>(v).unwrap(), site);
        let m: MacroSpec = toml::from_str(
            "id = \"m\"\ntitle = \"M\"\nsite = \"s\"\n\
             steps = [{ do = \"open\", url = \"https://social.example.com/\" }]\n",
        )
        .unwrap();
        assert_eq!(m.when, When::Published);
        assert!(m.applies(Screen::Posts, true, true));
        assert!(!m.applies(Screen::Posts, true, false));
        let back: MacroSpec = serde_json::from_value(serde_json::to_value(&m).unwrap()).unwrap();
        assert_eq!(back, m);
    }
}
