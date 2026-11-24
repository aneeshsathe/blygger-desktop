//! --- agentic writer --- Burrow's own AI writes the blyg's Agentic feed
//! edition (`PUT /api/agentic/{feed}`, contract 6.2.0's `putAgentic`). The
//! studio has no assistant of its own that writes one yet, so without this
//! (or an outside assistant with a token) the tab says "No edition yet"
//! forever.
//!
//! - [`candidates`]: what the model is given: others' posts you hold that
//!   are unread or arrived since the last edition, newest first, at most
//!   [`CANDIDATES_MAX`]. Never your own, never a withdrawn or Background one.
//! - The model names each post by the number it was given (`"ref": 3`), so
//!   it can't garble an id; [`from_reply`] maps the numbers back, drops any
//!   it wasn't given (and a post named twice), clamps every field to the
//!   contract's bounds (AgenticEditionInput; the studio's `AGENTIC_LIMITS`,
//!   counted in UTF-16 units as the studio counts) and wants at least one
//!   section left. Unusable output is a [`ReplyError`]; nothing is sent.
//! - [`send`]: read the revision, write from it; a 409 is read again and
//!   written once more ([`send_with`] is that rule over two closures).
//! - [`due`] / [`has_new`]: after a sync it writes only when the gap since
//!   the last edition passed and posts arrived since.
//!
//! The prompt is blyg-ai's (`prompts::agentic_edition_prompt`); the app
//! runs the model off the UI thread and calls these.

use serde_json::{Map, Value, json};

use crate::agentic::{Edition, Present, SectionKind};
use crate::backend::{Backend, Result};
use crate::model::{ReadingItem, RemoteId, SubId};

/// The most posts the model is given.
pub const CANDIDATES_MAX: usize = 60;
/// A post's opening words, in characters.
pub const EXCERPT_MAX: usize = 280;

// The contract's bounds (AgenticEditionInput, AgenticSection, AgenticEntry;
// the studio's `AGENTIC_LIMITS`), in UTF-16 units.
pub const TITLE_MAX: usize = 200;
pub const LEDE_MAX: usize = 4_000;
pub const AGENT_NAME_MAX: usize = 80;
pub const MODEL_MAX: usize = 120;
pub const SECTIONS_MAX: usize = 20;
pub const ENTRIES_MAX: usize = 50;
pub const HEADLINE_MAX: usize = 200;
pub const SUMMARY_MAX: usize = 2_000;
pub const REASON_MAX: usize = 500;
pub const NOTE_MAX: usize = 4_000;
pub const GROUP_MAX: usize = 60;
pub const SUB_MAX: usize = 128;
pub const REMOTE_ID_MAX: usize = 256;

/// One post the model is given, numbered by its place in the list (from 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub sub: SubId,
    pub remote_id: RemoteId,
    pub version: u32,
    /// The subscription's name.
    pub source: String,
    pub title: Option<String>,
    /// Its opening words after the title, on one line.
    pub excerpt: String,
    /// The post's own date (the reading list's sort key).
    pub at: String,
    /// When your blyg first held it.
    pub observed_at: String,
}

/// A timestamp as unix ms (`None` when it doesn't parse).
pub fn ms(s: &str) -> Option<i64> {
    crate::util::parse_ts_ms(s)
}

fn after(observed: &str, since: Option<i64>) -> bool {
    match (since, ms(observed)) {
        (None, _) => false,
        (Some(s), Some(o)) => o > s,
        (Some(_), None) => false,
    }
}

/// What the model is given, newest first.
pub fn candidates(rows: &[ReadingItem], since: Option<&str>) -> Vec<Candidate> {
    let since = since.and_then(ms);
    let mut picked: Vec<&ReadingItem> = rows
        .iter()
        .filter(|r| !crate::own::is_own_row(r) && r.state != "tombstone" && !r.is_background())
        .filter(|r| r.is_unread() || after(&r.observed_at, since))
        .collect();
    picked.sort_by_key(|r| std::cmp::Reverse(r.sort_at()));
    picked
        .into_iter()
        .take(CANDIDATES_MAX)
        .map(|r| {
            let title = crate::model::plain_title(&r.content_md).filter(|t| !t.is_empty());
            let all = crate::model::plain_text(&r.content_md);
            let words = match &title {
                Some(t) => all.strip_prefix(t.as_str()).unwrap_or(&all).trim(),
                None => all.trim(),
            };
            Candidate {
                sub: r.subscription_id.clone(),
                remote_id: r.remote_id.clone(),
                version: r.version,
                source: r.subscription_title.clone(),
                title,
                excerpt: crate::text::clip(words, EXCERPT_MAX).into_owned(),
                at: r.sort_at(),
                observed_at: r.observed_at.clone(),
            }
        })
        .collect()
}

/// Whether any post arrived since the last edition (`None`: never one).
pub fn has_new(cands: &[Candidate], since: Option<&str>) -> bool {
    match since.and_then(ms) {
        None => !cands.is_empty(),
        Some(s) => cands.iter().any(|c| after(&c.observed_at, Some(s))),
    }
}

