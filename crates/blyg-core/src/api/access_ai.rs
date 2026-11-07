//! --- access-ai --- The owner API calls behind client access and AI on the
//! blyg (see `crate::access_ai`).
//!
//! `/api/authorizations` answers an owner *session* only: the studio refuses
//! any `Authorization: Bearer` there with 401, whatever its scopes. So these
//! calls carry the studio session cookie and never the bearer. Signed in
//! with the password, that's the session Burrow already holds; signed in
//! with the browser or a token, it's one made from the password the owner
//! types into the Client access sheet, kept in `Api::session` (unused by
//! those credentials otherwise) for this run of the app only.

use serde_json::{Value, json};

use super::auth::{self, Credential};
use super::wire::Published;
use super::{Api, Body, command_body, enc, read_json};
use crate::access_ai::{
    AccessGate, AiModels, Authorization, ChangeCategory, ChangeClass, DraftedNote,
    NewAuthorization, NewToken, PublishExtras, ServerAiSettings,
};
use crate::backend::{CoreError, Result};

impl Api {
    // ------------------------------------------------------- client access

    /// Whether client access can be read now, or needs the password first.
    pub fn access_gate(&self) -> AccessGate {
        match &self.cred {
            Credential::Password(_) => AccessGate::Ready,
            _ if self.session.lock().unwrap().is_some() => AccessGate::Ready,
            _ => AccessGate::NeedsPassword,
        }
    }

    /// Sign in to the studio with its password, for client access only. A
    /// wrong password is `Unauthorized`.
    pub fn access_sign_in(&self, password: &str) -> Result<()> {
        let s = auth::login(&self.base, password)?;
        *self.session.lock().unwrap() = Some(s);
        Ok(())
    }

    /// The OAuth client id of the grant this Burrow signs in with (a
    /// browser sign-in), to mark its row. `None` for a token or password.
    pub fn own_client_id(&self) -> Option<String> {
        match &self.cred {
            Credential::OAuth(s) => Some(s.grant().client_id),
            _ => None,
        }
    }

    /// One call with the studio session and no bearer. A 401 with the
    /// password signs in again and retries once; otherwise the session is
    /// forgotten and the sheet asks for the password again.
    fn session_call(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value> {
        let body = body.map_or(Body::None, Body::Json);
        let url = format!("{}{}", self.base, path);
        let send = |cookie: &str| {
            let r = self
                .agent
                .request(method, &url)
                .set("accept", "application/json")
                .set("cookie", &format!("{}={cookie}", auth::SESSION_COOKIE));
            read_json(match &body {
                Body::Json(v) => r.send_json(v.clone()),
                _ => r.call(),
            })
        };
        match &self.cred {
            Credential::Password(p) => match send(&self.session(p)?) {
                Err(CoreError::Unauthorized) => {
                    *self.session.lock().unwrap() = None;
                    send(&self.session(p)?)
                }
                r => r,
            },
            _ => {
                let Some(cookie) = self.session.lock().unwrap().clone() else {
                    return Err(CoreError::Unauthorized);
                };
                let r = send(&cookie);
                if matches!(r, Err(CoreError::Unauthorized)) {
                    *self.session.lock().unwrap() = None;
                }
                r
            }
        }
    }

    /// `GET /api/authorizations`: every live grant, oldest first.
    pub fn authorizations(&self) -> Result<Vec<Authorization>> {
        #[derive(serde::Deserialize)]
        struct R {
            items: Vec<Authorization>,
        }
        let v = self.session_call("GET", "/api/authorizations", None)?;
        serde_json::from_value::<R>(v)
            .map(|r| r.items)
            .map_err(|e| {
                CoreError::Other(format!("unexpected response from /api/authorizations: {e}"))
            })
    }

    /// `POST /api/authorizations {name, scope, resource}`: a manual token
    /// (30 days). The token is in the answer once and never again.
    pub fn create_authorization(&self, new: &NewAuthorization) -> Result<NewToken> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct R {
            authorization: Authorization,
            access_token: String,
            #[serde(default)]
            expires_in: u64,
        }
        let body = json!({
            "name": new.name.trim(),
            "scope": new.scope,
            "resource": new.resource.as_str(),
        });
        let v = self.session_call("POST", "/api/authorizations", Some(body))?;
        // Not `call_as`: its error would quote the body, token and all.
        let r: R = serde_json::from_value(v)
            .map_err(|_| CoreError::Other("unexpected response from /api/authorizations".into()))?;
        Ok(NewToken {
            authorization: r.authorization,
            access_token: r.access_token,
            expires_in: r.expires_in,
        })
    }

