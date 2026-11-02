//! A tiny in-process mock of blygger-studio's private owner API, contract
//! 5.0.0 (studio v0.49.12), root-mounted (`/api/...`), on a std TcpListener
//! (one thread per connection, `Connection: close`). Enough of the API to
//! exercise blyg-core; never talks to a real server.
//!
//! Every `/api` request the app sends, and every response the mock gives, is
//! checked against `tests/fixtures/openapi.json`. A violation fails the test
//! when its `Mock` is dropped. A request the contract refuses also gets the
//! studio's answer: 400 `invalid-input` (unknown fields, an `offset`, ...).
//!
//! As on 5.0.0: every item carries a `revision` (1 on creation, +1 on every
//! successful item command, and on `State::edit`), and item commands need
//! the current `expectedRevision` (else 409 `revision-conflict`). Lists are
//! cursor pages (`{items, page: {limit, nextCursor}}`); a cursor names its
//! collection and a fingerprint of the collection's contents, so any change
//! to the collection between pages (or `State::expire_cursors`) answers 409
//! `cursor-expired`.
//!
//! Tests seed `subs`, `reading`, `mentions`, `settings` and `hoppers` as JSON
//! in either spelling (the old snake_case keys still work); the mock answers
//! with the contract's camelCase shapes.
//!
//! `set_down(true)` makes it accept and immediately drop connections, which
//! the client sees as a transport failure (= offline).

#![allow(dead_code)]

pub mod contract;

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use blyg_core::{Backend, CoreEvent, LiveBackend, SyncOptions};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const TOKEN: &str = "test-token-not-real";
/// The studio password (`OWNER_PASSWORD`) the mock's sign-in takes.
pub const PASSWORD: &str = "test password, not real";
/// What `GET /api/capabilities` says.
pub const CONTRACT_VERSION: &str = "5.0.0";

#[derive(Debug, Clone)]
pub struct SItem {
    pub id: String,
    /// The authored kind (`fragment` | `thread`), also after withdrawal.
    pub kind: String,
    pub status: String,
    pub version: u32,
    /// The item's revision: 1 on creation, +1 per successful item command.
    pub revision: u64,
    pub dirty: bool,
    pub created: String,
    pub updated: String,
    pub content_md: String,
    /// Contract `Version` objects (see `version_json`), oldest first.
    pub versions: Vec<Value>,
    /// `"default"` | `"show"` | `"hide"`.
    pub responses: String,
    pub stub_of: Value,
    pub forked_from: Value,
}

impl SItem {
    /// Whether the page shows responses (the policy, else the site default).
    pub fn showing(&self, default: bool) -> bool {
        match self.responses.as_str() {
            "show" => true,
            "hide" => false,
            _ => default,
        }
    }
}

/// A contract `Version`.
pub fn version_json(item: &SItem, version: u32, at: &str, note: Value, withdrawn: bool) -> Value {
    json!({
        "itemId": item.id, "version": version,
        "contentMd": if withdrawn { "" } else { item.content_md.as_str() },
        "contentHtml": "", "contentHash": format!("h{version}"),
        "publishedAt": at, "note": note, "pinnedAt": null,
        "kind": if withdrawn { "withdrawn" } else { item.kind.as_str() },
        "pinned": false, "noteGenerated": false, "transclusions": [], "generated": [],
        "stubOf": item.stub_of, "stubCite": null,
    })
}

#[derive(Default)]
pub struct State {
    pub items: BTreeMap<String, SItem>,
    /// Seeded or created subscriptions (either key spelling).
    pub subs: Vec<Value>,
    /// Imported reading rows (either key spelling; `state: "tombstone"` is
    /// a withdrawn post, `read_version` forces the served `readVersion`).
    /// `None` = none held (5.0.0 always has the routes).
    pub reading: Option<Vec<Value>>,
    /// A cap on `GET /api/reading/imported` page sizes, below the contract's.
    pub reading_page_size: usize,
    /// Stored read state, `(sub, remote_id)` → version (max-merged).
    pub reads: BTreeMap<(String, String), u32>,
    /// Read-state clears (`DELETE …/read`, `POST /api/reading/unread`):
    /// `(sub, remote_id)` → when (Unix ms, the real clock), so a read with
    /// an earlier `readAt` is ignored.
    pub read_clears: BTreeMap<(String, String), i64>,
    /// Body of every `POST /api/reading/read`, in order.
    pub read_batches: Vec<Value>,
    /// Body of every `POST /api/reading/unread`, in order.
    pub unread_batches: Vec<Value>,
    /// Inbound mentions (`None` = none).
    pub mentions: Option<Vec<Value>>,
    /// Settings (`None` = the defaults).
    pub settings: Option<Value>,
    /// Hoppers (`None` = none). A seed's `count` is the detail's `total`.
    pub hoppers: Option<Vec<Value>>,
    pub signals: BTreeMap<(String, String), i64>,
    pub hidden: BTreeMap<String, bool>,
    /// Body of every `PATCH /api/settings`, in order.
    pub settings_puts: Vec<Value>,
    /// `showResponsesDefault`, when `settings` doesn't set it.
    pub responses_default: bool,
    /// Uploaded media rows, id → contract `Media` plus `sha`.
    pub media: BTreeMap<String, Value>,
    /// Contract violations seen so far ("METHOD /path: what").
    pub violations: Vec<String>,
    /// Path prefixes whose replies aren't checked: for tests that serve
    /// deliberately malformed data to prove the app tolerates it.
    pub unchecked: Vec<String>,
    /// Live studio sessions (`blyg_session` values). Clear it to end them.
    pub sessions: Vec<String>,
    /// Successful `POST …/studio/login`s.
    pub logins: usize,
    /// The bearer token is refused (cookie only).
    pub no_bearer: bool,
    /// The public static surface (anything outside `/api/`), path → JSON
    /// body. Unlisted paths 404, which for `v{n}.json` means "not pinned".
    /// Serve at e.g. `/blyg/items/X.json` to test a subdirectory mount.
    pub public: BTreeMap<String, Value>,
    /// The `authorization` header of every public request (None = absent).
    pub public_auth: Vec<Option<String>>,
    // --- profiles ---
    /// Public text files (OPML, RSS/Atom), path → body, served as XML.
    pub public_text: BTreeMap<String, String>,
    /// Every header of every public request, lower-cased names, in order.
    pub public_headers: Vec<Vec<(String, String)>>,
    pub media_bodies: Vec<Vec<u8>>,
    /// "METHOD /path?query" per request, in order.
    pub log: Vec<String>,
    /// Mixed into every cursor's fingerprint: bump it (`expire_cursors`)
    /// to expire every cursor handed out so far.
    pub cursor_epoch: u64,
    /// `http://127.0.0.1:port`, for absolute URLs.
    pub origin: String,
    tick: u64,
    next_id: u64,
}

impl State {
    pub fn now(&mut self) -> String {
        self.tick += 1;
        format!("2030-01-01T00:00:00.{:06}Z", self.tick)
    }

    pub fn new_id(&mut self, prefix: &str) -> String {
        self.next_id += 1;
        format!("{prefix}{:0>24}", self.next_id)
    }

    /// Insert a server-side item directly (as if made in the web studio).
    pub fn add_item(&mut self, kind: &str, content: &str) -> String {
        let id = self.new_id("SV");
        let now = self.now();
        self.items.insert(
            id.clone(),
            SItem {
                id: id.clone(),
                kind: kind.into(),
                status: "draft".into(),
                version: 0,
                revision: 1,
                dirty: true,
                created: now.clone(),
                updated: now,
                content_md: content.into(),
                versions: vec![],
                responses: "default".into(),
                stub_of: Value::Null,
                forked_from: Value::Null,
            },
        );
        id
    }

