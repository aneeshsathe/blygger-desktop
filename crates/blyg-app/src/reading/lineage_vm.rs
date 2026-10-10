//! Lineage (⌘J): a post's neighbours one step each way. What it draws on
//! (its `stub_of`, `forked_from` and quotes) is exact; what draws on it is
//! what's known here. On a node that serves blygger-studio's `lineage-glyph`
//! extension the graph and counts come from it (`blyg_core::lineage`, ported
//! from studio commit dc632c5); otherwise from what this Mac holds (reading
//! rows, your own published posts, verified mentions of your posts) with the
//! same rules (`blyg_core::lineage::Local`): one reference per post and
//! target, fork > stub > quote, a passage quote marks it partial.
//!
//! The glyph, this view and the ring show how many of each kind, next to the
//! kinds: SPEC rule 1's one exception (2026-10-09, a user decision). Pure
//! model; `lineage.rs` draws it.

use blyg_core::lineage::{Edge, LineageGraph, LineageSummary, Local, Peer, RelationCounts, Via};
use blyg_core::profile::Relation;
use blyg_core::{Item, Kind, Mention, ReadingItem, post_key};

use super::vm;

// ------------------------------------------------------------------ kinds

/// One kind of relation on one side of a post: absent, only quotes of a
/// passage, or at least one of the whole post.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mark {
    #[default]
    None,
    Partial,
    Whole,
}

/// Which kinds of relation a post has on one side, and how many of each
/// (the studio's counts).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Kinds {
    pub fork: Mark,
    pub stub: Mark,
    pub quote: Mark,
    pub counts: RelationCounts,
}

impl Kinds {
    pub fn add(&mut self, rel: Relation, partial: bool) {
        let m = match rel {
            Relation::Forks => &mut self.fork,
            Relation::Stubs => &mut self.stub,
            Relation::Quotes => &mut self.quote,
        };
        *m = match (*m, partial) {
            (Mark::Whole, _) | (_, false) => Mark::Whole,
            _ => Mark::Partial,
        };
        self.counts.bump(rel);
    }

    pub fn get(&self, rel: Relation) -> Mark {
        match rel {
            Relation::Forks => self.fork,
            Relation::Stubs => self.stub,
            Relation::Quotes => self.quote,
        }
    }

    pub fn total(&self) -> u32 {
        self.counts.total()
    }

    pub fn of(edges: &[Edge]) -> Kinds {
        let mut k = Kinds::default();
        for e in edges {
            k.add(e.relation, e.partial);
        }
        k
    }

    /// The node's counts win over this Mac's: a kind it counts shows (whole,
    /// unless this Mac knows it's a passage), one it doesn't count doesn't.
    pub fn served(mut self, counts: RelationCounts) -> Kinds {
        for rel in [Relation::Forks, Relation::Stubs, Relation::Quotes] {
            let m = match rel {
                Relation::Forks => &mut self.fork,
                Relation::Stubs => &mut self.stub,
                Relation::Quotes => &mut self.quote,
            };
            *m = match (counts.get(rel), *m) {
                (0, _) => Mark::None,
                (_, Mark::None) => Mark::Whole,
                (_, m) => m,
            };
        }
        self.counts = counts;
        self
    }

    /// The kinds with their counts in words, for the glyph's tooltip:
    /// "1 fork, 2 replies (to passages)".
    pub fn words(&self) -> Vec<String> {
        let mut out = Vec::new();
        for rel in [Relation::Forks, Relation::Stubs, Relation::Quotes] {
            let n = self.counts.get(rel);
            if n == 0 {
                continue;
            }
            let noun = match (rel, n == 1) {
                (Relation::Forks, true) => "fork",
                (Relation::Forks, false) => "forks",
                (Relation::Stubs, true) => "reply",
                (Relation::Stubs, false) => "replies",
                (Relation::Quotes, true) => "quote",
                (Relation::Quotes, false) => "quotes",
            };
            let passage = if self.get(rel) == Mark::Partial {
                if n == 1 {
                    " (of a passage)"
                } else {
                    " (of passages)"
                }
            } else {
                ""
            };
            out.push(format!("{n} {noun}{passage}"));
        }
        out
    }
}

