//! Blocking owner-API client (ureq + rustls). Speaks blygger-studio's
//! private API contract 5.x (studio 0.49.12+, `openapi.json`), and nothing
//! older: the client checks the contract (`GET {blyg}/api/capabilities`)
//! before anything is pushed or pulled. Every call carries the owner
//! credential (a bearer token, or the studio session from a password
//! sign-in; see `auth`), which is never logged, printed or included in
//! errors (`Debug` redacts it). The API lives under the blyg's mount: a
//! blyg at `https://host/blyg` has its API at `https://host/blyg/api`.
//!
//! Lists are cursor pages (`{items, page: {limit, nextCursor}}`): a
//! `nextCursor` of null ends the list, and there are no totals. A list that
//! changes while it's read answers 409 `cursor-expired`; the pages read so
//! far are thrown away and the list is read again from the start
//! ([`Api::collect`]). Item commands carry the revision they were based on
//! (`expectedRevision`); a stale one is 409 `revision-conflict`.
//!
//! Error mapping: transport failures → `Offline`, 401 → `Unauthorized`,
//! 404 → `NotFound`, 409 `revision-conflict` → `RevisionConflict`, 409
//! `cursor-expired` → `CursorExpired`, 429 → `RateLimited`, any other
//! non-2xx → `Rejected{status, message, details}` (`details` from the
//! contract's `details.issues`, `details.errors` and `details.tried`).

pub mod access_ai; // --- access-ai --- (client access, AI on the blyg)
pub mod auth;
pub mod background_api; // --- background --- (studio 0.61, contract 5.10.0)
pub mod browse; // --- browse --- (studio 0.49)
pub mod changelog; // --- changelog display --- (studio 0.58, contract 5.8.0)
pub mod conditional; // conditional reads (studio 0.65, contract 5.14.0)
pub mod contacts; // --- contacts --- (studio 0.56, contract 5.7.0)
pub mod discover; // --- discover --- (studio 0.60, contract 5.9.0)
pub mod lineage; // --- lineage counts --- (studio extension lineage-glyph)
pub mod media;
pub mod oauth;
pub mod public;
pub mod read_batch; // read batches, conditional children (studio 0.65)
// --- site ---
pub mod site;
// --- end site ---
// --- upkeep --- stale quotes, signals, a version's text
pub mod upkeep;
// --- end upkeep ---
pub mod shelf_api; // --- shelf sync --- (studio 0.61)
pub mod syndication_api;
pub mod wire; // --- syndication --- (decision #98)

use std::io::Read;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::backend::{CoreError, Result};
use crate::model::{
    Hopper, HopperEdit, Kind, Mention, RemoteId, RemoteRef, ResponsesMode, ScopeProvenance,
    Settings, SubId, SubscribePreview, Subscription, SubscriptionKind, Version,
};
use auth::Credential;
pub use conditional::Traffic;
use conditional::{Counters, Validators};
use wire::*;

/// The private API contract major this Burrow speaks: blygger-studio's
/// 5.0.0 (studio 0.49.10+; Burrow's minimum is 0.49.12, which also keeps
/// tokens made for the old host-root `/api` of a mounted blyg working, up
/// to studio 0.64; 0.65 refuses them, and Burrow asks for a new sign-in:
/// `Api::audience_moved`). It is the oldest major Burrow speaks; the
/// newest is [`CONTRACT_MAJOR_NEWEST`]. An older one is refused before a
/// list is read. A newer one ([`Contract::Newer`]) syncs on, gated on the
/// capabilities it lists, but its lists never remove what's held here
/// ([`Prune::Withheld`]): a page in another shape would read as the end of
/// the list, and rows would be pruned.
pub const CONTRACT_MAJOR: u64 = 5;

/// The newest private API contract major this Burrow speaks. Contract 6
/// (studio 0.67) only drops `GET /api/changes`, which sync already does
/// without (`api::conditional`).
pub const CONTRACT_MAJOR_NEWEST: u64 = 6;

/// The oldest blygger-studio Burrow syncs with, as a literal, so UI text
/// can `concat!` it into `&'static str` consts (`blyg_core::min_studio!()`).
/// Every message that states Burrow's server floor is built from this, never
/// a typed-out version.
#[macro_export]
macro_rules! min_studio {
    () => {
        "0.49.12"
    };
}

/// The oldest blygger-studio Burrow syncs with, for messages.
pub const MIN_STUDIO: &str = crate::min_studio!();

/// Whether a studio's capabilities (`GET /api/capabilities`) list the
/// operation `op`, as an array of names or an object keyed by them. Every
/// feature a studio may lack is gated on this, never on `contractVersion`
/// (skill `capability-gating`): a version Burrow can't read ("next") still
/// gets what it lists.
pub fn offers(capabilities: &Value, op: &str) -> bool {
    match capabilities.get("operations") {
        Some(Value::Array(a)) => a.iter().any(|o| o.as_str() == Some(op)),
        Some(Value::Object(m)) => m.contains_key(op),
        _ => false,
    }
}

/// Whether to send `wholeBlyg` on `POST /api/subscriptions` to a studio on
/// `version`. The key (5.1.0, studio 0.50.0) has no operation of its own to
/// look for, so this is the floor rule: only the floor contract, 5.0, is
/// known to refuse it (its body is strict: 400). Anything else is sent it,
/// and a 400 is asked again without it (`Api::post_subscription`).
pub fn takes_whole_blyg(version: &str) -> bool {
    let mut parts = version.trim().split('.').map(|p| p.parse::<u64>().ok());
    !matches!(
        (parts.next().flatten(), parts.next().flatten()),
        (Some(CONTRACT_MAJOR), Some(0))
    )
}

/// What Burrow makes of a studio's `contractVersion`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Contract {
    /// 5.x or 6.x: everything syncs.
    Supported,
    /// Older than 5 (or no contract at all: a studio before capability
    /// discovery). The studio needs updating; nothing syncs.
    TooOld,
    /// A major newer than this Burrow knows, or a version that isn't a
    /// number. Sync goes on (each feature gated on the capabilities the
    /// studio lists), your writes go through, and its lists remove nothing
    /// held here until a Burrow that knows the contract
    /// ([`Prune::Withheld`]). Burrow says so once (`CoreEvent::ServerNewer`).
    ///
    /// A version that isn't a number ("next", "v7") is judged the same: it
    /// came from a studio that answers `/api/capabilities`, so it isn't one
    /// from before contract 5, and it's most likely a pre-release. Refusing
    /// it would stop all sync for nothing; taking it as Newer costs only
    /// the prunes, and is the safe guess whatever it turns out to be.
    Newer(String),
}

/// Whether a list read from a studio may remove what's held here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Prune {
    /// A contract Burrow knows: a list read to its end is the truth, and
    /// what it lacks goes (an item deleted, a post unsubscribed from…).
    #[default]
    Allowed,
    /// A newer contract ([`Contract::Newer`]): a list adds and updates, and
    /// removes nothing, whatever it lacks. A page in a shape Burrow doesn't
    /// know would otherwise read as an empty or ended list.
    Withheld,
}

impl Contract {
    /// What this contract's lists may remove here (`None`: nothing is read).
    pub fn prune(&self) -> Option<Prune> {
        match self {
            Contract::Supported => Some(Prune::Allowed),
            Contract::TooOld => None,
            Contract::Newer(_) => Some(Prune::Withheld),
        }
    }
}

/// Judge a studio's `contractVersion` (`None`: the studio has no
/// `/api/capabilities` under its mount).
pub fn contract(version: Option<&str>) -> Contract {
    let Some(v) = version else {
        return Contract::TooOld;
    };
    match v
        .trim()
        .split('.')
        .next()
        .and_then(|m| m.parse::<u64>().ok())
    {
        Some(major) if (CONTRACT_MAJOR..=CONTRACT_MAJOR_NEWEST).contains(&major) => {
            Contract::Supported
        }
        Some(major) if major < CONTRACT_MAJOR => Contract::TooOld,
        _ => Contract::Newer(v.trim().to_string()),
    }
}

pub struct Api {
    agent: ureq::Agent,
    /// The blyg URL, with its mount (public pages, the studio, the API).
    base: String,
    cred: Credential,
    /// With a password: the studio session cookie's value, once signed in.
    session: Mutex<Option<String>>,
    /// What items resolve against, from settings as last read or written.
    site: Mutex<Option<Site>>,
    /// Whether `POST /api/subscriptions` takes `wholeBlyg`, once known: from
    /// the floor rule (`takes_whole_blyg`), or from a 400 to the key.
    whole_blyg: Mutex<Option<bool>>,
    /// Each `GET` URL's last body and its `ETag` (`conditional`).
    validators: Mutex<Validators>,
    /// Send `If-None-Match` with a kept ETag (on unless turned off to
    /// measure the difference).
    conditional: AtomicBool,
    /// What's been asked, for measuring.
    counters: Counters,
}

/// The settings an item's derived fields depend on.
#[derive(Debug, Clone, Default)]
struct Site {
    /// `showResponsesDefault`: what `responses: "default"` means.
    responses_default: bool,
    /// `siteUrl` without its trailing `/`; `""` = unset.
    site_url: String,
    /// `highlightGeneratedDefault`: what `highlight: "default"` means.
    highlight_default: bool,
    /// `pickerTyping`.
    picker_typing: Option<crate::model::PickerTyping>,
}

