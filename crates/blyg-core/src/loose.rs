//! Loose ends (docs/SPEC.md § Loose ends): the omnibar's operators.
//!
//! `is:orphan`, `is:draft`, `is:leaf`, `is:stale`, `has:tk`,
//! `links:<id|host>`, `quotes:<id|host>`, `kind:thread|fragment` and
//! `v:<n>` are read off the query before the store sees it: the operators
//! filter, the remaining words search as they always have. A word that
//! looks like an operator but isn't one (`is:blue`) is searched as text.
//!
//! Everything here describes the author's own text (rule 1 allows that, as
//! reading time does): which posts nothing points at, which still hold an
//! ungenerated `[TK]`. Nothing here is a social count.

use std::collections::{HashMap, HashSet};

use crate::model::{Item, Kind, Status, post_key};

/// A draft untouched this long is stale (`is:stale`).
pub const STALE_MS: i64 = 30 * 24 * 3600 * 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// Published, and nothing points at it: no held post quotes, stubs or
    /// forks it, no verified mention, no other post of yours links or
    /// quotes it.
    Orphan,
    Draft,
    /// No outbound directive: no `![[id]]`, no stub, no fork.
    Leaf,
    /// A draft untouched for [`STALE_MS`].
    Stale,
    /// An ungenerated `[TK]` scope.
    HasTk,
    /// An inline `[[id]]` link (or a web link) to an id prefix or a host.
    Links(Target),
    /// A `![[id]]` quote (or a stub / fork) of an id prefix or a host.
    Quotes(Target),
    Kind(Kind),
    /// Last published as version `n` (`v:0`: never published).
    Version(u32),
}

/// What `links:` and `quotes:` name: an id (or its first characters, as
/// a list shows them) or a host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Id(String),
    Host(String),
}

impl Target {
    fn parse(v: &str) -> Option<Self> {
        let v = v.trim().trim_end_matches('/');
        if v.is_empty() {
            return None;
        }
        if v.contains('.') || v.contains("://") {
            return host_of(v).map(Target::Host);
        }
        v.chars()
            .all(|c| c.is_ascii_alphanumeric())
            .then(|| Target::Id(v.to_ascii_lowercase()))
    }

    fn id_matches(&self, id: &str) -> bool {
        matches!(self, Target::Id(p) if id.to_ascii_lowercase().starts_with(p.as_str()))
    }

    fn host_matches(&self, url_or_host: &str) -> bool {
        match (self, host_of(url_or_host)) {
            (Target::Host(h), Some(o)) => o == *h || o.ends_with(&format!(".{h}")),
            _ => false,
        }
    }
}

/// `https://www.Lin.example.org/x` → `lin.example.org`; a bare host as is.
fn host_of(s: &str) -> Option<String> {
    let s = s.trim();
    let rest = s.split_once("://").map_or(s, |(_, r)| r);
    let host = rest
        .split(['/', '?', '#'])
        .next()?
        .rsplit('@')
        .next()?
        .split(':')
        .next()?
        .to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    (!host.is_empty()).then_some(host)
}

/// A query read for operators: `ops` filter, `text` searches.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    pub ops: Vec<Op>,
    pub text: String,
}

impl Query {
    pub fn parse(q: &str) -> Self {
        // The common case, a query without a colon, costs one scan.
        if !q.contains(':') {
            return Query {
                ops: vec![],
                text: q.to_string(),
            };
        }
        let mut ops = Vec::new();
        let mut words = Vec::new();
        for w in q.split_whitespace() {
            match parse_op(w) {
                Some(op) => ops.push(op),
                None => words.push(w),
            }
        }
        if ops.is_empty() {
            return Query {
                ops,
                text: q.to_string(),
            };
        }
        Query {
            ops,
            text: words.join(" "),
        }
    }

    /// Only operators, no words: nothing to start a post with, so ⏎
    /// never creates one.
    pub fn is_operator_only(&self) -> bool {
        !self.ops.is_empty() && self.text.trim().is_empty()
    }

    fn needs_facts(&self) -> bool {
        self.ops
            .iter()
            .any(|o| matches!(o, Op::Orphan | Op::Quotes(Target::Host(_)) | Op::Links(_)))
    }

    fn needs_all(&self) -> bool {
        self.ops.contains(&Op::Orphan)
    }
}

