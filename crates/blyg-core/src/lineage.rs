//! Lineage counts: how many posts a post draws on ("up") and how many known
//! here draw on it ("down"), by relation (stub, transclusion, fork).
//!
//! Ported from blygger-studio's `lineage-glyph` extension
//! (`extensions/lineage-glyph/{contract,lineage,server}.ts` and
//! `ui/{index,lineage}.tsx` at commit dc632c5, branch ext/lineage-glyph: one
//! commit on studio 0.39.0, blygger-studio PR #53, still in review; its
//! extension files are identical to the pre-rebase 861c825). Two halves:
//!
//! - the extension's contract (its owner reads `GET /api/ext/lineage-glyph/
//!   summaries` and `/lineage`), vendored as Rust types, for a node that
//!   serves them (`crate::api::lineage`, cached by `crate::store`);
//! - the same counting rules over what this Mac holds ([`Local`]), for a
//!   node that doesn't.
//!
//! The rules, as the studio has them:
//!
//! - Ancestors are a post's own references: `stub_of`, `forked_from` and
//!   `transclusions[]`. One reference per (holder, target): a stub thread
//!   both stubs and transcludes its target, which is one act. Fork > stub >
//!   transclusion, and a partial transclusion marks the edge partial. A
//!   `{url}` stub is an ancestor without a post; a bare transclusion means
//!   the holding post's own origin.
//! - Descendants are known here only: posts held here (reading rows, your
//!   own public posts) that reference it, one per holder, plus, for your own
//!   posts, verified mentions (hidden ones too: hiding is about your public
//!   page) from posts not already counted.
//!
//! Counts are not capped: the studio prints `up · down` as plain totals.
//! SPEC rule 1's exception for the lineage glyph (2026-10-09) allows them.

pub(crate) mod reads;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::model::{Item, Mention, ReadingItem, Status, post_key};
use crate::profile::{Relation, normalize_origin};

/// The extension's name: the `/api/ext/<name>/` prefix.
pub const EXTENSION: &str = "lineage-glyph";
/// At most this many reading entry keys per summaries request
/// (`MAX_SUMMARY_KEYS` in the extension's contract).
pub const MAX_SUMMARY_KEYS: usize = 50;
/// A cached summary is fresh this long (the studio's `FRESH_MS`).
pub const SUMMARY_TTL_MS: i64 = 30 * 1000;
/// A cached graph (the ⌘J view) is fresh this long.
pub const GRAPH_TTL_MS: i64 = 30 * 1000;
/// After a node answered 404 (the extension isn't enabled there), ask
/// again after this long: the owner may turn it on.
pub const OFF_RECHECK_MS: i64 = 6 * 60 * 60 * 1000;

// ------------------------------------------------------------------ contract

/// How many references of each relation (`relationCounts`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationCounts {
    pub stub: u32,
    pub transclusion: u32,
    pub fork: u32,
}

impl RelationCounts {
    pub fn total(&self) -> u32 {
        self.stub + self.transclusion + self.fork
    }

    pub fn get(&self, rel: Relation) -> u32 {
        match rel {
            Relation::Stubs => self.stub,
            Relation::Quotes => self.transclusion,
            Relation::Forks => self.fork,
        }
    }

    pub fn bump(&mut self, rel: Relation) {
        match rel {
            Relation::Stubs => self.stub += 1,
            Relation::Quotes => self.transclusion += 1,
            Relation::Forks => self.fork += 1,
        }
    }
}

/// A post's glyph counts (`LineageGlyphSummary`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineageSummary {
    pub up: RelationCounts,
    pub down: RelationCounts,
}

impl LineageSummary {
    pub fn is_empty(&self) -> bool {
        self.up.total() == 0 && self.down.total() == 0
    }

    /// The studio's `glyphCounts`: "up · down".
    pub fn label(&self) -> String {
        format!("{} · {}", self.up.total(), self.down.total())
    }
}

/// A lineage relation as the contract spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WireRelation {
    Stub,
    Transclusion,
    Fork,
}

impl From<WireRelation> for Relation {
    fn from(r: WireRelation) -> Relation {
        match r {
            WireRelation::Stub => Relation::Stubs,
            WireRelation::Transclusion => Relation::Quotes,
            WireRelation::Fork => Relation::Forks,
        }
    }
}

