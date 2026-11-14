//! --- changelog display --- How much of an item's changelog a reader sees
//! (studio 0.58, private contract 5.8.0; spec decision #95, v0.4-plan
//! §18.7 and §18.10).
//!
//! The changelog is on the wire already (§5.2): each version's note, time
//! and pin. The blyg's settings choose what to show of it: the latest note
//! (the default, as before), every note, or an AI summary of them. Two
//! settings: `changelogDisplay` for an item's own public page (the studio
//! draws that page), `readerChangelogDisplay` for items in the reader
//! (Burrow draws those, desktop and web).
//!
//! A summary is the blyg's (`POST /changelog-summary`): written from the
//! notes only, never the versions' text, once per version, and kept there.
//! It is studio-private and always shown labelled with the model and "not
//! the author's words". Burrow asks for one only when an item is opened,
//! never for a card in a list.
//!
//! Decision #96: a client MUST NOT route or sort on a note of its own
//! accord. Nothing here (or anywhere in Burrow's reading) reads a note to
//! place an item; a note is only ever shown.

use serde_json::Value;

use crate::model::RemoteVersion;

/// What to show of a changelog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChangelogDisplay {
    /// The current version's note (what Burrow showed before 0.21).
    #[default]
    Latest,
    /// Every version's note, newest first.
    History,
    /// The blyg's AI summary of the notes (when an item is opened).
    Summary,
}

impl ChangelogDisplay {
    pub const ALL: [ChangelogDisplay; 3] = [
        ChangelogDisplay::Latest,
        ChangelogDisplay::History,
        ChangelogDisplay::Summary,
    ];

    pub fn parse(s: &str) -> Option<ChangelogDisplay> {
        match s.trim() {
            "latest" => Some(ChangelogDisplay::Latest),
            "history" => Some(ChangelogDisplay::History),
            "summary" => Some(ChangelogDisplay::Summary),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ChangelogDisplay::Latest => "latest",
            ChangelogDisplay::History => "history",
            ChangelogDisplay::Summary => "summary",
        }
    }

    /// Its words in Settings.
    pub fn label(self) -> &'static str {
        match self {
            ChangelogDisplay::Latest => "The latest note",
            ChangelogDisplay::History => "The full history",
            ChangelogDisplay::Summary => "An AI summary",
        }
    }

    /// The hint under it (the studio's own).
    pub fn hint(self) -> &'static str {
        match self {
            ChangelogDisplay::Latest => "the note for the current version, as before",
            ChangelogDisplay::History => "every version's note, newest first",
            ChangelogDisplay::Summary => {
                "written from the notes by the change-notes model; uses tokens"
            }
        }
    }
}

/// The changelog keys of the blyg's settings (`GET`/`PATCH /api/settings`).
/// `None`: not set here (absent from the answer, or not to be written).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangelogSettings {
    /// `changelogDisplay`: an item's own public page.
    pub page: Option<ChangelogDisplay>,
    /// `readerChangelogDisplay`: items in the reader.
    pub reader: Option<ChangelogDisplay>,
    /// `changelogSummaryPrompt`: the owner's instructions for summaries.
    pub summary_prompt: Option<String>,
}

impl ChangelogSettings {
    /// From a settings document. A studio before 5.8 has none of the keys.
    pub fn from_json(v: &Value) -> ChangelogSettings {
        let d = |k: &str| {
            v.get(k)
                .and_then(Value::as_str)
                .and_then(ChangelogDisplay::parse)
        };
        ChangelogSettings {
            page: d("changelogDisplay"),
            reader: d("readerChangelogDisplay"),
            summary_prompt: v
                .get("changelogSummaryPrompt")
                .and_then(Value::as_str)
                .map(str::to_string),
        }
    }

    /// Whether the answer had the keys at all (a 5.8 studio always sends them).
    pub fn served(&self) -> bool {
        self.page.is_some() || self.reader.is_some()
    }

    /// The `PATCH /api/settings` body: the keys this holds, and no other.
    pub fn to_json(&self) -> Value {
        let mut m = serde_json::Map::new();
        if let Some(d) = self.page {
            m.insert("changelogDisplay".into(), d.as_str().into());
        }
        if let Some(d) = self.reader {
            m.insert("readerChangelogDisplay".into(), d.as_str().into());
        }
        if let Some(p) = &self.summary_prompt {
            m.insert("changelogSummaryPrompt".into(), p.trim().into());
        }
        Value::Object(m)
    }