/// Collection page size (the contract's maximum outside `/api/reading`).
const PAGE: u32 = 100;

/// How many times one list is read again from the start after its cursor
/// expired (409 `cursor-expired`), before the read gives up for now.
pub const CURSOR_RESTARTS: usize = 3;

impl std::fmt::Debug for Api {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Api")
            .field("base", &self.base)
            .field("cred", &self.cred)
            .finish()
    }
}

/// Result of `PUT /api/items/:id/versions/:version/pin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pinned {
    pub already: bool,
    /// The item's revision after it.
    pub revision: u64,
}

/// A request body.
enum Body {
    None,
    Json(Value),
    Bytes { content_type: String, data: Vec<u8> },
}

impl Api {
    pub fn new(base_url: &str, cred: impl Into<Credential>) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(Duration::from_secs(30))
            .timeout_write(Duration::from_secs(60))
            .build();
        let base = base_url.trim_end_matches('/').to_string();
        Api {
            agent,
            base,
            cred: cred.into(),
            session: Mutex::new(None),
            site: Mutex::new(None),
            whole_blyg: Mutex::new(None),
            validators: Mutex::new(Validators::default()),
            conditional: AtomicBool::new(true),
            counters: Counters::default(),
        }
    }

    /// Ask `GET`s conditionally (`If-None-Match` with the ETag of the body
    /// kept for that URL). On by default; off sends every `GET` whole, as
    /// Burrow did before studio 0.65 (to measure the difference).
    pub fn set_conditional(&self, on: bool) {
        self.conditional.store(on, Ordering::Relaxed);
    }

    /// Whether this blyg has answered with an `ETag`: it offers
    /// conditional reads (studio 0.65+; there's no capability for it).
    pub fn conditional_reads(&self) -> bool {
        self.counters.etags_seen()
    }

    /// The requests, `304`s and body bytes so far.
    pub fn traffic(&self) -> Traffic {
        self.counters.snapshot()
    }

    /// Whether the credential was made for a different API address than
    /// this blyg's (`{blyg}/api`): a token or browser sign-in made for the
    /// host-root `{origin}/api` of a blyg mounted under a path. Studio 0.65
    /// refuses those (the alias is gone, studio#151), so only a new
    /// sign-in, which finds the API under the mount, helps. A password, or
    /// a token that doesn't say its audience: false.
    pub fn audience_moved(&self) -> bool {
        let token = match &self.cred {
            Credential::Token(t) => t.clone(),
            Credential::OAuth(s) => s.grant().access,
            Credential::Password(_) => return false,
        };
        oauth::audience_moved(&token, &self.api_url())
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    /// Where the REST API lives: `{blyg}/api` (the mount included).
    pub fn api_url(&self) -> String {
        format!("{}/api", self.base)
    }

    /// The studio session, signing in when there's none yet.
    fn session(&self, password: &str) -> Result<String> {
        if let Some(s) = self.session.lock().unwrap().clone() {
            return Ok(s);
        }
        let s = auth::login(&self.base, password)?;
        *self.session.lock().unwrap() = Some(s.clone());
        Ok(s)
    }

    /// A request with the credential on it, and the browser sign-in's
    /// access token it carries (for the 401 retry).
    fn request_url(&self, method: &str, url: &str) -> Result<(ureq::Request, Option<String>)> {
        let r = self
            .agent
            .request(method, url)
            .set("accept", "application/json");
        Ok(match &self.cred {
            Credential::Token(t) => (r.set("authorization", &format!("Bearer {t}")), None),
            Credential::Password(p) => (
                r.set(
                    "cookie",
                    &format!("{}={}", auth::SESSION_COOKIE, self.session(p)?),
                ),
                None,
            ),
            Credential::OAuth(s) => {
                let t = s.access_token()?;
                (r.set("authorization", &format!("Bearer {t}")), Some(t))
            }
        })
    }

    /// A request at an absolute URL, with the 403 scope message.
    /// `if_none_match`: ask conditionally with that ETag.
    fn send_url(
        &self,
        method: &str,
        url: &str,
        body: &Body,
        if_none_match: Option<&str>,
    ) -> (Result<Answer>, Option<String>) {
        let (mut req, bearer) = match self.request_url(method, url) {
            Ok(r) => r,
            Err(e) => return (Err(e), None),
        };
        if let Some(etag) = if_none_match {
            req = req.set("if-none-match", etag);
        }
        let res = match body {
            Body::None => req.call(),
            Body::Json(v) => req.send_json(v.clone()),
            Body::Bytes { content_type, data } => {
                req.set("content-type", content_type).send_bytes(data)
            }
        };
        // A bearer without the scope this call needs: say which permission
        // is missing (it's in the challenge).
        if let Err(ureq::Error::Status(403, r)) = &res
            && let Some(needed) = r
                .header("www-authenticate")
                .and_then(oauth::insufficient_scope)
        {
            let granted = match &self.cred {
                Credential::OAuth(s) => Some(s.grant().scope),
                _ => None,
            };
            let message = oauth::missing_scope_message(&needed, granted.as_deref());
            return (
                Err(CoreError::Rejected {
                    status: 403,
                    message,
                    details: vec![],
                }),
                bearer,
            );
        }
        let a = read_answer(res);
        match &a {
            Ok(a) => self.counters.note(a.status, a.text.len(), a.etag.is_some()),
            Err(_) => self.counters.note(0, 0, false),
        }
        (a, bearer)
    }

    /// One call. `path` starts with `/api/`, under the blyg's mount. With
    /// a password, a 401 means the session ended (30 days, or the server's
    /// secret changed): sign in again and retry once. With a browser
    /// sign-in, renew the access token and retry once; when the renewal is
    /// refused the grant is marked ended (sign in again).
    fn exec(&self, method: &str, path: &str, body: Body) -> Result<Value> {
        self.exec_url(method, &format!("{}{}", self.base, path), body)
    }

    /// `exec` at an absolute URL: a 401 signs in again (a password) or
    /// refreshes the browser sign-in once, and the request is retried. A
    /// `GET` is asked conditionally (`get_validated`).
    fn exec_url(&self, method: &str, url: &str, body: Body) -> Result<Value> {
        if method == "GET" {
            return self.get_validated(url).map(|(v, _)| v);
        }
        json_of(&self.exec_answer(method, url, &body, None)?.text)
    }

    /// One request, with the 401 retry.
    fn exec_answer(
        &self,
        method: &str,
        url: &str,
        body: &Body,
        if_none_match: Option<&str>,
    ) -> Result<Answer> {
        let (r, bearer) = self.send_url(method, url, body, if_none_match);
        match (&self.cred, r) {
            (Credential::Password(_), Err(CoreError::Unauthorized)) => {
                *self.session.lock().unwrap() = None;
                self.send_url(method, url, body, if_none_match).0
            }
            (Credential::OAuth(s), Err(CoreError::Unauthorized)) if bearer.is_some() => {
                s.after_unauthorized(bearer.as_deref())?;
                self.send_url(method, url, body, if_none_match).0
            }
            (_, r) => r,
        }
    }

    /// A `GET`, asked with the ETag of the body last read from `url` when
    /// there is one (`conditional`). A 304 hands back that body, never
    /// nothing; a 200 with an `ETag` is kept for next time. The `bool`:
    /// the blyg said nothing changed (304).
    fn get_validated(&self, url: &str) -> Result<(Value, bool)> {
        let kept = self.kept_etag(url);
        let etag = kept.as_ref().map(|k| k.etag.as_str());
        let a = self.exec_answer("GET", url, &Body::None, etag)?;
        if a.status != 304 {
            return self.keep(url, a).map(|v| (v, false));
        }
        // The kept body is the answer, unless the 304 names another
        // validator (or nothing was kept): then ask whole.
        if let Some(k) = &kept
            && a.etag.as_deref().is_none_or(|e| e == k.etag)
        {
            return Ok((json_of(&k.body)?, true));
        }
        self.forget_etag(url);
        let a = self.exec_answer("GET", url, &Body::None, None)?;
        if a.status == 304 {
            return Err(CoreError::Other(format!(
                "the blyg answered 304 to a request with no validator ({})",
                url_path(url)
            )));
        }
        self.keep(url, a).map(|v| (v, false))
    }

    /// A `GET`'s 200: keep it beside its `ETag` (when the blyg sent one).
    fn keep(&self, url: &str, a: Answer) -> Result<Value> {
        let v = json_of(&a.text)?;
        match &a.etag {
            Some(etag) => self.remember_etag(url, etag, &a.text),
            None => self.forget_etag(url),
        }
        Ok(v)
    }

    /// The body and ETag kept for `url` (none while conditional reads are
    /// off).
    fn kept_etag(&self, url: &str) -> Option<conditional::Kept> {
        if !self.conditional.load(Ordering::Relaxed) {
            return None;
        }
        self.validators.lock().unwrap().get(url)
    }

    /// Keep `text` as `url`'s body under `etag`.
    fn remember_etag(&self, url: &str, etag: &str, text: &str) {
        if self.conditional.load(Ordering::Relaxed) {
            self.validators.lock().unwrap().put(url, etag, text);
        }
    }

    fn forget_etag(&self, url: &str) {
        self.validators.lock().unwrap().forget(url);
    }

    fn call(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value> {
        self.exec(method, path, body.map_or(Body::None, Body::Json))
    }

    fn call_as<T: DeserializeOwned>(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> Result<T> {
        let v = self.call(method, path, body)?;
        serde_json::from_value(v)
            .map_err(|e| CoreError::Other(format!("unexpected response from {path}: {e}")))
    }

    /// 404 → `None` (the route isn't there).
    pub fn optional_of<T>(r: Result<T>) -> Result<Option<T>> {
        Self::optional(r)
    }

    /// 404 → `None` (the route isn't there).
    fn optional<T>(r: Result<T>) -> Result<Option<T>> {
        match r {
            Ok(v) => Ok(Some(v)),
            Err(CoreError::NotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// One page of a cursor-paged list: `path?query&limit=…[&cursor=…]`.
    fn page<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<CursorPage<T>> {
        self.page_validated(path, query, limit, cursor)
            .map(|(p, _)| p)
    }

    /// `page`, and whether the blyg said it hadn't changed (304: the page
    /// is the one last read at that URL, whole).
    fn page_validated<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<(CursorPage<T>, bool)> {
        let (v, same) = self.get_validated(&self.page_url(path, query, limit, cursor))?;
        serde_json::from_value(v)
            .map(|p| (p, same))
            .map_err(|e| CoreError::Other(format!("unexpected response from {path}: {e}")))
    }

    /// A page's absolute URL (the key its ETag is kept under).
    fn page_url(&self, path: &str, query: &str, limit: u32, cursor: Option<&str>) -> String {
        let mut url = format!("{}{path}?", self.base);
        if !query.is_empty() {
            url.push_str(query);
            url.push('&');
        }
        url.push_str(&format!("limit={limit}"));
        if let Some(c) = cursor {
            url.push_str("&cursor=");
            url.push_str(&enc(c));
        }
        url
    }

    /// Every element of a cursor-paged list, as one coherent acquisition:
    /// pages are read until `nextCursor` is null. When the list changes
    /// under the walk (409 `cursor-expired`), everything read so far is
    /// dropped and the walk starts again without a cursor, at most
    /// [`CURSOR_RESTARTS`] times; then `CursorExpired`. Never a partial list,
    /// so a caller may prune what the answer leaves out.
    fn collect<T: DeserializeOwned>(&self, path: &str, query: &str) -> Result<Vec<T>> {
        for _ in 0..=CURSOR_RESTARTS {
            match self.walk(path, query) {
                Err(CoreError::CursorExpired) => continue,
                r => return r,
            }
        }
        Err(CoreError::CursorExpired)
    }

    /// One walk of a list from its first page to its last.
    fn walk<T: DeserializeOwned>(&self, path: &str, query: &str) -> Result<Vec<T>> {
        let mut all = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let page: CursorPage<T> = self.page(path, query, PAGE, cursor.as_deref())?;
            all.extend(page.items);
            match page.page.next_cursor {
                // A server handing back the cursor it was given would loop.
                Some(next) if cursor.as_deref() == Some(next.as_str()) => {
                    return Err(CoreError::Other(format!(
                        "{path} answered the same page twice"
                    )));
                }
                Some(next) => cursor = Some(next),
                None => return Ok(all),
            }
        }
    }

    /// The cached site settings, reading them when unknown.
    fn site(&self) -> Site {
        if let Some(s) = self.site.lock().unwrap().clone() {
            return s;
        }
        let _ = self.settings();
        self.site.lock().unwrap().clone().unwrap_or_default()
    }

    /// Where the blyg's public pages live, without a trailing `/`: its
    /// `siteUrl` when set, else the blyg URL (which carries the mount).
    /// The studio builds its own origin the same way.
    pub fn public_base(&self) -> String {
        let s = self.site();
        if s.site_url.is_empty() {
            self.base.clone()
        } else {
            s.site_url
        }
    }

    fn resolve(&self, mut w: WireItem) -> WireItem {
        let site = self.site();
        let base = if site.site_url.is_empty() {
            &self.base
        } else {
            &site.site_url
        };
        w.resolve(site.responses_default, base);
        w.resolve_highlight(site.highlight_default);
        w
    }

    // ---------- contract ----------

    /// The studio's private-API contract version, from `GET
    /// {blyg}/api/capabilities` (under the mount; a studio before 0.49.7
    /// has none there). `Ok(None)`: not served (404), or not readable as
    /// one. Refused credentials (401 after a refresh, 403), offline,
    /// rate-limited or 5xx: an error, so nothing passes on a failed read.
    /// Whether the studio's capabilities list every one of `ops`
    /// ([`offers`]). A studio without capabilities offers nothing; a failed
    /// read (offline, a 5xx, a refused credential) decides nothing.
    pub fn offers_all(&self, ops: &[&str]) -> Result<bool> {
        match self.call("GET", "/api/capabilities", None) {
            Ok(v) => Ok(ops.iter().all(|op| offers(&v, op))),
            Err(e) if is_transient(&e) || matches!(e, CoreError::Unauthorized) => Err(e),
            Err(_) => Ok(false),
        }
    }

    pub fn contract_version(&self) -> Result<Option<String>> {
        match self.call("GET", "/api/capabilities", None) {
            Ok(v) => Ok(v
                .get("contractVersion")
                .and_then(Value::as_str)
                .map(str::to_string)),
            Err(e @ CoreError::Unauthorized) => Err(e),
            Err(
                e @ CoreError::Rejected {
                    status: 401 | 403, ..
                },
            ) => Err(e),
            Err(e) if is_transient(&e) => Err(e),
            // 404 (no capabilities under this mount), or anything that
            // isn't a capabilities answer at this address.
            Err(_) => Ok(None),
        }
    }

    // ---------- items ----------

    /// Every item. Re-reads settings first, so `responses: "default"`
    /// resolves against the current site default.
    pub fn list_items(&self) -> Result<Vec<WireItem>> {
        self.list_items_with(true)
    }

    /// Every item; `refresh_settings: false` resolves against the settings
    /// as last read (when the blyg says they haven't changed).
    pub fn list_items_with(&self, refresh_settings: bool) -> Result<Vec<WireItem>> {
        let items: Vec<WireItem> = self.collect("/api/items", "")?;
        if refresh_settings || self.site.lock().unwrap().is_none() {
            let _ = self.settings();
        }
        Ok(items.into_iter().map(|w| self.resolve(w)).collect())
    }

    /// How many items the server holds (`GET /api/items/summary`).
    pub fn count_items(&self) -> Result<u64> {
        #[derive(serde::Deserialize)]
        struct Summary {
            all: u64,
        }
        Ok(self
            .call_as::<Summary>("GET", "/api/items/summary", None)?
            .all)
    }

    /// `GET /api/items/:id` (no version history: [`Api::item_versions`]).
    pub fn get_item(&self, id: &str) -> Result<WireItem> {
        let w = self.call_as("GET", &format!("/api/items/{}", enc(id)), None)?;
        Ok(self.resolve(w))
    }

    /// The item and its whole version history (`versions` filled).
    pub fn get_item_full(&self, id: &str) -> Result<WireItem> {
        let mut w = self.get_item(id)?;
        w.versions = Some(self.item_versions(id)?);
        Ok(w)
    }

    /// `GET /api/items/:id/versions`, every page, oldest first.
    pub fn item_versions(&self, id: &str) -> Result<Vec<Version>> {
        let mut v: Vec<WireVersion> =
            self.collect(&format!("/api/items/{}/versions", enc(id)), "")?;
        v.sort_by_key(|w| w.version);
        Ok(v.into_iter().map(WireVersion::into_version).collect())
    }

    /// `POST /api/items {mode: "blank", kind, contentMd, stubOf?,
    /// provenance?}` → the new item (its id and revision). `provenance` has
    /// one entry per TK scope.
    pub fn create_item(
        &self,
        content_md: &str,
        kind: Kind,
        stub_of: Option<&RemoteRef>,
        provenance: Option<&[Option<ScopeProvenance>]>,
    ) -> Result<WireItem> {
        let mut body = json!({ "mode": "blank", "contentMd": content_md, "kind": kind_str(kind) });
        if let Some(s) = stub_of {
            let mut s = s.clone();
            // Upstream's `Citation` needs source, url and retrieved (strict).
            if s.cited.as_ref().is_some_and(|c| !c.complete()) {
                s.cited = None;
            }
            // A written origin has one spelling (§12.2).
            if let Some(o) = crate::origin::canonical(&s.origin) {
                s.origin = o;
            }
            body["stubOf"] = serde_json::to_value(&s).unwrap_or(Value::Null);
        }
        if let Some(p) = provenance {
            body["provenance"] = provenance_body(p);
        }
        self.call_as("POST", "/api/items", Some(body))
    }

    /// `PATCH /api/items/:id {expectedRevision, …}` → the item as stored.
    fn patch_item(&self, id: &str, revision: u64, mut body: Value) -> Result<WireItem> {
        body["expectedRevision"] = json!(revision);
        let w = self.call_as("PATCH", &format!("/api/items/{}", enc(id)), Some(body))?;
        Ok(self.resolve(w))
    }

    /// `PATCH /api/items/:id {contentMd}`.
    pub fn save_item(&self, id: &str, revision: u64, content_md: &str) -> Result<WireItem> {
        self.patch_item(id, revision, json!({ "contentMd": content_md }))
    }

    /// `PATCH /api/items/:id {contentMd, provenance}`: the text and its
    /// whole per-scope provenance in one atomic write.
    pub fn save_item_provenance(
        &self,
        id: &str,
        revision: u64,
        content_md: &str,
        provenance: &[Option<ScopeProvenance>],
    ) -> Result<WireItem> {
        self.patch_item(
            id,
            revision,
            json!({ "contentMd": content_md, "provenance": provenance_body(provenance) }),
        )
    }

    /// `PATCH /api/items/:id {contentMd, kind}`: a draft's kind can change
    /// until it is first published (409 after).
    pub fn save_item_kind(
        &self,
        id: &str,
        revision: u64,
        content_md: &str,
        kind: Kind,
    ) -> Result<WireItem> {
        self.patch_item(
            id,
            revision,
            json!({ "contentMd": content_md, "kind": kind_str(kind) }),
        )
    }

    pub fn publish(&self, id: &str, revision: u64, note: Option<&str>) -> Result<Published> {
        self.call_as(
            "POST",
            &format!("/api/items/{}/publish", enc(id)),
            Some(command_body(revision, note)),
        )
    }

    /// `POST /api/items/:id/withdraw` → (the withdrawal's version, the
    /// item's revision after it).
    pub fn withdraw(&self, id: &str, revision: u64, note: Option<&str>) -> Result<(u32, u64)> {
        #[derive(serde::Deserialize)]
        struct R {
            version: u32,
            revision: u64,
        }
        let r: R = self.call_as(
            "POST",
            &format!("/api/items/{}/withdraw", enc(id)),
            Some(command_body(revision, note)),
        )?;
        Ok((r.version, r.revision))
    }

    /// `PUT /api/items/:id/versions/:version/pin` (repeat-safe).
    pub fn pin(&self, id: &str, revision: u64, version: u32) -> Result<Pinned> {
        #[derive(serde::Deserialize)]
        struct R {
            #[serde(default)]
            already: bool,
            revision: u64,
        }
        let r: R = self.call_as(
            "PUT",
            &format!("/api/items/{}/versions/{version}/pin", enc(id)),
            Some(json!({ "expectedRevision": revision })),
        )?;
        Ok(Pinned {
            already: r.already,
            revision: r.revision,
        })
    }

    /// `POST /api/items/:id/restore {version}` → the item's revision after.
    pub fn restore(&self, id: &str, revision: u64, version: u32) -> Result<u64> {
        #[derive(serde::Deserialize)]
        struct R {
            revision: u64,
        }
        Ok(self
            .call_as::<R>(
                "POST",
                &format!("/api/items/{}/restore", enc(id)),
                Some(json!({ "expectedRevision": revision, "version": version })),
            )?
            .revision)
    }

    /// `DELETE /api/items/:id {expectedRevision}`: discards a draft.
    pub fn delete_item(&self, id: &str, revision: u64) -> Result<()> {
        self.call(
            "DELETE",
            &format!("/api/items/{}", enc(id)),
            Some(json!({ "expectedRevision": revision })),
        )
        .map(|_| ())
    }

    /// `POST /api/items/:id/generate {scope, expectedRevision}`. The server
    /// fills that TK scope with its own model, records the provenance and
    /// stores the spliced text. `revision`: `None` reads the item first and
    /// uses its current revision (for a caller that holds none).
    pub fn generate(&self, id: &str, scope: u32, revision: Option<u64>) -> Result<Generated> {
        let revision = match revision {
            Some(r) => r,
            None => self.get_item(id)?.revision,
        };
        #[derive(serde::Deserialize)]
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
        let r: R = self.call_as(
            "POST",
            &format!("/api/items/{}/generate", enc(id)),
            Some(json!({ "scope": scope, "expectedRevision": revision })),
        )?;
        Ok(Generated {
            text: r.text,
            model: r.model.unwrap_or_else(|| "unknown".into()),
            content_md: r.content_md,
            revision: r.revision,
        })
    }

    /// `GET /api/changes`: the blyg's change epoch and one revision counter
    /// per data domain. `None` when the route isn't there (404).
    pub fn changes(&self) -> Result<Option<ChangeState>> {
        Self::optional(self.call_as("GET", "/api/changes", None))
    }

    /// `POST /api/items {mode: "fork", source}` → the new draft.
    pub fn fork(&self, of: &RemoteRef) -> Result<WireItem> {
        // A written origin has one spelling (§12.2).
        let origin = crate::origin::canonical(&of.origin).unwrap_or_else(|| of.origin.clone());
        let body = json!({
            "mode": "fork",
            "source": { "origin": origin, "id": of.id, "version": of.version },
        });
        let w = self.call_as("POST", "/api/items", Some(body))?;
        Ok(self.resolve(w))
    }

    /// `PATCH /api/items/:id {responses}` → the item as stored (resolved:
    /// `shows_responses`, `responses_mode`).
    pub fn set_responses(&self, id: &str, revision: u64, mode: ResponsesMode) -> Result<WireItem> {
        self.patch_item(id, revision, json!({ "responses": mode.as_str() }))
    }

    /// `PATCH /api/items/:id {highlight}` (needs `owner:publish`, as it
    /// changes the public page at once) → the item as stored (resolved:
    /// `highlights`, `highlight`).
    pub fn set_highlight(
        &self,
        id: &str,
        revision: u64,
        mode: crate::model::HighlightMode,
    ) -> Result<WireItem> {
        self.patch_item(id, revision, json!({ "highlight": mode.as_str() }))
    }

    // ---------- media ----------

    /// `POST /api/media` (multipart). `item`: the item to attach it to and
    /// the revision that attachment is based on; `None` uploads it loose.
    pub fn upload_media(
        &self,
        bytes: &[u8],
        mime: &str,
        item: Option<(&str, u64)>,
        alt: Option<&str>,
    ) -> Result<Media> {
        let boundary = format!("----blygger{:x}", crate::util::now_ms() ^ 0x5eed_b1a9);
        let ext = match mime {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            "image/gif" => "gif",
            "image/webp" => "webp",
            "image/svg+xml" => "svg",
            _ => "bin",
        };
        let mut body: Vec<u8> = Vec::with_capacity(bytes.len() + 512);
        let text_field = |body: &mut Vec<u8>, name: &str, value: &str| {
            body.extend_from_slice(
                format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes(),
            );
        };
        if let Some((id, revision)) = item {
            text_field(&mut body, "itemId", id);
            text_field(&mut body, "expectedRevision", &revision.to_string());
        }
        if let Some(a) = alt {
            text_field(&mut body, "alt", a);
        }
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"upload.{ext}\"\r\nContent-Type: {mime}\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let v = self.exec(
            "POST",
            "/api/media",
            Body::Bytes {
                content_type: format!("multipart/form-data; boundary={boundary}"),
                data: body,
            },
        )?;
        serde_json::from_value(v)
            .map_err(|e| CoreError::Other(format!("unexpected media response: {e}")))
    }

    /// `DELETE /api/media/:id {expectedRevision?}`: detaches it from its
    /// item, deleting the bytes when no published version shows them.
    /// `revision` is the owning item's (required for an attachment, absent
    /// for a loose upload). 404 unknown; 409 when it's the site avatar.
    /// Returns the item's revision after it, for an attachment.
    pub fn delete_media(&self, id: &str, revision: Option<u64>) -> Result<Option<u64>> {
        let body = match revision {
            Some(r) => json!({ "expectedRevision": r }),
            None => json!({}),
        };
        let v = self.call("DELETE", &format!("/api/media/{}", enc(id)), Some(body))?;
        Ok(v.get("revision").and_then(Value::as_u64))
    }

    // ---------- subscriptions ----------

    pub fn list_subscriptions(&self) -> Result<Vec<Subscription>> {
        let subs: Vec<WireSubscription> = self.collect("/api/subscriptions", "")?;
        Ok(subs.into_iter().map(Subscription::from).collect())
    }

    /// Whether to send this studio `wholeBlyg`: what it answered before,
    /// else the floor rule on its contract. A failed read sends it (a 400
    /// is asked again without it).
    fn whole_blyg(&self) -> bool {
        if let Some(w) = *self.whole_blyg.lock().unwrap() {
            return w;
        }
        match self.contract_version() {
            Ok(v) => {
                let w = v.as_deref().is_none_or(takes_whole_blyg);
                *self.whole_blyg.lock().unwrap() = Some(w);
                w
            }
            Err(_) => true,
        }
    }

    /// `POST /api/subscriptions` with `body` plus `wholeBlyg: true` where the
    /// studio takes it. A URL that names one of a blyg's public hoppers
    /// otherwise proposes (and confirms) a hopper subscription, which Burrow
    /// doesn't model: it follows the whole blyg instead (spec 0.4 §16.12
    /// rule 4). The key changes nothing for any other URL. A 400 to the key
    /// is asked again without it; when that works, the key isn't sent again.
    fn post_subscription(&self, body: Value) -> Result<Value> {
        if !self.whole_blyg() {
            return self.call("POST", "/api/subscriptions", Some(body));
        }
        let mut with = body.clone();
        with["wholeBlyg"] = json!(true);
        match self.call("POST", "/api/subscriptions", Some(with)) {
            Err(CoreError::Rejected { status: 400, .. }) => {
                let r = self.call("POST", "/api/subscriptions", Some(body));
                if r.is_ok() {
                    *self.whole_blyg.lock().unwrap() = Some(false);
                }
                r
            }
            r => r,
        }
    }

    pub fn preview_subscription(&self, url: &str) -> Result<SubscribePreview> {
        let v = self.post_subscription(json!({ "url": url }))?;
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        let kind = if s("kind").as_deref() == Some("rss") {
            SubscriptionKind::Rss
        } else {
            SubscriptionKind::Blyg
        };
        Ok(SubscribePreview {
            kind,
            title: s("title").unwrap_or_default(),
            origin: s("origin"),
            feed_url: s("feedUrl"),
            // A blyg that claims another site answers `{asserted, actual}`;
            // no key means it matched. Feeds aren't checked.
            site_mismatch: (kind == SubscriptionKind::Blyg).then(|| {
                v.get("siteMismatch")
                    .is_some_and(|m| m.as_bool().unwrap_or(m.is_object()))
            }),
        })
    }

    /// `POST /api/subscriptions {url, confirm:true, title?}` → new subscription id.
    pub fn subscribe(&self, url: &str, title: Option<&str>) -> Result<SubId> {
        let mut body = json!({ "url": url, "confirm": true });
        if let Some(t) = title {
            body["title"] = json!(crate::text::for_save(t));
        }
        let v = self.post_subscription(body)?;
        let created: Created = serde_json::from_value(v)
            .map_err(|e| CoreError::Other(format!("unexpected subscription answer: {e}")))?;
        Ok(created.id.into())
    }

    /// `PATCH /api/subscriptions/:id`. `title`: `None` leaves the name alone,
    /// `Some(Some(t))` sets a name of your own, `Some(None)` sends
    /// `title: null`, handing the name back to the source, which refreshes
    /// it while polling.
    pub fn update_subscription(
        &self,
        id: &str,
        in_blogroll: Option<bool>,
        title: Option<Option<&str>>,
    ) -> Result<()> {
        let mut body = json!({});
        if let Some(b) = in_blogroll {
            body["inBlogroll"] = json!(b);
        }
        if let Some(t) = title {
            body["title"] = t.map_or(Value::Null, |t| json!(crate::text::for_save(t)));
        }
        self.call(
            "PATCH",
            &format!("/api/subscriptions/{}", enc(id)),
            Some(body),
        )
        .map(|_| ())
    }

    pub fn delete_subscription(&self, id: &str) -> Result<()> {
        self.call("DELETE", &format!("/api/subscriptions/{}", enc(id)), None)
            .map(|_| ())
    }

    /// `PATCH /api/subscriptions/:id {paused}`.
    pub fn pause_subscription(&self, id: &str, paused: bool) -> Result<()> {
        self.call(
            "PATCH",
            &format!("/api/subscriptions/{}", enc(id)),
            Some(json!({ "paused": paused })),
        )
        .map(|_| ())
    }

    /// `POST /api/subscriptions/poll`: the server polls every subscription
    /// that isn't paused, in the background, and answers at once with how
    /// many.
    pub fn poll_subscriptions(&self) -> Result<u32> {
        let v = self.call("POST", "/api/subscriptions/poll", None)?;
        Ok(v.get("polling").and_then(Value::as_u64).unwrap_or(0) as u32)
    }

    /// `{ok, changed}` (`changed` counts items); blyg subscriptions only
    /// (409 otherwise). True when anything changed.
    pub fn resync_subscription(&self, id: &str) -> Result<bool> {
        let v = self.call(
            "POST",
            &format!("/api/subscriptions/{}/resync", enc(id)),
            None,
        )?;
        let changed = v.get("changed");
        Ok(changed.and_then(Value::as_u64).is_some_and(|n| n > 0)
            || changed.and_then(Value::as_bool).unwrap_or(false))
    }

    // ---------- signals / mentions ----------

    pub fn set_signal(&self, sub: &SubId, remote_id: &RemoteId, thumb: i8) -> Result<()> {
        self.call(
            "PUT",
            &format!("/api/signals/{}/{}", enc(sub), enc(remote_id)),
            Some(json!({ "thumb": thumb })),
        )
        .map(|_| ())
    }

    /// Every signal the owner holds (`GET /api/signals`, cursor pages):
    /// `(subscriptionId, remoteId, thumb)`. `None` when the route isn't
    /// there.
    pub fn signals(&self) -> Result<Option<Vec<(String, String, i8)>>> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct S {
            subscription_id: String,
            remote_id: String,
            thumb: i8,
        }
        Ok(
            Self::optional(self.collect::<S>("/api/signals", ""))?.map(|v| {
                v.into_iter()
                    .map(|s| (s.subscription_id, s.remote_id, s.thumb))
                    .collect()
            }),
        )
    }

    pub fn clear_signal(&self, sub: &SubId, remote_id: &RemoteId) -> Result<()> {
        self.call(
            "DELETE",
            &format!("/api/signals/{}/{}", enc(sub), enc(remote_id)),
            None,
        )
        .map(|_| ())
    }

    pub fn set_mention_hidden(&self, id: &str, hidden: bool) -> Result<()> {
        self.call(
            "PATCH",
            &format!("/api/mentions/{}", enc(id)),
            Some(json!({ "hidden": hidden })),
        )
        .map(|_| ())
    }

    // ---------- settings ----------

    /// `PATCH /api/settings`: sends only the fields that are set.
    pub fn put_settings(&self, s: &Settings) -> Result<()> {
        let mut body = serde_json::Map::new();
        let mut put = |k: &str, v: &Option<String>| {
            if let Some(v) = v {
                body.insert(k.to_string(), json!(v));
            }
        };
        put("siteTitle", &s.site_title);
        put("authorName", &s.author_name);
        put("authorBio", &s.author_bio);
        put("siteUrl", &s.site_url);
        put("theme", &s.theme);
        put("avatarMediaId", &s.avatar_media_id);
        // `Some("")` clears it (the blyg then renders in UTC).
        put("timezone", &s.timezone);
        if let Some(on) = s.accept_mentions {
            body.insert("acceptMentions".into(), json!(on));
        }
        if let Some(on) = s.show_responses_default {
            body.insert("showResponsesDefault".into(), json!(on));
        }
        if let Some(on) = s.highlight_generated_default {
            body.insert("highlightGeneratedDefault".into(), json!(on));
        }
        if let Some(t) = s.picker_typing {
            body.insert("pickerTyping".into(), json!(t.as_str()));
        }
        body.insert(
            "authorLinks".into(),
            serde_json::to_value(&s.author_links).unwrap_or(json!([])),
        );
        let v = self.call("PATCH", "/api/settings", Some(Value::Object(body)))?;
        // The reply is the stored settings.
        match serde_json::from_value::<WireSettings>(v) {
            Ok(saved) => self.remember(Some(&saved.into())),
            Err(_) => *self.site.lock().unwrap() = None,
        }
        Ok(())
    }

    /// Cache what items resolve against. No settings (or no such field)
    /// means no site default (hidden) and no `siteUrl`.
    fn remember(&self, s: Option<&Settings>) {
        *self.site.lock().unwrap() = Some(Site {
            responses_default: s.and_then(|s| s.show_responses_default).unwrap_or(false),
            site_url: s
                .and_then(|s| s.site_url.as_deref())
                .map(|u| u.trim().trim_end_matches('/').to_string())
                .unwrap_or_default(),
            highlight_default: s
                .and_then(|s| s.highlight_generated_default)
                .unwrap_or(false),
            picker_typing: s.and_then(|s| s.picker_typing),
        });
    }

    /// `pickerTyping` as last read or written (`None` until settings were
    /// read).
    pub fn picker_typing(&self) -> Option<crate::model::PickerTyping> {
        self.site
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|s| s.picker_typing)
    }

    // ---------- reads (404 → None) ----------

    /// `GET /api/reading/imported`, one page: imported rows in the app's
    /// shape, keyset-paged by an opaque `cursor` over (observedAt,
    /// subscriptionId, remoteId), at most [`IMPORTED_READING_MAX`] a page.
    /// `content_html` is kept as served: the UI sanitizes it before display.
    /// A whole list is one walk from the first page to `nextCursor: null`
    /// (`Engine::read_reading`, which may also read just the head); on
    /// `CursorExpired` the walk starts over.
    pub fn reading(&self, limit: u32, cursor: Option<&str>) -> Result<Option<ReadingPage>> {
        let limit = limit.clamp(1, IMPORTED_READING_MAX);
        const PATH: &str = "/api/reading/imported";
        // The first page as last read, to tell a page whose rows are all as
        // they were (only its cursor moved, with a feed poll or a thumb)
        // from one with something new.
        let before = cursor
            .is_none()
            .then(|| self.kept_etag(&self.page_url(PATH, "", limit, cursor)))
            .flatten();
        let page: Option<(CursorPage<Value>, bool)> =
            Self::optional(self.page_validated(PATH, "", limit, cursor))?;
        let Some((page, not_modified)) = page else {
            return Ok(None);
        };
        let unchanged = not_modified
            || before.is_some_and(|k| {
                serde_json::from_str::<Value>(&k.body)
                    .ok()
                    .and_then(|v| v.get("items").cloned())
                    .is_some_and(|rows| rows.as_array() == Some(&page.items))
            });
        let mut items = Vec::with_capacity(page.items.len());
        let mut syndication = Vec::new(); // --- syndication ---
        for row in &page.items {
            let mut it = imported_row(row).map_err(|e| {
                CoreError::Other(format!(
                    "unexpected response from /api/reading/imported: {e}"
                ))
            })?;
            it.page = it.page.take().and_then(|pg| absolute_page(&it.origin, &pg));
            // --- syndication --- endcaps carry none (§16.14 rule 4).
            let served = (it.state != "tombstone")
                .then(|| crate::syndication::parse_member(row.get(crate::syndication::MEMBER)))
                .flatten()
                .unwrap_or_default();
            syndication.push((it.subscription_id.clone(), it.remote_id.clone(), served));
            items.push(it);
        }
        Ok(Some(ReadingPage {
            items,
            next: page.page.next_cursor,
            syndication,
            unchanged,
        }))
    }

    /// `GET /api/reading?limit=1`, for its read-state flags only (`readState`,
    /// `readStateClear`). `None` on 404.
    pub fn reading_flags(&self) -> Result<Option<ReadingFlags>> {
        Self::optional(self.call_as("GET", "/api/reading?limit=1", None))
    }

    // ---------- read state ----------

    /// `PUT /api/reading/:sub/:remoteId/read {version, readAt?}`. The server
    /// keeps `max(stored, version)`, so a replay is harmless. A read whose
    /// `readAt` is older than the row's latest "mark unread" is ignored
    /// (`stored: false`).
    pub fn put_read(
        &self,
        sub: &SubId,
        remote_id: &RemoteId,
        version: u32,
        read_at: Option<&str>,
    ) -> Result<ReadAck> {
        let mut body = json!({ "version": version });
        if let Some(at) = read_at {
            body["readAt"] = json!(at);
        }
        self.call_as(
            "PUT",
            &format!("/api/reading/{}/{}/read", enc(sub), enc(remote_id)),
            Some(body),
        )
    }

    /// `DELETE /api/reading/:sub/:remoteId/read`: mark one row unread.
    /// Idempotent; 200 for a row the server doesn't hold.
    pub fn delete_read(&self, sub: &SubId, remote_id: &RemoteId) -> Result<()> {
        self.call(
            "DELETE",
            &format!("/api/reading/{}/{}/read", enc(sub), enc(remote_id)),
            None,
        )
        .map(|_| ())
    }

    /// `POST /api/reading/unread {items: [{sub, remoteId}]}`, at most
    /// `READ_BATCH_MAX` entries, all or nothing.
    pub fn post_unreads(&self, items: &[UnreadMark]) -> Result<()> {
        self.call(
            "POST",
            "/api/reading/unread",
            Some(json!({ "items": items })),
        )
        .map(|_| ())
    }

    /// `POST /api/reading/read {items: [{sub, remoteId, version, readAt?}]}`,
    /// at most `READ_BATCH_MAX` entries (the caller chunks).
    pub fn put_reads(&self, items: &[ReadMark]) -> Result<()> {
        self.call("POST", "/api/reading/read", Some(json!({ "items": items })))
            .map(|_| ())
    }

    /// Inbound mentions (responses to this blyg's posts).
    pub fn mentions(&self) -> Result<Option<Vec<Mention>>> {
        let m: Option<Vec<WireMention>> =
            Self::optional(self.collect("/api/mentions", "direction=inbound"))?;
        Ok(m.map(|m| m.into_iter().map(Mention::from).collect()))
    }

    /// The server fills unset strings with defaults (`""`, or `"auto"`);
    /// `""` becomes `None`. Except `timezone`, where `Some("")` means "the
    /// server has the setting, and it is unset".
    pub fn settings(&self) -> Result<Option<Settings>> {
        let s: Option<WireSettings> = Self::optional(self.call_as("GET", "/api/settings", None))?;
        let s: Option<Settings> = s.map(Settings::from);
        self.remember(s.as_ref());
        Ok(s.map(|mut s| {
            for f in [
                &mut s.site_title,
                &mut s.author_name,
                &mut s.author_bio,
                &mut s.site_url,
                &mut s.theme,
                &mut s.avatar_media_id,
            ] {
                if f.as_deref() == Some("") {
                    *f = None;
                }
            }
            s
        }))
    }

    /// Every hopper, with its item count (`GET /api/hoppers/:id` → `total`,
    /// at most [`HOPPER_READS`] at a time). A hopper deleted between the
    /// list and its count is left out.
    pub fn hoppers(&self) -> Result<Option<Vec<Hopper>>> {
        let Some(list) = Self::optional(self.collect::<WireHopper>("/api/hoppers", ""))? else {
            return Ok(None);
        };
        let counted = bounded(list, HOPPER_READS, |h| match self.hopper_detail(&h.id) {
            Ok(d) => Ok(Some(h.into_hopper(d.total))),
            Err(CoreError::NotFound) => Ok(None),
            Err(e) => Err(e),
        })?;
        Ok(Some(counted.into_iter().flatten().collect()))
    }

    /// `GET /api/hoppers/{id}`: one hopper as the blyg has it now, with its
    /// count (`total`). `NotFound` when it was deleted.
    pub fn hopper(&self, id: &str) -> Result<Hopper> {
        let d = self.hopper_detail(id)?;
        Ok(d.hopper.into_hopper(d.total))
    }

    fn hopper_detail(&self, id: &str) -> Result<WireHopperDetail> {
        self.call_as("GET", &format!("/api/hoppers/{}", enc(id)), None)
    }

    // ---------- hoppers (studio's createHopper … removeHopperItem) ----------

    /// Every hopper with all its memberships, in the hopper's own order
    /// (newest added first), and the imported items it holds (`GET
    /// /api/hoppers`, then each one's `GET /api/hoppers/{id}/memberships`,
    /// every page, at most [`HOPPER_READS`] hoppers at a time). Contract 5
    /// has no cheaper read: the list carries no counts, revisions or
    /// memberships, so the caller skips this whole read while the
    /// `hoppers` change revision stands still. A hopper deleted between the
    /// list and its memberships is left out. `None` from a server without
    /// hoppers (404).
    pub fn hopper_details(&self) -> Result<Option<Vec<HopperDetail>>> {
        let Some(list) = Self::optional(self.collect::<WireHopper>("/api/hoppers", ""))? else {
            return Ok(None);
        };
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct M {
            subscription_id: String,
            remote_id: String,
            #[serde(default)]
            added_at: String,
            #[serde(default)]
            item: Option<Value>,
        }
        let read = bounded(list, HOPPER_READS, |h| {
            let rows: Vec<M> =
                match self.collect(&format!("/api/hoppers/{}/memberships", enc(&h.id)), "") {
                    Ok(rows) => rows,
                    Err(CoreError::NotFound) => return Ok(None),
                    Err(e) => return Err(e),
                };
            let mut members = Vec::with_capacity(rows.len());
            let mut items = Vec::new();
            for m in rows {
                if let Some(it) = m.item.filter(Value::is_object) {
                    items.push(it);
                }
                members.push(HopperMember {
                    subscription_id: m.subscription_id,
                    remote_id: m.remote_id,
                    added_at: m.added_at,
                });
            }
            let count = members.len() as u32;
            Ok(Some(HopperDetail {
                hopper: h.into_hopper(count),
                members,
                items,
            }))
        })?;
        Ok(Some(read.into_iter().flatten().collect()))
    }

    /// `POST /api/hoppers {name}` → the new hopper (201), private and empty.
    pub fn create_hopper(&self, name: &str) -> Result<Hopper> {
        let h: WireHopper = self.call_as(
            "POST",
            "/api/hoppers",
            Some(json!({ "name": crate::text::for_save(name) })),
        )?;
        Ok(h.into_hopper(0))
    }

    /// `PATCH /api/hoppers/{id} {name?, public?, description?}` → the
    /// hopper as it is now (`count` is the caller's to keep).
    pub fn update_hopper(&self, id: &str, edit: &HopperEdit) -> Result<Hopper> {
        let mut body = serde_json::Map::new();
        if let Some(n) = &edit.name {
            body.insert("name".into(), json!(crate::text::for_save(n)));
        }
        if let Some(p) = edit.public {
            body.insert("public".into(), json!(p));
        }
        if let Some(d) = &edit.description {
            body.insert("description".into(), json!(crate::text::for_save(d)));
        }
        let h: WireHopper = self.call_as(
            "PATCH",
            &format!("/api/hoppers/{}", enc(id)),
            Some(Value::Object(body)),
        )?;
        Ok(h.into_hopper(0))
    }

    /// `DELETE /api/hoppers/{id}`: the hopper and its memberships; the
    /// posts themselves are untouched.
    pub fn delete_hopper(&self, id: &str) -> Result<()> {
        self.call("DELETE", &format!("/api/hoppers/{}", enc(id)), None)
            .map(|_| ())
    }

    /// `PUT` (`on`) or `DELETE /api/hoppers/{id}/items/{sub}/{remoteId}`.
    /// `sub` is `~own` for one of your own posts. Both are idempotent.
    pub fn set_hopper_item(
        &self,
        id: &str,
        sub: &SubId,
        remote_id: &RemoteId,
        on: bool,
    ) -> Result<()> {
        self.call(
            if on { "PUT" } else { "DELETE" },
            &format!(
                "/api/hoppers/{}/items/{}/{}",
                enc(id),
                enc(sub),
                enc(remote_id)
            ),
            None,
        )
        .map(|_| ())
    }
}

/// --- hoppers --- One membership: a post in a hopper (`~own` for yours).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HopperMember {
    pub subscription_id: String,
    pub remote_id: String,
    pub added_at: String,
}

