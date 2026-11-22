//! --- agentic feed --- The Agentic feed's edition (blygger-studio 0.69.0,
//! contract 6.2.0; blygger-spec `docs/agentic-feed-plan.md`): a digest an
//! assistant writes of what the reader already holds, in sections, which
//! Reading's "Agentic feed" tab draws.
//!
//! - `GET /api/agentic` → `{feeds: [{id, title, revision, updatedAt}]}`: one
//!   feed, [`DEFAULT_FEED`], until the studio offers more. Its title is the
//!   `agenticTitle` setting, else [`DEFAULT_TITLE`].
//! - `GET /api/agentic/{feed}` → `{revision, updatedAt, mode, edition,
//!   layout, draft}`. `layout` is what the tab draws: sections whose entries
//!   are references (`sub`, `remoteId`; `~own` for your own posts) resolved
//!   to their reading entries, the assistant's words beside each. A
//!   reference that no longer resolves is left out by the studio, and a
//!   section left empty is dropped. `null` before the first edition.
//! - `PUT /api/agentic/{feed}` is the assistant's, not Burrow's: Burrow
//!   reads editions and never writes one.
//! - The feed list follows [`LIST_OPERATION`] ([`serves_feeds`]), an
//!   edition [`GET_OPERATION`] ([`serves`]), in `GET /api/capabilities`;
//!   never judged by the contract version.
//! - An edition is an outside assistant's writing, so [`parse_edition`]
//!   bounds it: each text clipped ([`SHORT_MAX`], [`TEXT_MAX`]), at most
//!   [`SECTIONS_MAX`] sections of [`ENTRIES_MAX`] entries.
//!
//! Every word the assistant wrote (headlines, summaries, notes, reasons,
//! the lede) is shown as the assistant's own and disclosed as generated.
//! The entries themselves are the reader's own posts, opened as any other.
//! Shapes and rules only, no I/O: `api::agentic` reads, the app draws.

use serde_json::Value;

use crate::model::{RemoteId, SubId};

/// The capabilities' operation names: listing the feeds, reading one.
pub const LIST_OPERATION: &str = "listAgenticFeeds";
pub const GET_OPERATION: &str = "getAgentic";

/// The default feed's id (studio's `DEFAULT_FEED`).
pub const DEFAULT_FEED: &str = "default";
/// The tab's name when `agenticTitle` is empty.
pub const DEFAULT_TITLE: &str = "Agentic feed";
/// The longest `agenticTitle` the studio keeps.
pub const TITLE_MAX: usize = 40;

/// Whether a `GET /api/capabilities` answer offers editions to read.
pub fn serves(capabilities: &Value) -> bool {
    crate::helpers::lists(capabilities, GET_OPERATION)
}

/// Whether it offers the feed list (`GET /api/agentic`).
pub fn serves_feeds(capabilities: &Value) -> bool {
    crate::helpers::lists(capabilities, LIST_OPERATION)
}

/// A title, name, headline or model's most characters, as read.
pub const SHORT_MAX: usize = 200;
/// A lede, note, summary or reason's most characters, as read.
pub const TEXT_MAX: usize = 1_000;
/// An edition's most sections, as read.
pub const SECTIONS_MAX: usize = 40;
/// A section's most entries, as read.
pub const ENTRIES_MAX: usize = 200;

/// The tab's name from `agenticTitle`: empty restores [`DEFAULT_TITLE`].
pub fn title_or_default(title: &str) -> String {
    match title.trim() {
        "" => DEFAULT_TITLE.to_string(),
        t => t.to_string(),
    }
}

/// One agentic feed, as `GET /api/agentic` lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feed {
    pub id: String,
    /// The tab's label and the heading of its editions.
    pub title: String,
    pub revision: u64,
    pub updated_at: Option<String>,
}

/// `GET /api/agentic`'s feeds. `None` when it isn't that answer.
pub fn parse_feeds(v: &Value) -> Option<Vec<Feed>> {
    let feeds = v.get("feeds")?.as_array()?;
    Some(
        feeds
            .iter()
            .filter_map(|f| {
                Some(Feed {
                    id: f.get("id")?.as_str()?.to_string(),
                    title: f
                        .get("title")
                        .and_then(Value::as_str)
                        .map(title_or_default)
                        .unwrap_or_else(|| DEFAULT_TITLE.into()),
                    revision: f.get("revision").and_then(Value::as_u64).unwrap_or(0),
                    updated_at: str_of(f, "updatedAt"),
                })
            })
            .collect(),
    )
}

