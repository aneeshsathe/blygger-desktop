//! OPML: reading a feed reader's export (to import it as subscriptions),
//! writing one of every subscription, and the paced import run.
//!
//! - [`outlines`] is the one OPML parser: a blogroll (`profile::parse_opml`)
//!   and an import both use it. It is lenient on purpose (real exports from
//!   Feedly, NetNewsWire, Inoreader, Miniflux, Reeder differ): nested
//!   outlines are flattened with their folder kept for display, `type` is
//!   ignored, HTML entities and double escaping are undone, and junk around
//!   the outlines is skipped. It never fetches anything.
//! - [`parse_bytes`] decodes a file (BOM, UTF-16, a "UTF-8" file that
//!   isn't), caps its size and feed count, and collapses duplicates.
//! - [`export`] writes OPML 2.0 the way the studio writes `blogroll.opml`,
//!   but of every subscription (the public blogroll lists only some).
//! - [`run_import`] subscribes to each feed through
//!   `POST /api/subscriptions {url, confirm:true}` (the same call as a
//!   hand-added subscription), a couple at a time and slowly, since every
//!   new subscription backfills its whole archive on the server; a 429 waits
//!   out its `Retry-After`. What's added goes into the local Reader folder
//!   [`IMPORT_FOLDER`].

use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::backend::{Backend, CoreError};
use crate::model::{Folder, Subscription, SubscriptionKind};
use crate::profile::{attrs, host_of, under_origin, xml_unescape};

/// The largest OPML file read (a 2,000-feed export is well under 1 MB).
pub const MAX_FILE_BYTES: usize = 5 * 1024 * 1024;
/// The most feeds one import takes.
pub const MAX_FEEDS: usize = 2000;
/// The local Reader folder imported subscriptions are filed in.
pub const IMPORT_FOLDER: &str = "Imported feeds";
/// The default file name for an export.
pub const EXPORT_FILE_NAME: &str = "burrow-subscriptions.opml";

// ================================================================ parse

/// One `<outline>` with a feed or site URL (http(s) only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outline {
    pub title: String,
    /// `xmlUrl`: the feed.
    pub xml_url: Option<String>,
    /// `htmlUrl`: the site.
    pub html_url: Option<String>,
    /// The folders it sits in, outermost first, joined with " / ".
    pub folder: Option<String>,
}

/// One feed of an import: an outline with an `xmlUrl`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpmlFeed {
    pub title: String,
    pub xml_url: String,
    pub html_url: Option<String>,
    /// For display only: the folder it was in, in the other reader.
    pub folder: Option<String>,
}

/// A parsed import file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Parsed {
    /// Each feed once, in file order.
    pub feeds: Vec<OpmlFeed>,
    /// Outlines that named a feed already listed (collapsed).
    pub duplicates: usize,
    /// Outlines with a site but no feed URL (skipped).
    pub without_feed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OpmlError {
    #[error(
        "That file is {} MB; an OPML file Burrow imports is at most {} MB",
        .0 / (1024 * 1024) + 1,
        MAX_FILE_BYTES / (1024 * 1024)
    )]
    TooBig(usize),
    #[error(
        "That file lists {0} feeds; Burrow imports at most {MAX_FEEDS} at a time. Split it in your other reader and import each part"
    )]
    TooMany(usize),
    #[error("That isn't an OPML file (no <opml> or <outline> in it)")]
    NotOpml,
    #[error("No feeds in that file: none of its outlines has an xmlUrl")]
    NoFeeds,
    #[error("Couldn't read the file: {0}")]
    Read(String),
}

/// Bytes to text: a UTF-8 or UTF-16 BOM is honoured and dropped; bytes that
/// aren't UTF-8 (a Windows-1252 file that says it's UTF-8) are read as
/// Windows-1252, so a stray `é` never loses the whole file.
pub fn decode(bytes: &[u8]) -> String {
    let utf16 = |be: bool| {
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| {
                if be {
                    u16::from_be_bytes(c)
                } else {
                    u16::from_le_bytes(c)
                }
            })
            .collect();
        char::decode_utf16(units)
            .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect::<String>()
    };
    match bytes {
        [0xFF, 0xFE, ..] => utf16(false),
        [0xFE, 0xFF, ..] => utf16(true),
        _ => {
            let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
            match std::str::from_utf8(b) {
                Ok(s) => s.to_string(),
                Err(_) => b.iter().map(|&c| cp1252(c)).collect(),
            }
        }
    }
}