/// --- hoppers --- A hopper with every membership, and the imported items
/// it holds (studio's `ImportedItem` rows, for posts this Mac doesn't hold).
#[derive(Debug, Clone)]
pub struct HopperDetail {
    pub hopper: Hopper,
    pub members: Vec<HopperMember>,
    pub items: Vec<Value>,
}

/// --- hoppers --- The contract's `Hopper`.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireHopper {
    id: String,
    name: String,
    #[serde(default)]
    slug: Option<String>,
    #[serde(default)]
    public: bool,
    #[serde(default)]
    slug_frozen: bool,
    #[serde(default)]
    description: Option<String>,
}

impl WireHopper {
    fn into_hopper(self, count: u32) -> Hopper {
        Hopper {
            id: self.id,
            name: self.name,
            slug: self.slug,
            public: self.public,
            count,
            description: self.description.filter(|d| !d.trim().is_empty()),
            slug_frozen: self.slug_frozen,
        }
    }
}

/// --- hoppers --- `GET /api/hoppers/{id}`: `{ hopper, total, sourceCount }`.
#[derive(serde::Deserialize)]
struct WireHopperDetail {
    hopper: WireHopper,
    #[serde(default)]
    total: u32,
}

/// --- hoppers --- How many hoppers are read at once (a request or more
/// each): enough to overlap the round trips, few enough to stay well under
/// the blyg's work budget.
pub const HOPPER_READS: usize = 4;