    /// Edit the working copy server-side (another device): a new revision.
    pub fn edit(&mut self, id: &str, content: &str) {
        self.set_content(id, content);
        self.items.get_mut(id).expect("item").revision += 1;
    }

    fn set_content(&mut self, id: &str, content: &str) {
        let now = self.now();
        let it = self.items.get_mut(id).expect("item");
        it.content_md = content.into();
        it.dirty = true;
        if it.version == 0 {
            it.updated = now;
        }
    }

    /// Expire every cursor handed out so far (the next page answers 409
    /// `cursor-expired`).
    pub fn expire_cursors(&mut self) {
        self.cursor_epoch += 1;
    }

    pub fn count(&self, prefix: &str) -> usize {
        self.log.iter().filter(|l| l.starts_with(prefix)).count()
    }
}

pub struct Mock {
    pub url: String,
    state: Arc<Mutex<State>>,
    down: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    addr: std::net::SocketAddr,
}

impl Mock {
    pub fn start() -> Mock {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let state = Arc::new(Mutex::new(State {
            reading_page_size: 500,
            origin: format!("http://{addr}"),
            ..Default::default()
        }));
        let down = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let (st, dn, sp) = (state.clone(), down.clone(), stop.clone());
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                if sp.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(conn) = conn else { continue };
                if dn.load(Ordering::SeqCst) {
                    let _ = conn.shutdown(Shutdown::Both);
                    continue;
                }
                let st = st.clone();
                std::thread::spawn(move || handle(conn, st));
            }
        });
        Mock {
            url: format!("http://{addr}"),
            state,
            down,
            stop,
            addr,
        }
    }

    pub fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap()
    }

    pub fn set_down(&self, down: bool) {
        self.down.store(down, Ordering::SeqCst);
    }
}

impl Drop for Mock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr);
        if std::thread::panicking() {
            return;
        }
        let v = std::mem::take(&mut self.state.lock().unwrap().violations);
        assert!(
            v.is_empty(),
            "owner-API traffic broke the OpenAPI contract:\n{}",
            v.join("\n")
        );
    }
}

// ------------------------------------------------------------------ HTTP

struct Req {
    method: String,
    path: String,
    query: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Req {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or(Value::Null)
    }
    fn query_param(&self, key: &str) -> Option<String> {
        url::form_urlencoded::parse(self.query.as_bytes())
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.into_owned())
    }
    /// The path's segments, percent-decoded.
    fn segs(&self) -> Vec<String> {
        self.path
            .trim_start_matches('/')
            .split('/')
            .map(decode)
            .collect()
    }
}

fn read_req(conn: &TcpStream) -> Option<Req> {
    let mut r = BufReader::new(conn.try_clone().ok()?);
    let mut line = String::new();
    r.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    let mut headers = vec![];
    loop {
        let mut h = String::new();
        r.read_line(&mut h).ok()?;
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let len: usize = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0; len];
    r.read_exact(&mut body).ok()?;
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target, String::new()),
    };
    Some(Req {
        method,
        path,
        query,
        headers,
        body,
    })
}

