//! --- hover cards --- An id is opaque; the thought behind it shouldn't be.
//! Hovering a `[[id]]`, `[[id | label]]` or `![[id | label]]` in the editor
//! shows a small card after a moment: the post's title, author and blyg,
//! version and first lines, and, on an unlabelled link, "label it" (which
//! writes the post's title as the link's label). ⌘-click (Ctrl-click on
//! Windows) opens the post: yours in the editor, anyone else's in Reading,
//! as following a quote does. The Reading stream's quote boxes and `[[id]]`
//! links show the same card on hover (they already open on a click).
//!
//! Nothing is fetched to show a card: it says what this Mac holds, in the
//! resolver's order (your posts, then the reading rows). Protocol rules:
//! another blyg's post shows its current version only (a quote of an older
//! one says which it quoted, never shows that text); a withdrawn post shows
//! as withdrawn, with no text; opening a post this Mac doesn't hold goes
//! through Reading's own open (the token-less public client).
//!
//! A child module of `app` (it reads `MainView`); `app.rs` keeps small hooks
//! marked `// --- hover cards ---`.

use std::ops::Range;

use blyg_core::profile::same_origin;
use blyg_core::{Cited, ReadingItem, Status};

use gpui_kit::base::input::RopeExt as _;

use super::*;

/// How long the mouse rests on a directive before its card shows.
pub const DELAY: Duration = Duration::from_millis(350);
/// The excerpt's length, in characters.
pub const EXCERPT_CHARS: usize = 160;

/// A `[[id]]` or `![[id]]` in the editor's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directive {
    /// The whole directive (with its `!`).
    pub range: Range<usize>,
    pub id: String,
    pub label: Option<String>,
    /// `![[…]]`: a quote; else a link.
    pub quote: bool,
}

/// Every directive in `text`, in order: the inline links (`[[…]]`, not
/// after a `!`) and the quotes (`![[…]]` anywhere, as the studio's
/// `directiveInline` reads them).
pub fn directives(text: &str) -> Vec<Directive> {
    let mut out: Vec<Directive> = blyg_core::grammar::link_refs(text)
        .into_iter()
        .map(|r| Directive {
            range: r.range,
            id: r.id,
            label: r.label,
            quote: false,
        })
        .collect();
    let mut at = 0;
    while let Some(i) = text[at..].find("![[") {
        let p = at + i;
        // The `[[…]]` right after the `!`, read as a link at its start (a
        // reference never spans lines).
        let line_end = text[p..].find('\n').map_or(text.len(), |n| p + n);
        match blyg_core::grammar::link_refs(&text[p + 1..line_end])
            .into_iter()
            .next()
        {
            Some(r) if r.range.start == 0 => {
                out.push(Directive {
                    range: p..p + 1 + r.range.end,
                    id: r.id,
                    label: r.label,
                    quote: true,
                });
                at = p + 1 + r.range.end;
            }
            _ => at = p + 1,
        }
    }
    out.sort_by_key(|d| d.range.start);
    out
}

/// The directive at byte `offset` of `text`, if any.
pub fn directive_at(text: &str, offset: usize) -> Option<Directive> {
    directives(text)
        .into_iter()
        .find(|d| d.range.start <= offset && offset < d.range.end)
}

/// Where the post a card describes is.
#[derive(Debug, Clone, PartialEq)]
pub enum Place {
    /// One of yours (it opens in the editor).
    Own(LocalId),
    /// Held on this Mac from another blyg (it opens in Reading).
    Held { origin: String },
    /// Not held here: an origin to open it from, when one is known.
    Elsewhere { origin: Option<String> },
}

/// What a card shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub id: String,
    pub title: String,
    /// "You · ≡ thread · v3", "Lin · lin.example.org · v2".
    pub meta: String,
    /// The first lines of the post as it is now, when they may be shown.
    pub excerpt: Option<String>,
    /// Why there's no text, or what to know: withdrawn, a draft, not held.
    pub note: Option<String>,
    pub place: Place,
    /// "label it" writes this as the link's label (an unlabelled link to a
    /// post with a title).
    pub label_it: Option<String>,
    pub quote: bool,
}

