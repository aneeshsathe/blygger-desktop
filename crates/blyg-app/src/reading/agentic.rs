//! --- agentic feed --- Reading's Agentic feed tab draws the edition an
//! assistant wrote (studio 0.69, contract 6.2.0; `blyg_core::agentic`): a
//! masthead (its title, who wrote it, when, and its lede), then sections,
//! each a heading, the assistant's note and a card per post. A card's
//! headline, summary and reason are the assistant's words, set in the
//! assistant's style and marked "✦"; the post's own title and opening
//! words are the author's. Entries of a group, and folded ones, are listed
//! under their card. The tab's name is the blyg's `agenticTitle` (Blyg › AI
//! on Your Blyg…), else "Agentic feed".
//!
//! The tab lists the edition's posts held here, in the edition's order, so
//! j/k, ⏎ and space move through them as through any list. With no post
//! open the edition is a page across the reading pane, as the studio sets
//! it; a card (or j/k) opens its post, the edition narrows to the list
//! column beside it, and esc puts the page back. Formulas in a card read
//! as text (`maths_text`). The edition is read when the tab is chosen and
//! whenever Reading refreshes while it's shown; Burrow's own AI may write
//! one (`agentic_writer.rs`).
//! A blyg without editions (its capabilities don't list `getAgentic`) shows
//! the tab empty, saying what it's for.

use std::collections::HashMap;

use blyg_core::ReadingItem;
use blyg_core::agentic::{
    DEFAULT_FEED, Edition, Layout, Placed, Present, SectionKind, grouped, title_or_default,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::sources_vm::{Smart, Source};
use super::vm::{self, Key};
use crate::app::MainView;

/// The masthead's mark on the assistant's words.
pub const ASSISTANT_BADGE: &str = "✦ written by your assistant";

/// What the tab says with no edition: how one gets made.
pub fn empty_note(title: &str) -> String {
    crate::keymap::hint_owned(format!(
        "No edition yet. {title} is a digest of your reading in sections, written by an \
         assistant. Burrow's own AI can write it: ⌥⌘E writes one now, and Settings › Agentic \
         feed (⌘,) writes one after each sync that brings new posts. An assistant on your blyg \
         can write it too; its brief is the Agentic feed prompt in Blyg › AI on Your Blyg…"
    ))
}

/// The edition and the tab's name, as last read.
#[derive(Debug, Default)]
pub struct Agentic {
    edition: Option<Edition>,
    /// `agenticTitle` as the blyg names the feed; `None` until read.
    title: Option<String>,
    /// The edition's keys, in its order (what the tab lists).
    keys: Vec<Key>,
    /// The blyg doesn't offer editions (as last asked).
    pub unoffered: bool,
    /// The last read failed, in words.
    pub error: Option<String>,
    /// Bumped by each read: only the latest one's answer lands.
    reads: u64,
    /// Each entry's card, by section then entry, built when the edition or
    /// the rows change ([`Agentic::refresh_cards`]), never while drawing.
    cards: Vec<Vec<Card>>,
    /// Where each key's first card is.
    card_at: HashMap<Key, (usize, usize)>,
    /// Cards built so far (tests count work by it).
    built: usize,
}

impl Agentic {
    pub fn set_edition(&mut self, e: Option<Edition>) {
        self.keys = e.as_ref().map(Edition::keys).unwrap_or_default();
        self.edition = e;
        self.cards.clear();
        self.card_at.clear();
    }

    /// Build every card from the rows held now: once per edition or rows
    /// change. The rows are indexed once, so it's linear in both.
    pub fn refresh_cards(&mut self, rows: &[ReadingItem]) {
        self.cards.clear();
        self.card_at.clear();
        let Some(l) = self.edition.as_ref().and_then(|e| e.layout.as_ref()) else {
            return;
        };
        let held: HashMap<Key, &ReadingItem> = rows.iter().map(|r| (vm::key(r), r)).collect();
        for (si, s) in l.sections.iter().enumerate() {
            let mut out = Vec::with_capacity(s.entries.len());
            for (ei, p) in s.entries.iter().enumerate() {
                let k = p.key();
                out.push(card(p, held.get(&k).copied()));
                self.card_at.entry(k).or_insert((si, ei));
            }
            self.built += out.len();
            self.cards.push(out);
        }
    }

    /// The card for `key` as last built.
    pub fn card_for(&self, key: &Key) -> Option<&Card> {
        let (si, ei) = *self.card_at.get(key)?;
        self.cards.get(si)?.get(ei)
    }

    /// The card at a section's entry, as last built.
    fn card_at(&self, si: usize, ei: usize) -> Option<&Card> {
        self.cards.get(si)?.get(ei)
    }

    /// Cards built so far (tests count work by it).
    #[cfg(test)]
    pub fn cards_built(&self) -> usize {
        self.built
    }

    pub fn set_title(&mut self, t: Option<String>) {
        self.title = t;
    }

    pub fn edition(&self) -> Option<&Edition> {
        self.edition.as_ref().filter(|e| !e.is_empty())
    }

    /// What the tab lists, in the edition's order.
    pub fn keys(&self) -> &[Key] {
        &self.keys
    }
}

/// The tab's name: the blyg's `agenticTitle`, else "Agentic feed".
pub fn tab_label(a: &Agentic) -> String {
    title_or_default(a.title.as_deref().unwrap_or(""))
}

/// What a card says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub headline: String,
    /// The headline is the assistant's (else the post's own title).
    pub headline_generated: bool,
    /// "Ada's harbour · 20 Nov", and whether it moved on since.
    pub byline: String,
    /// The assistant's dek.
    pub summary: Option<String>,
    /// The post's own opening words (excerpt, or more for "full").
    pub excerpt: Option<String>,
    /// Why the reader is seeing it (the assistant's).
    pub reason: Option<String>,
    /// The post is on this computer (it opens on a click).
    pub held: bool,
}

