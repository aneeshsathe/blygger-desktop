//! --- access-ai --- Two things the blyg does for its owner that Burrow now
//! manages as Studio does (blygger-studio 0.56, contract 5.7):
//!
//! - **Client access** (`/api/authorizations`): the OAuth grants and manual
//!   API tokens that can act as the owner. List them, make a manual token
//!   (shown once), revoke one, or revoke them all. The studio serves these
//!   to an owner *session* only (its sign-in cookie), never to a bearer
//!   token, so a Burrow signed in with the browser or a token asks for the
//!   studio password first and keeps the session in memory only.
//! - **AI on the blyg**: the server's own models, one per function (`GET
//!   /api/ai/models`, the `aiModel*` settings), its house style
//!   (`aiStylePrompt`), automatic change notes (`autoChangeNotes`), and the
//!   two publish helpers: an AI-drafted change note (`POST
//!   /api/items/{id}/note-draft`) and the typo/minor/major change classifier
//!   (`POST /api/items/{id}/classify`). Not Burrow's own AI providers
//!   (`blyg-ai`, Settings › AI), which run on this computer.
//!
//! The change-category rules are upstream's `src/change-category.ts`,
//! ported word for word: a category is never a wire field, only the leading
//! word of a note the author sees and confirms.

use serde::{Deserialize, Serialize};

// ------------------------------------------------------------ client access

/// The owner scopes a manual token can carry, in upstream's order, with
/// the words the studio's Client access page shows (`SCOPE_DESCRIPTIONS`).
pub const SCOPE_DESCRIPTIONS: [(&str, &str); 5] = [
    (
        "owner:read",
        "Read private drafts, reading material, media, and settings",
    ),
    (
        "owner:draft",
        "Create and edit drafts, upload media, and use configured AI generation",
    ),
    (
        "owner:publish",
        "Publish, withdraw, refresh, pin, and change public item responses or media",
    ),
    (
        "owner:manage",
        "Change settings, subscriptions, signals, collections, and response moderation",
    ),
    ("reading:state", "Mark reading entries read or unread"),
];

/// One grant that can act as the owner: an OAuth client's sign-in, or a
/// manual token (studio's `Authorization`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Authorization {
    /// The grant's id (what revoking names).
    pub id: String,
    pub client_id: String,
    /// The client's self-asserted name ("Burrow", or a manual token's name).
    pub name: String,
    /// A manual token, not an OAuth sign-in.
    pub manual: bool,
    /// The audience(s): the REST API's URL, the MCP URL, or both.
    pub resource: String,
    pub scope: Vec<String>,
    /// Unix seconds.
    pub created_at: i64,
    /// Unix seconds, when the grant ends (30 days for a manual token).
    #[serde(default)]
    pub expires_at: Option<i64>,
}

/// What a manual token is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TokenResource {
    /// The owner REST API (what Burrow and most tools use).
    #[default]
    Api,
    /// The studio's MCP server.
    Mcp,
}

impl TokenResource {
    pub fn as_str(self) -> &'static str {
        match self {
            TokenResource::Api => "api",
            TokenResource::Mcp => "mcp",
        }
    }
}

/// A manual token to make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAuthorization {
    /// 1–100 characters (the studio trims it).
    pub name: String,
    /// At least one of [`SCOPE_DESCRIPTIONS`]' scopes.
    pub scope: Vec<String>,
    pub resource: TokenResource,
}

/// A token that was just made. The token itself is shown once and never
/// kept by Burrow: `Debug` redacts it, so it can't reach a log.
#[derive(Clone, PartialEq, Eq)]
pub struct NewToken {
    pub authorization: Authorization,
    pub access_token: String,
    /// Seconds until it expires.
    pub expires_in: u64,
}

impl std::fmt::Debug for NewToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NewToken")
            .field("authorization", &self.authorization)
            .field("access_token", &"<redacted>")
            .field("expires_in", &self.expires_in)
            .finish()
    }
}

/// Whether Burrow can manage client access on this blyg right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessGate {
    /// It holds a studio session (signed in with the password, or the
    /// password was given for this sheet).
    Ready,
    /// The studio serves client access to its own sign-in only: the studio
    /// password is needed once (kept in memory, never saved).
    NeedsPassword,
    /// Not connected to a blyg.
    Unavailable,
}