fn handle(mut conn: TcpStream, state: Arc<Mutex<State>>) {
    let Some(req) = read_req(&conn) else { return };
    // The studio's sign-in (any mount): a form POST, answered with a 302
    // and the session cookie, like upstream's spa.ts.
    if req.method == "POST" && req.path.ends_with("/studio/login") {
        let mut st = state.lock().unwrap();
        st.log.push(format!("POST {}", req.path));
        let password = url::form_urlencoded::parse(&req.body)
            .find(|(k, _)| k == "password")
            .map(|(_, v)| v.into_owned())
            .unwrap_or_default();
        let head = if password == PASSWORD {
            st.logins += 1;
            let s = format!("{}.sig{}", 2_000_000_000 + st.logins, st.logins);
            st.sessions.push(s.clone());
            format!(
                "HTTP/1.1 302 Found\r\nlocation: /studio\r\nset-cookie: blyg_session={s}; Path=/; Max-Age=2592000; HttpOnly; Secure; SameSite=Lax\r\n"
            )
        } else {
            "HTTP/1.1 403 Forbidden\r\ncontent-type: text/html\r\n".to_string()
        };
        let _ = write!(conn, "{head}content-length: 0\r\nconnection: close\r\n\r\n");
        let _ = conn.flush();
        return;
    }
    let (status, body) = {
        let mut st = state.lock().unwrap();
        let (status, body) = route(&req, &mut st);
        check(&req, status, &body, &mut st);
        (status, body)
    };
    // --- profiles --- a bare string is a public text file (OPML, a feed).
    let (ctype, body) = match body {
        Value::String(t) => ("application/xml", t),
        v => ("application/json", v.to_string()),
    };
    let reason = match status {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Content",
        429 => "Too Many Requests",
        _ => "Other",
    };
    let _ = write!(
        conn,
        "HTTP/1.1 {status} {reason}\r\ncontent-type: {ctype}\r\ncache-control: no-store\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = conn.flush();
}

/// The request's query as pairs.
fn query_pairs(req: &Req) -> Vec<(String, String)> {
    url::form_urlencoded::parse(req.query.as_bytes())
        .into_owned()
        .collect()
}

/// Check one exchange against the contract. Record what's wrong.
fn check(req: &Req, status: u16, body: &Value, s: &mut State) {
    if !req.path.starts_with("/api/")
        || status == 401
        || s.unchecked.iter().any(|p| req.path.starts_with(p.as_str()))
    {
        return;
    }
    let segs = req.segs();
    let segs: Vec<&str> = segs.iter().map(String::as_str).collect();
    let c = contract::contract();
    let m = req.method.as_str();
    let what = format!("{m} {}", req.path);
    if let Err(e) = c.check_request(
        m,
        &segs,
        &query_pairs(req),
        req.header("content-type"),
        &req.body,
    ) {
        s.violations.push(format!("{what}: request: {e}"));
        return;
    }
    if let Err(e) = c.check_response(m, &segs, status, body) {
        s.violations.push(format!("{what}: mock response: {e}"));
    }
}

// ------------------------------------------------------------- shapes

/// A contract `ApiError`.
fn err(status: u16, code: &str, message: &str) -> (u16, Value) {
    (status, json!({ "code": code, "message": message }))
}

fn err_with(status: u16, code: &str, message: &str, details: Value) -> (u16, Value) {
    (
        status,
        json!({ "code": code, "message": message, "details": details }),
    )
}

fn invalid(message: &str) -> (u16, Value) {
    err_with(
        400,
        "invalid-input",
        message,
        json!({ "issues": [{ "path": [], "message": message }] }),
    )
}

fn not_found() -> (u16, Value) {
    err(404, "not-found", "not found")
}

/// `snake_case` → `camelCase`.
fn camel(k: &str) -> String {
    let mut out = String::new();
    let mut up = false;
    for c in k.chars() {
        if c == '_' {
            up = true;
        } else if up {
            out.extend(c.to_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// `camelCase` → `snake_case`.
fn snake(k: &str) -> String {
    let mut out = String::new();
    for c in k.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// A seed with its top-level keys in camelCase.
fn camelize(v: &Value) -> Value {
    match v.as_object() {
        Some(o) => Value::Object(o.iter().map(|(k, v)| (camel(k), v.clone())).collect()),
        None => json!({}),
    }
}

/// Set `key` (camelCase) on a seed, dropping its snake_case spelling.
fn put(v: &mut Value, key: &str, value: Value) {
    if let Some(o) = v.as_object_mut() {
        o.remove(&snake(key));
        o.insert(key.to_string(), value);
    }
}

/// A seed's field by its camelCase name, in either spelling.
fn field<'a>(v: &'a Value, key: &str) -> &'a Value {
    match v.get(key) {
        Some(x) => x,
        None => &v[snake(key).as_str()],
    }
}

/// `defaults` with `over`'s fields on top.
fn overlay(mut defaults: Value, over: &Value) -> Value {
    if let (Some(d), Some(o)) = (defaults.as_object_mut(), over.as_object()) {
        for (k, v) in o {
            d.insert(k.clone(), v.clone());
        }
    }
    defaults
}

/// The contract's `Item`.
fn item_json(it: &SItem) -> Value {
    json!({
        "id": it.id,
        "revision": it.revision,
        "kind": it.kind,
        "status": it.status,
        "created": it.created,
        "updated": it.updated,
        "version": it.version,
        "contentMd": it.content_md,
        "dirty": it.dirty,
        "responses": it.responses,
        "highlight": "default",
        "provenance": [],
        "stubOf": it.stub_of,
        "forkedFrom": it.forked_from,
        "forkCite": null,
    })
}

/// The item's pins, `{version, kind}`.
fn pins(it: &SItem) -> Value {
    it.versions
        .iter()
        .filter(|v| v["pinned"] == true && v["kind"] != "withdrawn")
        .map(|v| json!({ "version": v["version"], "kind": v["kind"] }))
        .collect()
}

/// `GET /api/items` row: the item plus its pins.
fn item_row(it: &SItem) -> Value {
    let mut v = item_json(it);
    v["pins"] = pins(it);
    v
}

/// `GET /api/items/:id`: the item plus pins, media and the published
/// version (no history: that's `GET /api/items/:id/versions`).
fn item_detail(it: &SItem, s: &State) -> Value {
    let mut v = item_row(it);
    v["media"] = s
        .media
        .values()
        .filter(|m| m["itemId"] == it.id.as_str())
        .map(media_json)
        .collect();
    v["publishedMedia"] = json!([]);
    v["published"] = it
        .versions
        .iter()
        .rev()
        .find(|x| x["kind"] != "withdrawn")
        .cloned()
        .unwrap_or(Value::Null);
    v
}

/// A contract `Media` from a stored row.
fn media_json(m: &Value) -> Value {
    let mut m = m.clone();
    m.as_object_mut().unwrap().remove("sha");
    m
}

/// A contract `Subscription` from whatever a test seeded.
pub fn sub_json(v: &Value) -> Value {
    overlay(
        json!({ "id": "", "kind": "blyg", "origin": "", "feedUrl": "", "title": "",
            "status": "active", "lastPollAt": null, "failCount": 0, "lastIndexSyncAt": null,
            "created": "2030-01-01T00:00:00Z", "titleFollowsSource": true, "inBlogroll": false,
            "flags": [] }),
        &camelize(v),
    )
}

/// Contract `Settings` from whatever a test seeded.
fn settings_json(v: &Value, responses_default: bool) -> Value {
    overlay(
        json!({ "siteTitle": "", "theme": "auto", "customTheme": null, "customThemeDark": null,
            "customThemeName": "", "font": "", "authorName": "", "authorUrl": "", "authorBio": "",
            "authorLinks": [], "siteUrl": "", "timezone": "", "avatarMediaId": "",
            "aiModel": "", "aiModelTk": "", "aiModelChangelog": "", "aiModelFeed": "",
            "aiModelClassify": "", "changeCategoryInNote": true, "feedPrompt": "",
            "aiStylePrompt": "", "acceptMentions": true, "updateCheck": false,
            "showResponsesDefault": responses_default, "highlightGeneratedDefault": false,
            "autoChangeNotes": false, "pickerTyping": "auto", "feedOrder": "", "frontPage": "",
            "postsPath": "", "linksPosition": "bottom", "linksThreads": [], "linksExternal": [],
            "linksCollections": false, "linksOpml": false, "linksBlyggerOrg": false,
            "linksBlyggerCom": false, "setup": "done", "updateFeedUrl": "",
            "updateNoticeAck": false, "extensions": [] }),
        &camelize(v),
    )
}

/// A contract `Mention` from whatever a test seeded (`source_author_json`,
/// a JSON string, becomes `sourceAuthor`).
fn mention_json(v: &Value, hidden: &BTreeMap<String, bool>) -> Value {
    let mut seed = camelize(v);
    if let Some(o) = seed.as_object_mut()
        && let Some(a) = o.remove("sourceAuthorJson")
    {
        let a = match a {
            Value::String(t) => serde_json::from_str(&t).unwrap_or(Value::Null),
            other => other,
        };
        o.insert("sourceAuthor".into(), a);
    }
    let mut m = overlay(
        json!({ "id": "", "source": "", "target": "", "targetItemId": "", "status": "pending",
            "relation": null, "sourceOrigin": null, "sourceId": null, "sourceKind": null,
            "sourceVersion": null, "sourceAuthor": null, "sourcePage": null,
            "firstSeen": "2030-01-01T00:00:00Z", "lastSeen": "2030-01-01T00:00:00Z",
            "verifiedAt": null, "attempts": 0, "error": null, "hidden": false }),
        &seed,
    );
    if let Some(h) = m["id"].as_str().and_then(|id| hidden.get(id)) {
        m["hidden"] = json!(h);
    }
    m
}

/// A contract `Hopper` (the seed's `count` is the detail's `total`).
fn hopper_json(v: &Value) -> Value {
    let mut h = overlay(
        json!({ "id": "", "name": "", "slug": null, "public": false,
            "created": "2030-01-01T00:00:00Z", "slugFrozen": false, "description": null }),
        &camelize(v),
    );
    h.as_object_mut().unwrap().remove("count");
    h
}

/// A reading row's `(subscriptionId, remoteId)`.
fn row_key(r: &Value) -> (String, String) {
    (
        field(r, "subscriptionId")
            .as_str()
            .unwrap_or("")
            .to_string(),
        field(r, "remoteId").as_str().unwrap_or("").to_string(),
    )
}

/// One seeded reading row, camelCase, with every field the contract's
/// imported shapes need (`state` kept for `ImportedItem`).
fn reading_row(r: &Value) -> Value {
    overlay(
        json!({ "subscriptionId": "", "remoteId": "", "subscriptionTitle": "", "origin": "",
            "kind": "fragment", "state": "current", "version": 1, "created": null,
            "updated": null, "observedAt": "2030-01-01T00:00:00Z", "contentMd": "",
            "contentHtml": "", "author": null, "page": null, "thumb": null, "hoppers": [],
            "transclusions": null, "pinnedVersionRetained": null, "stubOf": null,
            "forkedFrom": null, "generated": null, "l0": false }),
        &camelize(r),
    )
}

/// The contract's `ImportedReadingEntry` for one seeded row, joined with
/// the stored read state and signals.
fn imported_entry(s: &State, seed: &Value) -> Value {
    let mut r = reading_row(seed);
    let key = row_key(&r);
    let o = r.as_object_mut().unwrap();
    let state = o.remove("state").unwrap_or(Value::Null);
    o.entry("withdrawn").or_insert(json!(state == "tombstone"));
    let page = o["page"].clone();
    o.entry("sourceUrl").or_insert(page);
    // A seeded `read_version` is served as is (a test wants that value).
    if !o.contains_key("readVersion") {
        o.insert(
            "readVersion".into(),
            s.reads.get(&key).map_or(Value::Null, |v| json!(v)),
        );
    }
    if let Some(t) = s.signals.get(&key) {
        o.insert("thumb".into(), json!(t));
    }
    r
}

/// The contract's `ReadingEntry` for one seeded row.
fn reading_entry(s: &State, seed: &Value) -> Value {
    let r = imported_entry(s, seed);
    json!({
        "key": format!("{}:{}", r["subscriptionId"].as_str().unwrap_or(""), r["remoteId"].as_str().unwrap_or("")),
        "source": "imported", "kind": r["kind"], "withdrawn": r["withdrawn"], "l0": r["l0"],
        "contentHtml": r["contentHtml"], "displayAt": r["observedAt"],
        "imported": {
            "subscriptionId": r["subscriptionId"], "subscriptionTitle": r["subscriptionTitle"],
            "remoteId": r["remoteId"], "kind": r["kind"], "withdrawn": r["withdrawn"],
            "l0": r["l0"], "updated": r["updated"], "observedAt": r["observedAt"],
            "contentHtml": r["contentHtml"], "pinnedVersionRetained": r["pinnedVersionRetained"],
            "sourceUrl": r["sourceUrl"], "version": r["version"], "readVersion": r["readVersion"],
        },
    })
}

/// The contract's `ImportedItem` for one seeded row.
fn imported_item(seed: &Value) -> Value {
    let r = reading_row(seed);
    json!({
        "subscriptionId": r["subscriptionId"], "remoteId": r["remoteId"], "kind": r["kind"],
        "state": r["state"], "version": r["version"], "created": r["created"],
        "updated": r["updated"], "observedAt": r["observedAt"], "contentMd": r["contentMd"],
        "contentHtml": r["contentHtml"], "contentHash": null, "l0": r["l0"],
        "pinnedVersionRetained": r["pinnedVersionRetained"], "page": r["page"],
        "author": r["author"], "media": null, "transclusions": r["transclusions"],
        "stubOf": r["stubOf"], "forkedFrom": r["forkedFrom"], "generated": r["generated"],
    })
}

/// The seeded reading rows, `observedAt` DESC, `subscriptionId` DESC,
/// `remoteId` DESC (the contract's Imported Reading order).
fn reading_rows(s: &State) -> Vec<Value> {
    let mut rows: Vec<Value> = s.reading.clone().unwrap_or_default();
    rows.sort_by(|a, b| {
        let k = |r: &Value| {
            let (sub, rid) = row_key(r);
            (
                field(r, "observedAt").as_str().unwrap_or("").to_string(),
                sub,
                rid,
            )
        };
        k(b).cmp(&k(a))
    });
    rows
}

fn held(s: &State, key: &(String, String)) -> bool {
    s.reading.iter().flatten().any(|r| row_key(r) == *key)
}

// ---------------------------------------------------------- read state

/// Unix ms now.
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// `YYYY-MM-DDTHH:MM:SS[.fff](Z|±HH:MM)` → Unix ms.
fn parse_ms(t: &str) -> Option<i64> {
    let n = |s: &str| s.parse::<i64>().ok();
    let (date, rest) = t.split_once('T')?;
    let mut d = date.split('-');
    let (y, mo, da) = (n(d.next()?)?, n(d.next()?)?, n(d.next()?)?);
    let (clock, off) = if let Some(c) = rest.strip_suffix('Z') {
        (c, 0)
    } else {
        let i = rest.rfind(['+', '-'])?;
        let (c, o) = rest.split_at(i);
        let (oh, om) = o[1..].split_once(':')?;
        let sign = if o.starts_with('-') { -1 } else { 1 };
        (c, sign * (n(oh)? * 60 + n(om)?))
    };
    let (hms, frac) = clock.split_once('.').unwrap_or((clock, "0"));
    let mut h = hms.split(':');
    let (hh, mm, ss) = (n(h.next()?)?, n(h.next()?)?, n(h.next()?)?);
    let ms = n(&format!("{frac:0<3}")[..3])?;
    // Days from the civil date (Howard Hinnant's algorithm).
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if mo > 2 { mo - 3 } else { mo + 9 }) + 2) / 5 + da - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 86_400 + hh * 3600 + mm * 60 + ss) - off * 60) * 1000 + ms)
}

