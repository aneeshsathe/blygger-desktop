//! --- lineage --- The neighbourhood map, drawn inside the ⌘J sheet: `−`
//! zooms out to two hops, arrows move, ⏎ re-centres on the highlighted post
//! (the map is rebuilt around it), ⌫ walks back, `+` returns to the list,
//! `o` opens, esc closes. Keyboard first; a click selects and a second
//! click re-centres. Model and layout: `neighbourhood.rs`.

use blyg_core::profile::Relation;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::lineage::{hex_points, rel_color};
use super::neighbourhood::{self, Index, Map, MapSel};
use super::{RSheet, vm};
use crate::app::MainView;

/// The map in the sheet: the posts, and what the keyboard is on.
pub struct MapState {
    pub map: Map,
    pub sel: MapSel,
}

const MAP_W: f32 = 540.;
const MAP_H: f32 = 380.;
const HEX_CENTRE: f32 = 22.;
const HEX_NODE: f32 = 14.;
const LABEL_W: f32 = 96.;

/// `−` and `+` as the sheet reads them, whatever the layout calls them
/// (`+` is ⇧= on most; `=` zooms in too, as it does elsewhere).
pub(super) fn zoom_key(k: &Keystroke) -> Option<&'static str> {
    match (k.key.as_str(), k.key_char.as_deref()) {
        ("-" | "minus", _) | (_, Some("-" | "−")) => Some("-"),
        ("+" | "plus" | "=", _) | (_, Some("+")) => Some("+"),
        _ => None,
    }
}

fn kind_noun(rel: Relation) -> &'static str {
    match rel {
        Relation::Forks => "a fork",
        Relation::Stubs => "a reply",
        Relation::Quotes => "a quote",
    }
}

impl MainView {
    /// The map around the open sheet's centre, from what the sheet shows
    /// one hop out and what this Mac holds two hops out.
    fn lineage_build_map(&self) -> Option<Map> {
        let Some(RSheet::Lineage(s)) = self.reading.sheet.as_ref() else {
            return None;
        };
        let model = &s.model;
        Some(self.with_lineage_src(|src| {
            let index = Index::new(src.rows, src.own, src.own_origin);
            neighbourhood::build(model, src.local, &index)
        }))
    }

    /// `−` zooms out to the map; `+` back to the one-hop list.
    pub(crate) fn lineage_zoom(&mut self, out: bool, cx: &mut Context<Self>) {
        let map = if out { self.lineage_build_map() } else { None };
        if let Some(s) = self.lineage_mut() {
            s.ring = false;
            s.act = None;
            s.map = map.map(|map| MapState {
                map,
                sel: MapSel::CENTRE,
            });
        }
        cx.notify();
    }

    /// The map is up (tests and the tour).
    pub(crate) fn lineage_map_open(&self) -> bool {
        matches!(&self.reading.sheet, Some(RSheet::Lineage(s)) if s.map.is_some())
    }

    /// The highlighted node's title on the map (tests).
    #[cfg(test)]
    pub(crate) fn lineage_map_selected(&self) -> Option<String> {
        match &self.reading.sheet {
            Some(RSheet::Lineage(s)) => s
                .map
                .as_ref()
                .and_then(|m| m.map.node_at(m.sel))
                .map(|n| n.title.clone()),
            _ => None,
        }
    }

    /// The map's rows, as titles (tests).
    #[cfg(test)]
    pub(crate) fn lineage_map_rows(&self) -> Option<Vec<Vec<String>>> {
        match &self.reading.sheet {
            Some(RSheet::Lineage(s)) => s.map.as_ref().map(|m| {
                m.map
                    .rows
                    .iter()
                    .map(|r| r.iter().map(|&i| m.map.nodes[i].title.clone()).collect())
                    .collect()
            }),
            _ => None,
        }
    }