// ------------------------------------------------------------- AI on the blyg

/// One AI provider the studio knows (studio's `AiModels.providers[]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProvider {
    pub id: String,
    pub label: String,
    /// The Worker secret holding its key (absent for Workers AI and keyless
    /// endpoints).
    #[serde(default)]
    pub key_secret: Option<String>,
    /// Workers AI only: the binding it calls.
    #[serde(default)]
    pub binding: Option<String>,
    /// The key is set, the binding bound, or neither is needed.
    pub configured: bool,
}

impl AiProvider {
    /// Its readiness in words, as the studio says it (`providerStatus`).
    pub fn status(&self) -> &'static str {
        if self.binding.is_some() {
            return if self.configured {
                "binding set"
            } else {
                "no binding"
            };
        }
        if self.key_secret.is_none() {
            return "no key needed";
        }
        if self.configured { "key set" } else { "no key" }
    }
}

/// One listed model (studio's `AiModels.models[]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiModel {
    pub id: String,
    pub provider: String,
    pub label: String,
    #[serde(default)]
    pub note: Option<String>,
    /// `Some("decision")`: a decision model (Clef, Jev), offered only for the
    /// change classifier, never for text.
    #[serde(default)]
    pub kind: Option<String>,
}

impl AiModel {
    pub fn is_decision(&self) -> bool {
        self.kind.as_deref() == Some("decision")
    }
}

/// `GET /api/ai/models`: the studio's model list (its `models.json`, plus
/// `models.local.json` when `local`) and its providers' key status.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AiModels {
    #[serde(default)]
    pub providers: Vec<AiProvider>,
    #[serde(default)]
    pub models: Vec<AiModel>,
    #[serde(default)]
    pub local: bool,
}

impl AiModels {
    /// The models offered for a function: decision models for the
    /// classifier, text models for everything else.
    pub fn for_function(&self, f: AiFunction) -> Vec<&AiModel> {
        let decision = f == AiFunction::Classify;
        self.models
            .iter()
            .filter(|m| m.is_decision() == decision)
            .collect()
    }

    /// A model id's label ("Claude Sonnet · Anthropic"), or the id itself.
    pub fn label_of(&self, id: &str) -> String {
        match self.models.iter().find(|m| m.id == id) {
            Some(m) => {
                let provider = self
                    .providers
                    .iter()
                    .find(|p| p.id == m.provider)
                    .map(|p| p.label.as_str())
                    .unwrap_or(m.provider.as_str());
                format!("{} · {provider}", m.label)
            }
            None => id.to_string(),
        }
    }

    /// The provider of a listed model, when that provider has no key yet.
    pub fn unconfigured_provider(&self, id: &str) -> Option<&AiProvider> {
        let m = self.models.iter().find(|m| m.id == id)?;
        self.providers
            .iter()
            .find(|p| p.id == m.provider && !p.configured)
    }
}

/// The server's AI functions that take a model (Studio › Settings › AI).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiFunction {
    /// `aiModelTk`: filling `[TK]` scopes on the blyg.
    Tk,
    /// `aiModelChangelog`: drafting change notes.
    Changelog,
    /// `aiModelClassify`: the typo/minor/major classifier (decision models).
    Classify,
    /// `aiModelFeed`: Smart Feed scoring (saved, not used yet upstream).
    Feed,
    /// `aiModelHelpers` (studio 0.69): text helpers such as Catch up's
    /// summary; empty uses the change-notes model.
    Helpers,
}

impl AiFunction {
    pub const ALL: [AiFunction; 5] = [
        AiFunction::Tk,
        AiFunction::Changelog,
        AiFunction::Classify,
        AiFunction::Feed,
        AiFunction::Helpers,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AiFunction::Tk => "TK generation",
            AiFunction::Changelog => "Change notes",
            AiFunction::Classify => "Change classifier",
            AiFunction::Feed => "Feed scoring",
            AiFunction::Helpers => "Text helpers",
        }
    }

    /// The settings key.
    pub fn key(self) -> &'static str {
        match self {
            AiFunction::Tk => "aiModelTk",
            AiFunction::Changelog => "aiModelChangelog",
            AiFunction::Classify => "aiModelClassify",
            AiFunction::Feed => "aiModelFeed",
            AiFunction::Helpers => "aiModelHelpers",
        }
    }
}