impl Card {
    /// ⌘-click can open it.
    pub fn opens(&self) -> bool {
        !matches!(self.place, Place::Elsewhere { origin: None })
    }
}

pub(crate) fn kind_word(k: Kind) -> &'static str {
    match k {
        Kind::Fragment => "◦ fragment",
        Kind::Thread => "≡ thread",
    }
}

/// The first lines of a post, after its title.
pub fn excerpt_of(md: &str, html: &str) -> Option<String> {
    // A long post's tail never shows: cut before the work.
    let md: String = md.chars().take(4000).collect();
    let mut text = if md.trim().is_empty() {
        strip_tags(&html.chars().take(4000).collect::<String>())
    } else {
        blyg_core::plain_text(&md)
    };
    if let Some(t) = blyg_core::plain_title(&md)
        && let Some(rest) = text.strip_prefix(t.as_str())
    {
        text = rest.trim_start().to_string();
    }
    clip_excerpt(&text)
}

/// Plain text as a card's excerpt: one line, cut at [`EXCERPT_CHARS`].
pub fn clip_excerpt(text: &str) -> Option<String> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return None;
    }
    if text.chars().count() <= EXCERPT_CHARS {
        return Some(text);
    }
    let cut = blyg_core::text::prefix_graphemes(&text, EXCERPT_CHARS - 1);
    Some(format!("{}…", cut.trim_end()))
}

fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                tag = true;
                out.push(' ');
            }
            '>' => tag = false,
            c if !tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn reading_who(r: &ReadingItem) -> String {
    super::reading::lineage_vm::who_of(r)
}

/// The card for a held reading row (someone else's post, or one of your
/// own rows on the Reading screen). `quoted`: the version a quote names.
pub fn row_card(
    r: &ReadingItem,
    quoted: Option<u32>,
    quote: bool,
    own_origin: Option<&str>,
) -> Card {
    let host = super::reading::vm::host(&r.origin);
    let who = reading_who(r);
    let mine = own_origin.is_some_and(|o| same_origin(o, &r.origin));
    let withdrawn = r.state == "tombstone";
    let mut meta = vec![if mine { "You".to_string() } else { who.clone() }];
    if !mine && who != host {
        meta.push(host);
    }
    meta.push(kind_word(r.kind).to_string());
    let (excerpt, note) = if withdrawn {
        // Withdrawn: no text (a retained pin stays the author's, linked).
        meta.push("withdrawn".into());
        (
            None,
            Some(match r.pinned_version_retained {
                Some(v) => format!("Withdrawn by its author. Pinned v{v} is kept."),
                None => "Withdrawn by its author.".to_string(),
            }),
        )
    } else {
        meta.push(format!("v{}", r.version));
        // A quote of an earlier version: say so, but show only what's
        // public now (an unpinned past version is never shown).
        let note = quoted
            .filter(|&q| q != r.version)
            .map(|q| format!("Quoted at v{q}; this is v{}.", r.version));
        (excerpt_of(&r.content_md, &r.content_html), note)
    };
    Card {
        id: r.remote_id.clone(),
        title: super::reading::vm::post_title(r),
        meta: meta.join(" · "),
        excerpt,
        note,
        place: Place::Held {
            origin: r.origin.clone(),
        },
        label_it: None,
        quote,
    }
}

/// The card for one of your own posts.
pub fn own_card(i: &Item, quote: bool) -> Card {
    let mut meta = vec!["You".to_string(), kind_word(i.kind).to_string()];
    let (excerpt, note) = match i.status {
        Status::Public => {
            meta.push(if i.dirty {
                format!("v{} · unpublished edits", i.version)
            } else {
                format!("v{}", i.version)
            });
            (excerpt_of(&i.content_md, ""), None)
        }
        Status::Draft | Status::Scratch => {
            meta.push(if i.status == Status::Draft {
                "draft".into()
            } else {
                "scratch note".into()
            });
            (
                excerpt_of(&i.content_md, ""),
                Some("Not published yet: the page shows it once it is.".to_string()),
            )
        }
        Status::Withdrawn => {
            meta.push("withdrawn".into());
            (None, Some("You withdrew it.".to_string()))
        }
    };
    Card {
        id: i
            .server_id
            .as_ref()
            .map(|s| s.0.clone())
            .unwrap_or_default(),
        title: i.title(),
        meta: meta.join(" · "),
        excerpt,
        note,
        place: Place::Own(i.local_id.clone()),
        label_it: None,
        quote,
    }
}