/// Whether the gap since the last edition (`None`: never one) has passed.
pub fn due(since: Option<&str>, now_ms: i64, gap_ms: i64) -> bool {
    since.and_then(ms).is_none_or(|s| now_ms - s >= gap_ms)
}

/// When an edition was last written: its `generatedAt`, else `updatedAt`.
pub fn last_written(e: &Edition) -> Option<String> {
    e.layout
        .as_ref()
        .and_then(|l| l.generated_at.clone())
        .or_else(|| e.updated_at.clone())
        .filter(|_| e.revision > 0)
}

/// Who wrote it (AgenticEditionInput's `agent`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentInput {
    pub name: String,
    pub model: Option<String>,
}

/// One entry (AgenticEntry).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryInput {
    pub sub: SubId,
    pub remote_id: RemoteId,
    pub version: Option<u32>,
    pub headline: Option<String>,
    pub summary: Option<String>,
    pub reason: Option<String>,
    pub present: Option<Present>,
    pub group: Option<String>,
}

/// One section (AgenticSection).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionInput {
    pub title: String,
    pub kind: Option<SectionKind>,
    pub note: Option<String>,
    pub entries: Vec<EntryInput>,
}

/// An edition in sections mode (AgenticEditionInput; Burrow never sends
/// `scores`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditionInput {
    pub title: Option<String>,
    pub lede: Option<String>,
    pub agent: AgentInput,
    pub generated_at: Option<String>,
    pub sections: Vec<SectionInput>,
}

impl EditionInput {
    /// Entries across every section.
    pub fn entry_count(&self) -> usize {
        self.sections.iter().map(|s| s.entries.len()).sum()
    }

    /// The AgenticEditionInput: absent fields left out (the studio refuses
    /// unknown keys and empty strings).
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        put(&mut m, "title", &self.title);
        put(&mut m, "lede", &self.lede);
        let mut agent = Map::new();
        agent.insert("name".into(), self.agent.name.clone().into());
        put(&mut agent, "model", &self.agent.model);
        m.insert("agent".into(), Value::Object(agent));
        put(&mut m, "generatedAt", &self.generated_at);
        let sections: Vec<Value> = self
            .sections
            .iter()
            .map(|s| {
                let mut o = Map::new();
                o.insert("title".into(), s.title.clone().into());
                if let Some(k) = s.kind {
                    o.insert("kind".into(), k.wire().into());
                }
                put(&mut o, "note", &s.note);
                let entries: Vec<Value> = s
                    .entries
                    .iter()
                    .map(|p| {
                        let mut e = Map::new();
                        e.insert("sub".into(), p.sub.as_str().into());
                        e.insert("remoteId".into(), p.remote_id.as_str().into());
                        if let Some(v) = p.version.filter(|v| *v > 0) {
                            e.insert("version".into(), v.into());
                        }
                        put(&mut e, "headline", &p.headline);
                        put(&mut e, "summary", &p.summary);
                        put(&mut e, "reason", &p.reason);
                        if let Some(pr) = p.present {
                            e.insert("present".into(), pr.wire().into());
                        }
                        put(&mut e, "group", &p.group);
                        Value::Object(e)
                    })
                    .collect();
                o.insert("entries".into(), entries.into());
                Value::Object(o)
            })
            .collect();
        m.insert("sections".into(), sections.into());
        Value::Object(m)
    }
}

fn put(m: &mut Map<String, Value>, k: &str, v: &Option<String>) {
    if let Some(v) = v {
        m.insert(k.into(), v.clone().into());
    }
}

/// `PUT /api/agentic/{feed}`'s body.
pub fn put_body(edition: &EditionInput, expected_revision: u64) -> Value {
    json!({ "edition": edition.to_json(), "expectedRevision": expected_revision })
}

/// Why a model's reply can't be sent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReplyError {
    #[error("the model's reply wasn't an edition (no JSON object in it)")]
    NotJson,
    #[error("the model's edition named none of the posts it was given")]
    NoSections,
}

/// At most `max` UTF-16 units, a clipped one ending in "…"; trimmed, and
/// `None` when nothing is left.
fn clamp(s: &str, max: usize) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if s.encode_utf16().count() <= max {
        return Some(s.to_string());
    }
    let mut out = String::new();
    let mut units = 0;
    for c in s.chars() {
        if units + c.len_utf16() > max.saturating_sub(1) {
            break;
        }
        units += c.len_utf16();
        out.push(c);
    }
    let out = out.trim_end().to_string();
    Some(format!("{out}…"))
}

fn text(v: &Value, k: &str, max: usize) -> Option<String> {
    v.get(k).and_then(Value::as_str).and_then(|s| clamp(s, max))
}