/// The blyg's AI settings (part of `GET/PATCH /api/settings`). `None`: the
/// studio doesn't report that key (older than the feature), and it's never
/// sent. A model of `""` is "none": that function never calls a model.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ServerAiSettings {
    pub model_tk: Option<String>,
    pub model_changelog: Option<String>,
    pub model_classify: Option<String>,
    pub model_feed: Option<String>,
    /// `aiModelHelpers` (studio 0.69).
    pub model_helpers: Option<String>,
    /// `aiStylePrompt`: added to every `[TK]` generation on the blyg.
    pub style_prompt: Option<String>,
    /// `feedPrompt`: the Smart Feed's rubric (not used yet upstream).
    pub feed_prompt: Option<String>,
    /// `autoChangeNotes`: draft a note when a new version has none.
    pub auto_change_notes: Option<bool>,
    /// `changeCategoryInNote`: lead the note with the classifier's word.
    pub change_category_in_note: Option<bool>,
    // --- studio 0.69 ---
    /// `agenticTitle`: the Agentic feed tab's name (empty: "Agentic feed").
    pub agentic_title: Option<String>,
    /// `bgModelImported`: other blygs' new versions are judged by the change
    /// classifier, and a typo or minor label sends them to Background.
    pub bg_model_imported: Option<bool>,
    /// `bgInstruction`: while written, the classifier also reads each
    /// version's changelog note with it.
    pub bg_instruction: Option<String>,
    /// `bgFlagDisagreement`: a note claiming a small change against a large
    /// measured one keeps the entry in view, marked (on by default).
    pub bg_flag_disagreement: Option<bool>,
}

/// What the AI sheet may show of the classifier's judging of other blygs'
/// changes, as Studio's Settings › Background shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Judging {
    /// The studio has the setting (0.69 or later).
    pub offered: bool,
    /// A change classifier is chosen: the toggle can be turned on.
    pub can_judge: bool,
    /// Judging is on (with a classifier): the instruction shows.
    pub instruction: bool,
    /// An instruction is written too: the keep-in-view switch shows.
    pub flag: bool,
    /// The keep-in-view switch's value (on unless the studio says off).
    pub flag_on: bool,
}

impl ServerAiSettings {
    pub fn model(&self, f: AiFunction) -> Option<&str> {
        match f {
            AiFunction::Tk => self.model_tk.as_deref(),
            AiFunction::Changelog => self.model_changelog.as_deref(),
            AiFunction::Classify => self.model_classify.as_deref(),
            AiFunction::Feed => self.model_feed.as_deref(),
            AiFunction::Helpers => self.model_helpers.as_deref(),
        }
    }

    pub fn model_mut(&mut self, f: AiFunction) -> &mut Option<String> {
        match f {
            AiFunction::Tk => &mut self.model_tk,
            AiFunction::Changelog => &mut self.model_changelog,
            AiFunction::Classify => &mut self.model_classify,
            AiFunction::Feed => &mut self.model_feed,
            AiFunction::Helpers => &mut self.model_helpers,
        }
    }