/// Windows-1252 byte → char (0x80–0x9F differ from Latin-1).
fn cp1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž',
        '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}',
        'ž', 'Ÿ',
    ];
    match b {
        0x80..=0x9F => HIGH[(b - 0x80) as usize],
        _ => b as char,
    }
}

/// A title as exports write it: entities undone, twice when the exporter
/// escaped an already-escaped name (`&amp;amp;`), whitespace collapsed.
fn clean_title(s: &str) -> String {
    let once = xml_unescape(s);
    let twice = if once.contains('&') {
        xml_unescape(&once)
    } else {
        once
    };
    twice.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Where the start tag that begins at `from` (just past `<outline`) ends:
/// the index of its `>`, skipping any inside quoted attribute values.
fn tag_end(xml: &[u8], from: usize) -> Option<usize> {
    let mut quote = None;
    for (i, &c) in xml.iter().enumerate().skip(from) {
        match (quote, c) {
            (None, b'"' | b'\'') => quote = Some(c),
            (Some(q), _) if c == q => quote = None,
            (None, b'>') => return Some(i),
            // A quote left open runs into the next tag: give up on it there.
            (Some(_), b'<') => return None,
            _ => {}
        }
    }
    None
}

/// Every `<outline>` with an http(s) feed or site URL, in file order,
/// with the folders around it. No dedupe. Comments are skipped; junk is
/// tolerated.
pub fn outlines(xml: &str) -> Vec<Outline> {
    let lower = xml.to_ascii_lowercase();
    let lb = lower.as_bytes();
    // Open outlines: `Some(name)` for a folder, `None` for a feed left open.
    let mut stack: Vec<Option<String>> = vec![];
    let mut out = vec![];
    let mut i = 0;
    while let Some(off) = lower[i..].find('<') {
        let at = i + off;
        let rest = &lower[at..];
        if rest.starts_with("<!--") {
            i = rest.find("-->").map_or(lower.len(), |e| at + e + 3);
            continue;
        }
        if let Some(r) = rest.strip_prefix("</outline") {
            if r.starts_with(|c: char| c == '>' || c.is_ascii_whitespace()) {
                stack.pop();
            }
            i = at + 2;
            continue;
        }
        let name_end = at + "<outline".len();
        let is_outline = rest.starts_with("<outline")
            && lb
                .get(name_end)
                .is_some_and(|&c| c.is_ascii_whitespace() || c == b'>' || c == b'/');
        if !is_outline {
            i = at + 1;
            continue;
        }
        let Some(end) = tag_end(lb, name_end) else {
            i = at + 1;
            continue;
        };
        let inner = &xml[at + 1..end];
        let self_closing = inner.trim_end().ends_with('/');
        let a = attrs(inner.trim_end().trim_end_matches('/'));
        let http = |k: &str| {
            a.get(k).map(|s| s.trim().to_string()).filter(|s| {
                let l = s.to_ascii_lowercase();
                l.starts_with("http://") || l.starts_with("https://")
            })
        };
        let xml_url = http("xmlurl");
        let html_url = http("htmlurl");
        let label = a
            .get("title")
            .filter(|t| !t.trim().is_empty())
            .or_else(|| a.get("text"))
            .map(|s| clean_title(s))
            .filter(|s| !s.is_empty());
        if xml_url.is_some() || html_url.is_some() {
            let folders: Vec<&str> = stack
                .iter()
                .flatten()
                .map(String::as_str)
                .filter(|f| !f.is_empty())
                .collect();
            out.push(Outline {
                title: label
                    .or_else(|| html_url.as_deref().or(xml_url.as_deref()).and_then(host_of))
                    .unwrap_or_default(),
                xml_url,
                html_url,
                folder: (!folders.is_empty()).then(|| folders.join(" / ")),
            });
            if !self_closing {
                stack.push(None);
            }
        } else if !self_closing {
            // An unnamed folder ("") still nests; it just isn't shown.
            stack.push(Some(label.unwrap_or_default()));
        }
        i = end + 1;
    }
    out
}

/// Parse an OPML file's bytes for an import.
pub fn parse_bytes(bytes: &[u8]) -> Result<Parsed, OpmlError> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(OpmlError::TooBig(bytes.len()));
    }
    parse(&decode(bytes))
}

