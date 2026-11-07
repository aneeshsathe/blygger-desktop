//! The owner API as the page speaks it: what to ask and what the answer
//! means, with no network of its own. The page's `live.js` does the
//! `fetch` (same origin, the Studio session cookie, nothing else) and hands
//! the status and body back here.
//!
//! The rules are blyg-core's (`blyg_core::api`, contract 5): resources are
//! camelCase (`WireItem`, `imported_row`), lists are cursor pages
//! (`CursorPage`) read from the first page to `nextCursor: null`, and a
//! 409 `cursor-expired` drops what was read and starts the list again, at
//! most `CURSOR_RESTARTS` times, so a list is never partial. Item commands
//! carry `expectedRevision`; a stale one is 409 `revision-conflict`. Errors
//! are `{code, message, details}`.

use blyg_core::api::wire::{CursorPage, WireItem, kind_str};
use blyg_core::api::{CONTRACT_MAJOR, CURSOR_RESTARTS, Contract, contract};
use blyg_core::{Kind, RemoteRef};
use serde::Serialize;
use serde_json::{Value, json};

/// What an answer from the owner API means.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "kebab-case")]
pub enum ApiError {
    /// No answer (status 0: the fetch itself failed).
    Offline,
    /// 401: no Studio session (or it ran out). Sign in at Studio.
    Unauthorized,
    /// 403: the server refused this session or this origin.
    Forbidden {
        message: String,
    },
    NotFound,
    /// 409 `revision-conflict`: the item changed since it was read.
    RevisionConflict,
    /// 409 `cursor-expired`: the list changed while it was read.
    CursorExpired,
    /// 429.
    RateLimited {
        retry_after: u64,
    },
    /// Anything else that isn't 2xx.
    Rejected {
        status: u16,
        message: String,
        details: Vec<String>,
    },
}

impl ApiError {
    /// One line for a toast. Never carries a header or a cookie.
    pub fn message(&self) -> String {
        match self {
            ApiError::Offline => "Offline. Your changes are kept and sent when you're back.".into(),
            ApiError::Unauthorized => "Signed out. Sign in at Studio, then come back.".into(),
            ApiError::Forbidden { message } => format!("The blyg refused that: {message}"),
            ApiError::NotFound => "Not on the blyg any more".into(),
            ApiError::RevisionConflict => "Changed elsewhere since you opened it".into(),
            ApiError::CursorExpired => {
                "The list kept changing while it was read. Try again.".into()
            }
            ApiError::RateLimited { retry_after } => {
                format!("The blyg is busy. Trying again in {retry_after}s.")
            }
            ApiError::Rejected {
                message, details, ..
            } => {
                if details.is_empty() {
                    message.clone()
                } else {
                    format!("{message}: {}", details.join("; "))
                }
            }
        }
    }

    /// Worth trying again later without the user doing anything.
    pub fn transient(&self) -> bool {
        matches!(
            self,
            ApiError::Offline | ApiError::RateLimited { .. } | ApiError::CursorExpired
        ) || matches!(self, ApiError::Rejected { status, .. } if *status >= 500)
    }

    pub fn to_json(&self) -> Value {
        let mut v = serde_json::to_value(self).unwrap_or(Value::Null);
        v["text"] = json!(self.message());
        v
    }
}

/// Read one answer: 2xx → its JSON body (`null` when empty), else what it
/// means. `retry_after` is the `Retry-After` header, if any. Mirrors
/// blyg-core's `read_json`.
pub fn read(status: u16, body: &str, retry_after: Option<&str>) -> Result<Value, ApiError> {
    if status == 0 {
        return Err(ApiError::Offline);
    }
    if (200..300).contains(&status) {
        if body.trim().is_empty() {
            return Ok(Value::Null);
        }
        return serde_json::from_str(body).map_err(|_| ApiError::Rejected {
            status,
            message: "The blyg answered with something that isn't JSON".into(),
            details: vec![],
        });
    }
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let code = v.get("code").and_then(Value::as_str).unwrap_or("");
    let message = v
        .get("message")
        .and_then(Value::as_str)
        .filter(|m| !m.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("The blyg answered {status}"));
    Err(match (status, code) {
        (401, _) => ApiError::Unauthorized,
        (403, _) => ApiError::Forbidden { message },
        (404, _) => ApiError::NotFound,
        (409, "revision-conflict") => ApiError::RevisionConflict,
        (409, "cursor-expired") => ApiError::CursorExpired,
        (429, _) => ApiError::RateLimited {
            retry_after: retry_after
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(60),
        },
        _ => {
            let d = v.get("details");
            let list = |k: &str| d.and_then(|d| d.get(k)).and_then(Value::as_array);
            let mut details: Vec<String> = list("errors")
                .map(|a| {
                    a.iter()
                        .map(|e| match e {
                            Value::String(s) => s.clone(),
                            o => o
                                .get("reason")
                                .or_else(|| o.get("message"))
                                .and_then(Value::as_str)
                                .map_or_else(|| o.to_string(), str::to_string),
                        })
                        .collect()
                })
                .unwrap_or_default();
            if let Some(issues) = list("issues") {
                details.extend(issues.iter().map(|i| {
                    let msg = i
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("invalid");
                    let path: Vec<String> = i
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
                }));
            }
            ApiError::Rejected {
                status,
                message,
                details,
            }
        }
    })
}

