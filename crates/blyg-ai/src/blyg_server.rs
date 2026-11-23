//! Generation on the user's own blyg Worker: `POST /api/items/:id/generate
//! {scope}` → `{text, model}` (apps/blyg/src/api.ts + tk-generate.ts).
//!
//! blyg-core's `Api` has no method for this endpoint, so this is a tiny
//! direct call with the owner token. The server builds its own prompt (with
//! its own key, model and `ai_style_prompt`), **splices the output into its
//! working copy and records provenance itself**, so the caller must re-pull
//! the item afterwards rather than splicing locally. Not streamed: the text
//! arrives as a single delta.

use blyg_core::CoreError;
use blyg_core::api::Api;
use blyg_core::api::auth::Credential;
use blyg_core::generate::{Checkpoint, GenerateOutcome};

use crate::error::{AiError, Result};
use crate::provider::{GenRequest, GenResult, Provider, ProviderKind};

/// Owns an owner-API client, so it signs in the same way the app does (a
/// token, or the studio password) and reaches the host-rooted `/api`.
pub struct BlygServer {
    api: Api,
}

impl std::fmt::Debug for BlygServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlygServer")
            .field("api", &self.api)
            .finish()
    }
}

impl BlygServer {
    pub fn new(base_url: &str, cred: impl Into<Credential>) -> Self {
        BlygServer {
            api: Api::new(base_url, cred),
        }
    }
}

impl Provider for BlygServer {
    fn kind(&self) -> ProviderKind {
        ProviderKind::BlygServer
    }

    fn generate(&self, req: GenRequest, on_delta: &mut dyn FnMut(&str)) -> Result<GenResult> {
        req.cancel.check()?;
        let Some(scope) = &req.server_scope else {
            return Err(AiError::NotConfigured(
                "the blyg server can only fill TK scopes in items that have synced".into(),
            ));
        };
        let (text, model, content_md) =
            match self
                .api
                .generate(&scope.item_id, scope.scope as u32, None, false)
            {
                // `content_md` (studio 0.27.2+) is the whole body as stored: the
                // caller puts it in the editor instead of pulling.
                Ok(GenerateOutcome::Generated(r)) => (r.text, r.model, r.content_md),
                // Studio 0.70+: links it couldn't read stop it, saving nothing.
                // Going on without them is the author's call, which a provider
                // can't ask (`Backend::generate_on_blyg` with `proceed` can).
                Ok(GenerateOutcome::Checkpoint(c)) => {
                    return Err(AiError::Provider(checkpoint_message(&c)));
                }
                Err(CoreError::NotFound) => {
                    return Err(AiError::Provider(
                        "item not found on the server (or the server has no /generate)".into(),
                    ));
                }
                Err(CoreError::Unauthorized) => {
                    return Err(AiError::Unauthorized {
                        provider: "your blyg".into(),
                    });
                }
                Err(CoreError::Offline) => {
                    return Err(AiError::Provider("couldn't reach your blyg".into()));
                }
                Err(CoreError::Rejected {
                    status, message, ..
                }) => {
                    return Err(if message.contains("declined") {
                        AiError::Refused(message)
                    } else {
                        AiError::Provider(format!("blyg server: {message} ({status})"))
                    });
                }
                Err(e) => return Err(AiError::Provider(format!("blyg server: {e}"))),
            };
        req.cancel.check()?;
        on_delta(&text);
        Ok(GenResult {
            text,
            model,
            content_md,
        })
    }
}

/// "1 of 2 links couldn't be read (https://gone.example.com/: HTTP 404)".
fn checkpoint_message(c: &Checkpoint) -> String {
    let n = c.total.saturating_sub(c.read);
    let why: Vec<String> = c
        .failed
        .iter()
        .map(|f| format!("{}: {}", f.link, f.reason))
        .collect();
    format!(
        "{n} of {} link{} couldn't be read, so your blyg generated nothing ({})",
        c.total,
        if c.total == 1 { "" } else { "s" },
        why.join("; ")
    )
}