/// A post this Mac doesn't hold: the citation the quoting post froze, when
/// it has one.
pub fn unheld_card(id: &str, origin: Option<&str>, cited: Option<&Cited>, quote: bool) -> Card {
    let host = origin.map(super::reading::vm::host);
    let author = cited
        .and_then(|c| c.author.clone())
        .or_else(|| cited.and_then(|c| c.source.clone()));
    let meta = [author, host.clone()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
    Card {
        id: id.to_string(),
        title: match &host {
            Some(h) => format!("A post on {h}"),
            None => "A post not held on this Mac".to_string(),
        },
        meta,
        excerpt: cited
            .and_then(|c| c.excerpt.clone())
            .filter(|e| !e.trim().is_empty()),
        note: Some(match origin {
            Some(_) => "Not held on this Mac. A click opens it from its blyg.".to_string(),
            None => "Not held on this Mac, so it can't be shown or opened from here.".to_string(),
        }),
        place: Place::Elsewhere {
            origin: origin.map(str::to_string),
        },
        label_it: None,
        quote,
    }
}

/// The editor's card for directive `d`: yours first, then a held post (the
/// resolver's order), else "not held".
pub fn editor_card<R: std::borrow::Borrow<ReadingItem>>(
    d: &Directive,
    own: &[Item],
    rows: &[R],
    own_origin: Option<&str>,
) -> Card {
    let mine = own.iter().find(|i| {
        i.server_id
            .as_ref()
            .is_some_and(|s| s.0.eq_ignore_ascii_case(&d.id))
    });
    let mut card = match mine {
        Some(i) => {
            let mut c = own_card(i, d.quote);
            if !d.quote && d.label.is_none() && i.status != Status::Withdrawn {
                c.label_it = Some(i.title()).filter(|t| !t.trim().is_empty() && t != "…");
            }
            c
        }
        None => {
            let held = rows
                .iter()
                .map(std::borrow::Borrow::borrow)
                .filter(|r: &&ReadingItem| r.remote_id.eq_ignore_ascii_case(&d.id))
                // A current row before a withdrawn one.
                .min_by_key(|r| r.state == "tombstone");
            match held {
                Some(r) => {
                    let mut c = row_card(r, None, d.quote, own_origin);
                    if !d.quote && d.label.is_none() && r.state != "tombstone" {
                        c.label_it = Some(super::reading::vm::quote_label(r))
                            .filter(|t| !t.trim().is_empty() && t != "…");
                    }
                    c
                }
                None => unheld_card(&d.id, None, None, d.quote),
            }
        }
    };
    card.id = d.id.clone();
    card
}

/// The text with `d` given `label` (`[[id | label]]`).
pub fn labelled(text: &str, d: &Directive, label: &str) -> String {
    let src = blyg_core::grammar::link_source(&d.id, Some(label));
    format!("{}{}{}", &text[..d.range.start], src, &text[d.range.end..])
}

// ------------------------------------------------------------------ state

/// The card showing in the editor.
pub struct Shown {
    pub directive: Directive,
    pub card: Card,
    /// Window position under the directive.
    pub anchor: Point<Pixels>,
}

#[derive(Default)]
pub struct State {
    /// The directive under the mouse, while the delay runs.
    pending: Option<(Range<usize>, Task<()>)>,
    pub shown: Option<Shown>,
}

/// The card's look, shared by the editor's card and the stream's tooltip.
pub(crate) fn card_body(card: &Card, p: Palette, ui_font: SharedString) -> Div {
    div()
        .w(px(300.))
        .p(px(10.))
        .flex()
        .flex_col()
        .gap(px(3.))
        .bg(p.raised())
        .border_1()
        .border_color(p.edge())
        .rounded(px(8.))
        .shadow(vec![BoxShadow {
            color: p.drop().opacity(0.6),
            offset: point(px(0.), px(8.)),
            blur_radius: px(16.),
            spread_radius: px(-8.),
            inset: false,
        }])
        .font_family(ui_font)
        .text_size(px(12.))
        .line_height(relative(1.4))
        .text_color(p.ink)
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(px(13.))
                .line_clamp(2)
                .child(card.title.clone()),
        )
        .when(!card.meta.is_empty(), |d| {
            d.child(
                div()
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .truncate()
                    .child(card.meta.clone()),
            )
        })
        .when_some(card.excerpt.clone(), |d, e| {
            d.child(
                div()
                    .mt(px(3.))
                    .pl(px(8.))
                    .border_l_2()
                    .border_color(p.line)
                    .line_clamp(4)
                    .child(e),
            )
        })
        .when_some(card.note.clone(), |d, n| {
            d.child(
                div()
                    .mt(px(2.))
                    .text_size(px(11.))
                    .italic()
                    .text_color(p.muted)
                    .child(n),
            )
        })
}

