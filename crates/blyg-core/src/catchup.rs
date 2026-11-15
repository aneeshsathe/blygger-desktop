//! Catch up: the conversation that leads to a post, unrolled oldest first,
//! the order to read it in.
//!
//! From the post, walk up its lineage ([`Local::up`]: quotes, stubs and
//! forks) to every ancestor, at most [`MAX_HOPS`] hops and [`MAX_POSTS`]
//! posts, each post once (a cycle stops where it closes). With "whole
//! conversation" on, held posts that draw on any ancestor ([`Local::down`],
//! one hop) join as side branches, so side replies show up too. Then
//! oldest first by the post's own date, ties deepest ancestor first, and
//! the starting post last.
//!
//! Each entry carries its own words: its Markdown with every own-line
//! `![[id…]]` quote turned into a one-line marker ("↪ quotes Ada"), since
//! the quoted post is its own, earlier entry. Inline `[[id]]` links stay.
//! A post this Mac doesn't hold shows what the quoting post cited of it,
//! else nothing; nothing is ever fetched for it. Pure: the caller says
//! what's held.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::lineage::{Local, Peer, Via};
use crate::model::{Cited, Kind, ReadingItem, post_key};
use crate::profile::Relation;

/// At most this many hops up from the starting post.
pub const MAX_HOPS: usize = 8;
/// At most this many posts in all, the starting post included.
pub const MAX_POSTS: usize = 60;

/// What an entry shows as its words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Words {
    /// Held here: its Markdown, quotes turned into markers.
    Own(String),
    /// Not held: the excerpt the post that quotes it cited.
    Cited(String),
    /// Not held, and nothing was cited.
    NotHeld,
    /// A `{url}` stub or a bare id: not a post with words.
    None,
}

/// How an entry joins the conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub relation: Relation,
    pub partial: bool,
    /// The other end: for an ancestor, the post that draws on it (its
    /// child); for a side branch, the ancestor it draws on.
    pub who: String,
}

/// Something an entry draws on, within the conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawsOn {
    pub who: String,
    pub relation: Relation,
    pub partial: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// The post (or page) it is.
    pub peer: Peer,
    /// Its author or blyg, as well as this Mac can say.
    pub who: String,
    /// Its own date (published, else updated, else when it was imported),
    /// in ms; `None` when not held.
    pub at: Option<i64>,
    /// Hops up from the starting post (0 for it). A side branch takes the
    /// depth of the ancestor's child: one less than the ancestor's.
    pub depth: usize,
    /// The post as held here.
    pub held: Option<ReadingItem>,
    pub words: Words,
    /// How it joins; `None` on the starting post.
    pub link: Option<Link>,
    /// What it draws on that's in the conversation, in the order it names them.
    pub draws_on: Vec<DrawsOn>,
    /// Added by "whole conversation": draws on an ancestor, but doesn't lead
    /// to the starting post.
    pub side: bool,
    /// The starting post.
    pub start: bool,
}

impl Entry {
    /// A post's `(origin, id)`, if it's a post.
    pub fn post(&self) -> Option<(&str, &str)> {
        match &self.peer {
            Peer::Post { origin, id } => Some((origin, id)),
            _ => None,
        }
    }

    /// A `{url}` stub's page.
    pub fn url(&self) -> Option<&str> {
        match &self.peer {
            Peer::Url(u) => Some(u),
            _ => None,
        }
    }

    /// The chip beside the author: how it joins, in words.
    pub fn link_words(&self) -> String {
        match &self.link {
            None => "you're here".into(),
            Some(l) if self.side => format!("{} {}", side_verb(l.relation, l.partial), l.who),
            Some(l) => format!("{} {}", ancestor_verb(l.relation, l.partial), l.who),
        }
    }
}

/// An ancestor, from its child: "quoted by", "replied to by", …
pub fn ancestor_verb(rel: Relation, partial: bool) -> &'static str {
    match (rel, partial) {
        (Relation::Quotes, false) => "quoted by",
        (Relation::Quotes, true) => "a passage quoted by",
        (Relation::Stubs, false) => "replied to by",
        (Relation::Stubs, true) => "a passage replied to by",
        (Relation::Forks, _) => "forked by",
    }
}