/// A post's glyph: both sides.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Glyph {
    pub up: Kinds,
    pub down: Kinds,
}

impl Glyph {
    /// From this Mac's edges, with the node's counts over them when it
    /// serves them.
    pub fn of(local: &Local, origin: &str, id: &str, served: Option<&LineageSummary>) -> Glyph {
        let up = Kinds::of(local.up(origin, id));
        let down = Kinds::of(local.down(origin, id));
        match served {
            Some(s) => Glyph {
                up: up.served(s.up),
                down: down.served(s.down),
            },
            None => Glyph { up, down },
        }
    }

    pub fn is_empty(&self) -> bool {
        self.up.total() == 0 && self.down.total() == 0
    }

    /// The studio's `glyphCounts`: "up · down", uncapped.
    pub fn label(&self) -> String {
        format!("{} · {}", self.up.total(), self.down.total())
    }

    /// The tooltip: the counts by kind, each side in words.
    pub fn tip(&self) -> String {
        let mut lines = Vec::new();
        let u = self.up.words();
        if !u.is_empty() {
            lines.push(format!("Draws on {}: {}", self.up.total(), u.join(", ")));
        }
        let d = self.down.words();
        if !d.is_empty() {
            lines.push(format!(
                "{} known here draw{} on it: {}",
                self.down.total(),
                if self.down.total() == 1 { "s" } else { "" },
                d.join(", ")
            ));
        }
        lines.push("⌘J or a click: see who".into());
        lines.join("\n")
    }
}

// ------------------------------------------------------------------ the view's model

/// One neighbour of the centre.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Its origin; a `{url}` stub's URL (with an empty `id`).
    pub origin: String,
    /// Empty for a neighbour that isn't a post (a `{url}` stub).
    pub id: String,
    pub relation: Relation,
    pub partial: bool,
    pub title: String,
    /// The author, or "you".
    pub who: String,
    /// The post as held here, when it is.
    pub held: Option<ReadingItem>,
}

impl Node {
    /// It's a post, so it can be the centre.
    pub fn is_post(&self) -> bool {
        !self.id.is_empty() && !self.origin.is_empty()
    }
}

/// The post in the middle.
#[derive(Debug, Clone, PartialEq)]
pub struct Centre {
    pub origin: String,
    pub id: String,
    pub title: String,
    pub who: String,
    pub kind: Option<Kind>,
    /// Held in the reading list.
    pub held: Option<ReadingItem>,
    /// One of your published posts.
    pub own: bool,
}

impl Centre {
    /// How the lineage route names it.
    pub fn query(&self) -> blyg_core::lineage::Centre {
        use blyg_core::lineage::Centre as C;
        match (&self.held, self.own) {
            (Some(r), _) if !r.subscription_id.is_empty() => C::Imported {
                sub: r.subscription_id.clone(),
                id: r.remote_id.clone(),
            },
            (_, true) => C::Own(self.id.clone()),
            _ => C::Remote {
                origin: self.origin.clone(),
                id: self.id.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub centre: Centre,
    /// What the centre draws on, in the order it names them.
    pub ups: Vec<Node>,
    /// What draws on it: held posts first, then mention-only ones.
    pub downs: Vec<Node>,
    /// The neighbours are the node's (`lineage-glyph`), not this Mac's.
    pub served: bool,
}

impl Model {
    /// How many of each kind on each side: the lists counted, so the counts
    /// and what's listed always agree.
    pub fn summary(&self) -> LineageSummary {
        let mut s = LineageSummary::default();
        for n in &self.ups {
            s.up.bump(n.relation);
        }
        for n in &self.downs {
            s.down.bump(n.relation);
        }
        s
    }
}

/// What the model is built from.
pub struct Sources<'a> {
    pub rows: &'a [ReadingItem],
    pub own: &'a [Item],
    pub own_origin: Option<&'a str>,
    pub mentions: &'a [Mention],
    /// Every post's edges over the above (`Local::build`).
    pub local: &'a Local,
}

impl Sources<'_> {
    fn held(&self, origin: &str, id: &str) -> Option<&ReadingItem> {
        let key = post_key(origin, id);
        self.rows
            .iter()
            .find(|r| post_key(&r.origin, &r.remote_id) == key)
    }

    fn own_item(&self, origin: &str, id: &str) -> Option<&Item> {
        let own = self.own_origin?;
        if post_key(own, "").0 != post_key(origin, "").0 {
            return None;
        }
        self.own.iter().find(|i| {
            i.version > 0
                && i.server_id
                    .as_ref()
                    .is_some_and(|s| s.0.eq_ignore_ascii_case(id))
        })
    }

    /// (title, who) of a post, as well as this Mac can say.
    fn describe(&self, origin: &str, id: &str) -> Option<(String, String, Option<ReadingItem>)> {
        if let Some(h) = self.held(origin, id) {
            return Some((vm::post_title(h), who_of(h), Some(h.clone())));
        }
        self.own_item(origin, id)
            .map(|i| (i.title(), "you".to_string(), None))
    }
}

fn who_of(r: &ReadingItem) -> String {
    if r.subscription_id.is_empty() && r.subscription_title == "you" {
        return "you".into();
    }
    r.author
        .as_ref()
        .and_then(|a| a.name.clone())
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| r.subscription_title.clone())
}

