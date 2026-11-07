//! --- lineage --- Lineage while writing: one quiet line under the editor on
//! the Posts screen, the open post's lineage glyph and "draws on N · drawn
//! on by M", that opens (a click, or ⌘J on a post that isn't published)
//! into compact lists. A click on a row opens that post: yours in the
//! editor, anyone else's in Reading (as following a quote does).
//!
//! - A published post of yours: what it draws on and what draws on it, from
//!   the same sources as the Reading lineage (`lineage::Local` over the
//!   reading rows and `reading_refs`, your published posts and your
//!   verified mentions), so its numbers are the glyph's (rule 1's
//!   exception). ⌘J opens the lineage sheet for it, as on Reading.
//! - A draft or scratch note: only what it draws on, read live from its
//!   text (`![[id]]`, `stub_of`, `forked_from`).
//! - Either way, its `[[id]]` links are listed as "links to", never counted
//!   (a link isn't lineage).
//!
//! Built when the open post, its references or what this Mac holds change
//! (a store event), never per keystroke: the text is only scanned for its
//! references, and the lists are rebuilt when those change. With no reading
//! rows in memory yet (Reading never opened), the build runs off the main
//! thread. Nothing is fetched for it except your own mentions, once, as ⌘Y
//! does.
//!
//! A child module of `app` (it reads `MainView`); `app.rs` keeps small hooks
//! marked `// --- lineage ---`.

use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use blyg_core::lineage::{Local, Peer, Via};
use blyg_core::profile::{Relation, same_origin};
use blyg_core::{Mention, ReadingItem, Status};

use super::reading::lineage_vm::{self, Glyph, Kinds};
use super::reading::neighbourhood::Index;
use super::*;

/// Rows each list shows; the rest are a "+N more · ⌘J".
pub const LIST_MAX: usize = 5;

/// One post on one of the lists.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// Its blyg; empty when this Mac can't place it (a link to a post it
    /// doesn't hold).
    pub origin: String,
    /// Empty for a page (a `{url}` stub).
    pub id: String,
    pub title: String,
    pub who: String,
    /// How it relates; `None` for a plain `[[id]]` link.
    pub relation: Option<Relation>,
    pub partial: bool,
    /// One of yours (it opens in the editor).
    pub own: bool,
}

/// What the line and its lists show.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Strip {
    pub glyph: Glyph,
    pub draws_on: Vec<Entry>,
    pub drawn_on_by: Vec<Entry>,
    pub links: Vec<Entry>,
    /// A published post: "drawn on by" is known.
    pub published: bool,
}

impl Strip {
    pub fn is_empty(&self) -> bool {
        self.draws_on.is_empty() && self.drawn_on_by.is_empty() && self.links.is_empty()
    }

    /// The line's words: the glyph's numbers, then the links (not counted
    /// as lineage, so in words of their own).
    pub fn line(&self) -> String {
        let mut parts = vec![format!("draws on {}", self.glyph.up.total())];
        if self.published {
            parts.push(format!("drawn on by {}", self.glyph.down.total()));
        }
        match self.links.len() {
            0 => {}
            1 => parts.push("links to 1 post".into()),
            n => parts.push(format!("links to {n} posts")),
        }
        parts.join(" · ")
    }
}

/// Whether `item` is published as far as lineage goes (a public post with
/// a version on the blyg).
fn published(item: &Item) -> bool {
    item.version > 0 && item.status == Status::Public && item.server_id.is_some()
}

