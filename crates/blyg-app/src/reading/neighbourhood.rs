//! --- lineage --- The neighbourhood map: the ⌘J sheet zoomed out (`−`) to
//! two hops around the centre. What it draws on sits above, what draws on
//! it below (one hop, from the sheet's own model: the node's graph when it
//! served one, else this Mac's), and each of those posts' own neighbours a
//! row further out (two hops, from what this Mac holds: `lineage::Local`,
//! one keyed lookup per neighbour, nothing fetched). Not a global graph: a
//! neighbourhood you can walk (⏎ re-centres, ⌫ walks back, `+` returns to
//! the list).
//!
//! Rule 1: the map shows relations and kinds, never a count of its own; the
//! only numbers on the sheet stay the glyph's (the exception).
//!
//! Pure model and layout; `neighbourhood_view.rs` draws it.

use std::collections::HashMap;

use blyg_core::lineage::{Local, Peer};
use blyg_core::profile::{Relation, same_origin};
use blyg_core::{Item, ReadingItem, post_key};

use super::lineage_vm::{self, Model};
use super::vm;

/// Nodes shown per row; the rest scroll with the selection (a "+N" says how
/// many are off each side).
pub const ROW_MAX: usize = 7;
/// Second-hop posts at most, each side (above, below): a busy neighbour
/// can't flood the map.
pub const HOP2_MAX: usize = 21;
/// The rows, top to bottom: two hops up, one up, the centre, one down, two
/// down.
pub const ROWS: usize = 5;
pub const CENTRE_ROW: usize = 2;

/// One post (or page) on the map.
#[derive(Debug, Clone, PartialEq)]
pub struct MapNode {
    pub origin: String,
    /// Empty for a page (a `{url}` stub), which can't be the centre.
    pub id: String,
    pub title: String,
    pub who: String,
    /// One of your posts (drawn shaded).
    pub own: bool,
    /// Held on this Mac (a reading row or one of yours).
    pub held: bool,
    /// 0 the centre, 1 or 2 hops out (two hops drawn dashed).
    pub hop: u8,
    /// How it relates to the node it hangs off (its colour); `None` for the
    /// centre.
    pub relation: Option<Relation>,
    pub partial: bool,
}

impl MapNode {
    /// A post, so it can be the centre.
    pub fn is_post(&self) -> bool {
        !self.id.is_empty() && !self.origin.is_empty()
    }
}

/// `from` is drawn on by `to` (an ancestor above its descendant).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapEdge {
    pub from: usize,
    pub to: usize,
    pub relation: Relation,
    pub partial: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Map {
    pub nodes: Vec<MapNode>,
    pub edges: Vec<MapEdge>,
    /// Node indices per row, left to right.
    pub rows: [Vec<usize>; ROWS],
}

/// What the keyboard is on: a row and a place in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapSel {
    pub row: usize,
    pub ix: usize,
}

impl MapSel {
    pub const CENTRE: MapSel = MapSel {
        row: CENTRE_ROW,
        ix: 0,
    };
}

/// Posts as this Mac holds them, looked up by id (one pass over the rows,
/// so a map of a few dozen posts never scans thousands of rows per node).
pub struct Index<'a> {
    by_id: HashMap<String, Vec<&'a ReadingItem>>,
    own: HashMap<String, &'a Item>,
    own_origin: Option<&'a str>,
}

impl<'a> Index<'a> {
    pub fn new<R: std::borrow::Borrow<ReadingItem>>(
        rows: &'a [R],
        own: &'a [Item],
        own_origin: Option<&'a str>,
    ) -> Self {
        let mut by_id: HashMap<String, Vec<&ReadingItem>> = HashMap::new();
        for r in rows {
            let r: &'a ReadingItem = r.borrow();
            by_id
                .entry(r.remote_id.trim().to_lowercase())
                .or_default()
                .push(r);
        }
        let own = own
            .iter()
            .filter(|i| i.version > 0)
            .filter_map(|i| Some((i.server_id.as_ref()?.0.trim().to_lowercase(), i)))
            .collect();
        Index {
            by_id,
            own,
            own_origin,
        }
    }

    fn is_own_origin(&self, origin: &str) -> bool {
        self.own_origin.is_some_and(|o| same_origin(o, origin))
    }