/// Where a node is held on the serving node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Held {
    Own,
    Imported,
}

/// How a descendant is known: a stored document, or only a verified mention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Via {
    Reference,
    Mention,
}

/// A post as the serving node describes it (`lineageNodeBase`). Every field
/// is remote data: plain text, and `url` only ever http(s)/mailto (the
/// studio filters it with `isFollowableUrl`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphPost {
    pub origin: Option<String>,
    pub id: Option<String>,
    pub version: Option<u32>,
    pub held: Option<Held>,
    pub sub: Option<String>,
    /// "fragment" | "thread"
    pub kind: Option<String>,
    pub title: Option<String>,
    pub excerpt: Option<String>,
    pub source: Option<String>,
    pub url: Option<String>,
}

/// A neighbour (`LineageGlyphNode`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphNode {
    #[serde(flatten)]
    pub post: GraphPost,
    pub relation: WireRelation,
    pub partial: bool,
    pub via: Via,
}

/// One hop of lineage around a post (`LineageGlyphLineage`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineageGraph {
    pub node: GraphPost,
    pub ancestors: Vec<GraphNode>,
    pub descendants: Vec<GraphNode>,
}

impl LineageGraph {
    /// Its counts: the same numbers the summaries route gives the post.
    pub fn summary(&self) -> LineageSummary {
        let mut s = LineageSummary::default();
        for n in &self.ancestors {
            s.up.bump(n.relation.into());
        }
        for n in &self.descendants {
            s.down.bump(n.relation.into());
        }
        s
    }
}

/// `{summaries}` from the summaries route.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SummariesPage {
    pub summaries: HashMap<String, LineageSummary>,
}

/// A reading entry's key, as `GET /api/reading` names it and the summaries
/// route takes it: `imported:["<sub>","<id>"]` (the studio's
/// `JSON.stringify([sub, id])`).
pub fn imported_key(sub: &str, id: &str) -> String {
    format!(
        "imported:{}",
        serde_json::to_string(&[sub, id]).unwrap_or_default()
    )
}

/// The key of one of your own posts: `own:<id>`.
pub fn own_key(id: &str) -> String {
    format!("own:{id}")
}

/// The post the ⌘J view centres on, as the lineage route takes it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Centre {
    /// One of yours (`?id=`).
    Own(String),
    /// An imported post (`?sub=&id=`).
    Imported { sub: String, id: String },
    /// Any other node a lineage named (`?origin=&id=`).
    Remote { origin: String, id: String },
}

impl Centre {
    /// The query string, also the cache key.
    pub fn query(&self) -> String {
        let e = |s: &str| url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>();
        match self {
            Centre::Own(id) => format!("id={}", e(id)),
            Centre::Imported { sub, id } => format!("id={}&sub={}", e(id), e(sub)),
            Centre::Remote { origin, id } => format!("id={}&origin={}", e(id), e(origin)),
        }
    }
}

// ------------------------------------------------------------------ the local port

/// What an edge points at, from one end.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Peer {
    /// A post, keyed as [`post_key`] keys it.
    Post { origin: String, id: String },
    /// A `{url}` stub: an ancestor without a post.
    Url(String),
    /// A stub naming an id without an origin: counted, but it can't be found.
    Bare(String),
}

/// One edge of a post's lineage, seen from that post.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    /// Up: what the post draws on. Down: the post that draws on it.
    pub peer: Peer,
    pub relation: Relation,
    pub partial: bool,
    pub via: Via,
}

/// The studio's `referencesOf`: the references one document makes, deduped
/// per target (fork > stub > transclusion; partial if any is partial), in
/// first-seen order.
fn dedupe(refs: impl IntoIterator<Item = (Peer, Relation, bool)>) -> Vec<Edge> {
    let mut out: Vec<Edge> = Vec::new();
    for (peer, relation, partial) in refs {
        match out.iter_mut().find(|e| e.peer == peer) {
            None => out.push(Edge {
                peer,
                relation,
                partial,
                via: Via::Reference,
            }),
            Some(e) => {
                e.partial |= partial;
                // `Relation`'s order is quote < stub < fork: the RANK.
                if relation > e.relation {
                    e.relation = relation;
                }
            }
        }
    }
    out
}