/// The stream's hover card, as a tooltip view.
pub struct CardTip {
    pub card: Card,
    pub palette: Palette,
    pub font: SharedString,
}

impl Render for CardTip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        card_body(&self.card, self.palette, self.font.clone()).child(
            div()
                .mt(px(2.))
                .text_size(px(10.5))
                .text_color(self.palette.muted)
                .child(if self.card.opens() {
                    "A click opens it"
                } else {
                    "Not held on this Mac"
                }),
        )
    }
}

impl MainView {
    /// The stream's card for a quote box or `[[id]]` link (`card`), as a
    /// tooltip builder.
    pub(crate) fn hover_card_tip(
        &self,
        card: Card,
    ) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
        let palette = self.palette.on_page();
        let font = SharedString::from(self.theme.chrome_font(&self.prefs));
        move |_, cx| {
            let card = card.clone();
            let font = font.clone();
            cx.new(|_| CardTip {
                card,
                palette,
                font,
            })
            .into()
        }
    }

    /// The stream's cards for a run of text's `[[id]]` links, by the text
    /// range each link covers (GPUI asks by character index).
    pub(crate) fn hover_card_span_tips(
        &self,
        cards: Vec<(Range<usize>, Card)>,
    ) -> impl Fn(usize, &mut Window, &mut App) -> Option<AnyView> + 'static {
        let palette = self.palette.on_page();
        let font = SharedString::from(self.theme.chrome_font(&self.prefs));
        move |ix, _, cx| {
            let card = cards.iter().find(|(r, _)| r.contains(&ix))?.1.clone();
            let font = font.clone();
            Some(
                cx.new(|_| CardTip {
                    card,
                    palette,
                    font,
                })
                .into(),
            )
        }
    }

    /// The stream's card for `id` quoted or linked from `post`.
    pub(crate) fn stream_card(&self, post: &ReadingItem, id: &str, quote: bool) -> Card {
        let held = super::reading::stream_vm::quoted(&self.reading.rows, post, id);
        let (origin, version) = super::reading::stream_vm::quote_ref(post, id, held);
        match held {
            Some(r) => row_card(
                r,
                version.filter(|_| quote),
                quote,
                self.base_url.as_deref(),
            ),
            None => unheld_card(
                id,
                Some(&origin),
                super::reading::stream_vm::cited(post, id),
                quote,
            ),
        }
    }

    /// The directive under window position `pos` in the editor.
    fn hover_hit(&self, pos: Point<Pixels>, cx: &App) -> Option<Directive> {
        let s = self.editor.read(cx);
        let text = s.value();
        if !text.contains("[[") {
            return None;
        }
        let rope = s.text();
        let (rows, lh) = (s.visible_row_range()?, s.line_height()?);
        let lines = rope.lines_len();
        let first = rope.line_start_offset(rows.start.min(lines.saturating_sub(1)));
        let last = if rows.end >= lines {
            rope.len()
        } else {
            rope.line_start_offset(rows.end)
        };
        directives(&text)
            .into_iter()
            .filter(|d| d.range.end >= first && d.range.start <= last)
            .find(|d| {
                row_bounds(s, d.range.clone(), lh)
                    .iter()
                    .any(|b| b.contains(&pos))
            })
    }

    /// The mouse moved over the editor: wait out the delay on a directive,
    /// or put the card away when it left one.
    fn hover_moved(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let hit = self.hover_hit(pos, cx);
        let Some(d) = hit else {
            self.hover.pending = None;
            if self.hover.shown.take().is_some() {
                cx.notify();
            }
            return;
        };
        if self.hover.shown.as_ref().is_some_and(|s| s.directive == d) {
            return;
        }
        if self
            .hover
            .pending
            .as_ref()
            .is_some_and(|(r, _)| *r == d.range)
        {
            return;
        }
        if self.hover.shown.take().is_some() {
            cx.notify();
        }
        let range = d.range.clone();
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DELAY).await;
            let _ = this.update(cx, |v, cx| v.hover_show(d, cx));
        });
        self.hover.pending = Some((range, task));
    }

    /// Show the card for `d` (after the delay, or straight away from a
    /// test or the tour).
    pub(crate) fn hover_show(&mut self, d: Directive, cx: &mut Context<Self>) {
        self.hover.pending = None;
        let text = self.editor.read(cx).value();
        if text.get(d.range.clone()).is_none()
            || directive_at(&text, d.range.start) != Some(d.clone())
        {
            return; // the text moved on
        }
        let own = self.backend.items();
        let base = self.base_url.as_deref();
        let card = if self.reading.rows.is_empty() {
            editor_card(&d, &own, &self.backend.reading(), base)
        } else {
            editor_card(&d, &own, &self.reading.rows, base)
        };
        let anchor = {
            let s = self.editor.read(cx);
            s.line_height()
                .and_then(|lh| row_bounds(s, d.range.clone(), lh).into_iter().next())
                .map_or(point(px(200.), px(200.)), |b| {
                    point(b.left(), b.bottom() + px(4.))
                })
        };
        self.hover.shown = Some(Shown {
            directive: d,
            card,
            anchor,
        });
        cx.notify();
    }

    /// The card showing (tests and the tour).
    pub(crate) fn hover_card_shown(&self) -> bool {
        self.hover.shown.is_some()
    }

    #[cfg(test)]
    pub(crate) fn hover_card(&self) -> Option<&Card> {
        self.hover.shown.as_ref().map(|s| &s.card)
    }

    /// Open the post `d` names: yours in the editor, anyone else's in
    /// Reading (as following a quote does).
    pub(crate) fn hover_follow(
        &mut self,
        d: &Directive,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.hover.pending = None;
        self.hover.shown = None;
        let own = self.backend.items();
        let card = if self.reading.rows.is_empty() {
            editor_card(d, &own, &self.backend.reading(), self.base_url.as_deref())
        } else {
            editor_card(d, &own, &self.reading.rows, self.base_url.as_deref())
        };
        match card.place {
            Place::Own(id) => self.open(&id, window, cx),
            Place::Held { origin } => self.open_original(origin, d.id.clone(), None, window, cx),
            Place::Elsewhere { origin: Some(o) } => {
                self.open_original(o, d.id.clone(), None, window, cx)
            }
            Place::Elsewhere { origin: None } => self.show_toast(
                "That post isn't held on this Mac",
                Some("A link names no blyg, so there's nowhere to open it from".into()),
                cx,
            ),
        }
        cx.notify();
    }

    /// "label it": write the post's title as the link's label.
    fn hover_label_it(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(shown) = self.hover.shown.take() else {
            return;
        };
        let Some(label) = shown.card.label_it.clone() else {
            return;
        };
        let text = self.editor.read(cx).value().to_string();
        if directive_at(&text, shown.directive.range.start).as_ref() != Some(&shown.directive) {
            return;
        }
        let new = labelled(&text, &shown.directive, &label);
        self.splice_editor(&text, &new, None, window, cx);
        self.show_toast(
            format!("Labelled · the link reads “{label}” on the page"),
            None,
            cx,
        );
    }

    /// Hook: the editor's hover cards and ⌘-click, on the box around the
    /// text area.
    pub(super) fn hover_card_editor(&self, d: Div, cx: &mut Context<Self>) -> Div {
        d.on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
            if ev.pressed_button.is_none() {
                this.hover_moved(ev.position, cx);
            }
        }))
        .capture_any_mouse_down(cx.listener(|this, ev: &MouseDownEvent, window, cx| {
            if ev.button != MouseButton::Left || !ev.modifiers.secondary() {
                return;
            }
            if let Some(d) = this.hover_hit(ev.position, cx) {
                cx.stop_propagation();
                window.prevent_default();
                this.hover_follow(&d, window, cx);
            }
        }))
        .children(self.render_hover_card(cx))
    }

    fn render_hover_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let shown = self.hover.shown.as_ref()?;
        // The text changed under it: say nothing rather than something stale.
        let text = self.editor.read(cx).value();
        if directive_at(&text, shown.directive.range.start).as_ref() != Some(&shown.directive) {
            return None;
        }
        let p = self.palette;
        let font = SharedString::from(self.theme.chrome_font(&self.prefs));
        let card = &shown.card;
        let chip = |id: &'static str, label: String| {
            div()
                .id(id)
                .px(px(7.))
                .py(px(1.))
                .rounded(px(5.))
                .border_1()
                .border_color(p.line)
                .cursor_pointer()
                .hover(|s| s.bg(p.hover()))
                .text_size(px(11.))
                .child(label)
        };
        let d = shown.directive.clone();
        let open_label = crate::keymap::hint(match card.place {
            Place::Own(_) => "⌘-click opens it in the editor",
            _ => "⌘-click opens it in Reading",
        });
        let body = card_body(card, p, font)
            .id("hover-card")
            .debug_selector(|| "hover-card".into())
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                // Keep the editor's caret where it was.
                window.prevent_default();
                cx.stop_propagation();
            })
            .child(
                div()
                    .mt(px(4.))
                    .flex()
                    .flex_wrap()
                    .gap(px(6.))
                    .when_some(card.label_it.clone(), |row, label| {
                        row.child(
                            chip("hover-card-label", format!("label it “{label}”")).on_click(
                                cx.listener(|this, _, window, cx| this.hover_label_it(window, cx)),
                            ),
                        )
                    })
                    .when(card.opens(), |row| {
                        row.child(chip("hover-card-open", "open".into()).on_click(cx.listener(
                            move |this, _, window, cx| this.hover_follow(&d, window, cx),
                        )))
                    }),
            )
            .when(card.opens(), |b| {
                b.child(
                    div()
                        .text_size(px(10.5))
                        .text_color(p.muted)
                        .child(open_label),
                )
            });
        Some(
            deferred(
                anchored()
                    .position(shown.anchor)
                    .snap_to_window_with_margin(px(8.))
                    .child(body),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
}

/// A range's on-screen boxes, one per visual row (a directive can wrap).
fn row_bounds(s: &TextareaState, r: Range<usize>, lh: Pixels) -> Vec<Bounds<Pixels>> {
    let Some(b) = s.range_to_bounds(&r) else {
        return vec![];
    };
    if b.size.height <= lh * 1.5 {
        return vec![b];
    }
    let text = s.text().slice(r.clone()).to_string();
    let mut out = Vec::new();
    let mut seg = r.start;
    let mut prev = r.start;
    for (i, _) in text.char_indices().skip(1).chain([(text.len(), ' ')]) {
        let k = r.start + i;
        let fits = s
            .range_to_bounds(&(seg..k))
            .is_some_and(|b| b.size.height <= lh * 1.5);
        if !fits {
            out.extend(s.range_to_bounds(&(seg..prev)));
            seg = prev;
        }
        prev = k;
    }
    out.extend(s.range_to_bounds(&(seg..r.end)));
    out
}

#[cfg(test)]
#[path = "hover_card_tests.rs"]
mod tests;