/// The post's own title and its text after the title, on one line, with
/// formulas as text (a card is plain text: `\int_0^1` reads as markup).
fn title_and_words(md: &str) -> (Option<String>, String) {
    let held = super::maths_text::hold_maths(md);
    let all = blyg_core::plain_text(&held.md);
    let title = blyg_core::plain_title(&held.md);
    let after = match &title {
        Some(t) => all.strip_prefix(t.as_str()).unwrap_or(&all).trim(),
        None => all.as_str(),
    };
    (title.map(|t| held.back(&t)), held.back(after))
}

fn clip(s: &str, n: usize) -> String {
    let c = blyg_core::text::clip(s, n);
    c.into_owned()
}

/// A card for `p`, from the post as held here (`None`: not held yet).
pub fn card(p: &Placed, held: Option<&ReadingItem>) -> Card {
    let (own_title, after) = match held.map(|r| title_and_words(&r.content_md)) {
        Some((t, a)) => (t.filter(|t| !t.is_empty()), Some(a)),
        None => (None, None),
    };
    // The assistant's words may hold formulas too.
    let said = |s: &Option<String>| s.as_deref().map(super::maths_text::plain_maths);
    let (headline, headline_generated) = match (&p.headline, own_title) {
        (Some(h), _) => (super::maths_text::plain_maths(h), true),
        (None, Some(t)) => (t, false),
        (None, None) => match after.as_deref().filter(|a| !a.is_empty()) {
            Some(a) => (clip(a, 90), false),
            None => ("untitled".to_string(), false),
        },
    };
    let Some(r) = held else {
        return Card {
            headline,
            headline_generated,
            byline: "Not on this computer yet · it arrives with the next sync".into(),
            summary: said(&p.summary),
            excerpt: None,
            reason: said(&p.reason),
            held: false,
        };
    };
    let mut byline = format!(
        "{} · {}",
        source_name(r),
        vm::when_label(r, chrono::Utc::now())
    );
    if p.changed_since() || p.version.is_some_and(|v| r.version > v) {
        byline.push_str(" · changed since the assistant read it");
    }
    let excerpt = match p.present {
        Present::Full => after.filter(|a| !a.is_empty()),
        Present::Excerpt if p.summary.is_none() => {
            after.filter(|a| !a.is_empty()).map(|a| clip(&a, 280))
        }
        _ => None,
    };
    Card {
        headline,
        headline_generated,
        byline,
        summary: said(&p.summary),
        excerpt,
        reason: said(&p.reason),
        held: true,
    }
}

/// Whose a post is: your blyg, else its subscription's name.
fn source_name(r: &ReadingItem) -> String {
    if super::own::is_own_row(r) {
        super::own::OWN_LABEL.to_string()
    } else {
        r.subscription_title.clone()
    }
}

/// The masthead's line: who wrote it, with what, when, and the mark.
pub fn masthead_meta(l: &Layout, when: Option<String>) -> String {
    let mut s = format!("by {}", l.agent.name);
    if let Some(m) = &l.agent.model {
        s.push_str(&format!(" ({m})"));
    }
    if let Some(w) = when {
        s.push_str(&format!(" · {w}"));
    }
    s.push_str(&format!(" · {ASSISTANT_BADGE}"));
    s
}