fn post_peer(origin: Option<&str>, fallback: Option<&str>, id: &str) -> Peer {
    match origin
        .and_then(normalize_origin)
        .or(fallback.map(str::to_string))
    {
        Some(o) => {
            let (origin, id) = post_key(&o, id);
            Peer::Post { origin, id }
        }
        None => Peer::Bare(id.trim().to_lowercase()),
    }
}

/// A reading row's references.
fn row_refs(r: &ReadingItem) -> Vec<Edge> {
    let own = normalize_origin(&r.origin);
    let mut refs = Vec::new();
    if let Some(s) = &r.stub_of {
        match (&s.id, &s.url) {
            (Some(id), _) => refs.push((
                post_peer(s.origin.as_deref(), None, id),
                Relation::Stubs,
                false,
            )),
            (None, Some(url)) => refs.push((Peer::Url(url.clone()), Relation::Stubs, false)),
            _ => {}
        }
    }
    if let Some(f) = &r.forked_from {
        refs.push((
            post_peer(Some(&f.origin), None, &f.id),
            Relation::Forks,
            false,
        ));
    }
    for t in &r.transclusions {
        refs.push((
            post_peer(t.origin.as_deref(), own.as_deref(), &t.id),
            Relation::Quotes,
            t.selector.is_some(),
        ));
    }
    dedupe(refs)
}

/// One of your published posts' references (`Item::references`: a quote
/// names the held post with that id, else one of yours).
fn own_refs(i: &Item, own_origin: &str, rows: &[ReadingItem]) -> Vec<Edge> {
    dedupe(i.references(own_origin, rows).into_iter().map(|r| {
        (
            Peer::Post {
                origin: r.origin,
                id: r.id,
            },
            r.relation,
            r.partial,
        )
    }))
}

/// The relation a mention names (`stub` | `transclusion` | `fork`).
pub fn mention_relation(rel: Option<&str>) -> Option<Relation> {
    match rel? {
        "stub" => Some(Relation::Stubs),
        "fork" => Some(Relation::Forks),
        "transclusion" => Some(Relation::Quotes),
        _ => None,
    }
}

/// Every post's lineage edges over what this Mac holds: the studio's
/// `loadGraph` + `descendantsOf`, keyed by [`post_key`].
#[derive(Debug, Clone, Default)]
pub struct Local {
    up: HashMap<(String, String), Vec<Edge>>,
    down: HashMap<(String, String), Vec<Edge>>,
}

impl Local {
    /// Build it from the reading rows, your own posts (only published,
    /// public ones count), your blyg's origin and your inbound mentions.
    pub fn build(
        rows: &[ReadingItem],
        own: &[Item],
        own_origin: Option<&str>,
        mentions: &[Mention],
    ) -> Local {
        let mut local = Local::default();
        let mut holders: Vec<((String, String), Vec<Edge>)> = Vec::new();
        for r in rows {
            holders.push((post_key(&r.origin, &r.remote_id), row_refs(r)));
        }
        if let Some(base) = own_origin {
            for i in own
                .iter()
                .filter(|i| i.version > 0 && i.status == Status::Public)
            {
                let Some(sid) = &i.server_id else { continue };
                holders.push((post_key(base, &sid.0), own_refs(i, base, rows)));
            }
        }
        for (holder, edges) in holders {
            for e in &edges {
                let Peer::Post { origin, id } = &e.peer else {
                    continue;
                };
                let down = local.down.entry((origin.clone(), id.clone())).or_default();
                let from = Peer::Post {
                    origin: holder.0.clone(),
                    id: holder.1.clone(),
                };
                // One per holder: the first document seen counts.
                if !down.iter().any(|d| d.peer == from) {
                    down.push(Edge {
                        peer: from,
                        relation: e.relation,
                        partial: e.partial,
                        via: Via::Reference,
                    });
                }
            }
            local.up.entry(holder).or_insert(edges);
        }
        if let Some(base) = own_origin {
            let ours = post_key(base, "").0;
            for m in mentions.iter().filter(|m| m.status == "verified") {
                let (Some(rel), Some(sid)) =
                    (mention_relation(m.relation.as_deref()), &m.source_id)
                else {
                    continue;
                };
                let origin = m
                    .source_origin
                    .as_deref()
                    .and_then(normalize_origin)
                    .map(|o| post_key(&o, "").0)
                    .unwrap_or_default();
                let from = Peer::Post {
                    origin,
                    id: sid.trim().to_lowercase(),
                };
                let target = post_key(&ours, &m.target_item_id);
                let down = local.down.entry(target).or_default();
                if !down.iter().any(|d| d.peer == from) {
                    down.push(Edge {
                        peer: from,
                        relation: rel,
                        partial: false,
                        via: Via::Mention,
                    });
                }
            }
        }
        local
    }

