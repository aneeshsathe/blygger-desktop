//! --- access-ai --- The fake blyg's client access and AI settings: three
//! grants (Burrow's own browser sign-in among them), a model list like the
//! studio's own `models.json`, and AI settings with change notes and the
//! classifier on. The fake behaves like a Burrow signed in with the browser:
//! client access needs the studio password once (any but "wrong").

use blyg_core::access_ai::{
    AiModel, AiModels, AiProvider, Authorization, PublishExtras, ServerAiSettings,
};

/// Burrow's own OAuth client id on the fake blyg.
pub const OWN_CLIENT: &str = "cl-burrow-fake";

pub struct Seed {
    pub authorizations: Vec<Authorization>,
    /// The studio password was given (client access is open).
    pub session: bool,
    pub ai: ServerAiSettings,
    pub models: AiModels,
    /// The last `publish_with`'s extras (tests).
    pub last_extras: Option<PublishExtras>,
    /// Grants handed out by `create_authorization`.
    pub seq: u64,
}

#[allow(clippy::too_many_arguments)]
fn grant(
    id: &str,
    client: &str,
    name: &str,
    manual: bool,
    resource: &str,
    scope: &[&str],
    days_ago: i64,
    days_left: i64,
) -> Authorization {
    let now = chrono::Utc::now().timestamp();
    Authorization {
        id: id.into(),
        client_id: client.into(),
        name: name.into(),
        manual,
        resource: resource.into(),
        scope: scope.iter().map(|s| s.to_string()).collect(),
        created_at: now - days_ago * 86_400,
        expires_at: Some(now + days_left * 86_400),
    }
}

fn provider(
    id: &str,
    label: &str,
    key: Option<&str>,
    binding: Option<&str>,
    on: bool,
) -> AiProvider {
    AiProvider {
        id: id.into(),
        label: label.into(),
        key_secret: key.map(str::to_string),
        binding: binding.map(str::to_string),
        configured: on,
    }
}

fn model(id: &str, provider: &str, label: &str, note: &str, decision: bool) -> AiModel {
    AiModel {
        id: id.into(),
        provider: provider.into(),
        label: label.into(),
        note: Some(note.to_string()).filter(|n| !n.is_empty()),
        kind: decision.then(|| "decision".to_string()),
    }
}

impl Default for Seed {
    fn default() -> Self {
        const API: &str = "https://blyg.example.com/api";
        const ALL: [&str; 5] = [
            "owner:read",
            "owner:draft",
            "owner:publish",
            "owner:manage",
            "reading:state",
        ];
        Seed {
            authorizations: vec![
                grant("gr-burrow", OWN_CLIENT, "Burrow", false, API, &ALL, 12, 18),
                grant(
                    "gr-mcp",
                    "cl-notes",
                    "Notebook assistant",
                    false,
                    "https://blyg.example.com/mcp",
                    &["owner:read", "owner:draft"],
                    5,
                    25,
                ),
                grant(
                    "gr-feeds",
                    "cl-feeds",
                    "Feed checker",
                    true,
                    API,
                    &["owner:read"],
                    2,
                    28,
                ),
            ],
            session: false,
            ai: ServerAiSettings {
                model_tk: Some("claude-sonnet-5-5".into()),
                model_changelog: Some("claude-haiku-4-5".into()),
                model_classify: Some("@cf/cloudflare/clef-flash".into()),
                model_feed: Some(String::new()),
                style_prompt: Some("Plain words, short sentences, no exclamation marks.".into()),
                feed_prompt: Some(String::new()),
                auto_change_notes: Some(false),
                change_category_in_note: Some(true),
            },
            models: AiModels {
                providers: vec![
                    provider(
                        "anthropic",
                        "Anthropic",
                        Some("AI_PROVIDER_KEY"),
                        None,
                        true,
                    ),
                    provider("openai", "OpenAI", Some("OPENAI_API_KEY"), None, false),
                    provider(
                        "workers-ai",
                        "Cloudflare Workers AI",
                        None,
                        Some("AI"),
                        true,
                    ),
                ],
                models: vec![
                    model(
                        "claude-sonnet-5-5",
                        "anthropic",
                        "Claude Sonnet 5.5",
                        "balanced",
                        false,
                    ),
                    model(
                        "claude-haiku-4-5",
                        "anthropic",
                        "Claude Haiku 4.5",
                        "fast and cheap",
                        false,
                    ),
                    model(
                        "gpt-6-luna",
                        "openai",
                        "GPT-6 Luna",
                        "fast and cheap",
                        false,
                    ),
                    model(
                        "@cf/google/gemma-4-26b-a4b-it",
                        "workers-ai",
                        "Gemma 4 26B",
                        "billed to your Cloudflare account",
                        false,
                    ),
                    model(
                        "@cf/cloudflare/clef-flash",
                        "workers-ai",
                        "Clef-flash",
                        "decision model, fastest",
                        true,
                    ),
                ],
                local: false,
            },
            last_extras: None,
            seq: 0,
        }
    }
}
// --- end access-ai ---