/// A day as the masthead shows it: "21 Nov 2026", local time.
fn day_words(iso: &str) -> Option<String> {
    let t = chrono::DateTime::parse_from_rfc3339(iso).ok()?;
    Some(
        t.with_timezone(&chrono::Local)
            .format("%-d %b %Y")
            .to_string(),
    )
}

impl MainView {
    /// The sources pane's name for a smart feed (the Agentic feed wears the
    /// blyg's name for it).
    pub(crate) fn smart_label(&self, s: Smart) -> String {
        match s {
            Smart::Agentic => tab_label(&self.reading.agentic),
            other => other.label().to_string(),
        }
    }

    /// Read the feed's name and, while the tab is shown (or `always`), its
    /// edition, off the UI thread. Only the latest read's answer lands.
    pub(crate) fn agentic_refresh(&mut self, always: bool, cx: &mut Context<Self>) {
        let shown = self.reading.source == Source::Smart(Smart::Agentic);
        if !(shown || always) {
            return;
        }
        self.reading.agentic.reads += 1;
        let ticket = self.reading.agentic.reads;
        let backend = self.backend.clone();
        let task = cx.background_spawn(async move {
            let feeds = backend.agentic_feeds();
            let edition = match &feeds {
                Ok(Some(_)) => backend.agentic_edition(DEFAULT_FEED),
                Ok(None) => Ok(None),
                Err(e) => Err(blyg_core::CoreError::Other(e.to_string())),
            };
            (feeds, edition)
        });
        cx.spawn(async move |this, cx| {
            let (feeds, edition) = task.await;
            let _ = this.update(cx, |v, cx| {
                let a = &mut v.reading.agentic;
                if a.reads != ticket {
                    return;
                }
                match (feeds, edition) {
                    (Ok(feeds), Ok(edition)) => {
                        a.unoffered = feeds.is_none();
                        a.error = None;
                        a.set_title(
                            feeds
                                .as_deref()
                                .and_then(|f| f.iter().find(|f| f.id == DEFAULT_FEED))
                                .map(|f| f.title.clone()),
                        );
                        a.set_edition(edition);
                        let r = &mut v.reading;
                        r.agentic.refresh_cards(&r.rows);
                    }
                    // Keep what was shown; say the read failed.
                    (Err(e), _) | (_, Err(e)) => a.error = Some(e.to_string()),
                }
                v.reading.refilter();
                cx.notify();
            });
        })
        .detach();
    }