/// Apply one read; whether it was stored.
fn apply_read(s: &mut State, key: (String, String), version: u32, read_at: Option<&str>) -> bool {
    if !held(s, &key) {
        return false;
    }
    if let (Some(cleared), Some(at)) = (s.read_clears.get(&key), read_at.and_then(parse_ms))
        && at < *cleared
    {
        return false;
    }
    let e = s.reads.entry(key).or_insert(version);
    *e = (*e).max(version);
    true
}

fn clear_read(s: &mut State, key: (String, String)) {
    s.reads.remove(&key);
    s.read_clears.insert(key, now_ms());
}

// --------------------------------------------------------------- paging

/// A fingerprint, in the contract's integer range.
fn fingerprint(v: &impl Hash) -> u64 {
    let mut h = DefaultHasher::new();
    v.hash(&mut h);
    h.finish() & ((1 << 53) - 1)
}

/// `{items, page: {limit, nextCursor}}` over `all` (already in order), by
/// the request's `limit` and `cursor`. A cursor carries `key` (the
/// collection and its filters), its position, and the fingerprint of the
/// collection (`basis`, else `all`) when it was issued. `cap` lowers
/// every page size (`State::reading_page_size`).
#[allow(clippy::too_many_arguments)]
fn page(
    req: &Req,
    s: &State,
    key: &str,
    all: Vec<Value>,
    default: usize,
    max: usize,
    basis: Option<u64>,
    cap: usize,
) -> Result<Value, (u16, Value)> {
    if req.query_param("offset").is_some() {
        return Err(invalid("offset paging is gone: use cursor"));
    }
    let limit = match req.query_param("limit") {
        None => default,
        Some(l) => match l.parse::<usize>() {
            Ok(n) if (1..=max).contains(&n) => n,
            _ => return Err(invalid(&format!("limit must be 1 to {max}"))),
        },
    }
    .min(cap);
    let generation = fingerprint(&(
        basis.unwrap_or_else(|| fingerprint(&serde_json::to_string(&all).unwrap())),
        s.cursor_epoch,
    ));
    let start = match req.query_param("cursor") {
        None => 0,
        Some(c) => {
            let text = URL_SAFE_NO_PAD
                .decode(c.as_bytes())
                .ok()
                .and_then(|b| String::from_utf8(b).ok())
                .ok_or_else(|| invalid("malformed cursor"))?;
            let mut parts = text.rsplitn(3, '\n');
            let (Some(g), Some(p), Some(k)) = (parts.next(), parts.next(), parts.next()) else {
                return Err(invalid("malformed cursor"));
            };
            let (Ok(g), Ok(p)) = (g.parse::<u64>(), p.parse::<usize>()) else {
                return Err(invalid("malformed cursor"));
            };
            if k != key {
                return Err(invalid("cursor from another collection or filter"));
            }
            if g != generation {
                return Err(err(
                    409,
                    "cursor-expired",
                    "the collection changed: start again",
                ));
            }
            p
        }
    };
    let len = all.len();
    let items: Vec<Value> = all.into_iter().skip(start).take(limit).collect();
    let next = if start + limit < len {
        json!(URL_SAFE_NO_PAD.encode(format!("{key}\n{}\n{generation}", start + limit)))
    } else {
        Value::Null
    };
    Ok(json!({ "items": items, "page": { "limit": limit, "nextCursor": next } }))
}