/// How the edition was written: laid out in sections, or scored entries
/// the studio laid out as one section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Sections,
    Scores,
}

/// A section's layout hint (studio's `AGENTIC_SECTION_KINDS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SectionKind {
    /// The lead story.
    Lead,
    /// An ordinary section (the default).
    #[default]
    Section,
    /// A column of briefs.
    Briefs,
}

impl SectionKind {
    fn parse(s: Option<&str>) -> SectionKind {
        match s {
            Some("lead") => SectionKind::Lead,
            Some("briefs") => SectionKind::Briefs,
            _ => SectionKind::Section,
        }
    }
}

/// How much of an entry to show (studio's `AGENTIC_PRESENTS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Present {
    /// The whole text.
    Full,
    /// An excerpt (the default).
    #[default]
    Excerpt,
    /// Only its title.
    Title,
    /// Folded under the entry above it.
    Folded,
}

impl Present {
    fn parse(s: Option<&str>) -> Present {
        match s {
            Some("full") => Present::Full,
            Some("title") => Present::Title,
            Some("folded") => Present::Folded,
            _ => Present::Excerpt,
        }
    }
}

/// Who wrote the edition.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Agent {
    /// "resident" for the studio's own assistant, else the outside one's name.
    pub name: String,
    pub model: Option<String>,
}

/// One entry of a section: a post the reader holds, and the assistant's
/// words about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    /// The subscription it's held under, or `~own` for your own post.
    pub sub: SubId,
    pub remote_id: RemoteId,
    /// The version the assistant judged.
    pub version: Option<u32>,
    /// The version the post is at now (its resolved reading entry's).
    pub held_version: Option<u32>,
    pub headline: Option<String>,
    pub summary: Option<String>,
    pub reason: Option<String>,
    pub present: Present,
    /// Entries in a section with the same group show together.
    pub group: Option<String>,
}

impl Placed {
    /// The post as Burrow keys a reading row (`(sub, remote id)`).
    pub fn key(&self) -> (SubId, RemoteId) {
        (self.sub.clone(), self.remote_id.clone())
    }

    /// The post moved past the version the assistant judged.
    pub fn changed_since(&self) -> bool {
        matches!((self.version, self.held_version), (Some(v), Some(h)) if h > v)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub title: String,
    pub kind: SectionKind,
    pub note: Option<String>,
    pub entries: Vec<Placed>,
}

/// What the tab draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub title: Option<String>,
    pub lede: Option<String>,
    pub agent: Agent,
    pub generated_at: Option<String>,
    pub sections: Vec<Section>,
}

/// `GET /api/agentic/{feed}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edition {
    /// 0 for a never-written edition.
    pub revision: u64,
    pub updated_at: Option<String>,
    pub mode: Option<Mode>,
    /// `None` before the first edition.
    pub layout: Option<Layout>,
    /// The edition composed as a private draft thread on the blyg.
    pub draft: Option<String>,
}

impl Edition {
    /// Nothing to draw: no edition yet, or every section dropped.
    pub fn is_empty(&self) -> bool {
        self.layout.as_ref().is_none_or(|l| l.sections.is_empty())
    }

    /// Every entry's key, once each, in the edition's order: what the tab lists.
    pub fn keys(&self) -> Vec<(SubId, RemoteId)> {
        let mut seen = std::collections::HashSet::new();
        let mut out: Vec<(SubId, RemoteId)> = Vec::new();
        for s in self.layout.iter().flat_map(|l| &l.sections) {
            for p in &s.entries {
                let k = p.key();
                if seen.insert(k.clone()) {
                    out.push(k);
                }
            }
        }
        out
    }
}