/// `f` over `items` with at most `n` running at once, the answers in
/// `items`' order. A failure stops what hasn't started, and the first error
/// (in `items`' order) is the answer.
fn bounded<T: Send, R: Send>(
    items: Vec<T>,
    n: usize,
    f: impl Fn(T) -> Result<R> + Sync,
) -> Result<Vec<R>> {
    use std::sync::atomic::{AtomicBool, Ordering};
    let len = items.len();
    if n <= 1 || len <= 1 {
        return items.into_iter().map(f).collect();
    }
    let queue = Mutex::new(items.into_iter().enumerate());
    let out: Mutex<Vec<Option<Result<R>>>> = Mutex::new((0..len).map(|_| None).collect());
    let failed = AtomicBool::new(false);
    let worker = || {
        while !failed.load(Ordering::Relaxed) {
            let next = queue.lock().unwrap_or_else(|p| p.into_inner()).next();
            let Some((i, t)) = next else {
                return;
            };
            let r = f(t);
            if r.is_err() {
                failed.store(true, Ordering::Relaxed);
            }
            out.lock().unwrap_or_else(|p| p.into_inner())[i] = Some(r);
        }
    };
    std::thread::scope(|s| {
        for _ in 0..n.min(len) {
            s.spawn(worker);
        }
    });
    let mut v = Vec::with_capacity(len);
    for r in out.into_inner().unwrap_or_else(|p| p.into_inner()) {
        match r {
            Some(Ok(x)) => v.push(x),
            Some(Err(e)) => return Err(e),
            None => {}
        }
    }
    Ok(v)
}

