//! --- upkeep --- Studio's maintenance surfaces, for Burrow: stale quotes
//! (Studio's Updates tab and Refresh), your own signals and activity log
//! (Studio's Signals page), and a version's text for a word diff.
//!
//! Stale quotes (blygger-studio `src/freshness.ts`, docs/api.md § Quote
//! freshness): a thread bakes each quote at the version it held when it
//! published. A quote is *stale* when a newer version of what it quotes
//! exists, here or at the source's origin. `GET /freshness` lists every
//! published thread with a stale quote (or one that would block a
//! republish), stalest first, from the blyg's database only. `GET
//! /items/{id}/freshness` reports each quote of one thread, probing the
//! origins. `POST /items/{id}/refresh` republishes the thread's published
//! words with its quotes re-baked: a new version, the words unchanged.
//!
//! Nothing here is a social count: a thread's staleness describes your own
//! post, and signals are your own private record (never published).

use serde::{Deserialize, Serialize};

/// What one quote's freshness check found (`QuoteStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum QuoteStatus {
    /// Baked at the newest version anyone knows of.
    Current,
    /// A newer version is held on the blyg; a refresh re-bakes it.
    Refreshable,
    /// The origin has a newer version than the blyg imported; a refresh
    /// resyncs that subscription first.
    Behind,
    /// A newer version exists, but the quoted passage isn't in it: a
    /// republish would fail until the quote is edited.
    PassageMissing,
    /// The quoted post no longer resolves: a republish would fail.
    Unresolvable,
    /// The source was withdrawn and a pin is kept; a republish bakes the pin.
    Retained,
    /// A status this Burrow doesn't know yet (a newer studio).
    #[serde(other)]
    Other,
}

impl QuoteStatus {
    /// A refresh changes this quote.
    pub fn is_stale(self) -> bool {
        matches!(self, QuoteStatus::Refreshable | QuoteStatus::Behind)
    }

    /// This quote makes any republish fail until the thread is edited.
    pub fn is_blocking(self) -> bool {
        matches!(
            self,
            QuoteStatus::PassageMissing | QuoteStatus::Unresolvable
        )
    }
}

/// One quote of a thread (`QuoteFreshness`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteFreshness {
    /// The quoted post's id.
    pub id: String,
    /// The quoted post's blyg; absent for your own posts.
    #[serde(default)]
    pub origin: Option<String>,
    /// The version baked into the published thread.
    pub baked: u32,
    /// The version a republish would bake now; `None` when it no longer resolves.
    #[serde(default)]
    pub held: Option<u32>,
    /// The version the origin serves now, when it was asked and answered.
    #[serde(default)]
    pub live: Option<u32>,
    /// A partial quote (an excerpt).
    #[serde(default)]
    pub partial: bool,
    pub status: QuoteStatus,
    #[serde(default)]
    pub reason: Option<String>,
}

impl QuoteFreshness {
    /// The newest version known of the quoted post.
    pub fn newest(&self) -> u32 {
        self.held
            .unwrap_or(self.baked)
            .max(self.live.unwrap_or(0))
            .max(self.baked)
    }

    /// What changed, in words (Studio's `quoteState`): `v2 → v3 available`.
    pub fn words(&self) -> String {
        match self.status {
            QuoteStatus::Current => format!("current · v{}", self.baked),
            QuoteStatus::Refreshable => format!(
                "v{} → v{} available",
                self.baked,
                self.held.unwrap_or(self.baked)
            ),
            QuoteStatus::Behind => format!(
                "v{} → v{} at its blyg (fetched on refresh)",
                self.baked,
                self.live.unwrap_or(self.baked)
            ),
            QuoteStatus::Retained | QuoteStatus::PassageMissing => {
                self.reason.clone().unwrap_or_else(|| match self.status {
                    QuoteStatus::Retained => "its post was withdrawn; the pin is kept".into(),
                    _ => "the quoted passage isn't in the new version".into(),
                })
            }
            QuoteStatus::Unresolvable => format!(
                "no longer resolves: {}",
                self.reason.as_deref().unwrap_or("unknown")
            ),
            QuoteStatus::Other => format!("v{}", self.baked),
        }
    }
}

/// One thread's quotes (`ThreadFreshness`). `id` is the server id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadFreshness {
    pub id: String,
    /// The published version the report is about.
    pub version: u32,
    /// The working copy holds something other than the published words: a
    /// refresh is refused until the edits are published or discarded.
    #[serde(default)]
    pub dirty: bool,
    #[serde(default)]
    pub quotes: Vec<QuoteFreshness>,
    /// Quotes a refresh would change.
    #[serde(default)]
    pub stale: u32,
    /// Quotes that would make a republish fail.
    #[serde(default)]
    pub blocking: u32,
    /// The versions the stale quotes have missed, summed (the sort order).
    #[serde(default)]
    pub behind: u32,
}