    /// The Agentic feed tab's edition, or what the tab is for. `wide`: it
    /// takes the reading pane (no post open), set as a page, as the studio
    /// sets it; else it's the list column beside the open post.
    pub(super) fn render_agentic_edition(&self, wide: bool, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette.on_page();
        let a = &self.reading.agentic;
        let Some(ed) = a.edition() else {
            let mut note = empty_note(&tab_label(a));
            if let Some(e) = &a.error {
                note = format!("Couldn't read the edition: {e}");
            } else if a.unoffered {
                note.push_str(" Your blyg's studio doesn't offer editions yet (studio 0.69).");
            }
            return self.empty_note(note);
        };
        let Some(l) = ed.layout.as_ref() else {
            return div().into_any_element();
        };
        let said = |t: String, size: f32| {
            div()
                .italic()
                .text_size(px(size))
                .line_height(relative(1.4))
                .text_color(p.accent)
                .child(super::maths_text::plain_maths(&t))
        };
        // The page's type is larger across the pane.
        let k = if wide { 1.25 } else { 1.0 };
        let when = l
            .generated_at
            .as_deref()
            .or(ed.updated_at.as_deref())
            .and_then(day_words);
        let mut col = div()
            .w_full()
            .when(wide, |d| d.max_w(px(760.)).mx_auto().pt(px(18.)))
            .px(px(14.))
            .pb(px(16.))
            .flex()
            .flex_col()
            .gap(px(6. * k))
            .child(
                div()
                    .pt(px(8.))
                    .text_size(px(if wide { 26. } else { 17. }))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.ink)
                    .italic()
                    .child(l.title.clone().unwrap_or_else(|| "Your edition".into())),
            )
            .child(
                div()
                    .font_family(self.chrome())
                    .text_size(px(11. * k))
                    .text_color(p.muted)
                    .when(wide, |d| d.pb(px(6.)).border_b_1().border_color(p.line))
                    .child(masthead_meta(l, when)),
            )
            .children(l.lede.clone().map(|t| said(t, 13. * k)));
        let mut n = 0usize;
        for (si, s) in l.sections.iter().enumerate() {
            col = col.child(
                div()
                    .mt(px(10.))
                    .pb(px(2.))
                    .border_b_1()
                    .border_color(p.line)
                    .text_size(if s.kind == SectionKind::Lead {
                        px(14. * k)
                    } else {
                        px(12. * k)
                    })
                    .font_weight(FontWeight::SEMIBOLD)
                    .italic()
                    .text_color(p.accent)
                    .child(s.title.clone()),
            );
            if let Some(note) = &s.note {
                col = col.child(said(note.clone(), 12. * k));
            }
            for g in grouped(&s.entries) {
                let lead = (g.lead, &s.entries[g.lead]);
                let also: Vec<(usize, &Placed)> =
                    g.also.iter().map(|&i| (i, &s.entries[i])).collect();
                col = col.child(self.render_agentic_card(
                    (si, n),
                    lead,
                    &also,
                    s.kind == SectionKind::Briefs,
                    k,
                    cx,
                ));
                n += 1;
            }
        }
        div()
            .id("agentic-edition")
            .debug_selector(|| "agentic-edition".into())
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_y_scroll()
            .child(col)
            .into_any_element()
    }

    /// The card at section `si`'s entry `ei`: as built on the last change,
    /// else (an edition that landed before its rows) built here.
    fn agentic_card(&self, si: usize, ei: usize, p: &Placed) -> std::borrow::Cow<'_, Card> {
        match self.reading.agentic.card_at(si, ei) {
            Some(c) => std::borrow::Cow::Borrowed(c),
            None => {
                let k = p.key();
                let row = self.reading.rows.iter().find(|r| vm::key(r) == k);
                std::borrow::Cow::Owned(card(p, row))
            }
        }
    }

    fn render_agentic_card(
        &self,
        id: (usize, usize),
        (lead_ix, lead): (usize, &Placed),
        also: &[(usize, &Placed)],
        brief: bool,
        k: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette.on_page();
        let key = lead.key();
        let c = self.agentic_card(id.0, lead_ix, lead);
        let selected = self.reading.sel.as_ref() == Some(&key);
        let open = |key: Key| {
            cx.listener(move |this: &mut MainView, _: &ClickEvent, window, cx| {
                if this.reading.agentic.card_for(&key).is_some_and(|c| c.held) {
                    this.set_pane(super::sources_vm::Pane::List, cx);
                    this.open_reading(key.clone(), window, cx);
                }
            })
        };
        let mut d = div()
            .id(SharedString::from(format!(
                "agentic-card-{}-{}",
                id.0, id.1
            )))
            .px(px(8.))
            .py(px(6.))
            .rounded(px(6.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .when(selected, |d| d.bg(p.sel))
            .when(c.held, |d| d.cursor_pointer().hover(|d| d.bg(p.sel)))
            .on_click(open(key))
            .child(
                div()
                    .text_size(px(if brief { 12.5 } else { 13.5 } * k))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(if c.headline_generated {
                        p.accent
                    } else {
                        p.ink
                    })
                    .when(c.headline_generated, |d| d.italic())
                    .child(c.headline.clone()),
            )
            .child(
                div()
                    .font_family(self.chrome())
                    .text_size(px(10.5 * k))
                    .text_color(p.muted)
                    .child(c.byline.clone()),
            );
        if let Some(s) = &c.summary {
            d = d.child(
                div()
                    .italic()
                    .text_size(px(12. * k))
                    .text_color(p.accent)
                    .child(s.clone()),
            );
        }
        if let Some(x) = &c.excerpt {
            d = d.child(
                div()
                    .text_size(px(12. * k))
                    .line_height(relative(1.4))
                    .text_color(p.ink)
                    .child(x.clone()),
            );
        }
        if let Some(r) = &c.reason {
            d = d.child(
                div()
                    .italic()
                    .text_size(px(11. * k))
                    .text_color(p.muted)
                    .child(format!("✦ {r}")),
            );
        }
        for (i, (oi, o)) in also.iter().enumerate() {
            let ok = o.key();
            let oc = self.agentic_card(id.0, *oi, o);
            d = d.child(
                div()
                    .id(SharedString::from(format!(
                        "agentic-also-{}-{}-{i}",
                        id.0, id.1
                    )))
                    .pl(px(10.))
                    .text_size(px(11.5 * k))
                    .text_color(if oc.headline_generated {
                        p.accent
                    } else {
                        p.ink
                    })
                    .when(oc.held, |d| d.cursor_pointer())
                    .on_click(open(ok))
                    .child(format!(
                        "· {}  {}",
                        oc.headline,
                        oc.byline.split(" · ").next().unwrap_or("")
                    )),
            );
        }
        d.into_any_element()
    }
}