/// Why a connection check failed, worded for the "Connect your blyg" sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectError {
    /// Nothing answered at that address (DNS, refused, TLS, timeout).
    Unreachable,
    /// The server said 401: the token is wrong.
    WrongToken,
    /// The studio's sign-in refused the password.
    WrongPassword,
    /// No studio sign-in at `{blyg-url}/studio/login` (404).
    NoStudio,
    /// `GET {blyg}/api/items/summary` is 404 although the studio answered
    /// its capabilities: not where the owner API should be.
    MissingExtensions,
    /// The studio is older than [`MIN_STUDIO`] (its private API contract
    /// isn't 5.x, or it has none).
    Outdated,
    /// Anything else (a 5xx, or a page that isn't a blyg's JSON).
    Other(String),
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectError::Unreachable => {
                f.write_str("Couldn't reach that address. Check the URL and your connection.")
            }
            ConnectError::WrongToken => f.write_str(
                "The blyg said the token is wrong or has expired (401). Make a new one in Studio → More → Client access and paste it.",
            ),
            ConnectError::WrongPassword => f.write_str(
                "The blyg said the password is wrong. Type the studio password again.",
            ),
            ConnectError::NoStudio => f.write_str(
                "There's no blyg studio sign-in at that address (…/studio/login is 404). Check the URL, including any path the blyg lives under.",
            ),
            ConnectError::MissingExtensions => f.write_str(
                "This server has no owner API under that address (GET …/api/items/summary is 404). Check the URL, including any path the blyg lives under. See docs/SERVER.md.",
            ),
            ConnectError::Outdated => write!(
                f,
                "Your blyg needs updating: Burrow needs blygger-studio {MIN_STUDIO} or later. Update the studio, then connect again."
            ),
            ConnectError::Other(m) => f.write_str(m),
        }
    }
}