/// A post that draws on another: "quotes", "replies to", …
pub fn side_verb(rel: Relation, partial: bool) -> &'static str {
    match (rel, partial) {
        (Relation::Quotes, false) => "quotes",
        (Relation::Quotes, true) => "quotes a passage of",
        (Relation::Stubs, false) => "replies to",
        (Relation::Stubs, true) => "replies to a passage of",
        (Relation::Forks, _) => "forked from",
    }
}

/// The unrolled conversation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Conversation {
    /// Oldest first, the starting post last.
    pub entries: Vec<Entry>,
    /// The caps cut it short.
    pub truncated: bool,
}

impl Conversation {
    /// The starting post's position (always the last).
    pub fn start(&self) -> usize {
        self.entries.len().saturating_sub(1)
    }

    /// How deep the deepest entry goes (for indenting).
    pub fn max_depth(&self) -> usize {
        self.entries.iter().map(|e| e.depth).max().unwrap_or(0)
    }

    /// Ancestors only (no side branches, not the starting post).
    pub fn ancestors(&self) -> usize {
        self.entries.iter().filter(|e| !e.side && !e.start).count()
    }
}

/// A host for a URL or origin ("ada.blyg.example.com").
pub fn host(origin: &str) -> String {
    url::Url::parse(origin)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_else(|| origin.trim().trim_end_matches('/').to_string())
}

/// Who wrote a held post: its author's name, else its blyg's title.
pub fn who_of(r: &ReadingItem) -> String {
    r.author
        .as_ref()
        .and_then(|a| a.name.clone())
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| r.subscription_title.clone())
}

/// A held post's own date, in ms: published, else updated, else imported.
pub fn date_of(r: &ReadingItem) -> Option<i64> {
    let ms = |s: &Option<String>| s.as_deref().and_then(crate::util::parse_ts_ms);
    ms(&r.created)
        .or_else(|| ms(&r.updated))
        .or_else(|| crate::util::parse_ts_ms(&r.observed_at))
}

/// What `child` cited of `peer`, if it cited anything (its quote, stub or
/// fork of it).
fn cited_by<'a>(child: Option<&'a ReadingItem>, peer: &Peer) -> Option<&'a Cited> {
    let c = child?;
    let same = |o: Option<&str>, id: &str| match peer {
        Peer::Post { origin, id: pid } => {
            let o = o.unwrap_or(&c.origin);
            post_key(o, id) == (origin.clone(), pid.clone())
        }
        Peer::Url(_) => false,
        Peer::Bare(b) => o.is_none() && id.eq_ignore_ascii_case(b),
    };
    if let Some(t) = c
        .transclusions
        .iter()
        .find(|t| same(t.origin.as_deref(), &t.id) && t.cited.is_some())
    {
        return t.cited.as_ref();
    }
    if let Some(s) = &c.stub_of {
        let hit = match (peer, &s.id, &s.url) {
            (Peer::Url(u), None, Some(su)) => u == su,
            (_, Some(id), _) => same(s.origin.as_deref(), id),
            _ => false,
        };
        if hit && s.cited.is_some() {
            return s.cited.as_ref();
        }
    }
    if let Some(f) = &c.forked_from
        && same(Some(&f.origin), &f.id)
    {
        return f.cited.as_ref();
    }
    None
}

/// `md` with every own-line `![[id…]]` (outside code fences) replaced by
/// "↪ quotes <who>", `who(id)` naming the quoted post's author. Inline
/// `[[id]]` links are left as written.
pub fn own_words(md: &str, who: impl Fn(&str) -> String) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut fence: Option<&str> = None;
    for line in md.split('\n') {
        let t = line.trim_start();
        let marker = if t.starts_with("```") {
            Some("```")
        } else if t.starts_with("~~~") {
            Some("~~~")
        } else {
            None
        };
        if let Some(m) = marker {
            match fence {
                None => fence = Some(m),
                Some(f) if f == m => fence = None,
                _ => {}
            }
            out.push(line.to_string());
            continue;
        }
        if fence.is_none()
            && let Some(id) = crate::profile::transclusion_ids(line).into_iter().next()
        {
            out.push(format!("↪ quotes {}", who(&id)));
            continue;
        }
        out.push(line.to_string());
    }
    out.join("\n")
}