fn centre_of(origin: &str, id: &str, src: &Sources) -> Centre {
    let held = src.held(origin, id).cloned();
    let own = src.own_item(origin, id);
    match (&held, own) {
        (Some(r), _) => Centre {
            origin: r.origin.clone(),
            id: r.remote_id.clone(),
            title: vm::post_title(r),
            who: who_of(r),
            kind: Some(r.kind),
            held: Some(r.clone()),
            own: false,
        },
        (None, Some(i)) => Centre {
            origin: origin.to_string(),
            id: id.to_string(),
            title: i.title(),
            who: "you".into(),
            kind: Some(i.kind),
            held: None,
            own: true,
        },
        (None, None) => Centre {
            origin: origin.to_string(),
            id: id.to_string(),
            title: format!("A post on {}", vm::host(origin)),
            who: vm::host(origin),
            kind: None,
            held: None,
            own: false,
        },
    }
}

/// Build the view of `(origin, id)` from what this Mac holds.
pub fn build(origin: &str, id: &str, src: &Sources) -> Model {
    let centre = centre_of(origin, id, src);
    let cited = |rid: &str| {
        centre.held.as_ref().and_then(|r| {
            r.transclusions
                .iter()
                .find(|t| t.id.eq_ignore_ascii_case(rid))
                .and_then(|t| t.cited.clone())
        })
    };
    // What it draws on: its own references, described as well as we can.
    let ups = src
        .local
        .up(&centre.origin, &centre.id)
        .iter()
        .map(|e| {
            let (origin, id) = match &e.peer {
                Peer::Post { origin, id } => (origin.clone(), id.clone()),
                Peer::Url(u) => (u.clone(), String::new()),
                Peer::Bare(id) => (String::new(), id.clone()),
            };
            let (title, who, held) = match src.describe(&origin, &id) {
                Some(d) if !id.is_empty() => d,
                _ => {
                    let c = cited(&id);
                    let host = if origin.is_empty() {
                        "an unknown blyg".to_string()
                    } else {
                        vm::host(&origin)
                    };
                    (
                        c.as_ref()
                            .and_then(|c| c.excerpt.clone().or(c.source.clone()))
                            .unwrap_or_else(|| {
                                if id.is_empty() {
                                    format!("A page on {host}")
                                } else {
                                    format!("A post on {host}")
                                }
                            }),
                        c.as_ref().and_then(|c| c.author.clone()).unwrap_or(host),
                        None,
                    )
                }
            };
            Node {
                origin,
                id,
                relation: e.relation,
                partial: e.partial,
                title,
                who,
                held,
            }
        })
        .collect();

    // What draws on it: held posts, then mentions no held post accounts for.
    let downs = src
        .local
        .down(&centre.origin, &centre.id)
        .iter()
        .filter_map(|e| {
            let Peer::Post { origin, id } = &e.peer else {
                return None;
            };
            let mention = (e.via == Via::Mention)
                .then(|| {
                    src.mentions.iter().find(|m| {
                        m.source_id
                            .as_deref()
                            .is_some_and(|s| s.eq_ignore_ascii_case(id))
                    })
                })
                .flatten();
            let (origin, title, who, held) = match (src.describe(origin, id), mention) {
                (Some((t, w, h)), _) => (origin.clone(), t, w, h),
                (None, Some(m)) => {
                    let o = m.source_origin.clone().unwrap_or_else(|| m.source.clone());
                    (
                        o.clone(),
                        format!("A post on {}", vm::host(&o)),
                        m.source_author
                            .as_ref()
                            .and_then(|a| a.name.clone())
                            .filter(|n| !n.trim().is_empty())
                            .unwrap_or_else(|| vm::host(&o)),
                        None,
                    )
                }
                (None, None) => (
                    origin.clone(),
                    format!("A post on {}", vm::host(origin)),
                    vm::host(origin),
                    None,
                ),
            };
            Some(Node {
                origin,
                id: id.clone(),
                relation: e.relation,
                partial: e.partial,
                title,
                who,
                held,
            })
        })
        .collect();
    Model {
        centre,
        ups,
        downs,
        served: false,
    }
}

