//! --- stub quotes --- The stub editor's quote line (studio 0.31; studio
//! 0.49 moved the passage choice into the editor's own preview): above a
//! stub draft, a hint names what the draft does with the post it answers
//! (the whole post, a passage, a running commentary, or a response by
//! link), with "quote whole post" and "select a passage in the preview" /
//! "quote another passage".
//!
//! A passage is selected inside one of the stub's quote boxes in the editor
//! preview (the WebView reports it, `SurfaceEvent::StubSelection`), and
//! "❝ quote only this" (or ⇧⌘D) writes it under that quote's `![[id]]`,
//! whichever of several it is (`with_stub_quote_nth`). A selection in a
//! nested quote, or while the preview is behind the draft, offers nothing
//! ([`pick_passage`]). The text logic is `blyg_render::stub_quote`.

use blyg_render::stub_quote::{
    StubQuoteForm, add_whole_stub_quote, passage_count, quote_count, stub_hint, stub_quote_form,
    with_stub_quote, with_stub_quote_nth,
};
use gpui_kit::*;

use crate::app::MainView;
use crate::app::studio::StubPage;
use crate::app::studio::webview::StubPick;

/// A passage selected in the editor preview, as the page reported it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pick {
    /// The stub draft it was selected for.
    pub item: blyg_core::LocalId,
    pub pick: StubPick,
}

/// Every quote box (`blockquote.blyg-transclusion`) in the preview's HTML,
/// in document order, as the page's `querySelectorAll` lists them: its
/// `data-blyg-id` (an unresolved box has none) and whether it sits inside
/// another box.
pub fn quote_boxes(html: &str) -> Vec<(Option<String>, bool)> {
    // A double-quoted attribute of a tag (the sanitizer writes them so).
    fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
        let pat = format!(" {name}=\"");
        let start = tag.find(&pat)? + pat.len();
        let len = tag[start..].find('"')?;
        Some(&tag[start..start + len])
    }
    let lower = html.to_ascii_lowercase();
    // Per open blockquote: is it a box?
    let mut open: Vec<bool> = Vec::new();
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = lower[at..].find("<").map(|i| at + i) {
        at = i + 1;
        let rest = &lower[i..];
        let boundary = |n: usize| {
            rest.as_bytes()
                .get(n)
                .is_some_and(|b| b.is_ascii_whitespace() || *b == b'>' || *b == b'/')
        };
        if rest.starts_with("</blockquote") && boundary(12) {
            open.pop();
        } else if rest.starts_with("<blockquote") && boundary(11) {
            let end = rest.find('>').map_or(html.len(), |e| i + e);
            let tag = &html[i..end];
            let is_box = attr(tag, "class")
                .is_some_and(|c| c.split_ascii_whitespace().any(|x| x == "blyg-transclusion"));
            if is_box {
                let nested = open.iter().any(|&b| b);
                out.push((attr(tag, "data-blyg-id").map(str::to_string), nested));
            }
            open.push(is_box);
        }
    }
    out
}

/// The quote a preview selection goes under and its passage (normalized as
/// publish compares it): `Some((nth, passage))` when the page is the one
/// that shows the draft as it is now (same number, same text), the box is a
/// top-level quote of `id` (not nested in another), and the page's quotes
/// of `id` are the draft's directives one for one (none unresolved), so the
/// nth box is the nth directive. Anything else offers nothing.
pub fn pick_passage(
    pick: &StubPick,
    page: Option<StubPage<'_>>,
    text: &str,
    id: &str,
) -> Option<(usize, String)> {
    let page = page?;
    if pick.gen_ != page.gen_ || page.src != text {
        return None; // the preview is behind the draft
    }
    let boxes = quote_boxes(page.html);
    let (box_id, nested) = boxes.get(pick.index)?;
    if *nested || box_id.as_deref() != Some(id) {
        return None;
    }
    let mine: Vec<usize> = boxes
        .iter()
        .enumerate()
        .filter(|(_, (b, n))| !n && b.as_deref() == Some(id))
        .map(|(i, _)| i)
        .collect();
    if mine.len() != quote_count(text, id) {
        return None;
    }
    let nth = mine.iter().position(|&i| i == pick.index)?;
    let passage = blyg_render::normalize_selection(&pick.text);
    (!passage.is_empty()).then_some((nth, passage))
}