/// What the page is on, from `GET /api/capabilities`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// A blyg's owner API, signed in, on a contract this speaks.
    Live { contract: String },
    /// A blyg's owner API without a Studio session.
    SignIn,
    /// A blyg on a contract this doesn't speak.
    Unsupported { why: String },
    /// Not a blyg (a static server, a demo): the sample data.
    Demo,
    /// No answer at all.
    Offline,
}

/// Judge `GET /api/capabilities`. A 401 counts as a blyg only with its
/// `WWW-Authenticate` challenge and the contract's `{code}` body, so a
/// static server that happens to answer 401 stays a demo.
pub fn detect(status: u16, body: &str, www_authenticate: bool) -> Mode {
    match read(status, body, None) {
        Ok(v) => match v.get("contractVersion").and_then(Value::as_str) {
            Some(cv) => match contract(Some(cv)) {
                Contract::Supported => Mode::Live {
                    contract: cv.to_string(),
                },
                Contract::TooOld => Mode::Unsupported {
                    why: format!(
                        "This blyg speaks API contract {cv}; Burrow needs {CONTRACT_MAJOR}.x. Update the blyg."
                    ),
                },
                Contract::TooNew(v) => Mode::Unsupported {
                    why: format!("This blyg speaks API contract {v}, newer than Burrow knows."),
                },
            },
            None => Mode::Demo,
        },
        Err(ApiError::Offline) => Mode::Offline,
        Err(ApiError::Unauthorized)
            if www_authenticate
                && serde_json::from_str::<Value>(body)
                    .ok()
                    .and_then(|v| v.get("code").cloned())
                    .is_some() =>
        {
            Mode::SignIn
        }
        Err(_) => Mode::Demo,
    }
}

/// Percent-encode one path segment (blyg-core's `enc`).
pub fn enc(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}

/// One request for the page to make. `path` always starts with `/api/`:
/// the page refuses anything else.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Request {
    pub method: &'static str,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<Value>,
}

impl Request {
    pub fn get(path: String) -> Self {
        Request {
            method: "GET",
            path,
            body: None,
        }
    }
}

pub fn item_path(server_id: &str) -> String {
    format!("/api/items/{}", enc(server_id))
}

/// `POST /api/items {mode: "blank", kind, contentMd, stubOf?}`.
pub fn create_item(content_md: &str, kind: Kind, stub_of: Option<&RemoteRef>) -> Request {
    let mut body = json!({ "mode": "blank", "contentMd": content_md, "kind": kind_str(kind) });
    if let Some(s) = stub_of {
        let mut s = s.clone();
        // The contract's `Citation` is strict: all of it or none.
        if s.cited.as_ref().is_some_and(|c| !c.complete()) {
            s.cited = None;
        }
        body["stubOf"] = serde_json::to_value(&s).unwrap_or(Value::Null);
    }
    Request {
        method: "POST",
        path: "/api/items".into(),
        body: Some(body),
    }
}

/// `PATCH /api/items/:id {expectedRevision, contentMd, kind?}`. A kind
/// goes only with a never-published draft (the server refuses it after).
pub fn save_item(server_id: &str, revision: u64, content_md: &str, kind: Option<Kind>) -> Request {
    let mut body = json!({ "expectedRevision": revision, "contentMd": content_md });
    if let Some(k) = kind {
        body["kind"] = json!(kind_str(k));
    }
    Request {
        method: "PATCH",
        path: item_path(server_id),
        body: Some(body),
    }
}

/// `POST /api/items/:id/publish {expectedRevision, note?}`.
pub fn publish(server_id: &str, revision: u64, note: Option<&str>) -> Request {
    let mut body = json!({ "expectedRevision": revision });
    if let Some(n) = note.map(str::trim).filter(|n| !n.is_empty()) {
        body["note"] = json!(n);
    }
    Request {
        method: "POST",
        path: format!("{}/publish", item_path(server_id)),
        body: Some(body),
    }
}