/// Whether `q` is operators only (the omnibar's ⏎ creates nothing then).
pub fn is_operator_only(q: &str) -> bool {
    Query::parse(q).is_operator_only()
}

/// The words of `q` that search (for highlighting).
pub fn search_words(q: &str) -> String {
    Query::parse(q).text
}

fn parse_op(w: &str) -> Option<Op> {
    let (k, v) = w.split_once(':')?;
    let v_l = v.to_ascii_lowercase();
    match (k.to_ascii_lowercase().as_str(), v_l.as_str()) {
        ("is", "orphan") => Some(Op::Orphan),
        ("is", "draft") => Some(Op::Draft),
        ("is", "leaf") => Some(Op::Leaf),
        ("is", "stale") => Some(Op::Stale),
        ("has", "tk") => Some(Op::HasTk),
        ("kind", "thread") => Some(Op::Kind(Kind::Thread)),
        ("kind", "fragment") => Some(Op::Kind(Kind::Fragment)),
        ("v", n) => n.parse().ok().map(Op::Version),
        ("links", _) => Target::parse(v).map(Op::Links),
        ("quotes", _) => Target::parse(v).map(Op::Quotes),
        _ => None,
    }
}

/// What the store knows beyond the posts themselves.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    /// Ids (lowercase) of your posts that a held post quotes, stubs or
    /// forks, or that a verified mention names.
    pub inbound: HashSet<String>,
    /// Held posts' ids (lowercase) → their origin, so a `![[id]]` can be
    /// matched by host.
    pub held_origins: HashMap<String, String>,
}

impl Facts {
    /// The facts `reading` gives, for a blyg at `own_origin` (the
    /// default `Backend::loose_facts`, and the fake's).
    pub fn from_reading(own_origin: Option<&str>, reading: &[crate::model::ReadingItem]) -> Self {
        let own = own_origin.map(|o| post_key(o, "").0);
        let mut f = Facts::default();
        for r in reading {
            f.held_origins
                .insert(r.remote_id.to_ascii_lowercase(), r.origin.clone());
            for p in r.references() {
                if own.as_deref() == Some(p.origin.as_str()) {
                    f.inbound.insert(p.id);
                }
            }
        }
        f
    }
}

/// Run `q`: the operators filter what `plain` finds for the remaining
/// words. Without operators this is `plain(q)` and nothing more.
pub fn search(
    q: &str,
    plain: impl FnOnce(&str) -> Vec<Item>,
    all: impl FnOnce() -> Vec<Item>,
    facts: impl FnOnce() -> Facts,
) -> Vec<Item> {
    let query = Query::parse(q);
    if query.ops.is_empty() {
        return plain(q);
    }
    let hits = plain(&query.text);
    let facts = if query.needs_facts() {
        facts()
    } else {
        Facts::default()
    };
    let linked = if query.needs_all() {
        own_linked(&all())
    } else {
        HashSet::new()
    };
    let now = crate::util::now_ms();
    hits.into_iter()
        .filter(|i| {
            query
                .ops
                .iter()
                .all(|op| matches(op, i, &facts, &linked, now))
        })
        .collect()
}

/// Ids (lowercase) your own posts point at: quotes, links, stubs, forks.
fn own_linked(items: &[Item]) -> HashSet<String> {
    let mut out = HashSet::new();
    for it in items {
        for id in crate::profile::transclusion_ids(&it.content_md) {
            out.insert(id.to_ascii_lowercase());
        }
        for r in crate::grammar::link_refs(&it.content_md) {
            out.insert(r.id.to_ascii_lowercase());
        }
        if let Some(s) = &it.stub_of {
            out.insert(s.id.to_ascii_lowercase());
        }
        if let Some(f) = &it.forked_from {
            out.insert(f.id.to_ascii_lowercase());
        }
    }
    out
}

fn is_published(i: &Item) -> bool {
    i.status == Status::Public && i.version > 0
}

fn ungenerated_tk(i: &Item) -> bool {
    i.content_md.contains("[TK]")
        && crate::tk::parse(&i.content_md).is_some_and(|s| s.iter().any(|s| s.output.is_none()))
}

fn stale(i: &Item, now: i64) -> bool {
    i.status == Status::Draft
        && crate::util::parse_ts_ms(&i.updated).is_some_and(|t| now - t >= STALE_MS)
}

