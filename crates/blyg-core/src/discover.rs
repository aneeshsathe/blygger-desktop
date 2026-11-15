//! --- discover --- The blygs listed on blygger.com (studio 0.60's Discover,
//! `src/ui/discover.ts`), desktop only.
//!
//! The directory is the public `https://blygger.com/blygs.json`. The studio
//! reads it from the owner's browser; Burrow reads it natively, token-less,
//! through a bounded [`PublicClient`], only when Discover is opened. Burrow on
//! the web keeps to its own blyg's origin, so it has no Discover until the
//! studio serves the directory same-origin (`client-security`).
//!
//! The rules are the studio's, so a blyg shows up the same in both:
//!
//! - **Rows** need an origin and a listing date; anything else missing is
//!   filled in or dropped, never guessed ([`parse_directory`]). Burrow is
//!   stricter on URLs: https with a host only (the studio takes http too).
//! - **Already followed** is the OPML import's spelling rule
//!   ([`crate::opml::feed_key`]) on a subscription's feed and, unless it
//!   follows one hopper, its origin ([`subscribed_keys`]). Your own blyg is
//!   never offered ([`is_self`]).
//! - **Order**: newest listing first, the directory's own order (most active
//!   first) on ties ([`by_listing`]); five at a time ([`PAGE_SIZE`]).
//! - **New** is one timestamp, `Settings.discoverSeenAt`: the listing date of
//!   the newest listing Discover has shown. A row listed after it is new;
//!   with no mark yet, nothing is badged (everything would be). Opening
//!   Discover moves the mark forward, never back ([`next_seen_at`]), with a
//!   partial `PATCH /api/settings {discoverSeenAt}` only.

use std::collections::HashSet;
use std::time::Duration;

use serde_json::Value;

use crate::api::public::PublicClient;
use crate::backend::{Backend, CoreError, Result};
use crate::model::Subscription;
use crate::opml::feed_key;
use crate::util::{iso_from_ms, parse_ts_ms};

/// The public directory (release builds always read this one).
pub const DIRECTORY_URL: &str = "https://blygger.com/blygs.json";

/// Debug builds only: read the directory from here instead (a localhost
/// fixture in tests). Release builds ignore it.
pub const URL_ENV: &str = "BLYGGER_DISCOVER_URL";

/// Rows the list shows at first, and adds on each "show more" (the studio's).
pub const PAGE_SIZE: usize = 5;

/// The most of the directory Burrow reads (it is about 20 KB for 40 blygs).
pub const MAX_BYTES: u64 = 2 * 1024 * 1024;

/// The directory's URL: [`DIRECTORY_URL`], or in a debug build
/// [`URL_ENV`] when set.
pub fn directory_url() -> String {
    #[cfg(debug_assertions)]
    if let Ok(u) = std::env::var(URL_ENV)
        && !u.trim().is_empty()
    {
        return u.trim().to_string();
    }
    DIRECTORY_URL.to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityLevel {
    High,
    Medium,
    Low,
}

impl ActivityLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            ActivityLevel::High => "high",
            ActivityLevel::Medium => "medium",
            ActivityLevel::Low => "low",
        }
    }
}

/// How active the directory measured a blyg.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Activity {
    /// `None`: not measured yet.
    pub level: Option<ActivityLevel>,
    pub events: u64,
    /// Its latest post (ISO), when known.
    pub last_post: Option<String>,
}

/// One blyg the directory lists. Every text is remote input: show it as
/// plain text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryBlyg {
    /// Its origin (https), what Subscribe sends.
    pub origin: String,
    /// Its title, or its host when it gave none.
    pub title: String,
    /// Its home page (https), the origin when it gave none.
    pub home: String,
    pub feed: Option<String>,
    /// When blygger.com listed it, as the directory wrote it.
    pub listed_at: String,
    /// `listed_at` as unix ms.
    pub listed_ms: i64,
    pub activity: Activity,
}

impl DirectoryBlyg {
    /// `host/path` of its home page, for the list (the studio's).
    pub fn home_label(&self) -> String {
        match url::Url::parse(&self.home) {
            Ok(u) => format!(
                "{}{}",
                u.host_str().unwrap_or_default(),
                u.path().trim_end_matches('/')
            ),
            Err(_) => self.home.clone(),
        }
    }
}

/// Why the directory couldn't be read, in the studio's words.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DiscoverError {
    #[error("Could not reach blygger.com.")]
    Unreachable,
    #[error("blygger.com answered {0}. Try again in a moment.")]
    Status(u16),
    #[error("blygger.com answered with something that is not its directory.")]
    NotDirectory,
}

impl From<DiscoverError> for CoreError {
    fn from(e: DiscoverError) -> Self {
        CoreError::Other(e.to_string())
    }
}

/// An https URL with a host. Directory rows are remote input: nothing else
/// is linked to, subscribed to or opened.
pub fn safe_url(value: &Value) -> Option<String> {
    let s = value.as_str()?.trim();
    let u = url::Url::parse(s).ok()?;
    (u.scheme() == "https" && u.host_str().is_some_and(|h| !h.is_empty())).then(|| s.to_string())
}