/// Build the view of `(origin, id)` from the graph the node served: its
/// neighbours, described by the node, matched to what this Mac holds.
pub fn from_graph(origin: &str, id: &str, src: &Sources, graph: &LineageGraph) -> Model {
    let centre = centre_of(origin, id, src);
    let node = |n: &blyg_core::lineage::GraphNode| {
        let p = &n.post;
        let (origin, id) = match (&p.origin, &p.id) {
            (Some(o), Some(i)) => (o.clone(), i.clone()),
            (o, i) => (
                o.clone().or(p.url.clone()).unwrap_or_default(),
                i.clone().unwrap_or_default(),
            ),
        };
        let mine = p.held == Some(blyg_core::lineage::Held::Own);
        let local = (!id.is_empty())
            .then(|| src.describe(&origin, &id))
            .flatten();
        let host = if origin.is_empty() {
            "an unknown blyg".to_string()
        } else {
            vm::host(&origin)
        };
        let title = local
            .as_ref()
            .map(|d| d.0.clone())
            .or_else(|| p.title.clone().filter(|t| !t.trim().is_empty()))
            .or_else(|| p.excerpt.clone().filter(|t| !t.trim().is_empty()))
            .unwrap_or_else(|| format!("A post on {host}"));
        let who = if mine {
            "you".to_string()
        } else {
            local
                .as_ref()
                .map(|d| d.1.clone())
                .or_else(|| p.source.clone().filter(|s| !s.trim().is_empty()))
                .unwrap_or(host)
        };
        Node {
            origin,
            id,
            relation: n.relation.into(),
            partial: n.partial,
            title,
            who,
            held: local.and_then(|d| d.2),
        }
    };
    Model {
        centre,
        ups: graph.ancestors.iter().map(node).collect(),
        downs: graph.descendants.iter().map(node).collect(),
        served: true,
    }
}

/// One side's count in words, beside the hexagon: "draws on 2" /
/// "3 known here draw on it".
pub fn side_words(c: RelationCounts, up: bool) -> String {
    match (up, c.total()) {
        (true, 0) => "draws on nothing".into(),
        (true, n) => format!("draws on {n}"),
        (false, 0) => "nothing known here draws on it".into(),
        (false, 1) => "1 known here draws on it".into(),
        (false, n) => format!("{n} known here draw on it"),
    }
}

/// How many have already done what a ring action would do, known here.
pub fn already_words(rel: Relation, n: u32) -> String {
    let (one, many) = match rel {
        Relation::Forks => ("fork", "forks"),
        Relation::Stubs => ("reply", "replies"),
        Relation::Quotes => ("quote", "quotes"),
    };
    match n {
        0 => format!("No {many} of it known here yet."),
        1 => format!("1 {one} of it known here."),
        n => format!("{n} {many} of it known here."),
    }
}