    /// What `(origin, id)` draws on, in the order it names them.
    pub fn up(&self, origin: &str, id: &str) -> &[Edge] {
        self.up
            .get(&post_key(origin, id))
            .map_or(&[], Vec::as_slice)
    }

    /// What draws on `(origin, id)`: held posts first, then mention-only ones.
    pub fn down(&self, origin: &str, id: &str) -> &[Edge] {
        self.down
            .get(&post_key(origin, id))
            .map_or(&[], Vec::as_slice)
    }

    /// The studio's `lineageSummaries` for one post.
    pub fn summary(&self, origin: &str, id: &str) -> LineageSummary {
        LineageSummary {
            up: counts(self.up(origin, id)),
            down: counts(self.down(origin, id)),
        }
    }
}

/// Edges counted by relation.
pub fn counts(edges: &[Edge]) -> RelationCounts {
    let mut c = RelationCounts::default();
    for e in edges {
        c.bump(e.relation);
    }
    c
}

#[cfg(test)]
mod tests {
    //! Mirrors blygger-studio's `test/lineage.test.ts` (at dc632c5), over
    //! what this Mac holds instead of the node's D1 tables.
    use super::*;
    use crate::model::{Kind, LocalId, RemoteRef, ServerId, StubOf, TransclusionRef};

    const OURS: &str = "https://me.example/";
    const THEM: &str = "https://them.example/blyg/";
    const STRANGER: &str = "https://stranger.example/";
    const A: &str = "0000000000000000000000000A";
    const B: &str = "0000000000000000000000000B";
    const C: &str = "0000000000000000000000000C";
    const D: &str = "0000000000000000000000000D";
    const E: &str = "0000000000000000000000000E";
    const H: &str = "0000000000000000000000000H";
    const FRAG: &str = "0000000000000000000000000F";
    const THREAD: &str = "0000000000000000000000000T";

    fn imported(id: &str) -> ReadingItem {
        serde_json::from_value(serde_json::json!({
            "subscription_id": "them", "remote_id": id, "subscription_title": "Their blyg",
            "origin": THEM, "kind": "thread", "state": "current", "version": 2,
            "created": null, "updated": null, "observed_at": "2026-10-02T00:00:00Z",
            "content_md": "", "content_html": "", "author": null, "page": null,
            "thumb": null, "hoppers": [],
        }))
        .unwrap()
    }

    fn quote(id: &str, origin: Option<&str>, partial: bool) -> TransclusionRef {
        TransclusionRef {
            id: id.into(),
            version: Some(1),
            origin: origin.map(str::to_string),
            cited: None,
            selector: partial.then(|| serde_json::json!({ "exact": "Answer me this." })),
        }
    }