fn date(value: &Value) -> Option<(String, i64)> {
    let s = value.as_str()?;
    parse_ts_ms(s).map(|ms| (s.to_string(), ms))
}

/// The directory's `blygs`, in its order (most active first). A row missing
/// what a subscribe or a preview link needs (an https origin, a listing
/// date) is dropped, not guessed at; a JSON value that isn't the directory
/// is [`DiscoverError::NotDirectory`].
pub fn parse_directory(json: &Value) -> std::result::Result<Vec<DirectoryBlyg>, DiscoverError> {
    let rows = json
        .get("blygs")
        .and_then(Value::as_array)
        .ok_or(DiscoverError::NotDirectory)?;
    let mut out = Vec::new();
    for r in rows {
        if !r.is_object() {
            continue;
        }
        let (Some(origin), Some((listed_at, listed_ms))) =
            (safe_url(&r["origin"]), date(&r["listedAt"]))
        else {
            continue;
        };
        let a = &r["activity"];
        let level = match a["level"].as_str() {
            Some("high") => Some(ActivityLevel::High),
            Some("medium") => Some(ActivityLevel::Medium),
            Some("low") => Some(ActivityLevel::Low),
            _ => None,
        };
        let title = match r["title"].as_str().map(str::trim) {
            Some(t) if !t.is_empty() => t.to_string(),
            _ => url::Url::parse(&origin)
                .ok()
                .and_then(|u| {
                    u.host_str().map(|h| {
                        format!(
                            "{h}{}",
                            u.port().map(|p| format!(":{p}")).unwrap_or_default()
                        )
                    })
                })
                .unwrap_or_else(|| origin.clone()),
        };
        out.push(DirectoryBlyg {
            home: safe_url(&r["home"]).unwrap_or_else(|| origin.clone()),
            feed: safe_url(&r["feed"]),
            title,
            origin,
            listed_at,
            listed_ms,
            activity: Activity {
                level,
                events: a["events"]
                    .as_f64()
                    .filter(|n| *n >= 0.)
                    .map_or(0, |n| n as u64),
                last_post: a["lastPost"]
                    .as_str()
                    .filter(|s| parse_ts_ms(s).is_some())
                    .map(str::to_string),
            },
        });
    }
    Ok(out)
}

/// Read and parse the directory at `url` (blocking: call it off the UI
/// thread). No token, no cookies; at most [`MAX_BYTES`]; 20 s at most.
pub fn fetch(url: &str) -> std::result::Result<Vec<DirectoryBlyg>, DiscoverError> {
    let client = PublicClient::bounded(
        Duration::from_secs(10),
        Duration::from_secs(15),
        Duration::from_secs(20),
    );
    let bytes = match client.get_bytes(url, MAX_BYTES) {
        Ok((b, _)) => b,
        Err(CoreError::Offline) => return Err(DiscoverError::Unreachable),
        Err(CoreError::NotFound) => return Err(DiscoverError::Status(404)),
        Err(CoreError::Rejected { status, .. }) => return Err(DiscoverError::Status(status)),
        // Too large, or not an http(s) URL.
        Err(_) => return Err(DiscoverError::NotDirectory),
    };
    let json: Value = serde_json::from_slice(&bytes).map_err(|_| DiscoverError::NotDirectory)?;
    parse_directory(&json)
}

/// Newest listing first; the directory's own order (most active first)
/// breaks ties.
pub fn by_listing(blygs: &[DirectoryBlyg]) -> Vec<&DirectoryBlyg> {
    let mut v: Vec<&DirectoryBlyg> = blygs.iter().collect();
    // Stable: ties keep the directory's order.
    v.sort_by_key(|b| std::cmp::Reverse(b.listed_ms));
    v
}

/// Listed after the newest listing already shown; with no mark yet, every
/// listing is (the list badges nothing then, see [`badge_new`]).
pub fn is_new(blyg: &DirectoryBlyg, seen_at: &str) -> bool {
    if seen_at.is_empty() {
        return true;
    }
    parse_ts_ms(seen_at).is_some_and(|seen| blyg.listed_ms > seen)
}

/// Whether a row gets the "new" badge: there is a mark (`baseline`, as it
/// stood when this visit began), the row was listed after it, and it isn't
/// followed. The studio's rule.
pub fn badge_new(blyg: &DirectoryBlyg, baseline: Option<&str>, followed: bool) -> bool {
    !followed && baseline.is_some_and(|b| !b.is_empty() && is_new(blyg, b))
}

/// What `discoverSeenAt` becomes once these have been shown: the newest
/// listing date, never moving backwards. `seen_at` itself when nothing is
/// newer and it is a date.
pub fn next_seen_at(blygs: &[DirectoryBlyg], seen_at: &str) -> String {
    let mut newest = parse_ts_ms(seen_at);
    for b in blygs {
        newest = Some(newest.map_or(b.listed_ms, |n| n.max(b.listed_ms)));
    }
    match newest {
        Some(ms) => iso_from_ms(ms),
        None => seen_at.to_string(),
    }
}