    /// A post found by its id alone (a `[[id]]` link names no blyg): yours
    /// first, then a held one. (origin, title, who, own).
    pub fn find_id(&self, id: &str) -> Option<(String, String, String, bool)> {
        let key = id.trim().to_lowercase();
        if let (Some(o), Some(i)) = (self.own_origin, self.own.get(&key)) {
            return Some((o.to_string(), i.title(), "you".into(), true));
        }
        let r = self.by_id.get(&key)?.first()?;
        Some((
            r.origin.clone(),
            vm::post_title(r),
            lineage_vm::who_of(r),
            self.is_own_origin(&r.origin),
        ))
    }

    /// (title, who, held, own) of a post, as well as this Mac can say.
    pub fn describe(&self, origin: &str, id: &str) -> (String, String, bool, bool) {
        let key = id.trim().to_lowercase();
        let own = self.is_own_origin(origin);
        if own && let Some(i) = self.own.get(&key) {
            return (i.title(), "you".into(), true, true);
        }
        if let Some(r) = self
            .by_id
            .get(&key)
            .and_then(|rs| rs.iter().find(|r| same_origin(&r.origin, origin)))
        {
            return (vm::post_title(r), lineage_vm::who_of(r), true, own);
        }
        let host = if origin.is_empty() {
            "an unknown blyg".to_string()
        } else {
            vm::host(origin)
        };
        (format!("A post on {host}"), host, false, own)
    }
}

/// The map around `model`'s centre: its one-hop neighbours as the sheet
/// shows them, then each neighbour's own neighbours from `local`.
pub fn build(model: &Model, local: &Local, index: &Index) -> Map {
    let mut map = Map::default();
    let mut keys: HashMap<(String, String), usize> = HashMap::new();
    let c = &model.centre;
    let centre = MapNode {
        origin: c.origin.clone(),
        id: c.id.clone(),
        title: c.title.clone(),
        who: c.who.clone(),
        own: c.own || index.is_own_origin(&c.origin),
        held: c.held.is_some() || c.own,
        hop: 0,
        relation: None,
        partial: false,
    };
    keys.insert(post_key(&centre.origin, &centre.id), 0);
    map.nodes.push(centre);
    map.rows[CENTRE_ROW].push(0);

    // One hop: the sheet's own neighbours, in its order.
    for (row, up, nodes) in [(1, true, &model.ups), (3, false, &model.downs)] {
        for n in nodes.iter() {
            let key = if n.is_post() {
                post_key(&n.origin, &n.id)
            } else {
                (n.origin.clone(), String::new())
            };
            if keys.contains_key(&key) {
                continue;
            }
            let ix = map.nodes.len();
            map.nodes.push(MapNode {
                origin: n.origin.clone(),
                id: n.id.clone(),
                title: n.title.clone(),
                who: n.who.clone(),
                own: n.who == "you" || (n.is_post() && index.is_own_origin(&n.origin)),
                held: n.held.is_some() || n.who == "you",
                hop: 1,
                relation: Some(n.relation),
                partial: n.partial,
            });
            keys.insert(key, ix);
            map.rows[row].push(ix);
            let (from, to) = if up { (ix, 0) } else { (0, ix) };
            map.edges.push(MapEdge {
                from,
                to,
                relation: n.relation,
                partial: n.partial,
            });
        }
    }

    // Two hops: each neighbour's neighbours, a row further out.
    let firsts: Vec<(usize, usize)> = [1usize, 3]
        .iter()
        .flat_map(|&row| map.rows[row].iter().map(move |&ix| (row, ix)))
        .collect();
    let mut added = [0usize; 2];
    for (row, ix) in firsts {
        let (origin, id) = (map.nodes[ix].origin.clone(), map.nodes[ix].id.clone());
        if id.is_empty() {
            continue; // a page has no neighbours here
        }
        let out_row = if row == 1 { 0 } else { 4 };
        let side = usize::from(row == 3);
        let ups = local.up(&origin, &id).iter().map(|e| (e, true));
        let downs = local.down(&origin, &id).iter().map(|e| (e, false));
        for (e, is_up) in ups.chain(downs) {
            let (peer_origin, peer_id) = match &e.peer {
                Peer::Post { origin, id } => (origin.clone(), id.clone()),
                Peer::Url(u) => (u.clone(), String::new()),
                Peer::Bare(_) => continue,
            };
            let key = if peer_id.is_empty() {
                (peer_origin.clone(), String::new())
            } else {
                post_key(&peer_origin, &peer_id)
            };
            let other = match keys.get(&key) {
                Some(&o) => o,
                None => {
                    if added[side] >= HOP2_MAX {
                        continue;
                    }
                    added[side] += 1;
                    let (title, who, held, own) = if peer_id.is_empty() {
                        (
                            format!("A page on {}", vm::host(&peer_origin)),
                            vm::host(&peer_origin),
                            false,
                            false,
                        )
                    } else {
                        index.describe(&peer_origin, &peer_id)
                    };
                    let o = map.nodes.len();
                    map.nodes.push(MapNode {
                        origin: peer_origin,
                        id: peer_id,
                        title,
                        who,
                        own,
                        held,
                        hop: 2,
                        relation: Some(e.relation),
                        partial: e.partial,
                    });
                    keys.insert(key, o);
                    map.rows[out_row].push(o);
                    o
                }
            };
            if other == ix {
                continue;
            }
            let (from, to) = if is_up { (other, ix) } else { (ix, other) };
            let seen = map
                .edges
                .iter()
                .any(|m| (m.from, m.to) == (from, to) || (m.from, m.to) == (to, from));
            if !seen {
                map.edges.push(MapEdge {
                    from,
                    to,
                    relation: e.relation,
                    partial: e.partial,
                });
            }
        }
    }
    map
}