    /// The centre or its neighbours changed: draw the map again around it.
    /// The highlight stays where it was when the centre is the same post.
    pub(super) fn lineage_rebuild_map(&mut self) {
        let zoomed = matches!(&self.reading.sheet, Some(RSheet::Lineage(s)) if s.map.is_some());
        if !zoomed {
            return;
        }
        let Some(map) = self.lineage_build_map() else {
            return;
        };
        if let Some(s) = self.lineage_mut()
            && let Some(state) = s.map.as_mut()
        {
            let same = state.map.nodes.first().map(|n| (&n.origin, &n.id))
                == map.nodes.first().map(|n| (&n.origin, &n.id));
            let sel = if same && map.node_at(state.sel).is_some() {
                state.sel
            } else {
                MapSel::CENTRE
            };
            *state = MapState { map, sel };
        }
    }

    /// A key on the map; true when it was the map's.
    pub(super) fn lineage_map_key(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(s) = self.lineage_mut() else {
            return false;
        };
        let Some(state) = s.map.as_mut() else {
            return false;
        };
        let node = state.map.node_at(state.sel).cloned();
        let on_centre = state.sel == MapSel::CENTRE;
        match key {
            "up" | "down" | "left" | "right" => {
                state.sel = state.map.step(state.sel, key);
            }
            "enter" | "space" => match node {
                _ if on_centre => self.lineage_zoom(false, cx),
                Some(n) if n.is_post() => self.lineage_recentre(n.origin, n.id, false, cx),
                _ => {}
            },
            "backspace" => {
                let prev = self.lineage_mut().and_then(|s| s.hist.pop());
                if let Some((o, i)) = prev {
                    self.lineage_recentre(o, i, true, cx);
                }
            }
            "+" => self.lineage_zoom(false, cx),
            "-" => {}
            "o" => match node {
                Some(n) if !n.is_post() => {
                    if n.origin.starts_with("https://") || n.origin.starts_with("http://") {
                        cx.open_url(&n.origin);
                    }
                }
                Some(n) => {
                    // Opened as the list's `o` opens it (the reader's own
                    // open: a held post is marked read on the way).
                    self.close_reading_sheet(window, cx);
                    self.open_original(n.origin, n.id, None, window, cx);
                }
                None => {}
            },
            "escape" => self.close_reading_sheet(window, cx),
            _ => return false,
        }
        cx.notify();
        true
    }

    fn lineage_map_click(&mut self, sel: MapSel, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.lineage_mut().and_then(|s| s.map.as_mut()) else {
            return;
        };
        if state.sel == sel {
            self.lineage_map_key("enter", window, cx);
        } else {
            state.sel = sel;
            cx.notify();
        }
    }

    // ------------------------------------------------------------ drawing