/// Read and parse the OPML file at `path` (size-checked before reading).
pub fn read_file(path: &std::path::Path) -> Result<Parsed, OpmlError> {
    let len = std::fs::metadata(path)
        .map_err(|e| OpmlError::Read(e.to_string()))?
        .len() as usize;
    if len > MAX_FILE_BYTES {
        return Err(OpmlError::TooBig(len));
    }
    let bytes = std::fs::read(path).map_err(|e| OpmlError::Read(e.to_string()))?;
    parse_bytes(&bytes)
}

/// Parse OPML text for an import: feeds with an `xmlUrl`, each once.
pub fn parse(xml: &str) -> Result<Parsed, OpmlError> {
    let all = outlines(xml);
    let lower = xml.to_ascii_lowercase();
    if all.is_empty() && !lower.contains("<opml") {
        return Err(OpmlError::NotOpml);
    }
    let mut p = Parsed::default();
    let mut seen = HashSet::new();
    for o in all {
        let Some(xml_url) = o.xml_url else {
            p.without_feed += 1;
            continue;
        };
        if !seen.insert(feed_key(&xml_url)) {
            p.duplicates += 1;
            continue;
        }
        p.feeds.push(OpmlFeed {
            title: o.title,
            xml_url,
            html_url: o.html_url,
            folder: o.folder,
        });
    }
    if p.feeds.is_empty() {
        return Err(OpmlError::NoFeeds);
    }
    if p.feeds.len() > MAX_FEEDS {
        return Err(OpmlError::TooMany(p.feeds.len()));
    }
    Ok(p)
}

// ================================================================ matching

/// A feed URL for comparing: scheme, `www.`, case of the host, fragment
/// and a trailing slash don't count; the query does.
pub fn feed_key(url: &str) -> String {
    let t = url.trim();
    let Ok(u) = url::Url::parse(t) else {
        return t.trim_end_matches('/').to_ascii_lowercase();
    };
    let host = u.host_str().unwrap_or_default().to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    let port = u.port().map(|p| format!(":{p}")).unwrap_or_default();
    let path = u.path().trim_end_matches('/');
    let query = u.query().map(|q| format!("?{q}")).unwrap_or_default();
    format!("{host}{port}{path}{query}")
}

/// The subscription that already follows `feed`, if any: the same feed
/// URL, or (for a blyg) a feed or site under its origin.
pub fn followed<'a>(feed: &OpmlFeed, subs: &'a [Subscription]) -> Option<&'a Subscription> {
    let key = feed_key(&feed.xml_url);
    subs.iter().find(|s| {
        feed_key(&s.feed_url) == key
            || feed_key(&s.origin) == key
            || (s.kind == SubscriptionKind::Blyg
                && (under_origin(&feed.xml_url, &s.origin)
                    || feed
                        .html_url
                        .as_deref()
                        .is_some_and(|h| under_origin(h, &s.origin))))
    })
}

// ================================================================ export

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// OPML 2.0 of every subscription, in the studio's `blogroll.opml` shape:
/// `text`/`title` its name, `xmlUrl` its feed (a blyg's `feed.xml`),
/// `htmlUrl` its site (a feed's own URL when that's all that's known).
pub fn export(subs: &[Subscription]) -> String {
    let outlines: Vec<String> = subs
        .iter()
        .map(|s| {
            let title = esc(if s.title.trim().is_empty() {
                &s.origin
            } else {
                s.title.trim()
            });
            format!(
                "    <outline text=\"{title}\" title=\"{title}\" type=\"rss\" xmlUrl=\"{}\" htmlUrl=\"{}\"/>",
                esc(&s.feed_url),
                esc(&s.origin)
            )
        })
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<opml version=\"2.0\">\n  <head>\n    <title>Burrow subscriptions</title>\n  </head>\n  <body>\n{}{}  </body>\n</opml>\n",
        outlines.join("\n"),
        if outlines.is_empty() { "" } else { "\n" }
    )
}

// ================================================================ import

/// Why one feed wasn't added.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailKind {
    /// 422: the blyg couldn't resolve it to a blyg or a feed (not a feed,
    /// or nothing answered there).
    NotAFeed,
    /// Burrow couldn't reach your blyg.
    Unreachable,
    /// Any other refusal (with the server's reason).
    Refused,
}

impl FailKind {
    pub fn label(self) -> &'static str {
        match self {
            FailKind::NotAFeed => "not a feed",
            FailKind::Unreachable => "unreachable",
            FailKind::Refused => "refused",
        }
    }
}