/// Check a blyg URL and credential before saving them: sign in (with a
/// password), check the studio's API contract (`GET
/// {blyg}/api/capabilities`), then count its items. Returns that count.
pub fn verify_connection(
    base_url: &str,
    cred: impl Into<Credential>,
) -> std::result::Result<usize, ConnectError> {
    let cred = cred.into();
    let password = matches!(cred, Credential::Password(_));
    if let Credential::Password(p) = &cred {
        match auth::login(base_url, p) {
            Ok(_) => {}
            Err(CoreError::Offline) => return Err(ConnectError::Unreachable),
            Err(CoreError::Unauthorized) => return Err(ConnectError::WrongPassword),
            Err(CoreError::NotFound) => return Err(ConnectError::NoStudio),
            Err(e) => return Err(ConnectError::Other(e.to_string())),
        }
    }
    let api = Api::new(base_url, cred);
    let fail = |e: CoreError| match e {
        CoreError::Offline => ConnectError::Unreachable,
        CoreError::Unauthorized if password => ConnectError::WrongPassword,
        CoreError::Unauthorized => ConnectError::WrongToken,
        CoreError::NotFound => ConnectError::MissingExtensions,
        CoreError::Rejected {
            status, message, ..
        } => ConnectError::Other(format!("The server answered {status}: {message}")),
        CoreError::Other(_) => ConnectError::Other(
            "That address answered, but not like a blyg (its owner API didn't return JSON).".into(),
        ),
        e => ConnectError::Other(e.to_string()),
    };
    // A newer contract connects: sync says so once, and keeps what's held.
    if contract(api.contract_version().map_err(fail)?.as_deref()) == Contract::TooOld {
        return Err(ConnectError::Outdated);
    }
    api.count_items().map(|n| n as usize).map_err(fail)
}