/// How a neighbour relates, from its side of the edge.
pub fn relation_words(rel: Relation, partial: bool, up: bool) -> &'static str {
    match (up, rel, partial) {
        (true, Relation::Forks, _) => "forked from",
        (true, Relation::Stubs, false) => "replies to",
        (true, Relation::Stubs, true) => "replies to a passage of",
        (true, Relation::Quotes, false) => "quotes",
        (true, Relation::Quotes, true) => "quotes a passage of",
        (false, Relation::Forks, _) => "forked this",
        (false, Relation::Stubs, false) => "replied",
        (false, Relation::Stubs, true) => "replied to a passage",
        (false, Relation::Quotes, false) => "quoted this",
        (false, Relation::Quotes, true) => "quoted a passage",
    }
}

// ------------------------------------------------------------------ selection

/// What the keyboard is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sel {
    Centre,
    Up(usize),
    Down(usize),
}

impl Model {
    /// The neighbour `sel` is on (`None` on the centre).
    pub fn node(&self, sel: Sel) -> Option<&Node> {
        match sel {
            Sel::Centre => None,
            Sel::Up(i) => self.ups.get(i),
            Sel::Down(i) => self.downs.get(i),
        }
    }

    /// Where an arrow key moves `sel`.
    pub fn step(&self, sel: Sel, key: &str) -> Sel {
        let mid = |n: usize| n / 2;
        match (sel, key) {
            (Sel::Centre, "up") if !self.ups.is_empty() => Sel::Up(mid(self.ups.len())),
            (Sel::Centre, "down") if !self.downs.is_empty() => Sel::Down(mid(self.downs.len())),
            (Sel::Up(_), "down") | (Sel::Down(_), "up") => Sel::Centre,
            (Sel::Up(i), "left") => Sel::Up(i.saturating_sub(1)),
            (Sel::Up(i), "right") => Sel::Up((i + 1).min(self.ups.len().saturating_sub(1))),
            (Sel::Down(i), "left") => Sel::Down(i.saturating_sub(1)),
            (Sel::Down(i), "right") => Sel::Down((i + 1).min(self.downs.len().saturating_sub(1))),
            _ => sel,
        }
    }
}

// ------------------------------------------------------------------ the ring

/// The reader's actions, in the ring's fixed places (the same compass as
/// blygger-studio's): bottom puts their words in yours, the sides make
/// your own thing, the top only looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    Fork,
    Reply,
    Quote,
    Link,
    Versions,
    Open,
}

/// The four facts every action answers, in order.
pub const FACTS: [&str; 4] = [
    "a response?",
    "the author is told?",
    "their words in yours",
    "shows under their post?",
];

impl Act {
    pub const ALL: [Act; 6] = [
        Act::Fork,
        Act::Reply,
        Act::Quote,
        Act::Link,
        Act::Versions,
        Act::Open,
    ];

    /// Degrees clockwise from east (screen coordinates).
    pub fn angle(self) -> f32 {
        match self {
            Act::Fork => 0.,
            Act::Reply => 60.,
            Act::Quote => 120.,
            Act::Link => 180.,
            Act::Versions => 240.,
            Act::Open => 300.,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Act::Fork => "f",
            Act::Reply => "r",
            Act::Quote => "q",
            Act::Link => "l",
            Act::Versions => "v",
            Act::Open => "o",
        }
    }

    pub fn of_key(k: &str) -> Option<Act> {
        Act::ALL.into_iter().find(|a| a.key() == k)
    }