impl ThreadFreshness {
    /// Studio's `stalenessLine`: "3 versions behind · 2 stale quotes · 1
    /// needs editing", leaving out zeros.
    pub fn line(&self) -> String {
        let plural =
            |n: u32, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        let mut parts = Vec::new();
        if self.behind > 0 {
            parts.push(format!(
                "{} behind",
                plural(self.behind, "version", "versions")
            ));
        }
        if self.stale > 0 {
            parts.push(plural(self.stale, "stale quote", "stale quotes"));
        }
        if self.blocking > 0 {
            parts.push(
                format!(
                    "{} {}",
                    self.blocking,
                    if self.blocking == 1 { "needs" } else { "need" }
                ) + " editing",
            );
        }
        parts.join(" · ")
    }

    /// A refresh can run: something is stale, nothing blocks, and the
    /// blyg's working copy holds the published words.
    pub fn refreshable(&self) -> bool {
        self.stale > 0 && self.blocking == 0 && !self.dirty
    }
}

/// Studio's order: stalest first (`behind`, then stale quotes, then
/// blocking ones); the list's own order breaks ties.
pub fn sort_stalest_first(list: &mut [ThreadFreshness]) {
    list.sort_by(|a, b| {
        b.behind
            .cmp(&a.behind)
            .then(b.stale.cmp(&a.stale))
            .then(b.blocking.cmp(&a.blocking))
    });
}

/// What `POST /items/{id}/refresh` did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Refreshed {
    /// The item's revision after it.
    pub revision: u64,
    /// The new published version.
    pub version: u32,
    /// Ids of the quotes re-baked.
    #[serde(default)]
    pub refreshed: Vec<String>,
    /// Subscriptions resynced first, because their origin was ahead.
    #[serde(default)]
    pub resynced: u32,
    #[serde(default)]
    pub warning: Option<String>,
}

impl Refreshed {
    /// The toast's words: "v4 · 2 quotes refreshed · 1 blyg fetched again".
    pub fn words(&self) -> String {
        let n = self.refreshed.len();
        let mut s = format!(
            "v{} · {n} {} refreshed",
            self.version,
            if n == 1 { "quote" } else { "quotes" }
        );
        if self.resynced > 0 {
            s.push_str(&format!(
                " · {} {} fetched again",
                self.resynced,
                if self.resynced == 1 { "blyg" } else { "blygs" }
            ));
        }
        s
    }
}

// ------------------------------------------------------------ signals

/// One of your thumbs (`GET /thumbs`): 1 liked, -1 disliked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thumb {
    pub thumb: i8,
    pub at: String,
    pub origin: String,
    pub remote_id: String,
    /// The subscription it came through, while you still follow it.
    #[serde(default)]
    pub subscription_id: Option<String>,
    /// The post's title, when the blyg knows it.
    #[serde(default)]
    pub label: Option<String>,
}

/// What an activity-log entry records (`Interaction.kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionKind {
    ThumbUp,
    ThumbDown,
    ThumbClear,
    HopperAdd,
    HopperRemove,
    Stub,
    Fork,
    Quote,
    #[serde(other)]
    Other,
}

impl InteractionKind {
    /// Studio's `KIND_LABELS`.
    pub fn label(self) -> &'static str {
        match self {
            InteractionKind::ThumbUp => "👍 liked",
            InteractionKind::ThumbDown => "👎 disliked",
            InteractionKind::ThumbClear => "cleared a thumb",
            InteractionKind::HopperAdd => "added to hopper",
            InteractionKind::HopperRemove => "removed from hopper",
            InteractionKind::Stub => "responded with a stub",
            InteractionKind::Fork => "forked",
            InteractionKind::Quote => "quoted",
            InteractionKind::Other => "did something",
        }
    }
}

/// One entry of your private activity log (`GET /interactions`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Interaction {
    pub id: i64,
    pub at: String,
    pub kind: InteractionKind,
    pub origin: String,
    pub remote_id: String,
    #[serde(default)]
    pub version: Option<u32>,
    /// Your post it happened in (a stub, a fork, a quote), by server id.
    #[serde(default)]
    pub own_item_id: Option<String>,
    #[serde(default)]
    pub own_version: Option<u32>,
    #[serde(default)]
    pub hopper_id: Option<String>,
    #[serde(default)]
    pub hopper_name: Option<String>,
    /// Reconstructed from earlier records when the log began.
    #[serde(default)]
    pub backfilled: bool,
    #[serde(default)]
    pub subscription_id: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
}

impl Interaction {
    /// "added to hopper “Garden”", "quoted", …
    pub fn what(&self) -> String {
        let mut s = self.kind.label().to_string();
        if let Some(h) = &self.hopper_name {
            s.push_str(&format!(" “{h}”"));
        }
        if self.own_item_id.is_some() {
            s.push_str(" in your post");
        }
        s
    }
}

/// One page of a signals list, and the cursor for the next (`None`: the end).
#[derive(Debug, Clone, PartialEq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next: Option<String>,
}

impl<T> Default for Page<T> {
    fn default() -> Self {
        Page {
            items: Vec::new(),
            next: None,
        }
    }
}