/// Where the caret goes after a quote was added: on its directive line.
fn caret_at_new_quote(old: &str, new: &str) -> usize {
    let common = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    let mut at = common.min(new.len());
    while !new.is_char_boundary(at) {
        at -= 1;
    }
    new[at..].find("![[").map_or(new.len(), |i| at + i)
}

impl MainView {
    /// The open draft when it's a stub thread of a blyg post: (its local
    /// id, the target's id, the target's origin).
    pub(crate) fn stub_target(&self) -> Option<(blyg_core::LocalId, String, String)> {
        let c = self.current.as_ref()?;
        if c.kind != blyg_core::Kind::Thread
            || !matches!(
                c.status,
                blyg_core::Status::Draft | blyg_core::Status::Scratch
            )
        {
            return None;
        }
        let s = c.stub_of.as_ref()?;
        Some((c.local_id.clone(), s.id.clone(), s.origin.clone()))
    }

    /// The editor preview reported a selection (or that there's none).
    pub(crate) fn stub_selected(&mut self, pick: Option<StubPick>, cx: &mut Context<Self>) {
        let next = match (pick, self.stub_target()) {
            (Some(pick), Some((item, ..))) => Some(Pick { item, pick }),
            _ => None,
        };
        if self.reading.stub != next {
            self.reading.stub = next;
            cx.notify();
        }
    }

    /// The passage selected in the preview that "quote only this" would
    /// write, and which quote of the stub it goes under.
    pub(crate) fn stub_passage(&self, cx: &App) -> Option<(usize, String)> {
        let (item, id, _) = self.stub_target()?;
        if !self.studio.view.preview_visible() {
            return None;
        }
        let p = self.reading.stub.as_ref().filter(|p| p.item == item)?;
        let text = self.editor.read(cx).value();
        pick_passage(&p.pick, self.studio.stub_page(), &text, &id)
    }

    /// "❝ quote only this" (and ⇧⌘D while there is one): the selected
    /// passage under its own quote's directive. `false`: nothing to quote.
    pub(crate) fn quote_stub_passage(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let (Some((_, id, _)), Some((nth, passage))) = (self.stub_target(), self.stub_passage(cx))
        else {
            return false;
        };
        let text = self.editor.read(cx).value().to_string();
        let new = with_stub_quote_nth(&text, &id, Some(&passage), nth);
        self.reading.stub = None;
        self.studio_stub_clear();
        self.splice_editor(&text, &new, None, window, cx);
        self.show_toast("Quoting the passage", None, cx);
        true
    }

    /// "quote whole post": the stub's quote back to the whole post.
    pub(crate) fn quote_stub_whole(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((_, id, _)) = self.stub_target() else {
            return;
        };
        let (text, cursor) = {
            let s = self.editor.read(cx);
            (s.value().to_string(), s.cursor())
        };
        let new = with_stub_quote(&text, &id, None);
        self.splice_editor(&text, &new, Some(cursor.min(new.len())), window, cx);
    }