/// `POST /api/items/:id/restore {expectedRevision, version}`: the working
/// copy goes back to a published version (Discard changes).
pub fn restore(server_id: &str, revision: u64, version: u32) -> Request {
    Request {
        method: "POST",
        path: format!("{}/restore", item_path(server_id)),
        body: Some(json!({ "expectedRevision": revision, "version": version })),
    }
}

/// `DELETE /api/items/:id {expectedRevision}`: a draft goes.
pub fn delete_item(server_id: &str, revision: u64) -> Request {
    Request {
        method: "DELETE",
        path: item_path(server_id),
        body: Some(json!({ "expectedRevision": revision })),
    }
}

/// `PUT /api/reading/:sub/:remoteId/read {version}`, or `DELETE` it (unread).
pub fn read_mark(sub: &str, remote_id: &str, version: Option<u32>) -> Request {
    let path = format!("/api/reading/{}/{}/read", enc(sub), enc(remote_id));
    match version {
        Some(v) => Request {
            method: "PUT",
            path,
            body: Some(json!({ "version": v })),
        },
        None => Request {
            method: "DELETE",
            path,
            body: None,
        },
    }
}

/// `PUT` / `DELETE /api/hoppers/:id/items/:sub/:remoteId`.
pub fn hopper_item(hopper: &str, sub: &str, remote_id: &str, on: bool) -> Request {
    Request {
        method: if on { "PUT" } else { "DELETE" },
        path: format!(
            "/api/hoppers/{}/items/{}/{}",
            enc(hopper),
            enc(sub),
            enc(remote_id)
        ),
        body: None,
    }
}

/// An item answer (`GET`/`PATCH`/`POST /api/items…`).
pub fn parse_item(v: &Value) -> Result<WireItem, ApiError> {
    serde_json::from_value(v.clone()).map_err(|e| ApiError::Rejected {
        status: 200,
        message: format!("An item the blyg sent doesn't read: {e}"),
        details: vec![],
    })
}

/// Reads a whole cursor-paged list, one page per round trip: the page asks
/// [`Pager::next_path`], fetches it, and hands the answer to
/// [`Pager::feed`] until `next_path` is `None`. On `cursor-expired` the
/// pages read so far are dropped and the list starts over, at most
/// `CURSOR_RESTARTS` times. A server that hands back the cursor it was
/// given is an error rather than a loop.
#[derive(Debug, Clone)]
pub struct Pager {
    path: String,
    query: String,
    limit: u32,
    cursor: Option<String>,
    restarts: usize,
    items: Vec<Value>,
    done: bool,
}

impl Pager {
    pub fn new(path: &str, query: &str, limit: u32) -> Self {
        Pager {
            path: path.into(),
            query: query.into(),
            limit,
            cursor: None,
            restarts: 0,
            items: Vec::new(),
            done: false,
        }
    }

    pub fn next_path(&self) -> Option<String> {
        if self.done {
            return None;
        }
        let mut url = format!("{}?", self.path);
        if !self.query.is_empty() {
            url.push_str(&self.query);
            url.push('&');
        }
        url.push_str(&format!("limit={}", self.limit));
        if let Some(c) = &self.cursor {
            url.push_str("&cursor=");
            url.push_str(&enc(c));
        }
        Some(url)
    }

    /// One page's answer. `Ok(true)`: there's more to read.
    pub fn feed(
        &mut self,
        status: u16,
        body: &str,
        retry_after: Option<&str>,
    ) -> Result<bool, ApiError> {
        let v = match read(status, body, retry_after) {
            Err(ApiError::CursorExpired) if self.restarts < CURSOR_RESTARTS => {
                self.restarts += 1;
                self.items.clear();
                self.cursor = None;
                return Ok(true);
            }
            Err(e) => {
                self.done = true;
                return Err(e);
            }
            Ok(v) => v,
        };
        let page: CursorPage<Value> = serde_json::from_value(v).map_err(|e| {
            self.done = true;
            ApiError::Rejected {
                status,
                message: format!(
                    "{} answered with something that isn't a page: {e}",
                    self.path
                ),
                details: vec![],
            }
        })?;
        self.items.extend(page.items);
        match page.page.next_cursor {
            Some(next) if self.cursor.as_deref() == Some(next.as_str()) => {
                self.done = true;
                Err(ApiError::Rejected {
                    status,
                    message: format!("{} answered the same page twice", self.path),
                    details: vec![],
                })
            }
            Some(next) => {
                self.cursor = Some(next);
                Ok(true)
            }
            None => {
                self.done = true;
                Ok(false)
            }
        }
    }

    pub fn finished(&self) -> bool {
        self.done
    }

    pub fn take_items(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.items)
    }
}