impl Map {
    pub fn node_at(&self, sel: MapSel) -> Option<&MapNode> {
        self.rows
            .get(sel.row)?
            .get(sel.ix)
            .and_then(|&i| self.nodes.get(i))
    }

    /// Where an arrow key moves `sel`: along its row, or to the nearest node
    /// in the next row that has any (skipping empty rows).
    pub fn step(&self, sel: MapSel, key: &str) -> MapSel {
        let len = |r: usize| self.rows[r].len();
        match key {
            "left" => MapSel {
                ix: sel.ix.saturating_sub(1),
                ..sel
            },
            "right" => MapSel {
                ix: (sel.ix + 1).min(len(sel.row).saturating_sub(1)),
                ..sel
            },
            "up" | "down" => {
                let rows: Vec<usize> = if key == "up" {
                    (0..sel.row).rev().collect()
                } else {
                    (sel.row + 1..ROWS).collect()
                };
                let Some(row) = rows.into_iter().find(|&r| len(r) > 0) else {
                    return sel;
                };
                // The same place across, as a fraction of the row.
                let frac = if len(sel.row) <= 1 {
                    0.5
                } else {
                    sel.ix as f32 / (len(sel.row) - 1) as f32
                };
                let ix = (frac * (len(row) - 1) as f32).round() as usize;
                MapSel { row, ix }
            }
            _ => sel,
        }
    }

    /// The node index `sel` is on.
    pub fn index_of(&self, sel: MapSel) -> Option<usize> {
        self.rows.get(sel.row)?.get(sel.ix).copied()
    }

    /// Where node `node` sits (row, place), if it's on the map.
    #[cfg(test)]
    pub fn sel_of(&self, node: usize) -> Option<MapSel> {
        self.rows.iter().enumerate().find_map(|(row, ixs)| {
            ixs.iter()
                .position(|&i| i == node)
                .map(|ix| MapSel { row, ix })
        })
    }
}

// ------------------------------------------------------------------ layout

/// The slice of a row on screen: up to `ROW_MAX`, around `sel` when the
/// selection is in it.
pub fn window(n: usize, sel: Option<usize>) -> std::ops::Range<usize> {
    if n <= ROW_MAX {
        return 0..n;
    }
    let s = sel
        .unwrap_or(0)
        .saturating_sub(ROW_MAX / 2)
        .min(n - ROW_MAX);
    s..s + ROW_MAX
}

/// The rows' heights, as fractions of the stage.
const ROW_Y: [f32; ROWS] = [0.085, 0.29, 0.5, 0.71, 0.915];

/// Laid out on a `w` × `h` stage.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    /// Each node's centre, when it's on screen.
    pub at: Vec<Option<(f32, f32)>>,
    /// Per row: how many are off to the left and to the right.
    pub more: [(usize, usize); ROWS],
}