    pub(super) fn render_lineage_map(&self, m: &MapState, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette.on_page();
        let map = &m.map;
        let lay = neighbourhood::layout(map, m.sel, MAP_W, MAP_H);
        let selected = map.index_of(m.sel);
        let radius = |i: usize| if i == 0 { HEX_CENTRE } else { HEX_NODE };

        struct Edge {
            from: (f32, f32),
            to: (f32, f32),
            color: Hsla,
            partial: bool,
        }
        let edges: Vec<Edge> = map
            .edges
            .iter()
            .filter_map(|e| {
                let (a, b) = (lay.at[e.from]?, lay.at[e.to]?);
                // Leave each hexagon from the side facing the other.
                let (ra, rb) = (radius(e.from), radius(e.to));
                let dir = if b.1 >= a.1 { 1. } else { -1. };
                let far = map.nodes[e.from].hop == 2 || map.nodes[e.to].hop == 2;
                Some(Edge {
                    from: (a.0, a.1 + dir * ra * 0.87),
                    to: (b.0, b.1 - dir * rb * 0.87),
                    color: rel_color(&p, e.relation).opacity(if far { 0.55 } else { 0.9 }),
                    partial: e.partial,
                })
            })
            .collect();
        struct Hex {
            at: (f32, f32),
            r: f32,
            fill: Hsla,
            line: Hsla,
            width: f32,
            dashed: bool,
        }
        let hexes: Vec<Hex> = map
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| {
                let at = lay.at[i]?;
                let sel = selected == Some(i);
                Some(Hex {
                    at,
                    r: radius(i),
                    fill: if n.own { p.accent.opacity(0.16) } else { p.bg },
                    line: if sel {
                        p.accent
                    } else {
                        n.relation.map_or(p.ink, |r| rel_color(&p, r))
                    },
                    width: if sel { 2.4 } else { 1.4 },
                    dashed: n.hop == 2,
                })
            })
            .collect();
        let painter = canvas(
            |_, _, _| (),
            move |b, (), window, _| {
                let at = |x: f32, y: f32| point(b.origin.x + px(x), b.origin.y + px(y));
                for e in &edges {
                    let mut pb = PathBuilder::stroke(px(if e.partial { 2. } else { 1.5 }));
                    if e.partial {
                        pb = pb.dash_array(&[px(1.5), px(3.5)]);
                    }
                    let my = (e.from.1 + e.to.1) / 2.;
                    pb.move_to(at(e.from.0, e.from.1));
                    pb.cubic_bezier_to(at(e.to.0, e.to.1), at(e.from.0, my), at(e.to.0, my));
                    if let Ok(path) = pb.build() {
                        window.paint_path(path, e.color);
                    }
                }
                for h in &hexes {
                    let pts = hex_points(h.at.0, h.at.1, h.r);
                    let poly = |pb: &mut PathBuilder| {
                        pb.move_to(at(pts[0].0, pts[0].1));
                        for q in &pts[1..] {
                            pb.line_to(at(q.0, q.1));
                        }
                        pb.close();
                    };
                    let mut f = PathBuilder::fill();
                    poly(&mut f);
                    if let Ok(path) = f.build() {
                        window.paint_path(path, h.fill);
                    }
                    let mut l = PathBuilder::stroke(px(h.width));
                    if h.dashed {
                        l = l.dash_array(&[px(3.), px(2.5)]);
                    }
                    poly(&mut l);
                    if let Ok(path) = l.build() {
                        window.paint_path(path, h.line);
                    }
                }
            },
        )
        .absolute()
        .inset_0();