    /// "select a passage in the preview" / "quote another passage": a quote
    /// of the whole post to select from (put back at the top when the draft
    /// has none; after the caret's block when it already quotes a passage),
    /// and the preview on screen (⌘E's toggle, from either layout).
    pub(crate) fn stub_choose_passage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((_, id, _)) = self.stub_target() else {
            return;
        };
        let (text, cursor) = {
            let s = self.editor.read(cx);
            (s.value().to_string(), s.cursor())
        };
        let new = match stub_quote_form(&text, &id) {
            None => with_stub_quote(&text, &id, None),
            Some(StubQuoteForm::Whole) => text.clone(),
            Some(StubQuoteForm::Passage) => add_whole_stub_quote(&text, &id, cursor),
        };
        if new != text {
            let caret = caret_at_new_quote(&text, &new);
            self.splice_editor(&text, &new, Some(caret), window, cx);
        }
        if !self.studio.view.preview_visible() {
            let view = self.studio.view.toggle_preview();
            self.studio_set_view(view, cx);
        }
        cx.notify();
    }

    /// Hook (the editor pane, above the text): the stub's hint line and,
    /// while a passage is selected in the preview, "quote only this".
    /// `None` unless a stub draft is open.
    pub(crate) fn render_stub_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (_, id, _) = self.stub_target()?;
        let p = self.palette.on_page();
        let text = self.editor.read(cx).value().to_string();
        let form = stub_quote_form(&text, &id);
        let passages = passage_count(&text, &id);
        let chosen = self.stub_passage(cx).is_some();
        let link = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .cursor_pointer()
                .text_color(p.accent)
                .hover(|s| s.underline())
                .child(label)
        };
        let mut hint = stub_hint(&text, &id);
        if form.is_some() {
            hint.push_str(" To quote only part of it, select the text in the preview.");
        }
        let mut row = div()
            .id("stub-hint")
            .debug_selector(|| "stub-hint".into())
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x(px(8.))
            .gap_y(px(2.))
            // The whole width, so the hint wraps instead of running under
            // the preview.
            .child(div().w_full().text_color(p.muted).child(hint));
        if passages <= 1 && form != Some(StubQuoteForm::Whole) {
            row = row
                .child(link("stub-whole", "quote whole post").on_click(
                    cx.listener(|this, _, window, cx| this.quote_stub_whole(window, cx)),
                ));
        }
        row = row.child(
            link(
                "stub-choose",
                if form == Some(StubQuoteForm::Passage) {
                    "quote another passage"
                } else {
                    "select a passage in the preview"
                },
            )
            .on_click(cx.listener(|this, _, window, cx| this.stub_choose_passage(window, cx))),
        );
        let pill = chosen.then(|| {
            div()
                .id("stub-only")
                .debug_selector(|| "stub-only".into())
                .mt(px(6.))
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .id("stub-only-button")
                        .debug_selector(|| "stub-only-button".into())
                        .px(px(10.))
                        .py(px(4.))
                        .rounded(px(self.theme.chip_radius))
                        .border_1()
                        .border_color(p.accent)
                        .text_color(p.accent)
                        .cursor_pointer()
                        .child("❝ quote only this")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.quote_stub_passage(window, cx);
                        })),
                )
                .child(div().text_color(p.muted).child(crate::keymap::hint(
                    "⇧⌘D · the passage selected in the preview",
                )))
        });
        Some(
            div()
                .id("stub-bar")
                .flex_none()
                .px(px(18.))
                .pt(px(8.))
                .pb(px(6.))
                .font_family(self.chrome())
                .text_size(px(11.5))
                .line_height(relative(1.4))
                .child(row)
                .children(pill)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{StubPage, StubPick, caret_at_new_quote, pick_passage, quote_boxes};

    const ID: &str = "7c9wk2mhq0v3xj8tn5rzfd41bg";

    fn bq(id: Option<&str>, inner: &str) -> String {
        match id {
            Some(id) => format!(
                "<blockquote class=\"blyg-transclusion\" data-blyg-id=\"{id}\" data-blyg-version=\"1\">{inner}<p class=\"provenance\">from x · snapshot of v1</p></blockquote>"
            ),
            None => format!(
                "<blockquote class=\"blyg-transclusion unresolved\"><p>⚠ {inner}</p></blockquote>"
            ),
        }
    }

    fn pick(gen_: u64, index: usize, text: &str) -> StubPick {
        StubPick {
            gen_,
            index,
            text: text.into(),
        }
    }

    /// Two quotes of the target, the first quoting a post that itself
    /// quotes the target (a nested box), and a quote of another post.
    fn doc() -> (String, String) {
        let md = format!(
            "![[{ID}]]\n\nMy point.\n\n![[OTHER0000000000000000000]]\n\n![[{ID}]]\n\nMy end."
        );
        let nested = bq(Some(ID), "<p>Nested words.</p>");
        let html = [
            bq(Some(ID), &format!("<p>First words.</p>{nested}")),
            "<p data-line=\"2\">My point.</p>".into(),
            bq(Some("OTHER0000000000000000000"), "<p>Theirs.</p>"),
            bq(Some(ID), "<p>Second words.</p>"),
            "<p>My end.</p>".into(),
        ]
        .join("\n");
        (md, html)
    }

    #[test]
    fn the_boxes_are_listed_in_page_order_with_their_nesting() {
        let (_, html) = doc();
        let boxes = quote_boxes(&html);
        let ids: Vec<(Option<&str>, bool)> =
            boxes.iter().map(|(i, n)| (i.as_deref(), *n)).collect();
        assert_eq!(
            ids,
            [
                (Some(ID), false),
                (Some(ID), true),
                (Some("OTHER0000000000000000000"), false),
                (Some(ID), false),
            ]
        );
        // A plain blockquote isn't a box, and doesn't make one nested.
        let html = format!("<blockquote><p>mine</p></blockquote>{}", bq(None, "gone"));
        assert_eq!(quote_boxes(&html), [(None, false)]);
    }

    #[test]
    fn a_selection_goes_under_the_quote_it_was_made_in() {
        let (md, html) = doc();
        let page = Some(StubPage {
            gen_: 4,
            src: &md,
            html: &html,
        });
        // The 4th box on the page is the target's 2nd top-level quote.
        assert_eq!(
            pick_passage(&pick(4, 3, "  Second  words. "), page, &md, ID),
            Some((1, "Second words.".into()))
        );
        assert_eq!(
            pick_passage(&pick(4, 0, "First words."), page, &md, ID),
            Some((0, "First words.".into()))
        );
        // The passage written there is the one the preview showed.
        let out = blyg_render::stub_quote::with_stub_quote_nth(&md, ID, Some("Second words."), 1);
        assert!(out.ends_with(&format!("![[{ID}]]\n> Second words.\n\nMy end.")));
        assert!(out.starts_with(&format!("![[{ID}]]\n\nMy point.")));
    }

    #[test]
    fn a_nested_quote_or_another_posts_offers_nothing() {
        let (md, html) = doc();
        let page = Some(StubPage {
            gen_: 4,
            src: &md,
            html: &html,
        });
        assert_eq!(
            pick_passage(&pick(4, 1, "Nested words."), page, &md, ID),
            None
        );
        assert_eq!(pick_passage(&pick(4, 2, "Theirs."), page, &md, ID), None);
        assert_eq!(pick_passage(&pick(4, 9, "x"), page, &md, ID), None);
        assert_eq!(pick_passage(&pick(4, 0, " \n "), page, &md, ID), None);
    }

    #[test]
    fn a_stale_preview_offers_nothing() {
        let (md, html) = doc();
        let page = Some(StubPage {
            gen_: 4,
            src: &md,
            html: &html,
        });
        // Typed since the page was rendered.
        let typed = format!("{md}!");
        assert_eq!(pick_passage(&pick(4, 0, "First"), page, &typed, ID), None);
        // A selection reported by an older page.
        assert_eq!(pick_passage(&pick(3, 0, "First"), page, &md, ID), None);
        // No page yet (loading, or never told).
        assert_eq!(pick_passage(&pick(4, 0, "First"), None, &md, ID), None);
        // One of the target's quotes didn't resolve: the boxes no longer
        // pair with the directives, so nothing is offered.
        let md3 = format!("{md}\n\n![[{ID}]]");
        let html3 = format!("{html}\n{}", bq(None, "unresolvable"));
        let page3 = Some(StubPage {
            gen_: 5,
            src: &md3,
            html: &html3,
        });
        assert_eq!(pick_passage(&pick(5, 0, "First"), page3, &md3, ID), None);
    }

    #[test]
    fn the_caret_lands_on_the_added_quote() {
        let old = format!("![[{ID}]]\n> a\n\nMine.");
        let new = blyg_render::stub_quote::add_whole_stub_quote(&old, ID, 0);
        let at = caret_at_new_quote(&old, &new);
        assert_eq!(&new[at..], format!("![[{ID}]]\n\n"));
    }
}