/// One node met on the walk.
struct Met {
    peer: Peer,
    depth: usize,
    /// The post it was reached from (its child, or for a side branch, the
    /// ancestor it draws on), and how.
    from: Option<(usize, Relation, bool)>,
    side: bool,
}

/// Unroll the conversation leading to `(origin, id)`. `held` finds a post
/// this Mac holds (reading rows and your own published posts, as rows).
/// `whole` adds held side branches off every ancestor.
pub fn build(
    origin: &str,
    id: &str,
    local: &Local,
    held: &dyn Fn(&str, &str) -> Option<ReadingItem>,
    whole: bool,
) -> Conversation {
    let start = post_key(origin, id);
    let mut met: Vec<Met> = vec![Met {
        peer: Peer::Post {
            origin: start.0.clone(),
            id: start.1.clone(),
        },
        depth: 0,
        from: None,
        side: false,
    }];
    let mut seen: HashSet<Peer> = HashSet::new();
    seen.insert(met[0].peer.clone());
    // Each walked node's own references, to tell what it draws on.
    let mut refs: HashMap<usize, Vec<(Peer, Relation, bool)>> = HashMap::new();
    let mut truncated = false;
    let mut queue: VecDeque<usize> = VecDeque::from([0]);
    while let Some(i) = queue.pop_front() {
        let Peer::Post { origin, id } = met[i].peer.clone() else {
            continue;
        };
        let depth = met[i].depth;
        let up = local.up(&origin, &id);
        refs.insert(
            i,
            up.iter()
                .map(|e| (e.peer.clone(), e.relation, e.partial))
                .collect(),
        );
        if depth >= MAX_HOPS {
            truncated |= up.iter().any(|e| !seen.contains(&e.peer));
            continue;
        }
        for e in up {
            if seen.contains(&e.peer) {
                continue;
            }
            if met.len() >= MAX_POSTS {
                truncated = true;
                break;
            }
            seen.insert(e.peer.clone());
            met.push(Met {
                peer: e.peer.clone(),
                depth: depth + 1,
                from: Some((i, e.relation, e.partial)),
                side: false,
            });
            if matches!(e.peer, Peer::Post { .. }) {
                queue.push_back(met.len() - 1);
            }
        }
    }
    if whole {
        let ancestors: Vec<usize> = (1..met.len()).collect();
        'side: for a in ancestors {
            let Peer::Post { origin, id } = met[a].peer.clone() else {
                continue;
            };
            for e in local.down(&origin, &id) {
                if e.via != Via::Reference || seen.contains(&e.peer) {
                    continue;
                }
                let Peer::Post { origin: o, id: i } = &e.peer else {
                    continue;
                };
                if held(o, i).is_none() {
                    continue;
                }
                if met.len() >= MAX_POSTS {
                    truncated = true;
                    break 'side;
                }
                seen.insert(e.peer.clone());
                met.push(Met {
                    peer: e.peer.clone(),
                    depth: met[a].depth.saturating_sub(1),
                    from: Some((a, e.relation, e.partial)),
                    side: true,
                });
                let n = met.len() - 1;
                refs.insert(
                    n,
                    local
                        .up(o, i)
                        .iter()
                        .map(|e| (e.peer.clone(), e.relation, e.partial))
                        .collect(),
                );
            }
        }
    }

    // Describe each node.
    let posts: Vec<Option<ReadingItem>> = met
        .iter()
        .map(|m| match &m.peer {
            Peer::Post { origin, id } => held(origin, id),
            _ => None,
        })
        .collect();
    let mut who: Vec<String> = Vec::with_capacity(met.len());
    for (n, m) in met.iter().enumerate() {
        let w = match (&posts[n], &m.peer) {
            (Some(r), _) => who_of(r),
            (None, peer) => {
                // The child that cited it (an ancestor's child; a side
                // branch is always held).
                let child = m.from.and_then(|(c, _, _)| posts[c].as_ref());
                let cited = cited_by(child, peer);
                let host_of = match peer {
                    Peer::Post { origin, .. } => host(origin),
                    Peer::Url(u) => host(u),
                    Peer::Bare(_) => "an unknown blyg".into(),
                };
                cited
                    .and_then(|c| c.author.clone().or(c.source.clone()))
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or(host_of)
            }
        };
        who.push(w);
    }
    let index: HashMap<&Peer, usize> = met.iter().enumerate().map(|(n, m)| (&m.peer, n)).collect();

    let mut entries: Vec<Entry> = met
        .iter()
        .enumerate()
        .map(|(n, m)| {
            let words = match (&posts[n], &m.peer) {
                (Some(r), _) => {
                    let quoted = |qid: &str| {
                        let t = r
                            .transclusions
                            .iter()
                            .find(|t| t.id.eq_ignore_ascii_case(qid));
                        let o = t
                            .and_then(|t| t.origin.clone())
                            .unwrap_or_else(|| r.origin.clone());
                        let (o, i) = post_key(&o, qid);
                        let peer = Peer::Post {
                            origin: o.clone(),
                            id: i,
                        };
                        match index.get(&peer) {
                            Some(&k) => who[k].clone(),
                            None => held(&o, qid)
                                .map(|h| who_of(&h))
                                .or_else(|| t.and_then(|t| t.cited.as_ref()?.author.clone()))
                                .unwrap_or_else(|| host(&o)),
                        }
                    };
                    let body = if r.content_md.trim().is_empty() {
                        String::new()
                    } else {
                        own_words(&r.content_md, quoted)
                    };
                    Words::Own(body)
                }
                (None, Peer::Post { .. }) => {
                    let child = m.from.and_then(|(c, _, _)| posts[c].as_ref());
                    match cited_by(child, &m.peer)
                        .and_then(|c| c.excerpt.clone())
                        .filter(|x| !x.trim().is_empty())
                    {
                        Some(x) => Words::Cited(x),
                        None => Words::NotHeld,
                    }
                }
                (None, _) => Words::None,
            };
            let draws_on = refs
                .get(&n)
                .map(|rs| {
                    rs.iter()
                        .filter_map(|(p, rel, partial)| {
                            index.get(p).map(|&k| DrawsOn {
                                who: who[k].clone(),
                                relation: *rel,
                                partial: *partial,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Entry {
                peer: m.peer.clone(),
                who: who[n].clone(),
                at: posts[n].as_ref().and_then(date_of),
                depth: m.depth,
                held: posts[n].clone(),
                words,
                link: m.from.map(|(k, relation, partial)| Link {
                    relation,
                    partial,
                    who: who[k].clone(),
                }),
                draws_on,
                side: m.side,
                start: n == 0,
            }
        })
        .collect();
    // Oldest first; undated (not held) before dated; ties deepest first;
    // the starting post last.
    let first = entries.remove(0);
    entries.sort_by(|a, b| {
        a.at.unwrap_or(i64::MIN)
            .cmp(&b.at.unwrap_or(i64::MIN))
            .then(b.depth.cmp(&a.depth))
    });
    entries.push(first);
    Conversation { entries, truncated }
}

impl Entry {
    /// Its date as a day, `2026-10-04` (UTC); `None` when not held.
    pub fn day(&self) -> Option<String> {
        self.at
            .map(|ms| crate::util::iso_from_ms(ms)[..10].to_string())
    }

    /// Its words as plain text, for a prompt.
    pub fn text(&self) -> String {
        match &self.words {
            Words::Own(md) if md.trim().is_empty() => "(no text)".into(),
            Words::Own(md) => md.trim().to_string(),
            Words::Cited(x) => format!("(not held here; the quoting post cited:) {}", x.trim()),
            Words::NotHeld => "(not held here)".into(),
            Words::None => match self.url() {
                Some(u) => format!("(a web page: {u})"),
                None => "(a post that can't be found)".into(),
            },
        }
    }
}

/// The header's span: "Rue → Lin".
pub fn span_words(c: &Conversation) -> String {
    match (c.entries.first(), c.entries.last()) {
        (Some(a), Some(b)) if c.entries.len() > 1 => format!("{} → {}", a.who, b.who),
        (Some(a), _) => a.who.clone(),
        _ => String::new(),
    }
}

/// An entry's kind, for rendering its words (a post not held reads as a
/// fragment).
pub fn kind_of(e: &Entry) -> Kind {
    e.held.as_ref().map_or(Kind::Fragment, |r| r.kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{StubOf, TransclusionRef};

    const RUE: &str = "https://rue.blyg.example.com/";
    const ADA: &str = "https://ada.blyg.example.com/";
    const LIN: &str = "https://lin.blyg.example.com/";
    const A: &str = "01k2rve0chan0a000000000001";
    const B: &str = "01k2ada0chan0b000000000001";
    const C: &str = "01k211n0chan0c000000000001";
    const D: &str = "01k2rve0chan0d000000000001";
    const S: &str = "01k211n0chan0s000000000001";
    const GONE: &str = "01k2kt00chan0g000000000001";

    fn row(origin: &str, id: &str, who: &str, day: u32, md: &str) -> ReadingItem {
        serde_json::from_value(serde_json::json!({
            "subscription_id": format!("sub-{who}"), "remote_id": id,
            "subscription_title": format!("{who}'s blyg"),
            "origin": origin, "kind": "thread", "state": "current", "version": 1,
            "created": format!("2026-10-{day:02}T09:00:00Z"), "updated": null,
            "observed_at": "2026-11-01T00:00:00Z",
            "content_md": md, "content_html": "",
            "author": { "name": who }, "page": null, "thumb": null, "hoppers": [],
        }))
        .unwrap()
    }

    fn quotes(mut r: ReadingItem, origin: &str, id: &str) -> ReadingItem {
        r.transclusions.push(TransclusionRef {
            id: id.into(),
            version: Some(1),
            origin: Some(origin.into()),
            cited: None,
            selector: None,
        });
        r
    }

    /// A ← B ← C ← D: D quotes C quotes B quotes A.
    fn chain() -> Vec<ReadingItem> {
        vec![
            row(RUE, A, "Rue", 1, "Do quotes outlive links?"),
            quotes(
                row(
                    ADA,
                    B,
                    "Ada",
                    2,
                    &format!("Heavier.\n\n![[{A}]]\n\nThey freeze words."),
                ),
                RUE,
                A,
            ),
            quotes(
                row(
                    LIN,
                    C,
                    "Lin",
                    3,
                    &format!("![[{B} | Ada on weight]]\n\nPins."),
                ),
                ADA,
                B,
            ),
            quotes(
                row(RUE, D, "Rue", 4, &format!("![[{C}]]\n\nSee [[{A}]] again.")),
                LIN,
                C,
            ),
        ]
    }

    fn run(rows: &[ReadingItem], origin: &str, id: &str, whole: bool) -> Conversation {
        let local = Local::build(rows, &[], None, &[]);
        let held = |o: &str, i: &str| {
            rows.iter()
                .find(|r| post_key(&r.origin, &r.remote_id) == post_key(o, i))
                .cloned()
        };
        build(origin, id, &local, &held, whole)
    }

    fn ids(c: &Conversation) -> Vec<String> {
        c.entries
            .iter()
            .map(|e| match &e.peer {
                Peer::Post { id, .. } => id.clone(),
                Peer::Url(u) => u.clone(),
                Peer::Bare(b) => format!("bare:{b}"),
            })
            .collect()
    }

    #[test]
    fn a_four_deep_chain_unrolls_oldest_first() {
        let c = run(&chain(), RUE, D, false);
        assert_eq!(ids(&c), [A, B, C, D]);
        assert_eq!(
            c.entries.iter().map(|e| e.depth).collect::<Vec<_>>(),
            [3, 2, 1, 0]
        );
        assert!(c.entries.iter().all(|e| e.held.is_some()));
        assert!(c.entries[3].start && c.entries[3].link.is_none());
        assert_eq!(c.entries[0].link_words(), "quoted by Ada");
        assert_eq!(c.entries[3].link_words(), "you're here");
        assert_eq!(span_words(&c), "Rue → Rue");
        assert_eq!(c.ancestors(), 3);
        assert!(!c.truncated);
        // What each draws on, within the conversation.
        assert_eq!(c.entries[0].draws_on, []);
        assert_eq!(c.entries[2].draws_on[0].who, "Ada");
        assert_eq!(c.entries[2].draws_on[0].relation, Relation::Quotes);
    }

    #[test]
    fn quote_lines_become_markers_and_links_stay() {
        let c = run(&chain(), RUE, D, false);
        assert_eq!(
            c.entries[1].words,
            Words::Own("Heavier.\n\n↪ quotes Rue\n\nThey freeze words.".into())
        );
        // A labelled directive is a quote too.
        assert_eq!(
            c.entries[2].words,
            Words::Own("↪ quotes Ada\n\nPins.".into())
        );
        // The inline link is text, not a marker.
        assert_eq!(
            c.entries[3].words,
            Words::Own(format!("↪ quotes Lin\n\nSee [[{A}]] again."))
        );
    }

    #[test]
    fn a_quote_inside_a_code_fence_is_left_alone() {
        let md = format!("```\n![[{A}]]\n```\n![[{A}]]");
        assert_eq!(
            own_words(&md, |_| "Rue".into()),
            format!("```\n![[{A}]]\n```\n↪ quotes Rue")
        );
    }

    #[test]
    fn ties_put_the_deepest_ancestor_first_and_the_start_stays_last() {
        // Every post on the same day; the start is the oldest of all.
        let mut rows = chain();
        for r in &mut rows {
            r.created = Some("2026-10-05T09:00:00Z".into());
        }
        rows[3].created = Some("2026-09-01T09:00:00Z".into());
        let c = run(&rows, RUE, D, false);
        assert_eq!(ids(&c), [A, B, C, D]);
    }

    #[test]
    fn dates_win_over_depth() {
        // B was written after C (C quoted an earlier version of it).
        let mut rows = chain();
        rows[1].created = Some("2026-10-03T12:00:00Z".into());
        let c = run(&rows, RUE, D, false);
        assert_eq!(ids(&c), [A, C, B, D]);
    }

    #[test]
    fn a_cycle_is_walked_once() {
        let mut rows = chain();
        // A quotes D back.
        rows[0] = quotes(rows[0].clone(), RUE, D);
        rows[0].content_md = format!("![[{D}]]\n\nRound we go.");
        let c = run(&rows, RUE, D, false);
        assert_eq!(ids(&c), [A, B, C, D]);
        assert_eq!(c.entries.len(), 4);
    }

    #[test]
    fn the_caps_hold() {
        // A chain of 12: the walk stops at 8 hops.
        let id = |n: usize| format!("01k2rve0capp{n:014}");
        let mut rows = vec![];
        for n in 0..12 {
            let mut r = row(RUE, &id(n), "Rue", 1 + n as u32, "x");
            if n > 0 {
                r = quotes(r, RUE, &id(n - 1));
            }
            rows.push(r);
        }
        let c = run(&rows, RUE, &id(11), false);
        assert_eq!(c.entries.len(), MAX_HOPS + 1);
        assert!(c.truncated);
        assert_eq!(c.entries.iter().map(|e| e.depth).max(), Some(MAX_HOPS));

        // One post quoting 70: at most 60 in all.
        let mut wide = vec![];
        let mut hub = row(RUE, &id(999), "Rue", 28, "hub");
        for n in 0..70 {
            wide.push(row(ADA, &id(n), "Ada", 1, "leaf"));
            hub = quotes(hub, ADA, &id(n));
        }
        wide.push(hub);
        let c = run(&wide, RUE, &id(999), false);
        assert_eq!(c.entries.len(), MAX_POSTS);
        assert!(c.truncated);
        assert!(c.entries.last().unwrap().start);
    }

    #[test]
    fn whole_conversation_adds_held_side_branches() {
        let mut rows = chain();
        // Lin's side reply to B (a stub), and a reply to D (not an ancestor).
        let mut s = quotes(
            row(LIN, S, "Lin", 3, &format!("![[{B}]]\n\nAn aside.")),
            ADA,
            B,
        );
        s.stub_of = Some(StubOf {
            origin: Some(ADA.into()),
            id: Some(B.into()),
            version: Some(1),
            url: None,
            cited: None,
        });
        s.created = Some("2026-10-02T18:00:00Z".into());
        rows.push(s);
        rows.push(quotes(
            row(ADA, "01k2ada0chan0x000000000001", "Ada", 5, "later"),
            RUE,
            D,
        ));
        let narrow = run(&rows, RUE, D, false);
        assert_eq!(ids(&narrow), [A, B, C, D]);
        let c = run(&rows, RUE, D, true);
        assert_eq!(ids(&c), [A, B, S, C, D]);
        let side = &c.entries[2];
        assert!(side.side);
        assert_eq!(side.depth, 1);
        assert_eq!(side.link_words(), "replies to Ada");
        assert_eq!(side.words, Words::Own("↪ quotes Ada\n\nAn aside.".into()));
    }

    #[test]
    fn posts_not_held_show_what_was_cited_or_say_so() {
        let mut rows = chain();
        rows.remove(0); // A isn't held
        rows[0].transclusions[0].cited = Some(Cited {
            author: Some("Rue".into()),
            excerpt: Some("A quote carries its words.".into()),
            ..Default::default()
        });
        let c = run(&rows, RUE, D, false);
        assert_eq!(ids(&c), [A, B, C, D]);
        assert!(c.entries[0].held.is_none());
        assert_eq!(c.entries[0].who, "Rue");
        assert_eq!(
            c.entries[0].words,
            Words::Cited("A quote carries its words.".into())
        );
        assert_eq!(c.entries[0].at, None);

        // Nothing cited: the host, and "not held".
        rows[0].transclusions[0].cited = None;
        let c = run(&rows, RUE, D, false);
        assert_eq!(c.entries[0].who, "rue.blyg.example.com");
        assert_eq!(c.entries[0].words, Words::NotHeld);
    }

    #[test]
    fn url_stubs_and_bare_ids_are_entries_without_words() {
        let mut d = row(RUE, D, "Rue", 4, "On the news.");
        d.stub_of = Some(StubOf {
            url: Some("https://news.example.com/story".into()),
            ..Default::default()
        });
        let mut e = row(ADA, GONE, "Ada", 5, "Bare.");
        e.stub_of = Some(StubOf {
            id: Some("01k2nn00chan0n000000000001".into()),
            ..Default::default()
        });
        let c = run(&[d], RUE, D, false);
        assert_eq!(ids(&c), ["https://news.example.com/story", D]);
        assert_eq!(c.entries[0].words, Words::None);
        assert_eq!(c.entries[0].url(), Some("https://news.example.com/story"));
        assert_eq!(c.entries[0].who, "news.example.com");
        assert_eq!(c.entries[0].link_words(), "replied to by Rue");
        let c = run(&[e], ADA, GONE, false);
        assert_eq!(c.entries[0].words, Words::None);
        assert_eq!(c.entries[0].who, "an unknown blyg");
    }

    #[test]
    fn a_post_with_no_ancestors_is_alone() {
        let c = run(&chain(), RUE, A, false);
        assert_eq!(ids(&c), [A]);
        assert_eq!(c.ancestors(), 0);
        assert_eq!(span_words(&c), "Rue");
    }
}