    /// The reader's mode (the default when not set).
    pub fn reader_mode(&self) -> ChangelogDisplay {
        self.reader.unwrap_or_default()
    }
}

/// Whether a studio with this contract version has changelog display and
/// summaries (5.8.0 and later).
pub fn serves_changelog(contract: &str) -> bool {
    let mut parts = contract.trim().split('.').map(|p| p.parse::<u64>().ok());
    match (parts.next().flatten(), parts.next().flatten()) {
        (Some(5), Some(minor)) => minor >= 8,
        (Some(major), _) => major > 5,
        _ => false,
    }
}

/// Whose changelog to summarize.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SummaryOf {
    /// Your own published item, by its id on the blyg.
    Own(String),
    /// A held blyg item, by its subscription and remote id.
    Imported {
        subscription_id: String,
        remote_id: String,
    },
}

impl SummaryOf {
    /// The `POST /changelog-summary` body.
    pub fn body(&self) -> Value {
        match self {
            SummaryOf::Own(id) => serde_json::json!({ "itemId": id }),
            SummaryOf::Imported {
                subscription_id,
                remote_id,
            } => serde_json::json!({ "subscriptionId": subscription_id, "remoteId": remote_id }),
        }
    }
}

/// The blyg's summary of one version's changelog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangelogSummary {
    pub summary: String,
    pub model: String,
    pub version: u32,
    /// Written before (no model call this time).
    pub cached: bool,
}

impl ChangelogSummary {
    /// From a `POST /changelog-summary` answer.
    pub fn from_json(v: &Value) -> Option<ChangelogSummary> {
        let summary = v.get("summary")?.as_str()?.trim().to_string();
        if summary.is_empty() {
            return None;
        }
        Some(ChangelogSummary {
            summary,
            model: v
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            version: v.get("version").and_then(Value::as_u64).unwrap_or(0) as u32,
            cached: v.get("cached").and_then(Value::as_bool).unwrap_or(false),
        })
    }

    /// The label a summary always carries (#59, #95).
    pub fn label(&self) -> String {
        summary_label(&self.model)
    }
}

/// "AI summary · written by <model> from the changelog notes; not the
/// author's words".
pub fn summary_label(model: &str) -> String {
    let m = model.trim();
    let by = if m.is_empty() { "a model" } else { m };
    format!("AI summary · written by {by} from the changelog notes · not the author's words")
}

/// One version's line, as a reader sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteLine {
    pub version: u32,
    /// The note, trimmed (`None`: the version has none).
    pub note: Option<String>,
    pub pinned: bool,
    pub at: String,
}

/// A changelog's entries, oldest first (as on the wire), as lines.
pub fn lines(changelog: &[RemoteVersion]) -> Vec<NoteLine> {
    let mut v: Vec<NoteLine> = changelog
        .iter()
        .map(|v| NoteLine {
            version: v.version,
            note: v
                .note
                .as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(str::to_string),
            pinned: v.pinned,
            at: v.at.clone(),
        })
        .collect();
    v.sort_by_key(|l| l.version);
    v
}

/// Worth more than the latest note: more than one version, or any note at
/// all (the studio's `meaningful`).
pub fn meaningful(lines: &[NoteLine]) -> bool {
    lines.len() > 1 || lines.iter().any(|l| l.note.is_some())
}

/// What the reader shows above an opened item's body, for `mode`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shown {
    /// Nothing (a first version with no note, or nothing loaded).
    Nothing,
    /// `V3: note` (or "No changelog entry available").
    Latest { version: u32, note: Option<String> },
    /// Every line, newest first.
    History(Vec<NoteLine>),
    /// Ask the blyg for a summary (meanwhile, the history).
    Summary(Vec<NoteLine>),
}