/// The JSON object in a reply: the whole of it, else between its first
/// `{` and last `}` (a model's prose or code fence around it).
fn object_in(reply: &str) -> Option<Value> {
    let r = reply.trim();
    if let Ok(v @ Value::Object(_)) = serde_json::from_str::<Value>(r) {
        return Some(v);
    }
    let (a, b) = (r.find('{')?, r.rfind('}')?);
    if b < a {
        return None;
    }
    match serde_json::from_str::<Value>(&r[a..=b]) {
        Ok(v @ Value::Object(_)) => Some(v),
        _ => None,
    }
}

/// A post's number (from 1), as a number or a string of one.
fn ref_of(p: &Value) -> Option<usize> {
    let r = p.get("ref")?;
    r.as_u64()
        .map(|n| n as usize)
        .or_else(|| r.as_str().and_then(|s| s.trim().parse().ok()))
}

/// A model's reply as the edition to send (see the module's notes).
pub fn from_reply(
    reply: &str,
    given: &[Candidate],
    agent: AgentInput,
    generated_at: &str,
) -> std::result::Result<EditionInput, ReplyError> {
    let v = object_in(reply).ok_or(ReplyError::NotJson)?;
    let mut placed = std::collections::HashSet::new();
    let mut sections = Vec::new();
    for (i, s) in v
        .get("sections")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        if sections.len() == SECTIONS_MAX {
            break;
        }
        let mut entries = Vec::new();
        for p in s
            .get("entries")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if entries.len() == ENTRIES_MAX {
                break;
            }
            let Some(c) = ref_of(p)
                .and_then(|n| n.checked_sub(1))
                .and_then(|n| given.get(n))
            else {
                continue;
            };
            let fits = c.sub.encode_utf16().count() <= SUB_MAX
                && c.remote_id.encode_utf16().count() <= REMOTE_ID_MAX;
            if !fits || !placed.insert((c.sub.clone(), c.remote_id.clone())) {
                continue;
            }
            entries.push(EntryInput {
                sub: c.sub.clone(),
                remote_id: c.remote_id.clone(),
                version: Some(c.version).filter(|v| *v > 0),
                headline: text(p, "headline", HEADLINE_MAX),
                summary: text(p, "summary", SUMMARY_MAX),
                reason: text(p, "reason", REASON_MAX),
                present: p
                    .get("present")
                    .and_then(Value::as_str)
                    .and_then(Present::from_wire),
                group: text(p, "group", GROUP_MAX),
            });
        }
        if entries.is_empty() {
            continue;
        }
        sections.push(SectionInput {
            title: text(s, "title", TITLE_MAX).unwrap_or_else(|| format!("Section {}", i + 1)),
            kind: s
                .get("kind")
                .and_then(Value::as_str)
                .and_then(SectionKind::from_wire),
            note: text(s, "note", NOTE_MAX),
            entries,
        });
    }
    if sections.is_empty() {
        return Err(ReplyError::NoSections);
    }
    Ok(EditionInput {
        title: text(&v, "title", TITLE_MAX),
        lede: text(&v, "lede", LEDE_MAX),
        agent: AgentInput {
            name: clamp(&agent.name, AGENT_NAME_MAX).unwrap_or_else(|| "Burrow".into()),
            model: agent.model.as_deref().and_then(|m| clamp(m, MODEL_MAX)),
        },
        generated_at: Some(generated_at.to_string()),
        sections,
    })
}

/// What one `PUT /api/agentic/{feed}` came to.
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(
    clippy::large_enum_variant,
    reason = "one per write, at most a few an hour; a Box would only add a step to every match"
)]
pub enum Put {
    /// Stored: the edition as the blyg lays it out now.
    Written(Edition),
    /// 409 `revision-conflict`: it moved since it was read. Nothing stored.
    Conflict,
    /// The blyg doesn't let this sign-in write one (no `putAgentic`).
    NotOffered,
}

/// What sending came to.
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(
    clippy::large_enum_variant,
    reason = "one per write, at most a few an hour; a Box would only add a step to every match"
)]
pub enum Sent {
    Written(Edition),
    /// It moved under the write twice: given up, nothing stored.
    Conflicted,
    /// No such feed, or this sign-in may not write one.
    NotOffered,
}

/// Read the revision (`None`: not offered), write from it; on a conflict
/// read again and write once more.
pub fn send_with(
    mut read_revision: impl FnMut() -> Result<Option<u64>>,
    mut put: impl FnMut(u64) -> Result<Put>,
) -> Result<Sent> {
    for _ in 0..2 {
        let Some(rev) = read_revision()? else {
            return Ok(Sent::NotOffered);
        };
        match put(rev)? {
            Put::Written(e) => return Ok(Sent::Written(e)),
            Put::NotOffered => return Ok(Sent::NotOffered),
            Put::Conflict => {}
        }
    }
    Ok(Sent::Conflicted)
}

/// Send `edition` to `feed` through the backend.
pub fn send(backend: &dyn Backend, feed: &str, edition: &EditionInput) -> Result<Sent> {
    send_with(
        || Ok(backend.agentic_edition(feed)?.map(|e| e.revision)),
        |rev| backend.put_agentic_edition(feed, rev, edition),
    )
}