/// What happened to one feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// 201: subscribed (and filed in the import folder).
    Added {
        id: String,
        title: String,
    },
    /// 409: already following it.
    AlreadyFollowing,
    Failed {
        kind: FailKind,
        reason: String,
    },
}

/// How fast an import goes. Every new subscription makes the blyg backfill
/// the source's archive, so the default is far under the owner write
/// budget (120 a minute): two at a time, one started every 2 s (≤ 30 a
/// minute).
#[derive(Debug, Clone, Copy)]
pub struct Pace {
    pub concurrency: usize,
    /// The least time between two subscribes starting.
    pub interval: Duration,
    /// How many 429s one feed waits out before it counts as failed.
    pub max_waits: u32,
}

impl Default for Pace {
    fn default() -> Self {
        Pace {
            concurrency: 2,
            interval: Duration::from_secs(2),
            max_waits: 5,
        }
    }
}

/// What a run reports as it goes (from its worker threads).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// The blyg asked to slow down: nothing starts for this long.
    Waiting { seconds: u64 },
    /// Feed `index` finished.
    Done { index: usize, outcome: Outcome },
}

/// The local Reader folder named [`IMPORT_FOLDER`] (any case), made when
/// missing. Never a second one.
pub fn import_folder(backend: &dyn Backend) -> crate::Result<Folder> {
    let find = || {
        backend
            .folders()
            .into_iter()
            .find(|f| f.name.trim().eq_ignore_ascii_case(IMPORT_FOLDER))
    };
    if let Some(f) = find() {
        return Ok(f);
    }
    match backend.create_folder(IMPORT_FOLDER) {
        Ok(f) => Ok(f),
        // Made meanwhile (another window, the CLI): use that one.
        Err(e) => find().ok_or(e),
    }
}

/// Subscribe to one feed, mapping the answer. `Err(retry_after)` is a 429.
fn outcome_of(r: crate::Result<Subscription>) -> Result<Outcome, u64> {
    match r {
        Ok(s) => Ok(Outcome::Added {
            id: s.id,
            title: s.title,
        }),
        Err(CoreError::RateLimited { retry_after }) => Err(retry_after),
        Err(CoreError::Rejected { status: 409, .. }) => Ok(Outcome::AlreadyFollowing),
        Err(CoreError::Rejected {
            status: 422,
            message,
            details,
        }) => Ok(Outcome::Failed {
            kind: FailKind::NotAFeed,
            reason: with_details(message, &details),
        }),
        Err(CoreError::Offline) => Ok(Outcome::Failed {
            kind: FailKind::Unreachable,
            reason: "couldn't reach your blyg".into(),
        }),
        Err(CoreError::Rejected {
            message, details, ..
        }) => Ok(Outcome::Failed {
            kind: FailKind::Refused,
            reason: with_details(message, &details),
        }),
        Err(e) => Ok(Outcome::Failed {
            kind: FailKind::Refused,
            reason: e.to_string(),
        }),
    }
}

fn with_details(message: String, details: &[String]) -> String {
    if details.is_empty() {
        message
    } else {
        format!("{message} ({})", details.join("; "))
    }
}

/// Sleep `d`, waking early when `cancel` is set. False when cancelled.
fn nap(d: Duration, cancel: &AtomicBool) -> bool {
    let until = Instant::now() + d;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return false;
        }
        let now = Instant::now();
        if now >= until {
            return true;
        }
        std::thread::sleep((until - now).min(Duration::from_millis(100)));
    }
}

/// Subscribe to `feeds` (blocking; run it off the UI thread). Each one is
/// `POST /api/subscriptions {url, confirm:true}`: 201 is added and filed in
/// the [`IMPORT_FOLDER`] Reader folder, 409 already followed (left where
/// the user filed it), 422 not a feed, anything else failed with its
/// reason; nothing fails the whole run. A 429 holds every worker for its
/// `Retry-After`, then the same feed is tried again. Setting `cancel` stops
/// it after the ones in flight; their results stand. One result per feed,
/// `None` for the ones never tried.
pub fn run_import(
    backend: &dyn Backend,
    feeds: &[OpmlFeed],
    pace: &Pace,
    cancel: &AtomicBool,
    on: &(dyn Fn(Progress) + Sync),
) -> Vec<Option<Outcome>> {
    let folder = if feeds.is_empty() {
        None
    } else {
        import_folder(backend).ok()
    };
    run_paced(
        feeds,
        pace,
        cancel,
        &|url| backend.subscribe(url, None),
        &|id| {
            if let Some(f) = &folder {
                // Filed by id: membership is local and keyed to the id, so
                // a subscription that only shows up at the next pull is
                // already in the folder when it does.
                let _ = backend.set_subscription_folder(id, Some(&f.id));
            }
        },
        on,
    )
}