/// Percent-decode one path segment.
fn decode(seg: &str) -> String {
    url::form_urlencoded::parse(format!("x={}", seg.replace('+', "%2B")).as_bytes())
        .next()
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default()
}

/// An item command's `expectedRevision` against the item's revision.
fn check_revision(it: &SItem, body: &Value) -> Result<(), (u16, Value)> {
    let Some(e) = body["expectedRevision"].as_u64() else {
        return Err(invalid("expectedRevision is required"));
    };
    if e != it.revision {
        return Err(err_with(
            409,
            "revision-conflict",
            "the item changed since that revision",
            json!({ "itemId": it.id, "expectedRevision": e }),
        ));
    }
    Ok(())
}

/// `GET /api/changes` domains: a fingerprint of each domain's state, so a
/// direct change to `State` shows up as well as one through the API.
fn changes(s: &State) -> Value {
    let items: Vec<String> = s.items.values().map(|i| format!("{i:?}")).collect();
    let reading = (
        serde_json::to_string(&s.reading).unwrap(),
        format!("{:?}{:?}", s.reads, s.read_clears),
    );
    json!({
        "epoch": "mock",
        "domains": {
            "items": fingerprint(&(items, serde_json::to_string(&s.media).unwrap())),
            "reading": fingerprint(&reading),
            "subscriptions": fingerprint(&serde_json::to_string(&s.subs).unwrap()),
            "hoppers": fingerprint(&serde_json::to_string(&s.hoppers).unwrap()),
            "signals": fingerprint(&(format!("{:?}", s.signals), serde_json::to_string(&s.reading).unwrap())),
            "settings": fingerprint(&(serde_json::to_string(&s.settings).unwrap(), s.responses_default)),
            "feed": 0, "mentions": fingerprint(&(serde_json::to_string(&s.mentions).unwrap(), format!("{:?}", s.hidden))),
            "responses": 0, "interactions": 0, "browse": 0,
        },
    })
}

fn route(req: &Req, s: &mut State) -> (u16, Value) {
    let q = if req.query.is_empty() {
        String::new()
    } else {
        format!("?{}", req.query)
    };
    s.log.push(format!("{} {}{}", req.method, req.path, q));
    if !req.path.starts_with("/api/") {
        // Public static surface: no auth expected (record what arrived).
        s.public_auth
            .push(req.header("authorization").map(str::to_string));
        s.public_headers.push(
            req.headers
                .iter()
                .map(|(k, v)| (k.to_ascii_lowercase(), v.clone()))
                .collect(),
        );
        if let ("GET", Some(t)) = (req.method.as_str(), s.public_text.get(&req.path)) {
            // A JSON string body is written raw, as XML (see `handle`).
            return (200, Value::String(t.clone()));
        }
        return match (req.method.as_str(), s.public.get(&req.path)) {
            ("GET", Some(v)) => (200, v.clone()),
            _ => not_found(),
        };
    }
    let bearer = !s.no_bearer && req.header("authorization") == Some(&format!("Bearer {TOKEN}"));
    let session = req
        .header("cookie")
        .and_then(|c| {
            c.split(';')
                .find_map(|p| p.trim().strip_prefix("blyg_session="))
        })
        .is_some_and(|v| s.sessions.iter().any(|x| x == v));
    if !bearer && !session {
        return err(401, "unauthorized", "unauthorized");
    }
    let segs = req.segs();
    let segs: Vec<&str> = segs.iter().map(String::as_str).collect();
    // The studio validates every input first (unknown fields, `offset`, a
    // missing `expectedRevision`, ...): 400 before anything happens.
    let c = contract::contract();
    if c.knows(&req.method, &segs)
        && let Err(e) = c.check_request(
            &req.method,
            &segs,
            &query_pairs(req),
            req.header("content-type"),
            &req.body,
        )
    {
        return invalid(&e);
    }
    match dispatch(req, s, &segs) {
        Ok(r) | Err(r) => r,
    }
}

type Reply = Result<(u16, Value), (u16, Value)>;