    fn mine(id: &str, md: &str) -> Item {
        Item {
            local_id: LocalId(format!("L{id}")),
            server_id: Some(ServerId(id.into())),
            kind: Kind::Thread,
            status: Status::Public,
            version: 1,
            dirty: false,
            content_md: md.into(),
            created: "2026-10-01T00:00:00Z".into(),
            updated: "2026-10-01T00:00:00Z".into(),
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

    fn mention(source_origin: &str, source_id: &str, status: &str, hidden: bool) -> Mention {
        Mention {
            id: format!("m-{source_id}"),
            target_item_id: FRAG.into(),
            status: status.into(),
            relation: Some("stub".into()),
            source: format!("{source_origin}f/{source_id}/"),
            source_origin: Some(source_origin.into()),
            source_id: Some(source_id.into()),
            source_kind: Some("fragment".into()),
            source_version: Some(3),
            source_author: None,
            first_seen: "2026-10-01T00:00:00Z".into(),
            verified_at: None,
            hidden,
        }
    }

    fn rc(stub: u32, transclusion: u32, fork: u32) -> RelationCounts {
        RelationCounts {
            stub,
            transclusion,
            fork,
        }
    }

    #[test]
    fn a_thread_quoting_a_fragment_counts_both_ends() {
        // "one summaries request counts both ends for a page of reading entries"
        let own = [
            mine(FRAG, "Counted fragment."),
            mine(THREAD, &format!("![[{FRAG}]]\n\nCounting.")),
        ];
        let l = Local::build(&[], &own, Some(OURS), &[]);
        assert_eq!(
            l.summary(OURS, THREAD),
            LineageSummary {
                up: rc(0, 1, 0),
                down: rc(0, 0, 0)
            }
        );
        assert_eq!(
            l.summary(OURS, FRAG),
            LineageSummary {
                up: rc(0, 0, 0),
                down: rc(0, 1, 0)
            }
        );
    }

    #[test]
    fn an_imported_entry_counts_its_own_references() {
        // "imported entries are summarised under their imported key"
        let mut h = imported(H);
        h.transclusions = vec![quote(A, None, false)];
        let l = Local::build(&[h], &[], Some(OURS), &[]);
        assert_eq!(l.summary(THEM, H).up, rc(0, 1, 0));
        assert_eq!(l.summary(THEM, H).down, rc(0, 0, 0));
        assert_eq!(
            imported_key("them", H),
            format!("imported:[\"them\",\"{H}\"]")
        );
    }

    #[test]
    fn a_stub_that_also_quotes_us_is_one_partial_descendant() {
        // "an imported stub of ours is one descendant, partial, however many
        // ways it references us"
        let mut a = imported(A);
        a.stub_of = Some(StubOf {
            origin: Some(OURS.into()),
            id: Some(FRAG.into()),
            version: Some(1),
            url: None,
        });
        a.transclusions = vec![quote(FRAG, Some(OURS), true)];
        let own = [mine(FRAG, "Answer me this.")];
        let l = Local::build(&[a], &own, Some(OURS), &[]);
        let down = l.down(OURS, FRAG);
        assert_eq!(down.len(), 1);
        assert_eq!((down[0].relation, down[0].partial), (Relation::Stubs, true));
        assert_eq!(l.summary(OURS, FRAG).down, rc(1, 0, 0));
        // From their side, we are its one ancestor.
        assert_eq!(l.summary(THEM, A).up, rc(1, 0, 0));
    }

    #[test]
    fn a_bare_transclusion_means_the_holders_origin() {
        let b = imported(B);
        let mut c = imported(C);
        c.transclusions = vec![quote(B, None, false)];
        let l = Local::build(&[b, c], &[], Some(OURS), &[]);
        assert_eq!(l.summary(THEM, B).down, rc(0, 1, 0));
        assert_eq!(l.summary(OURS, B).down, rc(0, 0, 0));
    }

    #[test]
    fn a_fork_outranks_its_transclusion_and_a_url_stub_counts_up() {
        let mut d = imported(D);
        d.forked_from = Some(RemoteRef {
            origin: OURS.into(),
            id: FRAG.into(),
            version: 1,
        });
        d.transclusions = vec![quote(FRAG, Some(OURS), false)];
        d.stub_of = Some(StubOf {
            url: Some("https://news.example/story".into()),
            ..Default::default()
        });
        let l = Local::build(&[d], &[mine(FRAG, "Fork me.")], Some(OURS), &[]);
        assert_eq!(l.summary(THEM, D).up, rc(1, 0, 1));
        assert!(
            l.up(THEM, D)
                .iter()
                .any(|e| e.peer == Peer::Url("https://news.example/story".into()))
        );
        assert_eq!(l.summary(OURS, FRAG).down, rc(0, 0, 1));
    }

    #[test]
    fn verified_mentions_add_descendants_without_doubling_held_ones() {
        let mut e = imported(E);
        e.stub_of = Some(StubOf {
            origin: Some(OURS.into()),
            id: Some(FRAG.into()),
            version: Some(1),
            url: None,
        });
        let mentions = [
            mention(THEM, E, "verified", false),
            mention(STRANGER, "0000000000000000000000000S", "verified", false),
            // Hidden from your public page, still known here (as the studio counts it).
            mention(STRANGER, "0000000000000000000000000Q", "verified", true),
            mention(STRANGER, "0000000000000000000000000X", "failed", false),
        ];
        let l = Local::build(&[e], &[mine(FRAG, "Mentioned.")], Some(OURS), &mentions);
        let down = l.down(OURS, FRAG);
        assert_eq!(down.len(), 3);
        assert_eq!(down[0].via, Via::Reference);
        assert!(down[1..].iter().all(|d| d.via == Via::Mention));
        assert_eq!(l.summary(OURS, FRAG).down, rc(3, 0, 0));
    }

    #[test]
    fn unpublished_and_private_posts_of_yours_hold_nothing() {
        let mut draft = mine(THREAD, &format!("![[{FRAG}]]"));
        draft.version = 0;
        let l = Local::build(&[], &[draft, mine(FRAG, "x")], Some(OURS), &[]);
        assert!(l.summary(OURS, FRAG).is_empty());
    }

    #[test]
    fn the_types_match_the_vendored_contract() {
        let doc: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/lineage-glyph.json")).unwrap();
        let schemas = &doc["components"]["schemas"];
        let required = |v: &serde_json::Value| {
            let mut r: Vec<String> = v["required"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_str().unwrap().to_string())
                .collect();
            r.sort();
            r
        };
        let keys = |v: serde_json::Value| {
            let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
            k.sort();
            k
        };
        let node = GraphNode {
            post: GraphPost::default(),
            relation: WireRelation::Fork,
            partial: false,
            via: Via::Reference,
        };
        assert_eq!(
            keys(serde_json::to_value(&node).unwrap()),
            required(&schemas["LineageGlyphNode"])
        );
        assert_eq!(
            keys(serde_json::to_value(GraphPost::default()).unwrap()),
            required(&schemas["LineageGlyphLineage"]["properties"]["node"])
        );
        assert_eq!(
            keys(serde_json::to_value(LineageSummary::default().up).unwrap()),
            required(&schemas["LineageGlyphSummary"]["properties"]["up"])
        );
        assert_eq!(
            doc["paths"]["/api/ext/lineage-glyph/summaries"]["get"]["parameters"][0]["schema"]["description"],
            format!("A JSON array of up to {MAX_SUMMARY_KEYS} reading entry keys.")
        );
    }

    #[test]
    fn the_graph_summary_counts_its_lists_and_parses_the_contract() {
        let g: LineageGraph = serde_json::from_value(serde_json::json!({
            "node": { "origin": OURS, "id": FRAG, "version": 1, "held": "own", "sub": null,
                      "kind": "fragment", "title": "Thin layer", "excerpt": null,
                      "source": "you", "url": null },
            "ancestors": [],
            "descendants": [
                { "origin": THEM, "id": A, "version": 2, "held": "imported", "sub": "them",
                  "kind": "thread", "title": "A reply", "excerpt": null, "source": "Their blyg",
                  "url": null, "relation": "stub", "partial": true, "via": "reference" },
                { "origin": STRANGER, "id": "S", "version": 3, "held": null, "sub": null,
                  "kind": "fragment", "title": null, "excerpt": null, "source": "A Stranger",
                  "url": "https://stranger.example/f/S/", "relation": "transclusion",
                  "partial": false, "via": "mention" }
            ]
        }))
        .unwrap();
        assert_eq!(g.summary().down, rc(1, 1, 0));
        assert_eq!(g.summary().label(), "0 · 2");
        assert_eq!(g.descendants[1].via, Via::Mention);
        assert_eq!(
            Centre::Imported {
                sub: "them".into(),
                id: A.into()
            }
            .query(),
            format!("id={A}&sub=them")
        );
    }
}