/// Reading items declare `page` origin-relative (e.g. `f/a1`); resolve it so
/// the UI can open it directly. Absolute URLs pass through.
pub fn absolute_page(origin: &str, page: &str) -> Option<String> {
    if page.is_empty() {
        return None;
    }
    if let Ok(u) = url::Url::parse(page) {
        return Some(u.to_string());
    }
    let base = if origin.ends_with('/') {
        origin.to_string()
    } else {
        format!("{origin}/")
    };
    match url::Url::parse(&base).and_then(|b| b.join(page)) {
        Ok(u) => Some(u.to_string()),
        Err(_) => Some(page.to_string()),
    }
}

/// An item command's body: the revision it's based on, and a note.
fn command_body(revision: u64, note: Option<&str>) -> Value {
    let mut b = json!({ "expectedRevision": revision });
    if let Some(n) = note.map(str::trim).filter(|n| !n.is_empty()) {
        b["note"] = json!(crate::text::for_save(n));
    }
    b
}

/// Percent-encode a path segment.
fn enc(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}

/// A 2xx/3xx answer: its status, `ETag` and body text (empty for a 304).
struct Answer {
    status: u16,
    etag: Option<String>,
    text: String,
}

/// An answer's JSON (`Null` when it had no body).
fn json_of(s: &str) -> Result<Value> {
    if s.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(s).map_err(|e| CoreError::Other(format!("bad JSON from server: {e}")))
}

/// A URL's path, for messages (never its host or query).
fn url_path(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let path = rest.find('/').map_or("/", |i| &rest[i..]);
    path.split('?').next().unwrap_or(path)
}

fn read_json(res: std::result::Result<ureq::Response, ureq::Error>) -> Result<Value> {
    json_of(&read_answer(res)?.text)
}