fn matches(op: &Op, i: &Item, facts: &Facts, linked: &HashSet<String>, now: i64) -> bool {
    match op {
        Op::Orphan => {
            let Some(sid) = i.server_id.as_ref().map(|s| s.0.to_ascii_lowercase()) else {
                return false;
            };
            is_published(i) && !facts.inbound.contains(&sid) && !linked.contains(&sid)
        }
        Op::Draft => i.status == Status::Draft,
        Op::Leaf => {
            crate::profile::transclusion_ids(&i.content_md).is_empty()
                && i.stub_of.is_none()
                && i.forked_from.is_none()
        }
        Op::Stale => stale(i, now),
        Op::HasTk => ungenerated_tk(i),
        Op::Kind(k) => i.kind == *k,
        Op::Version(n) => i.version == *n,
        Op::Links(t) => {
            let md = &i.content_md;
            let by_id = crate::grammar::link_refs(md).into_iter().any(|r| {
                // A link, not the `[[…]]` of a `![[…]]` quote.
                let quote = r.range.start > 0 && md.as_bytes()[r.range.start - 1] == b'!';
                !quote
                    && (t.id_matches(&r.id)
                        || facts
                            .held_origins
                            .get(&r.id.to_ascii_lowercase())
                            .is_some_and(|o| t.host_matches(o)))
            });
            by_id || web_links(md).any(|u| t.host_matches(u))
        }
        Op::Quotes(t) => {
            let quoted = crate::profile::transclusion_ids(&i.content_md)
                .into_iter()
                .any(|id| {
                    t.id_matches(&id)
                        || facts
                            .held_origins
                            .get(&id.to_ascii_lowercase())
                            .is_some_and(|o| t.host_matches(o))
                });
            let stub = i
                .stub_of
                .as_ref()
                .is_some_and(|s| t.id_matches(&s.id) || t.host_matches(&s.origin));
            let fork = i
                .forked_from
                .as_ref()
                .is_some_and(|f| t.id_matches(&f.id) || t.host_matches(&f.origin));
            quoted || stub || fork
        }
    }
}

/// The `http(s)://…` addresses written in `md`.
fn web_links(md: &str) -> impl Iterator<Item = &str> {
    let mut at = 0;
    std::iter::from_fn(move || {
        let rest = &md[at..];
        let i = rest.find("http")?;
        let start = at + i;
        let tail = &md[start..];
        let end = tail
            .find(|c: char| c.is_whitespace() || matches!(c, ')' | ']' | '>' | '"' | '<'))
            .unwrap_or(tail.len());
        at = start + end.max(4);
        let u = &tail[..end];
        Some(if u.starts_with("http://") || u.starts_with("https://") {
            u
        } else {
            ""
        })
    })
    .filter(|u| !u.is_empty())
}

/// The loose ends among your posts, in words (never a social count).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Summary {
    pub orphans: usize,
    pub ungenerated_tk: usize,
    pub stale_drafts: usize,
}

impl Summary {
    pub fn of(items: &[Item], facts: &Facts) -> Self {
        let linked = own_linked(items);
        let now = crate::util::now_ms();
        let mut s = Summary::default();
        for i in items {
            if matches(&Op::Orphan, i, facts, &linked, now) {
                s.orphans += 1;
            }
            if ungenerated_tk(i) {
                s.ungenerated_tk += 1;
            }
            if stale(i, now) {
                s.stale_drafts += 1;
            }
        }
        s
    }

    /// The parts that apply, each with the query it stands for:
    /// `("2 orphans", "is:orphan")`.
    pub fn parts(&self) -> Vec<(String, &'static str)> {
        let plural =
            |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        let mut out = Vec::new();
        if self.orphans > 0 {
            out.push((plural(self.orphans, "orphan", "orphans"), "is:orphan"));
        }
        if self.ungenerated_tk > 0 {
            out.push((
                plural(self.ungenerated_tk, "ungenerated TK", "ungenerated TKs"),
                "has:tk",
            ));
        }
        if self.stale_drafts > 0 {
            out.push((
                plural(
                    self.stale_drafts,
                    "draft older than a month",
                    "drafts older than a month",
                ),
                "is:stale",
            ));
        }
        out
    }

    /// `"2 orphans · 1 ungenerated TK"`; empty when nothing is loose.
    pub fn words(&self) -> String {
        self.parts()
            .into_iter()
            .map(|(w, _)| w)
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

#[cfg(test)]
#[path = "loose_tests.rs"]
mod tests;