    /// Read from a settings document (`GET /api/settings`).
    pub fn from_json(v: &serde_json::Value) -> Self {
        let s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);
        let b = |k: &str| v.get(k).and_then(|x| x.as_bool());
        ServerAiSettings {
            model_tk: s("aiModelTk"),
            model_changelog: s("aiModelChangelog"),
            model_classify: s("aiModelClassify"),
            model_feed: s("aiModelFeed"),
            model_helpers: s("aiModelHelpers"),
            style_prompt: s("aiStylePrompt"),
            feed_prompt: s("feedPrompt"),
            auto_change_notes: b("autoChangeNotes"),
            change_category_in_note: b("changeCategoryInNote"),
            agentic_title: s("agenticTitle"),
            bg_model_imported: b("bgModelImported"),
            bg_instruction: s("bgInstruction"),
            bg_flag_disagreement: b("bgFlagDisagreement"),
        }
    }

    /// The `PATCH /api/settings` body: the keys this holds, and no other.
    pub fn to_json(&self) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        for f in AiFunction::ALL {
            if let Some(v) = self.model(f) {
                m.insert(f.key().into(), v.trim().into());
            }
        }
        if let Some(v) = &self.style_prompt {
            m.insert("aiStylePrompt".into(), v.trim().into());
        }
        if let Some(v) = &self.feed_prompt {
            m.insert("feedPrompt".into(), v.trim().into());
        }
        if let Some(v) = self.auto_change_notes {
            m.insert("autoChangeNotes".into(), v.into());
        }
        if let Some(v) = self.change_category_in_note {
            m.insert("changeCategoryInNote".into(), v.into());
        }
        if let Some(v) = &self.agentic_title {
            m.insert("agenticTitle".into(), v.trim().into());
        }
        if let Some(v) = self.bg_model_imported {
            m.insert("bgModelImported".into(), v.into());
        }
        if let Some(v) = &self.bg_instruction {
            m.insert("bgInstruction".into(), v.trim().into());
        }
        if let Some(v) = self.bg_flag_disagreement {
            m.insert("bgFlagDisagreement".into(), v.into());
        }
        serde_json::Value::Object(m)
    }

    /// Only what differs from `loaded` (what the sheet read): a save sends
    /// that and nothing else, so a setting changed elsewhere since (Studio)
    /// is left alone. Compared as sent (trimmed).
    pub fn changes_from(&self, loaded: &ServerAiSettings) -> ServerAiSettings {
        fn text(now: &Option<String>, was: &Option<String>) -> Option<String> {
            let t = |o: &Option<String>| o.as_deref().map(str::trim).map(str::to_string);
            (t(now) != t(was)).then(|| now.clone()).flatten()
        }
        fn flag(now: Option<bool>, was: Option<bool>) -> Option<bool> {
            (now != was).then_some(now).flatten()
        }
        let mut out = ServerAiSettings {
            style_prompt: text(&self.style_prompt, &loaded.style_prompt),
            feed_prompt: text(&self.feed_prompt, &loaded.feed_prompt),
            auto_change_notes: flag(self.auto_change_notes, loaded.auto_change_notes),
            change_category_in_note: flag(
                self.change_category_in_note,
                loaded.change_category_in_note,
            ),
            agentic_title: text(&self.agentic_title, &loaded.agentic_title),
            bg_model_imported: flag(self.bg_model_imported, loaded.bg_model_imported),
            bg_instruction: text(&self.bg_instruction, &loaded.bg_instruction),
            bg_flag_disagreement: flag(self.bg_flag_disagreement, loaded.bg_flag_disagreement),
            ..ServerAiSettings::default()
        };
        for f in AiFunction::ALL {
            *out.model_mut(f) = text(
                &self.model(f).map(str::to_string),
                &loaded.model(f).map(str::to_string),
            );
        }
        out
    }

    /// What the sheet may show of judging other blygs' changes.
    pub fn judging(&self) -> Judging {
        let set = |m: &Option<String>| m.as_deref().is_some_and(|m| !m.trim().is_empty());
        let offered = self.bg_model_imported.is_some();
        let can_judge = offered && set(&self.model_classify);
        let instruction = can_judge && self.bg_model_imported == Some(true);
        Judging {
            offered,
            can_judge,
            instruction,
            flag: instruction && set(&self.bg_instruction),
            flag_on: self.bg_flag_disagreement != Some(false),
        }
    }

    /// What the publish sheet may offer.
    pub fn helpers(&self) -> NoteHelpers {
        let set = |m: &Option<String>| m.as_deref().is_some_and(|m| !m.trim().is_empty());
        let classify = set(&self.model_classify);
        NoteHelpers {
            draft: set(&self.model_changelog),
            classify,
            auto: self.auto_change_notes == Some(true) && set(&self.model_changelog),
            // Upstream: on unless explicitly off, and only with a classifier.
            category_in_note: classify && self.change_category_in_note != Some(false),
        }
    }
}

