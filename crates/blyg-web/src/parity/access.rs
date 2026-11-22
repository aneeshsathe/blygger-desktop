//! --- web-parity --- Access tokens: the studio's Client access page, with
//! the desktop's model (Blyg › Client Access…, `blyg_core::access_ai`):
//! list the grants that can act as you (`GET /api/authorizations`), make a
//! manual token (`POST /api/authorizations {name, scope, resource}`), shown
//! once, and revoke one or all (`DELETE /api/authorizations[/{id}]`).
//!
//! The studio serves these to its own sign-in session only (a bearer gets
//! 401), which is what the web app uses, and keeps them outside the
//! operation catalogue, so `/api/capabilities` doesn't name them: the
//! screen asks, and a 404 means this blyg has no client access.
//!
//! A new token goes to the page to be shown once. Burrow keeps the grant
//! (no token) in memory; the page drops the token when you're done.

use blyg_core::access_ai::{Authorization, NewToken, SCOPE_DESCRIPTIONS, TokenResource};
use serde::Serialize;
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

use super::{err_out, err_text};
use crate::api::{self, ApiError, Request, enc};
use crate::{Burrow, json};

/// What the Access tokens screen knows.
#[derive(Debug, Default, Clone, PartialEq)]
pub enum Access {
    #[default]
    Unread,
    Ready(Vec<Authorization>),
    /// 404: this blyg serves no client access.
    Unsupported,
    Failed(String),
}

/// The create request, when the studio would take it: a trimmed name of
/// 1–100 characters and at least one known scope (kept in upstream's
/// order, once each).
pub fn token_request(name: &str, scopes: &[String], resource: &str) -> Result<Request, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Name the token after the tool it's for".into());
    }
    if name.chars().count() > 100 {
        return Err("A token's name is 100 characters at most".into());
    }
    let scope: Vec<&str> = SCOPE_DESCRIPTIONS
        .iter()
        .map(|(k, _)| *k)
        .filter(|k| scopes.iter().any(|s| s == k))
        .collect();
    if scope.is_empty() {
        return Err("Give it at least one permission".into());
    }
    let resource = match resource {
        "mcp" => TokenResource::Mcp,
        _ => TokenResource::Api,
    };
    Ok(Request {
        method: "POST",
        path: "/api/authorizations".into(),
        body: Some(json!({ "name": name, "scope": scope, "resource": resource.as_str() })),
    })
}

/// The answer to a create (`{authorization, accessToken, expiresIn}`).
pub fn parse_new_token(v: &Value) -> Option<NewToken> {
    Some(NewToken {
        authorization: serde_json::from_value(v.get("authorization")?.clone()).ok()?,
        access_token: v.get("accessToken")?.as_str()?.to_string(),
        expires_in: v.get("expiresIn").and_then(Value::as_u64).unwrap_or(0),
    })
}

fn day(secs: i64) -> String {
    chrono::DateTime::from_timestamp(secs, 0)
        .map(|d| d.format("%-d %b %Y").to_string())
        .unwrap_or_default()
}

#[derive(Serialize)]
struct TokenRow {
    id: String,
    name: String,
    kind: &'static str,
    resource: String,
    scopes: String,
    created: String,
    expires: Option<String>,
}

#[wasm_bindgen]
impl Burrow {
    /// `GET /api/authorizations`'s answer: `{n}` or `{error, code}`.
    pub fn acc_loaded(&mut self, status: u16, body: &str) -> String {
        match api::read(status, body, None) {
            Ok(v) => {
                let list: Vec<Authorization> = v
                    .get("items")
                    .and_then(|i| serde_json::from_value(i.clone()).ok())
                    .unwrap_or_default();
                let n = list.len();
                self.parity.access = Access::Ready(list);
                json(&json!({ "n": n }))
            }
            Err(ApiError::NotFound) => {
                self.parity.access = Access::Unsupported;
                json(&json!({ "n": 0 }))
            }
            Err(e) => {
                self.parity.access = Access::Failed(err_text(&e));
                err_out(&e)
            }
        }
    }

    /// Read the list again when it's next shown.
    pub fn acc_forget(&mut self) {
        self.parity.access = Access::Unread;
    }

    /// The Access tokens screen: `{state: loading|ready|unsupported|error,
    /// error, rows, scopes}`.
    pub fn acc_view(&self) -> String {
        let scopes: Vec<Value> = SCOPE_DESCRIPTIONS
            .iter()
            .map(|(k, d)| json!({ "key": k, "label": d }))
            .collect();
        let (state, rows, error): (&str, Vec<TokenRow>, Option<String>) = match &self.parity.access
        {
            Access::Unread => ("loading", vec![], None),
            Access::Unsupported => ("unsupported", vec![], None),
            Access::Failed(e) => ("error", vec![], Some(e.clone())),
            Access::Ready(l) => (
                "ready",
                l.iter()
                    .map(|a| TokenRow {
                        id: a.id.clone(),
                        name: a.name.clone(),
                        kind: if a.manual { "manual token" } else { "OAuth" },
                        resource: a.resource.clone(),
                        scopes: a.scope.join(", "),
                        created: day(a.created_at),
                        expires: a.expires_at.map(day),
                    })
                    .collect(),
                None,
            ),
        };
        json(&json!({ "state": state, "error": error, "rows": rows, "scopes": scopes }))
    }

    /// The request to make a manual token: `{req}` or `{error}`.
    /// `scopes_json`: a JSON array of scope names; `resource`: `api` or
    /// `mcp`.
    pub fn acc_create_request(&self, name: &str, scopes_json: &str, resource: &str) -> String {
        let scopes: Vec<String> = serde_json::from_str(scopes_json).unwrap_or_default();
        match token_request(name, &scopes, resource) {
            Ok(r) => json(&json!({ "req": r })),
            Err(e) => json(&json!({ "error": e })),
        }
    }

    /// The answer to a create: `{token, name, days}` for the page to show
    /// once, or `{error, code}`. Only the grant is kept here.
    pub fn acc_created(&mut self, status: u16, body: &str) -> String {
        let t = match api::read(status, body, None) {
            Ok(v) => match parse_new_token(&v) {
                Some(t) => t,
                None => return json(&json!({ "error": "The blyg's answer held no token" })),
            },
            Err(e) => return err_out(&e),
        };
        if let Access::Ready(l) = &mut self.parity.access {
            l.retain(|a| a.id != t.authorization.id);
            l.push(t.authorization.clone());
        }
        json(&json!({
            "token": t.access_token,
            "name": t.authorization.name,
            "days": t.expires_in.div_ceil(86_400),
        }))
    }

    /// The request to revoke grant `id`, or every grant for `all`.
    pub fn acc_revoke_request(&self, id: &str) -> String {
        let path = if id == "all" {
            "/api/authorizations".to_string()
        } else {
            format!("/api/authorizations/{}", enc(id))
        };
        json(&json!({ "req": Request { method: "DELETE", path, body: None } }))
    }

    /// The answer to a revoke: `{toast}` or `{error, code}`.
    pub fn acc_revoked(&mut self, id: &str, status: u16, body: &str) -> String {
        match api::read(status, body, None) {
            Ok(_) => {
                if let Access::Ready(l) = &mut self.parity.access {
                    if id == "all" {
                        l.clear();
                    } else {
                        l.retain(|a| a.id != id);
                    }
                }
                json(&json!({
                    "toast": if id == "all" { "Revoked all client access" } else { "Revoked" }
                }))
            }
            Err(e) => err_out(&e),
        }
    }
}