    /// `DELETE /api/authorizations/{id}`: revoke one grant.
    pub fn revoke_authorization(&self, id: &str) -> Result<()> {
        self.session_call("DELETE", &format!("/api/authorizations/{}", enc(id)), None)
            .map(|_| ())
    }

    /// `DELETE /api/authorizations`: revoke every client's access (studio
    /// sessions stay signed in).
    pub fn revoke_all_authorizations(&self) -> Result<()> {
        self.session_call("DELETE", "/api/authorizations", None)
            .map(|_| ())
    }

    // ------------------------------------------------------ AI on the blyg

    /// `GET /api/ai/models`. `None` on 404 (a studio without it).
    pub fn ai_models(&self) -> Result<Option<AiModels>> {
        Self::optional(self.call_as("GET", "/api/ai/models", None))
    }

    /// The AI keys of `GET /api/settings`. `None` on 404.
    pub fn server_ai_settings(&self) -> Result<Option<ServerAiSettings>> {
        let v: Option<Value> = Self::optional(self.call("GET", "/api/settings", None))?;
        Ok(v.map(|v| ServerAiSettings::from_json(&v)))
    }

    /// `PATCH /api/settings` with the AI keys only; the stored values back.
    pub fn put_server_ai_settings(&self, s: &ServerAiSettings) -> Result<ServerAiSettings> {
        let v = self.call("PATCH", "/api/settings", Some(s.to_json()))?;
        Ok(ServerAiSettings::from_json(&v))
    }

    /// `POST /api/items/{id}/note-draft {expectedRevision}`: a change note
    /// drafted from the working copy's change. Never publishes.
    pub fn draft_note(&self, id: &str, revision: u64) -> Result<DraftedNote> {
        let v = self.call(
            "POST",
            &format!("/api/items/{}/note-draft", enc(id)),
            Some(json!({ "expectedRevision": revision })),
        )?;
        let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        Ok(DraftedNote {
            note: s("note").trim().to_string(),
            model: s("model"),
        })
    }

    /// `POST /api/items/{id}/classify {expectedRevision}`: typo, minor or
    /// major for the change the working copy would publish. Off (409) until
    /// the blyg names a decision model.
    pub fn classify_change(&self, id: &str, revision: u64) -> Result<ChangeClass> {
        let v = self.call(
            "POST",
            &format!("/api/items/{}/classify", enc(id)),
            Some(json!({ "expectedRevision": revision })),
        )?;
        Ok(ChangeClass {
            category: v
                .get("category")
                .and_then(Value::as_str)
                .and_then(ChangeCategory::parse),
            probability: v.get("probability").and_then(Value::as_f64),
            model: v.get("model").and_then(Value::as_str).map(str::to_string),
            reason: v.get("reason").and_then(Value::as_str).map(str::to_string),
        })
    }

    /// `POST /api/items/{id}/publish` with the note's provenance and the
    /// confirmed change category (`noteGenerated`, `changeCategory`).
    pub fn publish_with(
        &self,
        id: &str,
        revision: u64,
        note: Option<&str>,
        extras: &PublishExtras,
    ) -> Result<Published> {
        let mut body = command_body(revision, note);
        if extras.note_generated && body.get("note").is_some() {
            body["noteGenerated"] = json!(true);
        }
        if let Some(c) = extras.change_category {
            body["changeCategory"] = json!(c.map(ChangeCategory::as_str));
        }
        self.call_as(
            "POST",
            &format!("/api/items/{}/publish", enc(id)),
            Some(body),
        )
    }
}
// --- end access-ai ---
