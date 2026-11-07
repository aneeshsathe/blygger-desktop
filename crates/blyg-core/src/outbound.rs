//! --- contacts --- What the Mentions screen reads beyond the inbound list
//! (Studio's `MentionsPage`):
//!
//! - `GET /api/mentions?direction=outbound`: the Webmentions your blyg sent
//!   for your posts' links, each with its delivery status (contract 5.0).
//! - `GET /api/mentions/summary`: whether there's anything each way
//!   (contract 5.2). Burrow shows no count from it: responses are a list,
//!   never a number, so the summary only says whether a side is empty.
//! - `GET /api/mention-groups`: the posts that have responses, newest
//!   first (contract 5.2).
//! - `GET /api/mentions/{id}/source`: whether a response's source post is
//!   held here (`holder`, so you can stub back) or its blyg subscribed.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One Webmention your blyg sent (the contract's `OutboundMention`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OutboundMention {
    pub id: String,
    /// Your post that links out.
    pub item_id: String,
    /// The version of your post that sent it.
    pub version: u32,
    /// The target post's version, when it's a blyg post.
    pub target_version: Option<u32>,
    /// The page your post links to.
    pub target: String,
    /// Its Webmention endpoint, once discovered.
    pub endpoint: Option<String>,
    /// `pending`, `sent`, `failed` or `no_endpoint`.
    pub status: String,
    pub attempts: u32,
    pub next_attempt_at: Option<String>,
    pub last_error: Option<String>,
    pub created: String,
}

impl OutboundMention {
    /// The status as a word: `sent`, `pending`, `failed`, `no endpoint`.
    pub fn status_label(&self) -> String {
        self.status.replace('_', " ")
    }

    /// The line under it: tries, and when the next one is (pending only).
    /// "1 try" / "3 tries" counts deliveries, not people.
    pub fn detail(&self) -> String {
        let tries = match self.attempts {
            0 => "not tried yet".to_string(),
            1 => "1 try".to_string(),
            n => format!("{n} tries"),
        };
        match (&self.next_attempt_at, self.status.as_str()) {
            (Some(_), "pending") => format!("{tries} · trying again"),
            _ => tries,
        }
    }

    /// Whether it's a problem worth the alert colour.
    pub fn failed(&self) -> bool {
        self.status == "failed"
    }
}

/// `GET /api/mentions/summary`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MentionSummary {
    pub inbound: u64,
    pub outbound: u64,
    pub groups: u64,
}

/// One `GET /api/mention-groups` row: a post of yours with responses.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MentionGroup {
    pub target_item_id: String,
    pub total: u64,
    pub visible: u64,
    pub verified_at: String,
    pub last_seen: String,
    pub first_id: String,
}

/// `GET /api/mentions/{id}/source`: where a response came from, as this
/// blyg knows it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MentionSource {
    /// The subscription that holds the source post (you can stub back).
    pub holder: Option<String>,
    /// The subscription to the source's blyg, held or not: `(id, title)`.
    pub subscription: Option<(String, String)>,
}

impl MentionSource {
    pub fn parse(v: &Value) -> MentionSource {
        let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        MentionSource {
            holder: s(v, "holder").filter(|h| !h.is_empty()),
            subscription: v
                .get("subscription")
                .filter(|s| s.is_object())
                .and_then(|sub| Some((s(sub, "id")?, s(sub, "title").unwrap_or_default()))),
        }
    }
}

/// Whether a `contractVersion` serves `/mention-groups` and
/// `/mentions/summary` (5.2.0).
pub fn serves_groups(version: &str) -> bool {
    let mut parts = version.trim().split('.').map(|p| p.parse::<u64>().ok());
    match (parts.next().flatten(), parts.next().flatten()) {
        (Some(5), Some(minor)) => minor >= 2,
        (Some(major), _) => major > 5,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn outbound_rows_parse_as_served() {
        let v = json!({
            "id": "o1", "itemId": "it1", "version": 2, "targetVersion": null,
            "target": "https://lin.example.org/f/x/", "endpoint": null,
            "status": "no_endpoint", "attempts": 3, "nextAttemptAt": null,
            "lastError": "no webmention endpoint", "created": "2030-01-01T00:00:00Z"
        });
        let o: OutboundMention = serde_json::from_value(v).unwrap();
        assert_eq!(o.status_label(), "no endpoint");
        assert_eq!(o.detail(), "3 tries");
        assert_eq!(o.last_error.as_deref(), Some("no webmention endpoint"));
        let p = OutboundMention {
            status: "pending".into(),
            attempts: 1,
            next_attempt_at: Some("2030-01-01T01:00:00Z".into()),
            ..o
        };
        assert_eq!(p.detail(), "1 try · trying again");
    }

    #[test]
    fn sources_name_the_holder_and_the_subscription() {
        let s = MentionSource::parse(&json!({
            "holder": "S1",
            "subscription": {"id": "S1", "title": "Lin", "origin": "https://lin.example.org"}
        }));
        assert_eq!(s.holder.as_deref(), Some("S1"));
        assert_eq!(s.subscription, Some(("S1".into(), "Lin".into())));
        assert_eq!(
            MentionSource::parse(&json!({"holder": null, "subscription": null})),
            MentionSource::default()
        );
    }

    #[test]
    fn groups_and_summary_need_5_2() {
        assert!(serves_groups("5.2.0"));
        assert!(serves_groups("5.7.1"));
        assert!(serves_groups("6.0.0"));
        assert!(!serves_groups("5.1.9"));
        assert!(!serves_groups(""));
    }
}