    pub fn label(self) -> &'static str {
        match self {
            Act::Fork => "Fork",
            Act::Reply => "Reply",
            Act::Quote => "Quote",
            Act::Link => "Link post",
            Act::Versions => "Versions",
            Act::Open => "Open ↗",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Act::Fork => "Fork this pin",
            Act::Reply => "Reply",
            Act::Quote => "Quote",
            Act::Link => "Link post",
            Act::Versions => "Versions",
            Act::Open => "Open on web",
        }
    }

    /// The relation its result has to the centre (colours the wedge).
    pub fn relation(self) -> Option<Relation> {
        match self {
            Act::Fork => Some(Relation::Forks),
            Act::Reply => Some(Relation::Stubs),
            Act::Quote => Some(Relation::Quotes),
            _ => None,
        }
    }

    /// The node it would add below the centre: (title, subtitle).
    pub fn ghost(self, passage: bool) -> Option<(&'static str, &'static str)> {
        match self {
            Act::Fork => Some(("your fork", "draft · yours to edit")),
            Act::Reply if passage => Some(("your reply, to a passage", "new thread")),
            Act::Reply => Some(("your reply", "new thread")),
            Act::Quote => Some(("your thread", "quoting this")),
            Act::Link => Some(("your fragment", "…links here…")),
            _ => None,
        }
    }

    pub fn one(self) -> &'static str {
        match self {
            Act::Fork => "Start your own copy of this post.",
            Act::Reply => "Respond to this post.",
            Act::Quote => "Quote it in a thread you're writing (⌘K).",
            Act::Link => "Write something of your own that links to this.",
            Act::Versions => "See how this post has changed.",
            Act::Open => "Read it on the author's own site (⌘O).",
        }
    }

    pub fn long(self) -> &'static str {
        match self {
            Act::Fork => {
                "Opens a draft that starts as the author's last pinned version, flattened to \
                 text you can rewrite freely. It's yours, and it records that it was forked \
                 from theirs."
            }
            Act::Reply => {
                "Opens a thread that starts with the post quoted as a frozen snapshot (or just \
                 the passage you selected), with your reply beneath it. The author's blyg is \
                 told, and can list it under their post."
            }
            Act::Quote => {
                "Adds ![[id]] to the thread you're writing (or a new one), which shows the post \
                 inside yours. It isn't a reply, but the author's blyg is told it was quoted."
            }
            Act::Link => {
                "Opens a fragment that starts with [[id]], which publishes as an ordinary link. \
                 Nothing is copied and nothing is sent: the author isn't told."
            }
            Act::Versions => {
                "The post's versions, with the author's change notes. Between pinned versions \
                 you can see the change word by word. Nothing is created."
            }
            Act::Open => {
                "Opens the post's public page in your browser. Whatever you're writing stays \
                 where it is."
            }
        }
    }

    /// The four facts: (answer, yes?).
    pub fn facts(self, passage: bool) -> [(&'static str, bool); 4] {
        match self {
            Act::Fork => [
                ("no, a line of its own", false),
                ("yes, as a fork", true),
                ("all of them, editable", true),
                ("listed as a fork", true),
            ],
            Act::Reply => [
                ("yes", true),
                ("yes, as a reply (stub)", true),
                (
                    if passage {
                        "just the passage you selected"
                    } else {
                        "the whole post, frozen"
                    },
                    true,
                ),
                ("yes, as a response", true),
            ],
            Act::Quote => [
                ("no, part of your thread", false),
                ("yes, as a quote", true),
                ("the whole post, frozen", true),
                ("yes, as a quote", true),
            ],
            Act::Link => [
                ("no", false),
                ("no, links are silent", false),
                ("none", false),
                ("no", false),
            ],
            Act::Versions | Act::Open => [
                ("n/a, reading only", false),
                ("no", false),
                ("none", false),
                ("n/a", false),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blyg_core::{RemoteRef, ServerId, Status, StubOf, TransclusionRef};

    const ADA: &str = "https://ada.example/";
    const BO: &str = "https://bo.example/";
    const ME: &str = "https://me.example/";

    fn row(origin: &str, id: &str, md: &str) -> ReadingItem {
        serde_json::from_value(serde_json::json!({
            "subscription_id": "S", "remote_id": id, "subscription_title": vm::host(origin),
            "origin": origin, "kind": "thread", "state": "current", "version": 1,
            "created": null, "updated": null, "observed_at": "2030-01-01T00:00:00Z",
            "content_md": md, "content_html": "", "author": null, "page": null,
            "thumb": null, "hoppers": [],
        }))
        .unwrap()
    }

    fn mine(id: &str, md: &str) -> Item {
        Item {
            local_id: blyg_core::LocalId(format!("L{id}")),
            server_id: Some(ServerId(id.into())),
            kind: Kind::Thread,
            status: Status::Public,
            version: 1,
            dirty: false,
            content_md: md.into(),
            created: "2030-01-01T00:00:00Z".into(),
            updated: "2030-01-01T00:00:00Z".into(),
            permalink: None,
            stub_of: None,
            forked_from: None,
            show_responses: true,
            responses_mode: None,
            pending_sync: false,
            conflict: false,
            highlight: false,
            highlight_mode: None,
        }
    }

    #[test]
    fn kinds_count_and_mark_passages() {
        let mut k = Kinds::default();
        k.add(Relation::Stubs, true);
        assert_eq!(k.stub, Mark::Partial);
        assert_eq!(k.words(), ["1 reply (of a passage)"]);
        k.add(Relation::Stubs, false);
        k.add(Relation::Stubs, true);
        assert_eq!(k.stub, Mark::Whole);
        assert_eq!(k.fork, Mark::None);
        assert_eq!(k.counts.stub, 3);
        assert_eq!(k.words(), ["3 replies"]);
        assert!(Kinds::default().words().is_empty());
    }

    #[test]
    fn the_nodes_counts_win_over_this_macs() {
        let mut k = Kinds::default();
        k.add(Relation::Quotes, true);
        k.add(Relation::Forks, false);
        let served = k.served(RelationCounts {
            stub: 2,
            transclusion: 4,
            fork: 0,
        });
        assert_eq!(
            (served.fork, served.stub, served.quote),
            (Mark::None, Mark::Whole, Mark::Partial)
        );
        assert_eq!(served.total(), 6);
        let g = Glyph {
            up: Kinds::default(),
            down: served,
        };
        assert_eq!(g.label(), "0 · 6");
        assert_eq!(
            g.tip(),
            "6 known here draw on it: 2 replies, 4 quotes (of passages)\n⌘J or a click: see who"
        );
    }

    #[test]
    fn the_glyph_counts_rows_your_posts_and_mentions() {
        let mut reply = row(BO, "R", "Re");
        reply.stub_of = Some(StubOf {
            origin: Some(ADA.into()),
            id: Some("T".into()),
            version: Some(1),
            url: None,
        });
        let mut fork = mine("F1", "Forked");
        fork.forked_from = Some(RemoteRef {
            origin: ADA.into(),
            id: "T".into(),
            version: 1,
        });
        let m = Mention {
            id: "m".into(),
            target_item_id: "MINE".into(),
            status: "verified".into(),
            relation: Some("transclusion".into()),
            source: "https://bo.example/t/x".into(),
            source_origin: Some(BO.into()),
            source_id: Some("X".into()),
            source_kind: None,
            source_version: None,
            source_author: None,
            first_seen: "t".into(),
            verified_at: None,
            hidden: false,
        };
        let local = Local::build(&[reply], &[fork], Some(ME), &[m]);
        let t = Glyph::of(&local, ADA, "T", None);
        assert_eq!(
            (t.down.fork, t.down.stub, t.down.quote),
            (Mark::Whole, Mark::Whole, Mark::None)
        );
        assert_eq!(t.label(), "0 · 2");
        assert_eq!(
            Glyph::of(&local, ME, "mine", None).down.counts.transclusion,
            1
        );
        assert_eq!(Glyph::of(&local, BO, "R", None).label(), "1 · 0");
    }

    #[test]
    fn the_model_has_one_step_each_way_and_counts_what_it_lists() {
        const T: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let mut centre = row(ADA, T, "Tides\nbody");
        centre.forked_from = Some(RemoteRef {
            origin: BO.into(),
            id: "K".into(),
            version: 2,
        });
        centre.transclusions = vec![TransclusionRef {
            id: "Z".into(),
            version: Some(1),
            origin: Some("https://far.example/".into()),
            cited: Some(blyg_core::Cited {
                source: Some("Far".into()),
                author: Some("Fay".into()),
                excerpt: Some("A far post".into()),
                ..Default::default()
            }),
            selector: None,
        }];
        let kit = row(BO, "K", "Kit's original");
        let own = mine("Q1", &format!("Mine\n![[{T}]]"));
        let rows = [centre.clone(), kit];
        let local = Local::build(&rows, std::slice::from_ref(&own), Some(ME), &[]);
        let src = Sources {
            rows: &rows,
            own: std::slice::from_ref(&own),
            own_origin: Some(ME),
            mentions: &[],
            local: &local,
        };
        let m = build(ADA, &T.to_lowercase(), &src);
        assert_eq!(m.centre.title, "Tides");
        let ups: Vec<(&str, Relation, &str)> = m
            .ups
            .iter()
            .map(|n| (n.who.as_str(), n.relation, n.title.as_str()))
            .collect();
        assert_eq!(
            ups,
            [
                ("bo.example", Relation::Forks, "Kit's original"),
                ("Fay", Relation::Quotes, "A far post"),
            ]
        );
        assert_eq!(m.downs.len(), 1);
        assert_eq!(
            (m.downs[0].who.as_str(), m.downs[0].relation),
            ("you", Relation::Quotes)
        );
        // The view's counts are the glyph's.
        assert_eq!(m.summary(), local.summary(ADA, T));
        assert_eq!(m.summary().label(), "2 · 1");
        assert!(!m.served);
        // Arrows: up to the middle of the row above, along it, back down.
        assert_eq!(m.step(Sel::Centre, "up"), Sel::Up(1));
        assert_eq!(m.step(Sel::Up(1), "left"), Sel::Up(0));
        assert_eq!(m.step(Sel::Up(0), "left"), Sel::Up(0));
        assert_eq!(m.step(Sel::Up(0), "down"), Sel::Centre);
        assert_eq!(m.step(Sel::Centre, "down"), Sel::Down(0));
    }

    #[test]
    fn a_served_graph_becomes_the_model() {
        const T: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let centre = row(ADA, T, "Tides");
        let held = row(BO, "K", "Kit's reply");
        let rows = [centre, held];
        let local = Local::build(&rows, &[], Some(ME), &[]);
        let src = Sources {
            rows: &rows,
            own: &[],
            own_origin: Some(ME),
            mentions: &[],
            local: &local,
        };
        let graph: LineageGraph = serde_json::from_value(serde_json::json!({
            "node": { "origin": ADA, "id": T, "version": 1, "held": "imported", "sub": "S",
                      "kind": "thread", "title": "Tides", "excerpt": null, "source": "Ada", "url": null },
            "ancestors": [
                { "origin": null, "id": null, "version": null, "held": null, "sub": null, "kind": null,
                  "title": null, "excerpt": null, "source": "News site", "url": "https://news.example/s",
                  "relation": "stub", "partial": false, "via": "reference" }
            ],
            "descendants": [
                { "origin": BO, "id": "K", "version": 1, "held": "imported", "sub": "S2", "kind": "thread",
                  "title": "K", "excerpt": null, "source": "Bo", "url": null,
                  "relation": "transclusion", "partial": true, "via": "reference" },
                { "origin": ME, "id": "M1", "version": 1, "held": "own", "sub": null, "kind": "fragment",
                  "title": "Mine", "excerpt": null, "source": "you", "url": null,
                  "relation": "fork", "partial": false, "via": "reference" }
            ]
        }))
        .unwrap();
        let m = from_graph(ADA, T, &src, &graph);
        assert!(m.served);
        assert_eq!(m.summary(), graph.summary());
        assert_eq!(
            (
                m.ups[0].origin.as_str(),
                m.ups[0].who.as_str(),
                m.ups[0].is_post()
            ),
            ("https://news.example/s", "News site", false)
        );
        assert_eq!(m.downs[0].title, "Kit's reply", "described as held here");
        assert!(m.downs[0].held.is_some());
        assert_eq!(m.downs[1].who, "you");
        assert_eq!(
            m.centre.query(),
            blyg_core::lineage::Centre::Imported {
                sub: "S".into(),
                id: T.into()
            }
        );
    }

    #[test]
    fn the_ring_has_six_keys_in_fixed_places() {
        let keys: Vec<&str> = Act::ALL.iter().map(|a| a.key()).collect();
        assert_eq!(keys, ["f", "r", "q", "l", "v", "o"]);
        for (i, a) in Act::ALL.iter().enumerate() {
            assert_eq!(a.angle(), i as f32 * 60.);
            assert_eq!(Act::of_key(a.key()), Some(*a));
        }
        assert_eq!(Act::Reply.facts(true)[2].0, "just the passage you selected");
    }
}