/// What the blyg's AI can do in the publish sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NoteHelpers {
    /// A change-notes model is set: "Draft a note" is offered.
    pub draft: bool,
    /// A decision model is set: the change is classified.
    pub classify: bool,
    /// `autoChangeNotes`: an empty note is drafted when the sheet opens.
    pub auto: bool,
    /// `changeCategoryInNote`: the category leads the note.
    pub category_in_note: bool,
}

/// `POST /api/items/{id}/note-draft`'s answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftedNote {
    pub note: String,
    pub model: String,
}

/// The change categories, smallest first (upstream `CHANGE_CATEGORIES`).
/// `Refresh` is a republish of the same words, decided without a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeCategory {
    Refresh,
    Typo,
    Minor,
    Major,
}

impl ChangeCategory {
    pub const ALL: [ChangeCategory; 4] = [
        ChangeCategory::Refresh,
        ChangeCategory::Typo,
        ChangeCategory::Minor,
        ChangeCategory::Major,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ChangeCategory::Refresh => "refresh",
            ChangeCategory::Typo => "typo",
            ChangeCategory::Minor => "minor",
            ChangeCategory::Major => "major",
        }
    }

    pub fn parse(s: &str) -> Option<ChangeCategory> {
        Self::ALL
            .into_iter()
            .find(|c| c.as_str().eq_ignore_ascii_case(s))
    }
}

/// `POST /api/items/{id}/classify`'s answer. `category: None` is "no
/// category could be had" (`reason` says why); publishing goes on.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangeClass {
    pub category: Option<ChangeCategory>,
    pub probability: Option<f64>,
    /// The decision model that answered; `None` for `refresh`.
    pub model: Option<String>,
    pub reason: Option<String>,
}

/// What a publish carries beyond its note (`POST …/publish`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PublishExtras {
    /// `noteGenerated`: the note is exactly the drafted text, unedited.
    pub note_generated: bool,
    /// `changeCategory`: `None` leaves the classification alone; `Some(None)`
    /// clears it; `Some(Some(c))` confirms `c`.
    pub change_category: Option<Option<ChangeCategory>>,
    /// --- tangle --- `tangle: true`: the author confirmed the new items the
    /// thread's spans make (sent only after `CoreError::TangleConfirm`).
    pub tangle: bool,
}

// ------------------------------------------- change categories (pure, ported)

/// The first word if it's a category, case-insensitive (`LEADING`: the
/// word at a word boundary).
fn leading(note: &str) -> Option<(ChangeCategory, &str)> {
    let t = note.trim_start();
    for c in ChangeCategory::ALL {
        let w = c.as_str();
        if t.len() >= w.len() && t[..w.len()].eq_ignore_ascii_case(w) {
            let rest = &t[w.len()..];
            let boundary = rest
                .chars()
                .next()
                .is_none_or(|ch| !(ch.is_alphanumeric() || ch == '_'));
            if boundary {
                return Some((c, rest));
            }
        }
    }
    None
}

/// The category word a note already starts with ("Typo fix." → typo).
pub fn leading_category(note: &str) -> Option<ChangeCategory> {
    leading(note).map(|(c, _)| c)
}

/// The note without a prefix [`compose_note`] would have written
/// ("minor: x" → "x", "minor" → "").
pub fn strip_category(note: &str) -> String {
    if let Some((_, rest)) = leading(note) {
        let r = rest.trim_start();
        if r.is_empty() {
            return String::new();
        }
        if let Some(after) = r.strip_prefix(':') {
            return after.trim().to_string();
        }
    }
    note.trim().to_string()
}

/// The note as it publishes with a category: empty → the bare word; a note
/// → `minor: note`; a note that already starts with a category word is left
/// alone; no category leaves the note as it is.
pub fn compose_note(note: &str, category: Option<ChangeCategory>) -> String {
    let text = note.trim();
    let Some(c) = category else {
        return text.to_string();
    };
    if text.is_empty() {
        return c.as_str().to_string();
    }
    if leading_category(text).is_some() {
        return text.to_string();
    }
    format!("{}: {text}", c.as_str())
}

/// Swap the category on a note the author may have edited.
pub fn recategorize(note: &str, category: Option<ChangeCategory>) -> String {
    compose_note(&strip_category(note), category)
}