/// `https://www.lin.example.org/` → `lin.example.org` (for a row's subtitle).
/// An internationalized host shows as Unicode with its `xn--` form beside
/// it (§14, [`crate::origin::display_host`]).
pub fn host(origin: &str) -> String {
    crate::origin::display_host(origin).unwrap_or_else(|| origin.to_string())
}

/// What a signal row is called: its label, else `host · id`.
pub fn row_label(label: Option<&str>, origin: &str, remote_id: &str) -> String {
    match label.map(str::trim).filter(|l| !l.is_empty()) {
        Some(l) => l.to_string(),
        None => {
            let id: String = remote_id.chars().take(8).collect();
            format!("{} · {id}", host(origin))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn quote(status: &str, baked: u32, held: Option<u32>, live: Option<u32>) -> QuoteFreshness {
        serde_json::from_value(json!({
            "id": "q1", "baked": baked, "held": held, "live": live,
            "partial": false, "status": status
        }))
        .unwrap()
    }

    #[test]
    fn a_thread_report_reads_as_studio_writes_it() {
        let t: ThreadFreshness = serde_json::from_value(json!({
            "id": "abc", "version": 3, "dirty": false,
            "quotes": [
                {"id": "q1", "origin": "https://lin.example.org/", "baked": 1, "held": 3,
                 "live": null, "partial": true, "status": "refreshable"},
                {"id": "q2", "baked": 2, "held": null, "live": null, "partial": false,
                 "status": "unresolvable", "reason": "not found"},
                {"id": "q3", "baked": 1, "held": 1, "live": 1, "partial": false,
                 "status": "some-new-status"}
            ],
            "stale": 1, "blocking": 1, "behind": 2
        }))
        .unwrap();
        assert_eq!(t.quotes[0].status, QuoteStatus::Refreshable);
        assert_eq!(t.quotes[2].status, QuoteStatus::Other);
        assert_eq!(
            t.line(),
            "2 versions behind · 1 stale quote · 1 needs editing"
        );
        assert!(!t.refreshable(), "a blocking quote stops a refresh");
        assert_eq!(t.quotes[1].words(), "no longer resolves: not found");
    }

    #[test]
    fn quote_words_say_what_changed() {
        assert_eq!(
            quote("refreshable", 2, Some(3), None).words(),
            "v2 → v3 available"
        );
        assert_eq!(
            quote("behind", 2, Some(2), Some(5)).words(),
            "v2 → v5 at its blyg (fetched on refresh)"
        );
        assert_eq!(quote("current", 4, Some(4), None).words(), "current · v4");
        assert_eq!(quote("behind", 2, Some(2), Some(5)).newest(), 5);
        assert!(quote("behind", 1, None, Some(2)).status.is_stale());
        assert!(
            quote("passage-missing", 1, Some(2), None)
                .status
                .is_blocking()
        );
    }

    #[test]
    fn stalest_first_keeps_ties_in_order() {
        let t = |id: &str, behind, stale| ThreadFreshness {
            id: id.into(),
            version: 1,
            dirty: false,
            quotes: vec![],
            stale,
            blocking: 0,
            behind,
        };
        let mut l = vec![t("a", 1, 1), t("b", 3, 1), t("c", 1, 1), t("d", 1, 2)];
        sort_stalest_first(&mut l);
        let ids: Vec<_> = l.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["b", "d", "a", "c"]);
    }

    #[test]
    fn a_refresh_says_what_it_did() {
        let r: Refreshed = serde_json::from_value(json!({
            "ok": true, "revision": 9, "version": 4, "refreshed": ["a", "b"], "resynced": 1
        }))
        .unwrap();
        assert_eq!(r.words(), "v4 · 2 quotes refreshed · 1 blyg fetched again");
    }

    #[test]
    fn signals_read_and_label() {
        let i: Interaction = serde_json::from_value(json!({
            "id": 7, "at": "2026-11-01T10:00:00Z", "kind": "hopper_add",
            "origin": "https://lin.example.org/", "remoteId": "01abcdefghjk",
            "version": 2, "ownItemId": null, "ownVersion": null, "hopperId": "h1",
            "hopperName": "Garden", "backfilled": false, "subscriptionId": "s1", "label": null
        }))
        .unwrap();
        assert_eq!(i.what(), "added to hopper “Garden”");
        assert_eq!(
            row_label(i.label.as_deref(), &i.origin, &i.remote_id),
            "lin.example.org · 01abcdef"
        );
        let t: Thumb = serde_json::from_value(json!({
            "thumb": -1, "at": "2026-11-01T10:00:00Z", "origin": "https://lin.example.org/",
            "remoteId": "x", "subscriptionId": null, "label": "On benches"
        }))
        .unwrap();
        assert_eq!(t.thumb, -1);
        assert_eq!(
            row_label(t.label.as_deref(), &t.origin, &t.remote_id),
            "On benches"
        );
    }
}