/// `GET /api/agentic/{feed}`'s answer (or a `PUT`'s). `None` when it isn't one.
pub fn parse_edition(v: &Value) -> Option<Edition> {
    let revision = v.get("revision")?.as_u64()?;
    let mode = match v.get("mode").and_then(Value::as_str) {
        Some("sections") => Some(Mode::Sections),
        Some("scores") => Some(Mode::Scores),
        _ => None,
    };
    let layout = v.get("layout").filter(|l| l.is_object()).map(layout);
    Some(Edition {
        revision,
        updated_at: str_of(v, "updatedAt"),
        mode,
        layout,
        draft: v
            .get("draft")
            .and_then(|d| d.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn layout(l: &Value) -> Layout {
    let agent = l.get("agent");
    Layout {
        title: short(l, "title"),
        lede: text(l, "lede"),
        agent: Agent {
            name: agent
                .and_then(|a| a.get("name"))
                .and_then(Value::as_str)
                .map(|n| clip(n, SHORT_MAX))
                .unwrap_or_default(),
            model: agent.and_then(|a| short(a, "model")),
        },
        generated_at: str_of(l, "generatedAt"),
        sections: l
            .get("sections")
            .and_then(Value::as_array)
            .map(|a| a.iter().take(SECTIONS_MAX).map(section).collect())
            .unwrap_or_default(),
    }
}

fn section(s: &Value) -> Section {
    Section {
        title: short(s, "title").unwrap_or_default(),
        kind: SectionKind::parse(s.get("kind").and_then(Value::as_str)),
        note: text(s, "note"),
        entries: s
            .get("entries")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(placed).take(ENTRIES_MAX).collect())
            .unwrap_or_default(),
    }
}

fn placed(p: &Value) -> Option<Placed> {
    let entry = p.get("entry");
    // The version the entry is at now: an imported entry's, else your own's.
    let held_version = entry.and_then(|e| {
        ["imported", "own"]
            .iter()
            .find_map(|k| e.get(*k)?.get("version")?.as_u64())
    });
    Some(Placed {
        sub: SubId::from(p.get("sub")?.as_str()?),
        remote_id: RemoteId::from(p.get("remoteId")?.as_str()?),
        version: p
            .get("version")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok()),
        held_version: held_version.and_then(|v| u32::try_from(v).ok()),
        headline: short(p, "headline"),
        summary: text(p, "summary"),
        reason: text(p, "reason"),
        present: Present::parse(p.get("present").and_then(Value::as_str)),
        group: str_of(p, "group"),
    })
}

/// A short field, clipped to [`SHORT_MAX`].
fn short(v: &Value, k: &str) -> Option<String> {
    str_of(v, k).map(|s| clip(&s, SHORT_MAX))
}

/// A text field, clipped to [`TEXT_MAX`].
fn text(v: &Value, k: &str) -> Option<String> {
    str_of(v, k).map(|s| clip(&s, TEXT_MAX))
}

/// At most `n` characters, a clipped one ending in an ellipsis.
fn clip(s: &str, n: usize) -> String {
    match s.char_indices().nth(n) {
        Some(_) => {
            let keep: String = s.chars().take(n.saturating_sub(1)).collect();
            format!("{keep}…")
        }
        None => s.to_string(),
    }
}

/// A non-empty string field (null, absent and blank read as `None`).
fn str_of(v: &Value, k: &str) -> Option<String> {
    v.get(k)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
}

/// A card and the entries shown under it (indices into the section's
/// entries).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub lead: usize,
    pub also: Vec<usize>,
}

/// A section's entries with each group gathered under its first entry,
/// and a folded entry joining the card above it (studio's `grouped`).
pub fn grouped(entries: &[Placed]) -> Vec<Group> {
    let mut out: Vec<Group> = Vec::new();
    let mut by_group: Vec<(&str, usize)> = Vec::new();
    for (i, p) in entries.iter().enumerate() {
        let slot = p
            .group
            .as_deref()
            .and_then(|g| by_group.iter().find(|(n, _)| *n == g).map(|(_, s)| *s));
        if let Some(s) = slot {
            out[s].also.push(i);
        } else if p.present == Present::Folded && !out.is_empty() {
            let last = out.len() - 1;
            out[last].also.push(i);
        } else {
            out.push(Group {
                lead: i,
                also: Vec::new(),
            });
            if let Some(g) = p.group.as_deref() {
                by_group.push((g, out.len() - 1));
            }
        }
    }
    out
}