/// A subscription that follows one hopper at another blyg (spec 0.4
/// §16.12): its feed is `{origin}h/{slug}/feed.xml`. Following one hopper
/// isn't following the blyg. (The studio reads `hopperFeedUrl`; Burrow's
/// subscriptions don't keep it, so this reads the feed's path.)
fn follows_hopper(s: &Subscription) -> bool {
    let (Ok(o), Ok(f)) = (url::Url::parse(&s.origin), url::Url::parse(&s.feed_url)) else {
        return false;
    };
    if o.host_str() != f.host_str() {
        return false;
    }
    let base = o.path().trim_end_matches('/');
    let Some(rest) = f.path().strip_prefix(base) else {
        return false;
    };
    let parts: Vec<&str> = rest.trim_start_matches('/').split('/').collect();
    matches!(parts.as_slice(), ["h", slug, "feed.xml"] if !slug.is_empty())
}

/// The keys of what the owner already follows: each subscription's feed and
/// (unless it follows one hopper) its origin.
pub fn subscribed_keys(subs: &[Subscription]) -> HashSet<String> {
    let mut keys = HashSet::new();
    for s in subs {
        keys.insert(feed_key(&s.feed_url));
        if !follows_hopper(s) {
            keys.insert(feed_key(&s.origin));
        }
    }
    keys
}

/// Followed already, by origin or by feed.
pub fn is_subscribed(blyg: &DirectoryBlyg, keys: &HashSet<String>) -> bool {
    keys.contains(&feed_key(&blyg.origin))
        || blyg
            .feed
            .as_deref()
            .is_some_and(|f| keys.contains(&feed_key(f)))
}

/// The owner's own blyg is listed too; it is never offered to them.
pub fn is_self(blyg: &DirectoryBlyg, own_origin: Option<&str>) -> bool {
    own_origin.is_some_and(|o| feed_key(&blyg.origin) == feed_key(o))
}

/// The list Discover shows: not followed (`keep` holds rows touched during
/// this visit, so a blyg just subscribed stays, saying so), newest listing
/// first.
pub fn unfollowed<'a>(
    blygs: &'a [DirectoryBlyg],
    keys: &HashSet<String>,
    keep: &dyn Fn(&DirectoryBlyg) -> bool,
) -> Vec<&'a DirectoryBlyg> {
    by_listing(blygs)
        .into_iter()
        .filter(|b| !is_subscribed(b, keys) || keep(b))
        .collect()
}

/// "today", "yesterday", "3 days ago", "a month ago", … (the studio's).
pub fn ago(iso: &str, now_ms: i64) -> String {
    let Some(at) = parse_ts_ms(iso) else {
        return String::new();
    };
    let days = (now_ms - at).div_euclid(86_400_000);
    if days < 1 {
        return "today".into();
    }
    if days == 1 {
        return "yesterday".into();
    }
    if days < 30 {
        return format!("{days} days ago");
    }
    let months = days / 30;
    if months < 12 {
        return if months == 1 {
            "a month ago".into()
        } else {
            format!("{months} months ago")
        };
    }
    let years = days / 365;
    if years <= 1 {
        "a year ago".into()
    } else {
        format!("{years} years ago")
    }
}

/// The words beside a row's activity badge: when it last posted, or that
/// nobody has measured it.
pub fn activity_line(blyg: &DirectoryBlyg, now_ms: i64) -> String {
    match (blyg.activity.level, &blyg.activity.last_post) {
        (None, _) => "activity not measured yet".into(),
        (Some(_), Some(p)) => format!("last post {}", ago(p, now_ms)),
        (Some(l), None) => format!("{} activity", l.as_str()),
    }
}

/// One visit to Discover: the directory (minus your own blyg) and the mark
/// as it stood when the visit began.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Visit {
    pub blygs: Vec<DirectoryBlyg>,
    /// `discoverSeenAt` before this visit; `None` when the blyg doesn't keep
    /// it (then nothing is badged and nothing is written).
    pub baseline: Option<String>,
    /// The mark couldn't be moved (the list still shows).
    pub seen_error: Option<String>,
}

/// Open Discover (blocking: off the UI thread): read the mark, read the
/// directory through `backend`, and move the mark to the newest listing
/// shown.
pub fn open(backend: &dyn Backend) -> Result<Visit> {
    // The mark first, as the studio waits for its settings.
    let baseline = backend.discover_seen_at().ok().flatten();
    let blygs = backend.discover_directory()?;
    Ok(open_with(backend, blygs, baseline))
}

/// [`open`] over a directory already read.
pub fn open_with(
    backend: &dyn Backend,
    blygs: Vec<DirectoryBlyg>,
    baseline: Option<String>,
) -> Visit {
    let own = backend.base_url();
    let blygs: Vec<DirectoryBlyg> = blygs
        .into_iter()
        .filter(|b| !is_self(b, own.as_deref()))
        .collect();
    let mut seen_error = None;
    if let Some(seen) = &baseline {
        let next = next_seen_at(&blygs, seen);
        if &next != seen
            && let Err(e) = backend.set_discover_seen_at(&next)
        {
            seen_error = Some(e.to_string());
        }
    }
    Visit {
        blygs,
        baseline,
        seen_error,
    }
}

#[cfg(test)]
mod tests;