        let mut labels: Vec<AnyElement> = Vec::new();
        for (row, ixs) in map.rows.iter().enumerate() {
            for (k, &i) in ixs.iter().enumerate() {
                let Some((x, y)) = lay.at[i] else { continue };
                let n = &map.nodes[i];
                let r = radius(i);
                let sel = MapSel { row, ix: k };
                let width = if i == 0 { 150. } else { LABEL_W };
                labels.push(
                    div()
                        .id(("lineage-map-node", i))
                        .absolute()
                        .left(px(x - width / 2.))
                        .top(px(y - r))
                        .w(px(width))
                        .h(px(2. * r + 26.))
                        .flex()
                        .flex_col()
                        .items_center()
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.lineage_map_click(sel, window, cx)
                        }))
                        .child(div().h(px(2. * r + 1.)).flex_none())
                        .child(
                            div()
                                .max_w_full()
                                .truncate()
                                .text_size(px(if i == 0 { 11.5 } else { 10.5 }))
                                .when(i == 0 || selected == Some(i), |d| {
                                    d.font_weight(FontWeight::SEMIBOLD)
                                })
                                .text_color(if n.hop == 2 { p.muted } else { p.ink })
                                .child(neighbourhood::short(
                                    &n.title,
                                    if i == 0 { 24 } else { 15 },
                                )),
                        )
                        .child(
                            div()
                                .max_w_full()
                                .truncate()
                                .text_size(px(9.5))
                                .text_color(p.muted)
                                .child(if n.own {
                                    "you".to_string()
                                } else {
                                    n.who.clone()
                                }),
                        )
                        .into_any_element(),
                );
            }
        }
        for (row, (left, right)) in lay.more.iter().enumerate() {
            let y = MAP_H * [0.085, 0.29, 0.5, 0.71, 0.915][row] - 8.;
            for (n, is_left) in [(*left, true), (*right, false)] {
                if n == 0 {
                    continue;
                }
                labels.push(
                    div()
                        .absolute()
                        .top(px(y))
                        .when(is_left, |d| d.left(px(4.)))
                        .when(!is_left, |d| d.right(px(4.)))
                        .px(px(5.))
                        .rounded(px(6.))
                        .bg(p.sel)
                        .text_size(px(10.))
                        .text_color(p.muted)
                        .child(format!("+{n}"))
                        .into_any_element(),
                );
            }
        }
        div()
            .id("lineage-map")
            .debug_selector(|| "lineage-map".into())
            .relative()
            .flex_none()
            .w(px(MAP_W))
            .h(px(MAP_H))
            .rounded(px(10.))
            .border_1()
            .border_color(p.line)
            .bg(p.sel.opacity(0.35))
            .overflow_hidden()
            .child(painter)
            .children(labels)
            .into_any_element()
    }

    pub(super) fn render_lineage_map_side(
        &self,
        s: &super::lineage::Sheet,
        m: &MapState,
    ) -> AnyElement {
        let p = self.palette.on_page();
        let node = m.map.node_at(m.sel);
        let side = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .text_size(px(12.5))
            .line_height(relative(1.45));
        let Some(n) = node else {
            return side.into_any_element();
        };
        let how = match (m.sel.row, n.relation) {
            (_, None) => "the centre".to_string(),
            (1, Some(r)) => format!("the centre {} it", lineage_vm_words(r, n.partial, true)),
            (3, Some(r)) => lineage_vm_words(r, n.partial, false).to_string(),
            (_, Some(r)) => format!("two hops out: {} of a neighbour", kind_noun(r)),
        };
        let swatch = |rel: Relation, what: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(px(5.))
                .child(
                    div()
                        .w(px(10.))
                        .h(px(3.))
                        .rounded(px(1.5))
                        .bg(rel_color(&p, rel)),
                )
                .child(what)
        };
        let keys: Vec<(&str, &str)> = [
            Some(("← ↑ ↓ →", "move")),
            Some((
                "⏎",
                if m.sel == MapSel::CENTRE {
                    "back to the list"
                } else {
                    "make it the centre"
                },
            )),
            (!s.hist.is_empty()).then_some(("⌫", "back")),
            Some(("+", "zoom in: the list")),
            Some(("o", "open in the reader")),
            Some(("esc", "close")),
        ]
        .into_iter()
        .flatten()
        .collect();
        side.child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(px(14.))
                .child(n.title.clone()),
        )
        .child(
            div()
                .text_color(p.muted)
                .text_size(px(11.5))
                .child(if n.own {
                    format!("you · {}", vm::host(&n.origin))
                } else {
                    format!("{} · {}", n.who, vm::host(&n.origin))
                }),
        )
        .child(div().mt(px(4.)).text_size(px(12.)).child(how))
        .when(!n.held && n.is_post(), |d| {
            d.child(
                div()
                    .text_size(px(12.))
                    .text_color(p.muted)
                    .child("Not held on this Mac · o opens it from its blyg"),
            )
        })
        .child(
            div()
                .mt(px(10.))
                .flex()
                .flex_wrap()
                .gap(px(10.))
                .text_size(px(11.))
                .text_color(p.muted)
                .child(swatch(Relation::Quotes, "quote"))
                .child(swatch(Relation::Stubs, "reply"))
                .child(swatch(Relation::Forks, "fork")),
        )
        .child(
            div()
                .mt(px(2.))
                .text_size(px(11.))
                .text_color(p.muted)
                .child("Dashed: two hops out, from what this Mac holds · shaded: yours"),
        )
        .child(
            div()
                .mt(px(10.))
                .flex()
                .flex_col()
                .gap(px(3.))
                .text_size(px(11.5))
                .text_color(p.muted)
                .children(keys.into_iter().map(|(k, what)| {
                    div()
                        .flex()
                        .gap(px(8.))
                        .child(div().w(px(56.)).flex_none().text_color(p.ink).child(k))
                        .child(div().flex_1().min_w_0().child(what))
                })),
        )
        .into_any_element()
    }
}

fn lineage_vm_words(rel: Relation, partial: bool, up: bool) -> &'static str {
    super::lineage_vm::relation_words(rel, partial, up)
}