fn dispatch(req: &Req, s: &mut State, segs: &[&str]) -> Reply {
    let body = req.json();
    Ok(match (req.method.as_str(), segs) {
        ("GET", ["api", "capabilities"]) => {
            let o = s.origin.clone();
            let ops: Vec<String> = contract::contract()
                .operation_ids()
                .into_iter()
                .filter(|id| !id.starts_with("ext"))
                .collect();
            (
                200,
                json!({
                    "contractVersion": CONTRACT_VERSION,
                    "endpoints": { "publicBase": format!("{o}/"), "studio": format!("{o}/studio"),
                        "api": format!("{o}/api"), "issuer": o, "mcp": format!("{o}/mcp"),
                        "media": format!("{o}/media") },
                    "operations": ops,
                    "extensions": [],
                }),
            )
        }
        ("GET", ["api", "changes"]) => (200, changes(s)),
        // ---- items
        ("GET", ["api", "items", "summary"]) => {
            let n = |f: &dyn Fn(&SItem) -> bool| s.items.values().filter(|i| f(i)).count();
            (
                200,
                json!({
                    "all": s.items.len(),
                    "drafts": n(&|i| i.status == "draft"),
                    "changes": n(&|i| i.status == "public" && i.dirty),
                    "public": n(&|i| i.status == "public"),
                    "withdrawn": n(&|i| i.status == "withdrawn"),
                }),
            )
        }
        ("GET", ["api", "items"]) => {
            let status = req.query_param("status");
            let dirty = req.query_param("dirty");
            let mut v: Vec<&SItem> = s
                .items
                .values()
                .filter(|i| status.as_ref().is_none_or(|st| &i.status == st))
                .filter(|i| dirty.as_ref().is_none_or(|d| i.dirty.to_string() == *d))
                .collect();
            v.sort_by(|a, b| (&b.updated, &b.id).cmp(&(&a.updated, &a.id)));
            let all = v.into_iter().map(item_row).collect();
            let key = format!("items?status={status:?}&dirty={dirty:?}");
            (200, page(req, s, &key, all, 100, 100, None, usize::MAX)?)
        }
        ("POST", ["api", "items"]) => match body["mode"].as_str() {
            None | Some("blank") => {
                let kind = if body["kind"] == "thread" {
                    "thread"
                } else {
                    "fragment"
                };
                let id = s.add_item(kind, body["contentMd"].as_str().unwrap_or(""));
                let it = s.items.get_mut(&id).unwrap();
                if let Some(st) = body.get("stubOf") {
                    it.stub_of = st.clone();
                }
                (201, item_json(it))
            }
            Some("fork") => {
                let id = s.add_item("fragment", "forked content");
                let it = s.items.get_mut(&id).unwrap();
                it.forked_from = body["source"].clone();
                (201, item_json(it))
            }
            Some("response") => {
                let quote = body["selection"].as_str().unwrap_or("");
                let id = s.add_item("thread", &format!("> {quote}\n\n"));
                (201, item_json(&s.items[&id]))
            }
            Some(_) => invalid("unknown mode"),
        },
        ("GET", ["api", "items", id, "versions"]) => {
            let Some(it) = s.items.get(*id) else {
                return Err(not_found());
            };
            let only = req.query_param("version");
            let all: Vec<Value> = it
                .versions
                .iter()
                .rev()
                .filter(|v| {
                    only.as_ref()
                        .is_none_or(|o| o.parse::<u64>().ok() == v["version"].as_u64())
                })
                .cloned()
                .collect();
            let key = format!("versions/{id}?version={only:?}");
            (200, page(req, s, &key, all, 100, 100, None, usize::MAX)?)
        }
        ("GET", ["api", "items", id]) => match s.items.get(*id) {
            None => not_found(),
            Some(it) => (200, item_detail(it, s)),
        },
        ("PATCH", ["api", "items", id]) => {
            let Some(it) = s.items.get(*id).cloned() else {
                return Err(not_found());
            };
            check_revision(&it, &body)?;
            if let Some(k) = body["kind"].as_str()
                && k != it.kind
                && it.version > 0
            {
                return Err(err(
                    409,
                    "conflict",
                    "kind is fixed after the first publication",
                ));
            }
            if let Some(content) = body["contentMd"].as_str() {
                s.set_content(id, content);
            }
            let it = s.items.get_mut(*id).unwrap();
            if let Some(k) = body["kind"].as_str() {
                it.kind = k.into();
            }
            if let Some(r) = body["responses"].as_str() {
                it.responses = r.into();
            }
            if let Some(st) = body.get("stubOf") {
                it.stub_of = st.clone();
            }
            it.revision += 1;
            (200, item_json(it))
        }
        ("DELETE", ["api", "items", id]) => {
            let Some(it) = s.items.get(*id) else {
                return Err(not_found());
            };
            check_revision(it, &body)?;
            if it.version > 0 {
                return Err(err(
                    409,
                    "conflict",
                    "published items are withdrawn, not deleted",
                ));
            }
            s.items.remove(*id);
            (200, json!({ "ok": true, "outcome": "discarded" }))
        }
        ("PUT", ["api", "items", id, "versions", v, "pin"]) => {
            let Some(it) = s.items.get_mut(*id) else {
                return Err(not_found());
            };
            check_revision(it, &body)?;
            let Ok(v) = v.parse::<u64>() else {
                return Err(invalid("version must be an integer"));
            };
            let Some(ver) = it.versions.iter_mut().find(|x| x["version"] == v) else {
                return Err(err(404, "not-found", "version not found"));
            };
            if ver["kind"] == "withdrawn" {
                return Err(err(409, "conflict", "cannot pin a withdrawal"));
            }
            let already = ver["pinned"] == true;
            ver["pinned"] = json!(true);
            it.revision += 1;
            (
                200,
                json!({ "ok": true, "revision": it.revision, "version": v, "already": already }),
            )
        }
        (
            "POST",
            [
                "api",
                "items",
                id,
                action @ ("publish" | "withdraw" | "restore"),
            ],
        ) => {
            let now = s.now();
            let Some(it) = s.items.get_mut(*id) else {
                return Err(not_found());
            };
            check_revision(it, &body)?;
            match *action {
                "publish" => {
                    if it.kind == "fragment" && it.content_md.chars().count() > 1000 {
                        return Err(invalid("fragment exceeds 1000 characters"));
                    }
                    for bad in ["![[BAD]]", "[[BAD]]"] {
                        // Protocol 0.3: an unresolvable `[[id]]` link fails
                        // publish too.
                        if it.content_md.contains(&bad[..bad.len() - 2]) {
                            return Err(err_with(
                                400,
                                "invalid-input",
                                "one or more references do not resolve",
                                json!({ "errors": [{ "directive": bad, "reason": "unknown item" }] }),
                            ));
                        }
                    }
                    it.version += 1;
                    it.revision += 1;
                    it.status = "public".into();
                    it.dirty = false;
                    it.updated = now.clone();
                    let note = body.get("note").cloned().unwrap_or(Value::Null);
                    let v = version_json(it, it.version, &now, note, false);
                    it.versions.push(v);
                    (
                        200,
                        json!({ "ok": true, "revision": it.revision, "version": it.version }),
                    )
                }
                "withdraw" => {
                    if it.status == "withdrawn" {
                        return Err(err(409, "conflict", "already withdrawn"));
                    }
                    if it.status != "public" {
                        return Err(err(409, "conflict", "not published"));
                    }
                    it.version += 1;
                    it.revision += 1;
                    it.status = "withdrawn".into();
                    let note = body.get("note").cloned().unwrap_or(Value::Null);
                    let v = version_json(it, it.version, &now, note, true);
                    it.versions.push(v);
                    (
                        200,
                        json!({ "ok": true, "revision": it.revision, "version": it.version }),
                    )
                }
                _ => {
                    let v = body["version"].as_u64().unwrap_or(0);
                    let Some(ver) = it.versions.iter().find(|x| x["version"] == v) else {
                        return Err(err(404, "not-found", "version not found"));
                    };
                    if ver["kind"] == "withdrawn" {
                        return Err(err(409, "conflict", "cannot restore a withdrawal"));
                    }
                    it.content_md = format!("restored v{v}");
                    it.dirty = true;
                    it.revision += 1;
                    (
                        200,
                        json!({ "ok": true, "revision": it.revision, "restored": v,
                            "publishesAs": it.version + 1 }),
                    )
                }
            }
        }
        ("POST", ["api", "media"]) => {
            let ct = req.header("content-type").unwrap_or("");
            let parts = contract::multipart(ct, &req.body).unwrap_or_default();
            let text = |name: &str| {
                parts
                    .iter()
                    .find(|p| p.name == name)
                    .map(|p| String::from_utf8_lossy(&p.data).to_string())
            };
            let Some(file) = parts.iter().find(|p| p.name == "file") else {
                return Err(invalid("file field required (multipart)"));
            };
            if file.content_type.as_deref() != Some("image/png") {
                return Err(err(415, "unsupported-media", "unsupported type"));
            }
            s.media_bodies.push(req.body.clone());
            let sha = hex(&Sha256::digest(&file.data));
            let item_id = text("itemId");
            let inline = text("inline").as_deref() == Some("true");
            let mut revision = None;
            if let Some(iid) = &item_id {
                let Some(it) = s.items.get(iid) else {
                    return Err(not_found());
                };
                let Some(e) = text("expectedRevision").and_then(|e| e.parse::<u64>().ok()) else {
                    return Err(invalid("expectedRevision is required with itemId"));
                };
                check_revision(it, &json!({ "expectedRevision": e }))?;
                revision = Some(it.revision);
            }
            if let Some(m) = s.media.values().find(|m| {
                m["sha"] == sha.as_str()
                    && m["itemId"].as_str() == item_id.as_deref()
                    && m["inline"] == inline
            }) {
                let mut v =
                    json!({ "id": m["id"], "url": m["url"], "mime": m["mime"], "duplicate": true });
                if let Some(r) = revision {
                    v["revision"] = json!(r);
                }
                return Ok((200, v));
            }
            let id = s.new_id("M");
            let created = s.now();
            let url = format!("{}/media/{id}/{sha}.png", s.origin);
            s.media.insert(
                id.clone(),
                json!({ "id": id, "itemId": item_id, "mime": "image/png", "alt": text("alt"),
                    "created": created, "inline": inline, "url": url, "sha": sha }),
            );
            let mut v = json!({ "id": id, "url": url, "mime": "image/png" });
            if let Some(iid) = &item_id {
                let it = s.items.get_mut(iid).unwrap();
                it.revision += 1;
                v["revision"] = json!(it.revision);
            }
            (201, v)
        }
        ("DELETE", ["api", "media", id]) => {
            let Some(m) = s.media.get(*id).cloned() else {
                return Err(not_found());
            };
            match m["itemId"].as_str().and_then(|i| s.items.get_mut(i)) {
                Some(it) => {
                    if body.get("expectedRevision").is_some() {
                        check_revision(it, &body)?;
                    }
                    it.revision += 1;
                    let revision = it.revision;
                    s.media.get_mut(*id).unwrap()["itemId"] = Value::Null;
                    (
                        200,
                        json!({ "ok": true, "outcome": "detached", "revision": revision }),
                    )
                }
                None => {
                    s.media.remove(*id);
                    (200, json!({ "ok": true, "outcome": "deleted" }))
                }
            }
        }
        // ---- subscriptions
        ("GET", ["api", "subscriptions"]) => {
            let mut all: Vec<Value> = s.subs.iter().map(sub_json).collect();
            all.sort_by(|a, b| {
                let k = |v: &Value| (v["created"].to_string(), v["id"].to_string());
                k(a).cmp(&k(b))
            });
            (
                200,
                page(req, s, "subscriptions", all, 100, 100, None, usize::MAX)?,
            )
        }
        ("GET", ["api", "subscriptions", id]) => match s.subs.iter().find(|x| x["id"] == *id) {
            Some(sub) => (200, sub_json(sub)),
            None => not_found(),
        },
        ("POST", ["api", "subscriptions"]) => {
            let Some(url) = body["url"].as_str().map(str::to_string) else {
                return Err(invalid("url required"));
            };
            if url.contains("nowhere") {
                return Err(err_with(
                    422,
                    "unprocessable",
                    "could not resolve this URL to a blyg or a feed",
                    json!({ "tried": [] }),
                ));
            }
            // --- profiles --- a feed URL previews as RSS.
            if body["confirm"] != true && (url.ends_with(".xml") || url.contains("/rss")) {
                return Ok((
                    200,
                    json!({ "needsConfirm": true, "kind": "rss", "feedUrl": url, "title": "A feed" }),
                ));
            }
            if body["confirm"] != true {
                return Ok((
                    200,
                    json!({ "needsConfirm": true, "kind": "blyg", "origin": url, "title": "Their blyg" }),
                ));
            }
            if s.subs.iter().any(|x| sub_json(x)["origin"] == url.as_str()) {
                return Err(err(409, "conflict", "already subscribed"));
            }
            let id = s.new_id("SUB");
            let title = body["title"].as_str().unwrap_or("Their blyg").to_string();
            let sub = sub_json(&json!({ "id": id, "kind": "blyg", "origin": url,
                "feedUrl": format!("{url}feed.xml"), "title": title }));
            s.subs.push(sub.clone());
            (201, sub)
        }
        ("PATCH", ["api", "subscriptions", id]) => {
            let Some(sub) = s.subs.iter_mut().find(|x| x["id"] == *id) else {
                return Err(not_found());
            };
            if let Some(b) = body["inBlogroll"].as_bool() {
                put(sub, "inBlogroll", json!(b));
            }
            match &body["title"] {
                Value::String(t) => {
                    put(sub, "title", json!(t));
                    put(sub, "titleFollowsSource", json!(false));
                }
                Value::Null if body.get("title").is_some() => {
                    put(sub, "titleFollowsSource", json!(true));
                }
                _ => {}
            }
            if let Some(p) = body["paused"].as_bool() {
                put(sub, "status", json!(if p { "paused" } else { "active" }));
            }
            (200, sub_json(sub))
        }
        ("DELETE", ["api", "subscriptions", id]) => {
            let before = s.subs.len();
            s.subs.retain(|x| x["id"] != *id);
            if s.subs.len() == before {
                return Err(not_found());
            }
            (200, json!({ "ok": true }))
        }
        ("POST", ["api", "subscriptions", id, "resync"]) => {
            if !s.subs.iter().any(|x| x["id"] == *id) {
                return Err(not_found());
            }
            (200, json!({ "ok": true, "changed": 0 }))
        }
        // ---- signals / mentions / settings
        ("GET", ["api", "signals"]) => {
            let mut rows: BTreeMap<(String, String), Value> = BTreeMap::new();
            for r in s.reading.iter().flatten() {
                if let Some(t) = field(r, "thumb").as_i64() {
                    rows.insert(row_key(r), json!(t));
                }
            }
            for (k, t) in &s.signals {
                rows.insert(k.clone(), json!(t));
            }
            let all = rows
                .into_iter()
                .map(|((sub, rid), t)| {
                    json!({ "subscriptionId": sub, "remoteId": rid, "thumb": t,
                        "at": "2030-01-01T00:00:00Z" })
                })
                .collect();
            (
                200,
                page(req, s, "signals", all, 100, 100, None, usize::MAX)?,
            )
        }
        ("PUT", ["api", "signals", sub, rid]) => {
            let t = body["thumb"].as_i64();
            if t != Some(1) && t != Some(-1) {
                return Err(invalid("thumb must be 1 or -1"));
            }
            s.signals
                .insert((sub.to_string(), rid.to_string()), t.unwrap());
            (200, json!({ "ok": true }))
        }
        ("DELETE", ["api", "signals", sub, rid]) => {
            s.signals.remove(&(sub.to_string(), rid.to_string()));
            (200, json!({ "ok": true }))
        }
        ("GET", ["api", "mentions"]) => {
            let direction = req.query_param("direction").unwrap_or("inbound".into());
            let all: Vec<Value> = if direction == "inbound" {
                s.mentions
                    .iter()
                    .flatten()
                    .map(|m| mention_json(m, &s.hidden))
                    .collect()
            } else {
                vec![]
            };
            let mut p = page(
                req,
                s,
                &format!("mentions/{direction}"),
                all,
                100,
                100,
                None,
                usize::MAX,
            )?;
            p["direction"] = json!(direction);
            (200, p)
        }
        ("PATCH", ["api", "mentions", id]) => {
            let Some(h) = body["hidden"].as_bool() else {
                return Err(invalid("hidden must be a boolean"));
            };
            s.hidden.insert(id.to_string(), h);
            (200, json!({ "ok": true, "hidden": h }))
        }
        ("GET", ["api", "settings"]) => (
            200,
            settings_json(
                s.settings.as_ref().unwrap_or(&json!({})),
                s.responses_default,
            ),
        ),
        ("PATCH", ["api", "settings"]) => {
            s.settings_puts.push(body.clone());
            let cur = settings_json(
                s.settings.as_ref().unwrap_or(&json!({})),
                s.responses_default,
            );
            let next = overlay(cur, &body);
            if let Some(d) = next["showResponsesDefault"].as_bool() {
                s.responses_default = d;
            }
            // The site default lives in `responses_default`.
            let mut stored = next.clone();
            stored
                .as_object_mut()
                .unwrap()
                .remove("showResponsesDefault");
            s.settings = Some(stored);
            (200, next)
        }
        // ---- reading
        ("GET", ["api", "reading"]) => {
            let sub = req.query_param("sub");
            let kind = req.query_param("kind");
            let hopper = req.query_param("hopper");
            let rows: Vec<Value> = reading_rows(s)
                .into_iter()
                .filter(|r| sub.as_ref().is_none_or(|x| row_key(r).0 == *x))
                .filter(|r| kind.as_ref().is_none_or(|k| field(r, "kind") == k.as_str()))
                .filter(|r| {
                    hopper.as_ref().is_none_or(|h| {
                        field(r, "hoppers")
                            .as_array()
                            .is_some_and(|a| a.iter().any(|x| x == h.as_str()))
                    })
                })
                .collect();
            let basis = fingerprint(&serde_json::to_string(&rows).unwrap());
            let all = rows.iter().map(|r| reading_entry(s, r)).collect();
            let key = format!("reading?sub={sub:?}&kind={kind:?}&hopper={hopper:?}");
            let mut p = page(req, s, &key, all, 25, 50, Some(basis), usize::MAX)?;
            p["readState"] = json!(true);
            p["readStateClear"] = json!(true);
            (200, p)
        }
        // The app's reading rows: every imported item, the full shape.
        ("GET", ["api", "reading", "imported"]) => {
            let rows = reading_rows(s);
            // Read state and signals don't expire a cursor; the rows do.
            let basis = fingerprint(&serde_json::to_string(&rows).unwrap());
            let all = rows.iter().map(|r| imported_entry(s, r)).collect();
            let cap = s.reading_page_size.max(1);
            let p = page(req, s, "reading/imported", all, 50, 100, Some(basis), cap)?;
            (200, p)
        }
        ("GET", ["api", "imports", sub, rid]) => {
            let key = (sub.to_string(), rid.to_string());
            match s.reading.iter().flatten().find(|r| row_key(r) == key) {
                Some(r) => (200, imported_item(r)),
                None => not_found(),
            }
        }
        ("PUT", ["api", "reading", sub, rid, "read"]) => {
            let Some(v) = body["version"].as_u64() else {
                return Err(invalid("version must be a positive integer"));
            };
            let key = (sub.to_string(), rid.to_string());
            let stored = apply_read(s, key.clone(), v as u32, body["readAt"].as_str());
            let held = s.reads.get(&key).map_or(Value::Null, |v| json!(v));
            (
                200,
                json!({ "ok": true, "stored": stored, "readVersion": held }),
            )
        }
        ("DELETE", ["api", "reading", sub, rid, "read"]) => {
            clear_read(s, (sub.to_string(), rid.to_string()));
            (
                200,
                json!({ "ok": true, "stored": false, "readVersion": null }),
            )
        }
        ("POST", ["api", "reading", "read"]) => {
            let items = body["items"].as_array().cloned().unwrap_or_default();
            s.read_batches.push(body.clone());
            for it in &items {
                let (Some(sub), Some(rid), Some(v)) = (
                    it["sub"].as_str(),
                    it["remoteId"].as_str(),
                    it["version"].as_u64(),
                ) else {
                    continue; // validated whole before this
                };
                let key = (sub.to_string(), rid.to_string());
                apply_read(s, key, v as u32, it["readAt"].as_str());
            }
            (200, json!({ "ok": true, "received": items.len() }))
        }
        ("POST", ["api", "reading", "unread"]) => {
            let items = body["items"].as_array().cloned().unwrap_or_default();
            s.unread_batches.push(body.clone());
            for it in &items {
                if let (Some(sub), Some(rid)) = (it["sub"].as_str(), it["remoteId"].as_str()) {
                    clear_read(s, (sub.to_string(), rid.to_string()));
                }
            }
            (200, json!({ "ok": true, "received": items.len() }))
        }
        // ---- hoppers
        ("GET", ["api", "hoppers"]) => {
            let mut all: Vec<Value> = s.hoppers.iter().flatten().map(hopper_json).collect();
            all.sort_by(|a, b| {
                let k = |v: &Value| (v["created"].to_string(), v["id"].to_string());
                k(a).cmp(&k(b))
            });
            (
                200,
                page(req, s, "hoppers", all, 100, 100, None, usize::MAX)?,
            )
        }
        ("GET", ["api", "hoppers", id]) => {
            let Some(h) = s.hoppers.iter().flatten().find(|x| x["id"] == *id) else {
                return Err(not_found());
            };
            let members = members(s, id);
            let total = h["count"].as_u64().unwrap_or(members.len() as u64);
            let sources: BTreeSet<String> = members.iter().map(|r| row_key(r).0).collect();
            (
                200,
                json!({ "hopper": hopper_json(h), "total": total, "sourceCount": sources.len() }),
            )
        }
        ("GET", ["api", "hoppers", id, "memberships"]) => {
            if !s.hoppers.iter().flatten().any(|x| x["id"] == *id) {
                return Err(not_found());
            }
            let all = members(s, id)
                .iter()
                .map(|r| {
                    let (sub, rid) = row_key(r);
                    json!({ "hopperId": id, "subscriptionId": sub, "remoteId": rid,
                        "addedAt": "2030-01-01T00:00:00Z", "item": imported_item(r) })
                })
                .collect();
            (
                200,
                page(
                    req,
                    s,
                    &format!("hoppers/{id}/memberships"),
                    all,
                    100,
                    100,
                    None,
                    usize::MAX,
                )?,
            )
        }
        _ => not_found(),
    })
}