/// Whether a confirmed note is generated: exactly the machine's text,
/// unedited. `machine` is `None` when the note began as the author's words.
pub fn note_is_generated(note: &str, machine: Option<&str>) -> bool {
    let text = note.trim();
    machine.is_some_and(|m| !text.is_empty() && text == m.trim())
}

/// Whether a grant is the one this Burrow signs in with.
pub fn is_own_grant(a: &Authorization, own_client_id: Option<&str>) -> bool {
    own_client_id.is_some_and(|c| !c.is_empty() && a.client_id == c)
}

// --- end access-ai ---

#[cfg(test)]
mod tests {
    use super::*;
    use ChangeCategory::*;

    #[test]
    fn compose_follows_upstream() {
        assert_eq!(compose_note("", Some(Minor)), "minor");
        assert_eq!(compose_note("  fixed it ", Some(Minor)), "minor: fixed it");
        assert_eq!(compose_note("Typo fix.", Some(Major)), "Typo fix.");
        assert_eq!(compose_note("typography", Some(Major)), "major: typography");
        assert_eq!(compose_note("x", None), "x");
        assert_eq!(compose_note("", None), "");
    }

    #[test]
    fn recategorize_swaps_only_its_own_prefix() {
        assert_eq!(
            recategorize("minor: fixed it", Some(Major)),
            "major: fixed it"
        );
        assert_eq!(recategorize("minor", Some(Typo)), "typo");
        assert_eq!(recategorize("minor: fixed it", None), "fixed it");
        // A word that starts the author's sentence isn't a prefix we wrote.
        assert_eq!(strip_category("Typo fix."), "Typo fix.");
        assert_eq!(strip_category("MINOR : x"), "x");
        assert_eq!(leading_category("Major rewrite"), Some(Major));
        assert_eq!(leading_category("majority"), None);
    }

    #[test]
    fn generated_means_unedited_machine_text() {
        assert!(note_is_generated(" minor: a ", Some("minor: a")));
        assert!(!note_is_generated("minor: b", Some("minor: a")));
        assert!(!note_is_generated("", Some("")));
        assert!(!note_is_generated("mine", None));
    }

    #[test]
    fn settings_round_trip_only_their_keys() {
        let v = serde_json::json!({
            "siteTitle": "x", "aiModelTk": "m1", "aiModelChangelog": "",
            "aiModelClassify": "jev", "aiStylePrompt": "plain", "autoChangeNotes": true,
        });
        let s = ServerAiSettings::from_json(&v);
        assert_eq!(s.model(AiFunction::Tk), Some("m1"));
        assert_eq!(s.change_category_in_note, None);
        let out = s.to_json();
        assert!(out.get("siteTitle").is_none());
        assert!(out.get("changeCategoryInNote").is_none());
        assert_eq!(out["aiModelClassify"], "jev");
        let h = s.helpers();
        assert!(!h.draft && !h.auto && h.classify && h.category_in_note);
    }

    #[test]
    fn a_new_token_never_debugs_its_secret() {
        let t = NewToken {
            authorization: Authorization {
                id: "g".into(),
                client_id: "c".into(),
                name: "n".into(),
                manual: true,
                resource: "https://blyg.example.com/api".into(),
                scope: vec!["owner:read".into()],
                created_at: 0,
                expires_at: None,
            },
            access_token: "tok-secret".into(),
            expires_in: 1,
        };
        assert!(!format!("{t:?}").contains("secret"));
    }

    #[test]
    fn models_split_decision_from_text() {
        let m: AiModels = serde_json::from_value(serde_json::json!({
            "providers": [{"id": "p", "label": "P", "keySecret": "P_KEY", "configured": false}],
            "models": [
                {"id": "a", "provider": "p", "label": "A"},
                {"id": "j", "provider": "p", "label": "Jev", "kind": "decision"}
            ],
            "local": false
        }))
        .unwrap();
        assert_eq!(m.for_function(AiFunction::Tk).len(), 1);
        assert_eq!(m.for_function(AiFunction::Classify)[0].id, "j");
        assert_eq!(m.providers[0].status(), "no key");
        assert_eq!(m.label_of("a"), "A · P");
        assert!(m.unconfigured_provider("a").is_some());
    }
}