/// What an opened item shows of its changelog `log` (oldest first) at
/// `current`. Latest shows from v2 on, as before; the longer modes only
/// when the changelog says more than its first version.
pub fn shown(mode: ChangelogDisplay, log: &[RemoteVersion], current: u32) -> Shown {
    let lines = lines(log);
    match mode {
        ChangelogDisplay::Latest => {
            if current <= 1 {
                return Shown::Nothing;
            }
            let note = lines
                .iter()
                .find(|l| l.version == current)
                .and_then(|l| l.note.clone());
            Shown::Latest {
                version: current,
                note,
            }
        }
        _ if !meaningful(&lines) => Shown::Nothing,
        ChangelogDisplay::History => Shown::History(newest_first(lines)),
        ChangelogDisplay::Summary => Shown::Summary(newest_first(lines)),
    }
}

fn newest_first(mut lines: Vec<NoteLine>) -> Vec<NoteLine> {
    lines.reverse();
    lines
}

/// A changelog from the owner API, oldest first: an imported item's
/// `GET /imports/{sub}/{id}/history` (`{current, changelog: [{version, at,
/// note, pinned}]}`), or an own item's `GET /items/{id}/versions`
/// (`{items: [{version, publishedAt, note, pinned, kind}]}`, a withdrawal
/// left out).
pub fn parse_history(v: &Value) -> Vec<RemoteVersion> {
    let current = v.get("current").and_then(Value::as_u64);
    let list = v
        .get("changelog")
        .or_else(|| v.get("items"))
        .and_then(Value::as_array);
    let mut out: Vec<RemoteVersion> = list
        .into_iter()
        .flatten()
        .filter(|e| e.get("kind").and_then(Value::as_str) != Some("withdrawn"))
        .filter_map(|e| {
            let version = e.get("version").and_then(Value::as_u64)? as u32;
            Some(RemoteVersion {
                version,
                at: e
                    .get("at")
                    .or_else(|| e.get("publishedAt"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                note: e.get("note").and_then(Value::as_str).map(str::to_string),
                pinned: e.get("pinned").and_then(Value::as_bool).unwrap_or(false),
                current: current == Some(version as u64),
                media: vec![],
                lineage: Default::default(),
            })
        })
        .collect();
    out.sort_by_key(|r| r.version);
    out
}

/// `v3 · 2026-11-02 · “note” · pinned` for a history row.
pub fn history_row(l: &NoteLine) -> String {
    let day = l.at.get(..10).unwrap_or(&l.at);
    let note = match &l.note {
        Some(n) => format!("“{n}”"),
        None => "no note".into(),
    };
    let pin = if l.pinned { " · 📌 pinned" } else { "" };
    if day.is_empty() {
        format!("v{} · {note}{pin}", l.version)
    } else {
        format!("v{} · {day} · {note}{pin}", l.version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rv(version: u32, note: Option<&str>, pinned: bool) -> RemoteVersion {
        RemoteVersion {
            version,
            at: format!("2026-11-0{version}T10:00:00Z"),
            note: note.map(str::to_string),
            pinned,
            current: false,
            media: vec![],
            lineage: Default::default(),
        }
    }

    #[test]
    fn displays_parse_and_print() {
        for d in ChangelogDisplay::ALL {
            assert_eq!(ChangelogDisplay::parse(d.as_str()), Some(d));
        }
        assert_eq!(ChangelogDisplay::parse("everything"), None);
        assert_eq!(ChangelogDisplay::default(), ChangelogDisplay::Latest);
    }

    #[test]
    fn settings_read_and_write_only_their_keys() {
        let s = ChangelogSettings::from_json(&json!({
            "siteTitle": "x",
            "changelogDisplay": "history",
            "readerChangelogDisplay": "summary",
            "changelogSummaryPrompt": "Short.",
        }));
        assert!(s.served());
        assert_eq!(s.page, Some(ChangelogDisplay::History));
        assert_eq!(s.reader_mode(), ChangelogDisplay::Summary);
        assert_eq!(
            s.to_json(),
            json!({"changelogDisplay": "history", "readerChangelogDisplay": "summary",
                   "changelogSummaryPrompt": "Short."})
        );
        let old = ChangelogSettings::from_json(&json!({ "siteTitle": "x" }));
        assert!(!old.served(), "a studio before 5.8 has none");
        assert_eq!(old.reader_mode(), ChangelogDisplay::Latest);
        let only = ChangelogSettings {
            reader: Some(ChangelogDisplay::History),
            ..Default::default()
        };
        assert_eq!(only.to_json(), json!({"readerChangelogDisplay": "history"}));
    }

    #[test]
    fn contract_5_8_serves_it() {
        assert!(serves_changelog("5.8.0"));
        assert!(serves_changelog(" 5.12.1"));
        assert!(serves_changelog("6.0.0"));
        assert!(!serves_changelog("5.7.3"));
        assert!(!serves_changelog("4.9.0"));
        assert!(!serves_changelog("next"));
    }

    #[test]
    fn the_request_names_an_own_item_or_a_held_one() {
        assert_eq!(
            SummaryOf::Own("01abc".into()).body(),
            json!({"itemId": "01abc"})
        );
        assert_eq!(
            SummaryOf::Imported {
                subscription_id: "s1".into(),
                remote_id: "r1".into()
            }
            .body(),
            json!({"subscriptionId": "s1", "remoteId": "r1"})
        );
    }

    #[test]
    fn a_summary_is_always_labelled() {
        let s = ChangelogSummary::from_json(&json!({
            "summary": " Two rewrites, then a typo. ", "model": "@cf/example/model",
            "version": 3, "cached": true
        }))
        .unwrap();
        assert_eq!(s.summary, "Two rewrites, then a typo.");
        assert_eq!((s.version, s.cached), (3, true));
        assert!(s.label().contains("@cf/example/model"));
        assert!(s.label().contains("not the author's words"));
        assert!(summary_label("").contains("a model"));
        assert_eq!(ChangelogSummary::from_json(&json!({"summary": ""})), None);
    }

    #[test]
    fn histories_parse_from_either_route() {
        let imported = parse_history(&json!({
            "current": 2, "withdrawn": false,
            "changelog": [
                {"version": 2, "at": "2026-11-02T00:00:00Z", "note": "more", "pinned": false, "generated": false},
                {"version": 1, "at": "2026-11-01T00:00:00Z", "note": null, "pinned": true, "generated": false}
            ]
        }));
        assert_eq!(
            imported.iter().map(|v| v.version).collect::<Vec<_>>(),
            [1, 2]
        );
        assert!(imported[1].current && imported[0].pinned);
        let own = parse_history(&json!({"items": [
            {"version": 1, "publishedAt": "2026-11-01T00:00:00Z", "note": "first", "pinned": false, "kind": "fragment"},
            {"version": 2, "publishedAt": "2026-11-02T00:00:00Z", "note": null, "pinned": false, "kind": "withdrawn"}
        ]}));
        assert_eq!(own.len(), 1, "a withdrawal isn't a version to show");
        assert_eq!(own[0].at, "2026-11-01T00:00:00Z");
        assert!(parse_history(&json!({})).is_empty());
    }

    #[test]
    fn what_an_opened_item_shows() {
        let log = vec![
            rv(1, None, false),
            rv(2, Some(" fixed the ending "), true),
            rv(3, None, false),
        ];
        assert_eq!(
            shown(ChangelogDisplay::Latest, &log, 3),
            Shown::Latest {
                version: 3,
                note: None
            }
        );
        assert_eq!(
            shown(ChangelogDisplay::Latest, &log[..1], 1),
            Shown::Nothing,
            "a first version shows no lead, as before"
        );
        match shown(ChangelogDisplay::History, &log, 3) {
            Shown::History(l) => {
                assert_eq!(l.iter().map(|l| l.version).collect::<Vec<_>>(), [3, 2, 1]);
                assert_eq!(l[1].note.as_deref(), Some("fixed the ending"));
                assert_eq!(
                    history_row(&l[1]),
                    "v2 · 2026-11-02 · “fixed the ending” · 📌 pinned"
                );
                assert_eq!(history_row(&l[0]), "v3 · 2026-11-03 · no note");
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            shown(ChangelogDisplay::Summary, &log, 3),
            Shown::Summary(_)
        ));
        // One noteless version: nothing worth more than the version line.
        assert_eq!(
            shown(ChangelogDisplay::Summary, &log[..1], 1),
            Shown::Nothing
        );
        assert!(matches!(
            shown(ChangelogDisplay::History, &[rv(1, Some("first"), false)], 1),
            Shown::History(_)
        ));
    }
}