/// The reading rows in hopper `id`, subscriptionId ASC, remoteId ASC.
fn members(s: &State, id: &str) -> Vec<Value> {
    let mut rows: Vec<Value> = s
        .reading
        .iter()
        .flatten()
        .filter(|r| {
            field(r, "hoppers")
                .as_array()
                .is_some_and(|a| a.iter().any(|x| x == id))
        })
        .cloned()
        .collect();
    rows.sort_by_key(row_key);
    rows
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

// --------------------------------------------------------------- helpers

pub struct Env {
    pub mock: Mock,
    pub dir: tempfile::TempDir,
    pub events: Arc<Mutex<Vec<CoreEvent>>>,
}

impl Env {
    pub fn new() -> Env {
        Env {
            mock: Mock::start(),
            dir: tempfile::tempdir().unwrap(),
            events: Arc::default(),
        }
    }

    pub fn data_dir(&self) -> PathBuf {
        self.dir.path().to_path_buf()
    }

    /// Backend with no background worker: sync happens only on `sync_now`,
    /// `pull_now` and the remote methods.
    pub fn manual(&self) -> LiveBackend {
        self.open(SyncOptions {
            start_worker: false,
            ..fast()
        })
    }

    pub fn open(&self, opts: SyncOptions) -> LiveBackend {
        self.open_token(opts, TOKEN)
    }

    pub fn open_token(&self, opts: SyncOptions, token: &str) -> LiveBackend {
        self.open_cred(opts, token)
    }

    pub fn open_cred(
        &self,
        opts: SyncOptions,
        cred: impl Into<blyg_core::api::auth::Credential>,
    ) -> LiveBackend {
        let b = LiveBackend::open_with(&self.data_dir(), &self.mock.url, cred, opts).unwrap();
        let ev = self.events.clone();
        b.set_event_sink(Box::new(move |e| ev.lock().unwrap().push(e)));
        b
    }

    pub fn events(&self) -> Vec<CoreEvent> {
        self.events.lock().unwrap().clone()
    }
}

pub fn fast() -> SyncOptions {
    SyncOptions {
        debounce: Duration::from_millis(100),
        pull_interval: Duration::from_millis(300),
        backoff_initial: Duration::from_millis(50),
        backoff_max: Duration::from_millis(200),
        start_worker: true,
        reading_pages: 4,
        // Seeded rows name made-up origins: never fetch from them.
        lineage_fetches: 0,
    }
}

/// Poll `f` until it's true or `timeout` passes.
pub fn wait_until(timeout: Duration, mut f: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + timeout;
    while Instant::now() < end {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    f()
}