/// The `[[id]]` links in `text` (not the `![[id]]` quotes), ids in
/// first-occurrence order.
pub fn link_ids(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for r in blyg_core::grammar::link_refs(text) {
        if text[..r.range.start].ends_with('!') {
            continue;
        }
        let id = r.id.to_lowercase();
        if !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

/// A fingerprint of what the text references: the strip is rebuilt when
/// it changes, not on every keystroke.
pub fn refs_print(text: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    blyg_core::profile::transclusion_ids(text).hash(&mut h);
    link_ids(text).hash(&mut h);
    h.finish()
}

fn page_entry(url: &str, rel: Relation, partial: bool) -> Entry {
    Entry {
        origin: url.to_string(),
        id: String::new(),
        title: format!("A page on {}", super::reading::vm::host(url)),
        who: super::reading::vm::host(url),
        relation: Some(rel),
        partial,
        own: false,
    }
}

fn post_entry(
    index: &Index,
    origin: &str,
    id: &str,
    rel: Option<Relation>,
    partial: bool,
) -> Entry {
    let (title, who, _held, own) = index.describe(origin, id);
    Entry {
        origin: origin.to_string(),
        id: id.to_string(),
        title,
        who,
        relation: rel,
        partial,
        own,
    }
}

/// The strip for `item` with `text` in the editor, from what this Mac
/// holds. Pure: the window calls it on the main thread when the reading
/// rows are in memory, else off it.
pub fn compute<R: std::borrow::Borrow<ReadingItem>>(
    item: &Item,
    text: &str,
    rows: &[R],
    own: &[Item],
    own_origin: Option<&str>,
    mentions: &[Mention],
    local: &Local,
) -> Strip {
    let index = Index::new(rows, own, own_origin);
    let mut strip = Strip::default();
    let base = own_origin.filter(|_| published(item));
    match (base, item.server_id.as_ref()) {
        (Some(base), Some(sid)) => {
            strip.published = true;
            strip.glyph = Glyph::of(local, base, &sid.0, None);
            let entry = |e: &blyg_core::lineage::Edge| match &e.peer {
                Peer::Post { origin, id } => {
                    Some(post_entry(&index, origin, id, Some(e.relation), e.partial))
                }
                Peer::Url(u) => Some(page_entry(u, e.relation, e.partial)),
                Peer::Bare(_) => None,
            };
            strip.draws_on = local.up(base, &sid.0).iter().filter_map(entry).collect();
            strip.drawn_on_by = local
                .down(base, &sid.0)
                .iter()
                .filter_map(|e| {
                    let mut en = entry(e)?;
                    // A post known only by its mention: the mention names it.
                    if e.via == Via::Mention
                        && let Some(m) = mentions.iter().find(|m| {
                            m.source_id
                                .as_deref()
                                .is_some_and(|s| s.eq_ignore_ascii_case(&en.id))
                        })
                        && en.title.starts_with("A post on ")
                    {
                        if let Some(name) = m
                            .source_author
                            .as_ref()
                            .and_then(|a| a.name.clone())
                            .filter(|n| !n.trim().is_empty())
                        {
                            en.who = name;
                        }
                        if let Some(o) = &m.source_origin {
                            en.origin = o.clone();
                        }
                    }
                    Some(en)
                })
                .collect();
        }
        _ => {
            // Live from the text: what publishing it would make it draw on.
            let live = Item {
                content_md: text.to_string(),
                ..item.clone()
            };
            let mut up = Kinds::default();
            for r in live.references(own_origin.unwrap_or_default(), rows) {
                up.add(r.relation, r.partial);
                strip.draws_on.push(post_entry(
                    &index,
                    &r.origin,
                    &r.id,
                    Some(r.relation),
                    r.partial,
                ));
            }
            strip.glyph = Glyph {
                up,
                down: Kinds::default(),
            };
        }
    }
    // Links: what the text links to, found by id (a link names no blyg).
    for id in link_ids(text) {
        if strip
            .draws_on
            .iter()
            .any(|e| e.id.eq_ignore_ascii_case(&id))
        {
            continue;
        }
        let entry = match index.find_id(&id) {
            Some((origin, title, who, own)) => Entry {
                origin,
                id: id.clone(),
                title,
                who,
                relation: None,
                partial: false,
                own,
            },
            None => Entry {
                origin: String::new(),
                id: id.clone(),
                title: "A post not held on this Mac".into(),
                who: String::new(),
                relation: None,
                partial: false,
                own: false,
            },
        };
        strip.links.push(entry);
    }
    strip
}

/// What the cached strip was built for.
#[derive(Debug, Clone, PartialEq)]
struct Key {
    id: LocalId,
    version: u32,
    status: Status,
    refs: u64,
    stamp: u64,
    mentions: usize,
    rows: usize,
}

/// The window's strip state.
#[derive(Default)]
pub struct State {
    /// The lists are open under the line.
    pub open: bool,
    /// Bumped when the store says what this Mac holds changed.
    stamp: u64,
    cache: RefCell<Option<(Key, Rc<Strip>)>>,
    /// A build off the main thread is running for this key.
    building: RefCell<Option<Key>>,
}

impl State {
    /// Hook: the store changed (items or reading).
    pub fn stale(&mut self) {
        self.stamp += 1;
    }
}

impl MainView {
    fn lineage_strip_key(&self, cx: &App) -> Option<(Key, Item, String)> {
        let item = self.current.as_ref()?;
        let text = self.editor.read(cx).value().to_string();
        let key = Key {
            id: item.local_id.clone(),
            version: item.version,
            status: item.status,
            refs: refs_print(&text),
            stamp: self.lineage_strip.stamp,
            mentions: self.reading.mentions.ready().map_or(0, Vec::len),
            rows: self.reading.rows.len(),
        };
        Some((key, item.clone(), text))
    }

    /// The strip for the open post: cached, built now when the reading rows
    /// are in memory, else built off the main thread (the last one shows
    /// meanwhile, if it was this post's).
    pub(crate) fn lineage_strip_now(&self, cx: &mut Context<Self>) -> Option<Rc<Strip>> {
        let (key, item, text) = self.lineage_strip_key(cx)?;
        let st = &self.lineage_strip;
        let cached = st.cache.borrow().clone();
        if let Some((k, s)) = &cached
            && *k == key
        {
            return Some(s.clone());
        }
        let same_post = cached.as_ref().filter(|(k, _)| k.id == key.id);
        let mentions: Vec<Mention> = self.reading.mentions.ready().cloned().unwrap_or_default();
        let base = self.base_url.clone();
        // In memory once Reading has built its lineage over your posts too
        // (`own_origin`): a few keyed lookups.
        if !self.reading.rows.is_empty() && self.reading.own_origin.is_some() {
            let own = self.backend.items();
            // Your own rows among them are fine: your posts are looked up
            // as yours first.
            let strip = Rc::new(compute(
                &item,
                &text,
                &self.reading.rows,
                &own,
                base.as_deref(),
                &mentions,
                &self.reading.lineage,
            ));
            *st.cache.borrow_mut() = Some((key, strip.clone()));
            self.lineage_strip_wants_mentions(&item, cx);
            return Some(strip);
        }
        if st.building.borrow().as_ref() != Some(&key) {
            *st.building.borrow_mut() = Some(key.clone());
            let backend = self.backend.clone();
            let task = cx.background_spawn(async move {
                let rows = backend.reading();
                let own = backend.items();
                let local = Local::build(&rows, &own, base.as_deref(), &mentions);
                compute(
                    &item,
                    &text,
                    &rows,
                    &own,
                    base.as_deref(),
                    &mentions,
                    &local,
                )
            });
            cx.spawn(async move |this, cx| {
                let strip = task.await;
                let _ = this.update(cx, |v, cx| {
                    let st = &v.lineage_strip;
                    if st.building.borrow().as_ref() == Some(&key) {
                        *st.building.borrow_mut() = None;
                        *st.cache.borrow_mut() = Some((key, Rc::new(strip)));
                        cx.notify();
                    }
                });
            })
            .detach();
            if let Some(i) = self.current.clone() {
                self.lineage_strip_wants_mentions(&i, cx);
            }
        }
        same_post.map(|(_, s)| s.clone())
    }

    /// A published post's "drawn on by" includes your verified mentions:
    /// load them once, as ⌘Y does.
    fn lineage_strip_wants_mentions(&self, item: &Item, cx: &mut Context<Self>) {
        if !published(item) || !matches!(self.reading.mentions, super::reading::Load::Idle) {
            return;
        }
        cx.spawn(async move |this, cx| {
            let _ = this.update(cx, |v, cx| {
                if matches!(v.reading.mentions, super::reading::Load::Idle)
                    && v.backend.read_extensions_available()
                {
                    v.load_mentions(cx);
                }
            });
        })
        .detach();
    }

    /// ⌘J on Posts with no published post to centre on: open or close the
    /// lists instead. False when there's nothing to list either.
    pub(crate) fn lineage_strip_toggle(&mut self, cx: &mut Context<Self>) -> bool {
        let has = self.lineage_strip_now(cx).is_some_and(|s| !s.is_empty());
        if has {
            self.lineage_strip.open = !self.lineage_strip.open;
            cx.notify();
        }
        has
    }

    /// The line is showing (tests and the tour).
    #[cfg(test)]
    pub(crate) fn lineage_strip_shown(&self) -> Option<Rc<Strip>> {
        self.lineage_strip
            .cache
            .borrow()
            .as_ref()
            .filter(|(k, _)| self.current.as_ref().is_some_and(|c| c.local_id == k.id))
            .map(|(_, s)| s.clone())
    }

    /// Open `e`: your post in the editor, anyone else's in Reading.
    fn lineage_strip_open(&mut self, e: Entry, window: &mut Window, cx: &mut Context<Self>) {
        if e.origin.is_empty() {
            self.show_toast(
                "That post isn't held on this Mac",
                Some("A link names no blyg; quote it or follow it on the web".into()),
                cx,
            );
            return;
        }
        if e.id.is_empty() {
            cx.open_url(&e.origin);
            return;
        }
        let mine = e.own
            || self
                .base_url
                .as_deref()
                .is_some_and(|b| same_origin(b, &e.origin));
        if mine
            && let Some(i) = self.backend.items().into_iter().find(|i| {
                i.server_id
                    .as_ref()
                    .is_some_and(|s| s.0.eq_ignore_ascii_case(&e.id))
            })
        {
            self.open(&i.local_id, window, cx);
            return;
        }
        self.open_original(e.origin, e.id, None, window, cx);
    }

    /// Hook: the line under the editor (nothing when there's nothing to
    /// show, or on a new empty post).
    pub(super) fn render_lineage_strip(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.reading.view != reading::View::Posts {
            return None;
        }
        let strip = self.lineage_strip_now(cx)?;
        if strip.is_empty() {
            return None;
        }
        let p = self.palette;
        let open = self.lineage_strip.open;
        let chrome = self.theme.chrome_font(&self.prefs);
        let line = div()
            .id("lineage-strip")
            .debug_selector(|| "lineage-strip".into())
            .flex()
            .items_center()
            .gap(px(8.))
            .h(px(26.))
            .px(px(14.))
            .cursor_pointer()
            .hover(|s| s.bg(p.sel.opacity(0.5)))
            .on_click(cx.listener(|this, _, _, cx| {
                this.lineage_strip.open = !this.lineage_strip.open;
                cx.notify();
            }))
            .when(!strip.glyph.is_empty(), |d| {
                d.child(super::reading::lineage::glyph_painter(strip.glyph, &p))
            })
            .child(
                div()
                    .debug_selector(|| "lineage-strip-line".into())
                    .whitespace_nowrap()
                    .child(strip.line()),
            )
            .child(div().flex_1())
            .child(
                div()
                    .whitespace_nowrap()
                    .text_size(px(10.5))
                    .child(crate::keymap::hint(if strip.published {
                        if open {
                            "⌘J the map · click to fold"
                        } else {
                            "⌘J the map · click for the lists"
                        }
                    } else if open {
                        "click to fold"
                    } else {
                        "⌘J or click for the list"
                    })),
            );
        let column = |title: &'static str, entries: &[Entry], key: &'static str| {
            let more = entries.len().saturating_sub(LIST_MAX);
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(px(10.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.muted)
                        .child(title),
                )
                .children(entries.iter().take(LIST_MAX).enumerate().map(|(i, e)| {
                    let rel = e.relation;
                    let words = match rel {
                        Some(r) => lineage_vm::relation_words(r, e.partial, key == "up"),
                        None => "linked",
                    };
                    let who = if e.own {
                        "you".to_string()
                    } else {
                        e.who.clone()
                    };
                    let sub = if who.is_empty() {
                        words.to_string()
                    } else {
                        format!("{who} · {words}")
                    };
                    let en = e.clone();
                    // --- tray --- a row can be dragged into the post as a quote.
                    let drag = crate::app::tray::DraggedQuote {
                        id: e.id.clone(),
                        title: e.title.clone(),
                        label: None,
                        key: e
                            .own
                            .then(|| (blyg_core::OWN_SUBSCRIPTION.to_string(), e.id.clone())),
                        from_tray: false,
                    };
                    div()
                        .id((key, i))
                        .map(|d| crate::app::tray::draggable(d, drag))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .px(px(4.))
                        .rounded(px(4.))
                        .cursor_pointer()
                        .hover(|s| s.bg(p.sel))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.lineage_strip_open(en.clone(), window, cx)
                        }))
                        .child(div().flex_none().w(px(3.)).h(px(12.)).rounded(px(1.5)).bg(
                            match rel {
                                Some(r) => super::reading::lineage::rel_color(&p, r),
                                None => p.line,
                            },
                        ))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_color(p.ink)
                                .child(e.title.clone()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .whitespace_nowrap()
                                .text_color(p.muted)
                                .child(sub),
                        )
                }))
                .when(more > 0, |d| {
                    d.child(
                        div()
                            .px(px(4.))
                            .text_color(p.muted)
                            .child(crate::keymap::hint_owned(format!("+{more} more · ⌘J"))),
                    )
                })
        };
        let lists = open.then(|| {
            div()
                .id("lineage-strip-lists")
                .debug_selector(|| "lineage-strip-lists".into())
                .flex()
                .gap(px(16.))
                .px(px(14.))
                .pb(px(8.))
                .max_h(px(150.))
                .overflow_y_scroll()
                .when(!strip.draws_on.is_empty(), |d| {
                    d.child(column("Draws on", &strip.draws_on, "up"))
                })
                .when(strip.published && !strip.drawn_on_by.is_empty(), |d| {
                    d.child(column("Drawn on by", &strip.drawn_on_by, "down"))
                })
                .when(!strip.links.is_empty(), |d| {
                    d.child(column("Links to", &strip.links, "link"))
                })
        });
        Some(
            div()
                .flex_none()
                .border_t_1()
                .border_color(p.line)
                .font_family(chrome)
                .text_size(px(11.5))
                .text_color(p.muted)
                .child(line)
                .children(lists)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
#[path = "lineage_strip_tests.rs"]
mod tests;