/// [`run_import`]'s loop, over a `subscribe` call and a `file` call (an
/// added subscription's id).
pub(crate) fn run_paced(
    feeds: &[OpmlFeed],
    pace: &Pace,
    cancel: &AtomicBool,
    subscribe: &(dyn Fn(&str) -> crate::Result<Subscription> + Sync),
    file: &(dyn Fn(&str) + Sync),
    on: &(dyn Fn(Progress) + Sync),
) -> Vec<Option<Outcome>> {
    let results: Mutex<Vec<Option<Outcome>>> = Mutex::new(vec![None; feeds.len()]);
    let next = Mutex::new(0usize);
    // When the next subscribe may start (pacing and 429s).
    let gate = Mutex::new(Instant::now());
    let lock = |m: &Mutex<Instant>| *m.lock().unwrap_or_else(|p| p.into_inner());
    let worker = || {
        loop {
            let index = {
                let mut n = next.lock().unwrap_or_else(|p| p.into_inner());
                if *n >= feeds.len() || cancel.load(Ordering::Relaxed) {
                    return;
                }
                *n += 1;
                *n - 1
            };
            let feed = &feeds[index];
            let mut waits = 0;
            let outcome = loop {
                // Take the next start slot.
                let start = {
                    let mut g = gate.lock().unwrap_or_else(|p| p.into_inner());
                    let start = (*g).max(Instant::now());
                    *g = start + pace.interval;
                    start
                };
                if !nap(start.saturating_duration_since(Instant::now()), cancel) {
                    return;
                }
                match outcome_of(subscribe(&feed.xml_url)) {
                    Ok(o) => break o,
                    Err(secs) if waits < pace.max_waits => {
                        waits += 1;
                        let until = Instant::now() + Duration::from_secs(secs);
                        {
                            let mut g = gate.lock().unwrap_or_else(|p| p.into_inner());
                            *g = (*g).max(until);
                        }
                        on(Progress::Waiting { seconds: secs });
                        if !nap(
                            lock(&gate).saturating_duration_since(Instant::now()),
                            cancel,
                        ) {
                            return;
                        }
                    }
                    Err(_) => {
                        break Outcome::Failed {
                            kind: FailKind::Refused,
                            reason: "the blyg kept asking Burrow to slow down".into(),
                        };
                    }
                }
            };
            if let Outcome::Added { id, .. } = &outcome {
                file(id);
            }
            results.lock().unwrap_or_else(|p| p.into_inner())[index] = Some(outcome.clone());
            on(Progress::Done { index, outcome });
        }
    };
    if pace.concurrency <= 1 {
        // One at a time runs on the caller's thread (no thread of its own).
        worker();
    } else {
        std::thread::scope(|s| {
            for _ in 0..pace.concurrency {
                s.spawn(worker);
            }
        });
    }
    results.into_inner().unwrap_or_else(|p| p.into_inner())
}

/// The counts of a finished (or cancelled) run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    pub added: usize,
    pub already: usize,
    /// `(feed index, kind, reason)`.
    pub failed: Vec<(usize, FailKind, String)>,
    /// Never tried (cancelled first).
    pub skipped: usize,
}

impl Summary {
    pub fn of(results: &[Option<Outcome>]) -> Summary {
        let mut s = Summary::default();
        for (i, r) in results.iter().enumerate() {
            match r {
                Some(Outcome::Added { .. }) => s.added += 1,
                Some(Outcome::AlreadyFollowing) => s.already += 1,
                Some(Outcome::Failed { kind, reason }) => s.failed.push((i, *kind, reason.clone())),
                None => s.skipped += 1,
            }
        }
        s
    }

    /// "3 added, 1 already followed, 1 failed".
    pub fn line(&self) -> String {
        let mut parts = vec![
            format!("{} added", self.added),
            format!("{} already followed", self.already),
            format!("{} failed", self.failed.len()),
        ];
        if self.skipped > 0 {
            parts.push(format!("{} not tried", self.skipped));
        }
        parts.join(", ")
    }
}

#[cfg(test)]
mod tests;