fn read_answer(res: std::result::Result<ureq::Response, ureq::Error>) -> Result<Answer> {
    match res {
        Ok(r) => {
            let status = r.status();
            let etag = r.header("etag").map(str::to_string);
            // A 304 has no body. Not reading one (ureq would wait for the
            // connection to close, and then reuse a dead one) drops the
            // connection instead of pooling it.
            if status == 304 {
                return Ok(Answer {
                    status,
                    etag,
                    text: String::new(),
                });
            }
            let mut text = String::new();
            r.into_reader()
                .take(64 * 1024 * 1024)
                .read_to_string(&mut text)
                .map_err(|_| CoreError::Offline)?;
            Ok(Answer { status, etag, text })
        }
        Err(ureq::Error::Transport(_)) => Err(CoreError::Offline),
        Err(ureq::Error::Status(401, _)) => Err(CoreError::Unauthorized),
        Err(ureq::Error::Status(404, _)) => Err(CoreError::NotFound),
        Err(ureq::Error::Status(429, r)) => Err(CoreError::RateLimited {
            retry_after: retry_after(r.header("retry-after")),
        }),
        Err(ureq::Error::Status(code, r)) => {
            let body: Value = r
                .into_string()
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(Value::Null);
            let code_str = body.get("code").and_then(Value::as_str).unwrap_or("");
            match (code, code_str) {
                (409, "revision-conflict") => return Err(CoreError::RevisionConflict),
                (409, "cursor-expired") => return Err(CoreError::CursorExpired),
                (409, "tangle-confirmation-required") => return Err(tangle_confirm(&body)),
                _ => {}
            }
            let message = body
                .get("message")
                .and_then(Value::as_str)
                .filter(|m| !m.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("server returned {code}"));
            let d = body.get("details");
            let list = |k: &str| d.and_then(|d| d.get(k)).and_then(Value::as_array);
            let mut details: Vec<String> = list("errors")
                .map(|a| a.iter().map(detail).collect())
                .unwrap_or_default();
            // Validation failures: `details.issues: [{path, message}]`.
            if let Some(issues) = list("issues") {
                details.extend(issues.iter().map(issue));
            }
            // An unresolvable subscribe (422): `details.tried: [url, …]`.
            if let Some(tried) = list("tried") {
                let n = tried.len();
                details.push(format!(
                    "tried {n} address{}",
                    if n == 1 { "" } else { "es" }
                ));
            }
            Err(CoreError::Rejected {
                status: code,
                message,
                details,
            })
        }
    }
}

/// --- tangle --- the 409 `tangle-confirmation-required` body:
/// `details: {count, parts: [label, …]}`.
fn tangle_confirm(body: &Value) -> CoreError {
    let d = body.get("details");
    let parts: Vec<String> = d
        .and_then(|d| d.get("parts"))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let count = d
        .and_then(|d| d.get("count"))
        .and_then(Value::as_u64)
        .map_or(parts.len(), |n| n as usize);
    CoreError::TangleConfirm { count, parts }
}

/// `errors` entries are strings, `{directive, reason}` (transclusions) or TK issue objects.
fn detail(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Object(o) => {
            let d = o.get("directive").and_then(Value::as_str);
            let r = o
                .get("reason")
                .or_else(|| o.get("message"))
                .and_then(Value::as_str);
            match (d, r) {
                (Some(d), Some(r)) => format!("{d}: {r}"),
                (None, Some(r)) => r.to_string(),
                _ => v.to_string(),
            }
        }
        other => other.to_string(),
    }
}

/// One validation issue: `path.to.field: message`.
fn issue(v: &Value) -> String {
    let msg = v
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("invalid");
    let path: Vec<String> = v
        .get("path")
        .and_then(Value::as_array)
        .map(|p| {
            p.iter()
                .map(|s| s.as_str().map_or_else(|| s.to_string(), str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if path.is_empty() {
        msg.to_string()
    } else {
        format!("{}: {msg}", path.join("."))
    }
}

/// When to try again after a 429, in seconds: `Retry-After` as a number of
/// seconds (studio sends `60`); a minute when it's missing or unreadable (an
/// HTTP date included). Clamped to 1 s – 1 h.
pub(crate) fn retry_after(h: Option<&str>) -> u64 {
    h.and_then(|h| h.trim().parse::<u64>().ok())
        .unwrap_or(60)
        .clamp(1, 3600)
}

/// True for failures worth retrying later (network down, server 5xx, the
/// blyg's work budget spent, or a list that kept changing while it was
/// read: none of them drops a queued change).
pub fn is_transient(e: &CoreError) -> bool {
    matches!(
        e,
        CoreError::Offline | CoreError::RateLimited { .. } | CoreError::CursorExpired
    ) || matches!(e, CoreError::Rejected { status, .. } if *status >= 500)
}

/// Upstream's `GenerationProvenance` array: `{sources: [{id, version}],
/// model?, at?}` or `null` per scope.
fn provenance_body(scopes: &[Option<ScopeProvenance>]) -> Value {
    Value::Array(
        scopes
            .iter()
            .map(|p| match p {
                None => Value::Null,
                Some(p) => {
                    let mut e = json!({ "model": p.model, "sources": p.sources });
                    if let Some(at) = &p.at {
                        e["at"] = json!(at);
                    }
                    e
                }
            })
            .collect(),
    )
}

#[cfg(test)]
#[path = "contract_tests.rs"]
mod contract_tests;

#[cfg(test)]
mod tests {
    use super::*;

    // --- hoppers ---
    #[test]
    fn bounded_keeps_the_order_and_never_runs_more_than_n_at_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let (now, peak) = (AtomicUsize::new(0), AtomicUsize::new(0));
        let out = bounded((0..20).collect(), 4, |i: u32| {
            let n = now.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(n, Ordering::SeqCst);
            // Work that takes a while, so the tasks overlap.
            crate::wait::wall_clock(Duration::from_millis(5));
            now.fetch_sub(1, Ordering::SeqCst);
            Ok(i * 2)
        })
        .unwrap();
        assert_eq!(out, (0..20).map(|i| i * 2).collect::<Vec<_>>());
        let peak = peak.load(Ordering::SeqCst);
        assert!((1..=4).contains(&peak), "{peak} at once");
        // A failure is the answer.
        let r = bounded((0..10).collect(), 3, |i: u32| {
            if i == 6 {
                Err(CoreError::Offline)
            } else {
                Ok(i)
            }
        });
        assert!(matches!(r, Err(CoreError::Offline)));
        assert!(bounded(Vec::<u32>::new(), 4, Ok).unwrap().is_empty());
    }

    #[test]
    fn only_contract_five_is_spoken() {
        assert_eq!(contract(Some("5.0.0")), Contract::Supported);
        assert_eq!(contract(Some(" 5.3.1 ")), Contract::Supported);
        assert_eq!(contract(Some("4.0.0")), Contract::TooOld);
        assert_eq!(contract(Some("1.0.0")), Contract::TooOld);
        assert_eq!(contract(None), Contract::TooOld, "no capabilities: too old");
        // Contract 6 (studio 0.67) only drops `GET /changes`, which sync
        // already does without: a studio on 0.67+ must still sync.
        assert_eq!(contract(Some("6.0.0")), Contract::Supported);
        assert_eq!(contract(Some("6.1.0")), Contract::Supported);
    }

    #[test]
    fn retry_after_reads_seconds_and_defaults_to_a_minute() {
        assert_eq!(retry_after(Some("60")), 60);
        assert_eq!(retry_after(Some(" 5 ")), 5);
        assert_eq!(retry_after(Some("0")), 1);
        assert_eq!(retry_after(Some("999999")), 3600);
        assert_eq!(retry_after(None), 60);
        assert_eq!(retry_after(Some("Wed, 21 Oct 2015 07:28:00 GMT")), 60);
    }

    #[test]
    fn a_rate_limit_and_an_expired_cursor_are_transient() {
        assert!(is_transient(&CoreError::RateLimited { retry_after: 60 }));
        assert!(is_transient(&CoreError::CursorExpired));
        assert!(!is_transient(&CoreError::RevisionConflict));
    }

    #[test]
    fn the_api_lives_under_the_mount() {
        let api = Api::new("https://blyg.example.com/blyg/", "t");
        assert_eq!(api.api_url(), "https://blyg.example.com/blyg/api");
        let root = Api::new("https://blyg.example.com", "t");
        assert_eq!(root.api_url(), "https://blyg.example.com/api");
    }

    #[test]
    fn a_command_body_carries_its_revision_and_a_trimmed_note() {
        assert_eq!(command_body(3, None), json!({ "expectedRevision": 3 }));
        assert_eq!(
            command_body(3, Some("  ")),
            json!({ "expectedRevision": 3 })
        );
        assert_eq!(
            command_body(4, Some(" fixed a typo ")),
            json!({ "expectedRevision": 4, "note": "fixed a typo" })
        );
    }

    #[test]
    fn the_tangle_gate_reads_its_count_and_parts() {
        let body = serde_json::json!({
            "code": "tangle-confirmation-required",
            "message": "publishing this thread also publishes 2 new items; send tangle: true to confirm",
            "details": {"count": 2, "parts": ["A passage…", "an idea (linked)"]}
        });
        match tangle_confirm(&body) {
            CoreError::TangleConfirm { count, parts } => {
                assert_eq!(count, 2);
                assert_eq!(parts, ["A passage…", "an idea (linked)"]);
            }
            e => panic!("{e}"),
        }
        let e = CoreError::TangleConfirm {
            count: 1,
            parts: vec![],
        };
        assert_eq!(
            e.to_string(),
            "publishing this thread also publishes 1 new item"
        );
    }
}