/// Where every node goes: rows top to bottom, each row's shown nodes spread
/// evenly across, the centre in the middle.
pub fn layout(map: &Map, sel: MapSel, w: f32, h: f32) -> Layout {
    let mut out = Layout {
        at: vec![None; map.nodes.len()],
        more: [(0, 0); ROWS],
    };
    let margin = 36.;
    for (row, ixs) in map.rows.iter().enumerate() {
        let win = window(ixs.len(), (sel.row == row).then_some(sel.ix));
        out.more[row] = (win.start, ixs.len() - win.end);
        let n = win.len();
        if n == 0 {
            continue;
        }
        let slot = (w - 2. * margin) / n as f32;
        let y = h * ROW_Y[row];
        for (k, &node) in ixs[win].iter().enumerate() {
            let x = if n == 1 {
                w / 2.
            } else {
                margin + slot * (k as f32 + 0.5)
            };
            out.at[node] = Some((x, y));
        }
    }
    out
}

/// A node's title on the map: at most `width` characters, ending in "…".
pub fn short(title: &str, width: usize) -> String {
    let t = title.trim();
    if t.chars().count() <= width {
        return t.to_string();
    }
    let mut s: String = t.chars().take(width.saturating_sub(1)).collect();
    s = s.trim_end().to_string();
    s.push('…');
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use blyg_core::lineage::Local;
    use blyg_core::{Kind, ReadingItem, TransclusionRef};

    const A: &str = "https://a.example.org/";
    const B: &str = "https://b.example.org/";

    fn row(origin: &str, id: &str, text: &str) -> ReadingItem {
        ReadingItem {
            subscription_id: format!("sub-{origin}"),
            remote_id: id.into(),
            subscription_title: vm::host(origin),
            origin: origin.into(),
            kind: Kind::Thread,
            state: "current".into(),
            version: 1,
            created: None,
            updated: None,
            observed_at: "2026-11-01T00:00:00Z".into(),
            content_md: text.into(),
            content_html: String::new(),
            author: None,
            page: None,
            thumb: None,
            hoppers: vec![],
            pinned_version_retained: None,
            read_version: None,
            stub_of: None,
            forked_from: None,
            transclusions: vec![],
            lineage_known: false,
            source_url: None,
            generated: None,
        }
    }

    fn quoting(mut r: ReadingItem, origin: &str, id: &str) -> ReadingItem {
        r.transclusions.push(TransclusionRef {
            id: id.into(),
            origin: Some(origin.into()),
            version: Some(1),
            selector: None,
            cited: None,
        });
        r
    }

    /// root ← mid ← top (top quotes mid, mid quotes root); side quotes mid
    /// too; far quotes top.
    fn chain() -> Vec<ReadingItem> {
        vec![
            row(A, "root", "Root post"),
            quoting(row(A, "mid", "Middle post"), A, "root"),
            quoting(row(B, "top", "Top post"), A, "mid"),
            quoting(row(B, "side", "Side post"), A, "mid"),
            quoting(row(A, "far", "Far post"), B, "top"),
        ]
    }

    fn map_of(rows: &[ReadingItem], origin: &str, id: &str) -> Map {
        let local = Local::build(rows, &[], None, &[]);
        let src = lineage_vm::Sources {
            rows,
            own: &[],
            own_origin: None,
            mentions: &[],
            local: &local,
        };
        let model = lineage_vm::build(origin, id, &src);
        build(&model, &local, &Index::new(rows, &[], None))
    }

    fn titles(m: &Map, row: usize) -> Vec<&str> {
        m.rows[row]
            .iter()
            .map(|&i| m.nodes[i].title.as_str())
            .collect()
    }

    #[test]
    fn two_hops_each_way_in_their_rows() {
        let rows = chain();
        let m = map_of(&rows, A, "mid");
        assert_eq!(titles(&m, CENTRE_ROW), ["Middle post"]);
        assert_eq!(titles(&m, 1), ["Root post"]);
        assert_eq!(titles(&m, 3), ["Top post", "Side post"]);
        // Root draws on nothing; Top is drawn on by Far.
        assert!(titles(&m, 0).is_empty());
        assert_eq!(titles(&m, 4), ["Far post"]);
        let far = m.nodes.iter().position(|n| n.title == "Far post").unwrap();
        assert_eq!(m.nodes[far].hop, 2);
        assert_eq!(m.nodes[far].relation, Some(Relation::Quotes));
        // Edges run from what's drawn on to what draws on it.
        let top = m.nodes.iter().position(|n| n.title == "Top post").unwrap();
        assert!(m.edges.contains(&MapEdge {
            from: top,
            to: far,
            relation: Relation::Quotes,
            partial: false
        }));
        assert!(m.edges.iter().any(|e| e.from == 0 && e.to == top));
    }

    #[test]
    fn a_post_is_on_the_map_once_and_cross_links_are_edges() {
        let mut rows = chain();
        // Side also quotes Top: both are one hop down, so it's an edge
        // between them, not a new node.
        rows[3] = quoting(rows[3].clone(), B, "top");
        let m = map_of(&rows, A, "mid");
        assert_eq!(m.nodes.len(), 5, "{:?}", m.nodes);
        let top = m.nodes.iter().position(|n| n.title == "Top post").unwrap();
        let side = m.nodes.iter().position(|n| n.title == "Side post").unwrap();
        assert!(m.edges.iter().any(|e| e.from == top && e.to == side));
        // Re-centred on Far, Mid is two hops up.
        let m = map_of(&rows, A, "far");
        assert_eq!(titles(&m, 1), ["Top post"]);
        assert!(titles(&m, 0).contains(&"Middle post"));
    }

    #[test]
    fn the_second_hop_is_capped() {
        let mut rows = vec![
            row(A, "hub", "Hub"),
            quoting(row(A, "c", "Centre"), A, "hub"),
        ];
        for i in 0..60 {
            rows.push(quoting(row(B, &format!("q{i}"), "A quote"), A, "hub"));
        }
        let m = map_of(&rows, A, "c");
        assert_eq!(m.rows[0].len(), HOP2_MAX);
        let l = layout(&m, MapSel::CENTRE, 540., 380.);
        assert_eq!(l.more[0], (0, HOP2_MAX - ROW_MAX));
        assert_eq!(l.at.iter().flatten().count(), 2 + ROW_MAX);
    }

    #[test]
    fn arrows_walk_rows_and_skip_empty_ones() {
        let rows = chain();
        let m = map_of(&rows, A, "mid");
        let s = MapSel::CENTRE;
        let up = m.step(s, "up");
        assert_eq!(m.node_at(up).unwrap().title, "Root post");
        // Nothing above Root: up stays.
        assert_eq!(m.step(up, "up"), up);
        let down = m.step(s, "down");
        assert_eq!(down.row, 3);
        let right = m.step(down, "right");
        assert_eq!(m.node_at(right).unwrap().title, "Side post");
        assert_eq!(m.step(right, "right"), right, "clamped");
        let far = m.step(down, "down");
        assert_eq!(m.node_at(far).unwrap().title, "Far post");
        assert_eq!(m.step(m.step(far, "up"), "up").row, CENTRE_ROW);
        assert_eq!(m.sel_of(0), Some(MapSel::CENTRE));
    }

    #[test]
    fn layout_puts_rows_top_to_bottom_and_spreads_them() {
        let rows = chain();
        let m = map_of(&rows, A, "mid");
        let l = layout(&m, MapSel::CENTRE, 540., 380.);
        let at = |t: &str| l.at[m.nodes.iter().position(|n| n.title == t).unwrap()].unwrap();
        assert_eq!(at("Middle post"), (270., 190.));
        assert!(at("Root post").1 < 190.);
        assert!(at("Far post").1 > at("Top post").1);
        assert!(at("Top post").0 < at("Side post").0);
    }

    #[test]
    fn unheld_neighbours_are_described_by_their_blyg() {
        let rows = vec![quoting(row(A, "c", "Centre"), B, "gone")];
        let m = map_of(&rows, A, "c");
        let n = &m.nodes[m.rows[1][0]];
        assert_eq!(n.title, "A post on b.example.org");
        assert!(!n.held);
        assert_eq!(short("Three things on the bench", 12), "Three thing…");
        assert_eq!(short("Short", 12), "Short");
    }

    #[test]
    fn building_a_map_among_thousands_of_posts_is_quick() {
        let mut rows = chain();
        for i in 0..4000 {
            rows.push(row(B, &format!("bulk{i}"), "Bulk"));
        }
        let local = Local::build(&rows, &[], None, &[]);
        let src = lineage_vm::Sources {
            rows: &rows,
            own: &[],
            own_origin: None,
            mentions: &[],
            local: &local,
        };
        let model = lineage_vm::build(A, "mid", &src);
        let t = std::time::Instant::now();
        let m = build(&model, &local, &Index::new(&rows, &[], None));
        assert_eq!(m.nodes.len(), 5);
        // Generous for an unoptimised test build; a release build is ~1 ms.
        assert!(t.elapsed().as_millis() < 400, "{:?}", t.elapsed());
    }
}
