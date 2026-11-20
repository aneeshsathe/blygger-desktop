//! Mathematics in HTML someone else published (decision #105, spec §14;
//! the studio's `importer/sanitize.ts` and `math-render.ts`, 0.64.1).
//!
//! A `math` element is never kept. Inside `math` a browser may read bytes
//! differently from a sanitizer, so each formula is **redrawn**: the element
//! is discarded, and the decoded text of its first top-level
//! `application/x-tex` annotation is the one thing taken from inside it. A
//! formula without that annotation is dropped; a nested `math` is removed.
//!
//! Two lol-html passes, as the studio does with HTMLRewriter (the same
//! engine). The first changes nothing but a marker attribute, named with a
//! random nonce no input can know, on every `math` element, and collects
//! each top-level one's TeX and display flag. The second replaces a marked
//! top-level `math` with a text token (random too) and removes any other.
//! The tokens are swapped for the drawings last, so a formula lands only
//! where the parser itself saw a `math` element.

use std::cell::RefCell;
use std::rc::Rc;

use lol_html::html_content::{ContentType, EndTag};
use lol_html::{RewriteStrSettings, element, rewrite_str, text};

use crate::util::escape_html;

/// The longest TeX drawn as a formula; longer is shown as written.
pub const TEX_MAX: usize = 4000;
/// Formulas drawn per document; later ones are shown as their TeX.
pub const FORMULAS_MAX: usize = 1000;
/// Characters of TeX drawn per document (the studio's R4 budget).
pub const TEX_BUDGET: usize = 100_000;

/// `written`: a formula as its author typed it, delimiters included.
pub fn written(tex: &str, display: bool) -> String {
    if display {
        format!("$${tex}$$")
    } else {
        format!("${tex}$")
    }
}

/// `texAsText`: a formula that could not be drawn, shown as written. For a
/// reader, not an author.
pub fn tex_as_text(tex: &str, display: bool) -> String {
    format!(
        "<code class=\"math-source\">{}</code>",
        escape_html(&written(tex, display))
    )
}

/// The length JavaScript gives `s` (UTF-16 code units), which the studio's
/// limits count in.
pub(crate) fn js_len(s: &str) -> usize {
    if s.len() <= TEX_MAX {
        // UTF-16 never needs more units than UTF-8 needs bytes.
        return s.len().min(s.encode_utf16().count());
    }
    s.encode_utf16().count()
}

/// One top-level formula the first pass saw.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Formula {
    tex: Option<String>,
    display: bool,
}

/// A random lowercase alphanumeric string. `RandomState` is seeded from the
/// OS per thread, so a remote author can't predict it.
fn nonce() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::new();
    for i in 0..2u64 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(i);
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

/// Whether `html` might hold a `math` element (`/<math/i`).
pub(crate) fn has_math(html: &str) -> bool {
    html.as_bytes()
        .windows(5)
        .any(|w| w[0] == b'<' && w[1..].eq_ignore_ascii_case(b"math"))
}

/// The formulas of one document, between the passes and the drawing.
#[derive(Debug)]
pub struct Formulas {
    marker: String,
    token: String,
    formulas: Vec<Formula>,
}

impl Formulas {
    /// `collectFormulas`: the first pass. `None` when the document holds no
    /// `math`, or the rewriter gave up (then the second pass removes every
    /// `math`, since none is marked).
    pub(crate) fn collect(html: &str) -> (String, Option<Formulas>) {
        if !has_math(html) {
            return (html.to_string(), None);
        }
        let marker = format!("data-m{}", nonce());
        let mut token = format!("m{}t", nonce());
        while html.contains(&token) {
            token = format!("m{}t", nonce());
        }
        #[derive(Default)]
        struct State {
            formulas: Vec<Formula>,
            open: Vec<usize>,
            reading: Option<(usize, String)>,
        }
        let state = Rc::new(RefCell::new(State::default()));
        let (s1, s2, s3) = (state.clone(), state.clone(), state.clone());
        let mark = marker.clone();
        let settings = RewriteStrSettings::new()
            .with_enable_esi_tags(false)
            .append_element_content_handler(element!("math", move |el| {
                let mut st = s1.borrow_mut();
                let top = st.open.is_empty();
                let index = st.formulas.len();
                let display = top
                    && el
                        .get_attribute("display")
                        .is_some_and(|d| d.trim().eq_ignore_ascii_case("block"));
                st.formulas.push(Formula { tex: None, display });
                let value = if top {
                    index.to_string()
                } else {
                    "nested".into()
                };
                el.set_attribute(&mark, &value)?;
                // `<math/>` has no content and no end tag to wait for.
                if let Some(handlers) = el.end_tag_handlers() {
                    let s = s1.clone();
                    let pop: lol_html::EndTagHandler<'static> =
                        Box::new(move |_: &mut EndTag<'_>| {
                            s.borrow_mut().open.pop();
                            Ok(())
                        });
                    handlers.push(pop);
                    st.open.push(index);
                }
                Ok(())
            }))
            .append_element_content_handler(element!("annotation", move |el| {
                let mut st = s2.borrow_mut();
                if st.open.len() != 1 {
                    return Ok(());
                }
                let index = st.open[0];
                let tex_encoding = el
                    .get_attribute("encoding")
                    .is_some_and(|e| e.trim().eq_ignore_ascii_case("application/x-tex"));
                if st.formulas[index].tex.is_some() || !tex_encoding {
                    return Ok(());
                }
                // `<annotation/>`: nothing to read.
                if let Some(handlers) = el.end_tag_handlers() {
                    let s = s2.clone();
                    let read: lol_html::EndTagHandler<'static> =
                        Box::new(move |_: &mut EndTag<'_>| {
                            let mut st = s.borrow_mut();
                            if let Some((i, text)) = st.reading.take() {
                                st.formulas[i].tex =
                                    Some(html_escape::decode_html_entities(&text).into_owned());
                            }
                            Ok(())
                        });
                    handlers.push(read);
                    st.reading = Some((index, String::new()));
                }
                Ok(())
            }))
            .append_element_content_handler(text!("annotation", move |chunk| {
                if let Some((_, text)) = s3.borrow_mut().reading.as_mut() {
                    text.push_str(chunk.as_str());
                }
                Ok(())
            }));
        let marked = rewrite_str(html, settings);
        let formulas = std::mem::take(&mut state.borrow_mut().formulas);
        match marked {
            Ok(marked) => (
                marked,
                Some(Formulas {
                    marker,
                    token,
                    formulas,
                }),
            ),
            // Nothing marked: the second pass drops every `math`.
            Err(_) => (html.to_string(), None),
        }
    }

    /// What the second pass does with one `math` element: the token for a
    /// marked top-level formula with TeX, else `None` (remove it).
    pub(crate) fn token_for(
        this: Option<&Formulas>,
        el: &lol_html::html_content::Element,
    ) -> Option<String> {
        let f = this?;
        let index = el.get_attribute(&f.marker)?;
        if index.is_empty() || !index.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let i: usize = index.parse().ok()?;
        f.formulas.get(i)?.tex.as_ref()?;
        Some(format!("{}{i}x", f.token))
    }

    /// Swap each token in `html` for its formula's drawing, within the
    /// document's budgets. With `text_only`, a token is drawn only where it
    /// stands in text, never inside a tag (for HTML serialized by another
    /// parser, which double-quotes every attribute and escapes `<` in text).
    pub(crate) fn fill(&self, html: &str, text_only: bool) -> String {
        let mut out = String::with_capacity(html.len());
        let mut budget = Budget::default();
        let mut rest = html;
        let mut tag = TagState::default();
        while let Some(at) = rest.find(&self.token) {
            let after = &rest[at + self.token.len()..];
            let digits = after.bytes().take_while(u8::is_ascii_digit).count();
            if digits == 0 || after.as_bytes().get(digits) != Some(&b'x') {
                out.push_str(&rest[..at + self.token.len()]);
                if text_only {
                    tag.scan(&rest[..at + self.token.len()]);
                }
                rest = after;
                continue;
            }
            out.push_str(&rest[..at]);
            if text_only {
                tag.scan(&rest[..at]);
            }
            let formula = after[..digits]
                .parse::<usize>()
                .ok()
                .and_then(|i| self.formulas.get(i));
            if let Some(Formula {
                tex: Some(tex),
                display,
            }) = formula
                && !(text_only && tag.inside)
            {
                out.push_str(&budget.draw(tex, *display));
            }
            rest = &after[digits + 1..];
        }
        out.push_str(rest);
        out
    }
}

/// Where a scan of serialized HTML stands: inside a tag, and inside a
/// quoted attribute value there.
#[derive(Default)]
struct TagState {
    inside: bool,
    quote: Option<u8>,
}

impl TagState {
    fn scan(&mut self, s: &str) {
        for b in s.bytes() {
            match (self.inside, self.quote, b) {
                (false, _, b'<') => self.inside = true,
                (true, None, b'"' | b'\'') => self.quote = Some(b),
                (true, Some(q), c) if c == q => self.quote = None,
                (true, None, b'>') => self.inside = false,
                _ => {}
            }
        }
    }
}

/// The per-document drawing budget: only what is drawn is charged.
#[derive(Default)]
struct Budget {
    drawn: usize,
    spent: usize,
}

impl Budget {
    fn draw(&mut self, tex: &str, display: bool) -> String {
        let len = js_len(tex);
        // A formula over TEX_MAX is never drawn, and must not turn the rest
        // of a thread to text (session 70).
        if len > TEX_MAX {
            return tex_as_text(tex, display);
        }
        if self.drawn >= FORMULAS_MAX || self.spent + len > TEX_BUDGET {
            // The budget is spent: every later formula is text too.
            self.spent = TEX_BUDGET;
            return tex_as_text(tex, display);
        }
        self.drawn += 1;
        self.spent += len;
        redraw(tex, display)
    }
}

/// A formula from someone else's page, drawn by our own renderer, or shown
/// as its TeX when it can't be drawn.
fn redraw(tex: &str, display: bool) -> String {
    tex_as_text(tex, display)
}

/// Every `math` in `html` replaced by a token or removed, for a sanitizer
/// other than this crate's (the reader's ammonia). Clean the returned HTML,
/// then pass the result to [`MathHoles::fill`].
pub fn hole_math(html: &str) -> MathHoles {
    let (marked, formulas) = Formulas::collect(html);
    if formulas.is_none() && !has_math(html) {
        return MathHoles {
            html: marked,
            formulas: None,
        };
    }
    let f = formulas.as_ref();
    let settings = RewriteStrSettings::new()
        .with_enable_esi_tags(false)
        .append_element_content_handler(element!("math", |el| {
            match Formulas::token_for(f, el) {
                Some(token) => el.replace(&token, ContentType::Text),
                None => el.remove(),
            }
            Ok(())
        }));
    let html = match rewrite_str(&marked, settings) {
        Ok(h) => h,
        // Nothing of the input is trusted.
        Err(_) => escape_html(html),
    };
    MathHoles { html, formulas }
}

/// HTML with every formula taken out, waiting for its drawings.
#[derive(Debug)]
pub struct MathHoles {
    html: String,
    formulas: Option<Formulas>,
}

impl MathHoles {
    /// The HTML to clean: no `math` element is left in it.
    pub fn html(&self) -> &str {
        &self.html
    }

    /// Draw the formulas into `cleaned` (the sanitizer's output of
    /// [`MathHoles::html`]), where each one's token still stands in text.
    pub fn fill(&self, cleaned: &str) -> String {
        match &self.formulas {
            Some(f) => f.fill(cleaned, true),
            None => cleaned.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn through(html: &str) -> String {
        let holes = hole_math(html);
        holes.fill(holes.html())
    }

    #[test]
    fn a_formula_becomes_its_tex_in_code() {
        let html = "<p>a <math><semantics><mi>x</mi><annotation encoding=\"application/x-tex\">x^2 &lt; y</annotation></semantics></math> b</p>";
        let out = through(html);
        assert!(!out.contains("<math"), "{out}");
        assert!(out.contains("x^2 &lt; y"), "{out}");
        assert!(out.starts_with("<p>a "), "{out}");
    }

    #[test]
    fn no_annotation_no_formula() {
        assert_eq!(
            through("<p>a<math><mi onclick=\"x()\">x</mi><script>alert(1)</script></math>b</p>"),
            "<p>ab</p>"
        );
        assert_eq!(through("<p>a<math/>b</p>"), "<p>ab</p>");
        // Another encoding is not TeX.
        assert_eq!(
            through("<math><annotation encoding=\"text/html\"><b>x</b></annotation></math>"),
            ""
        );
    }

    #[test]
    fn display_comes_from_the_top_level_element() {
        let out = through(
            "<math display=\" BLOCK \"><annotation encoding=\"application/x-tex\">a</annotation></math>",
        );
        assert!(out.contains("$$a$$"), "{out}");
    }

    #[test]
    fn only_the_first_top_level_annotation_counts() {
        let out = through(
            "<math><annotation encoding=\"application/x-tex\">one</annotation>\
             <annotation encoding=\"application/x-tex\">two</annotation>\
             <math><annotation encoding=\"application/x-tex\">nested</annotation></math></math>",
        );
        assert!(out.contains("$one$"), "{out}");
        assert!(!out.contains("two") && !out.contains("nested"), "{out}");
        // A nested formula's annotation never stands in for the top-level one.
        let out = through(
            "<math><math><annotation encoding=\"application/x-tex\">inner</annotation></math></math>",
        );
        assert_eq!(out, "");
    }

    #[test]
    fn hostile_tex_stays_text() {
        let out = through(
            "<math><annotation encoding=\"application/x-tex\">x&lt;/annotation&gt;&lt;script&gt;alert(1)&lt;/script&gt;</annotation></math>",
        );
        assert!(!out.contains("<script"), "{out}");
        assert!(out.contains("&lt;script&gt;"), "{out}");
        // A real closing tag ends the annotation; what follows is markup the
        // formula discards.
        let out = through(
            "<math><annotation encoding=\"application/x-tex\">x</annotation><script>alert(1)</script><mi onmouseover=\"x()\">y</mi></math>",
        );
        assert_eq!(out, "<code class=\"math-source\">$x$</code>");
    }

    #[test]
    fn a_planted_token_is_never_substituted() {
        // The input cannot know the token, but it can try to look like one.
        let holes = hole_math(
            "<p title=\"mfoo0x\">mfoo0x</p><math><annotation encoding=\"application/x-tex\">a</annotation></math>",
        );
        let token = holes.formulas.as_ref().unwrap().token.clone();
        assert!(!token.starts_with("mfoo"));
        let out = holes.fill(holes.html());
        assert_eq!(
            out,
            "<p title=\"mfoo0x\">mfoo0x</p><code class=\"math-source\">$a$</code>"
        );
        // A token the cleaner moved into a tag is not drawn there.
        let cleaned = format!("<p title=\"{token}0x\">t</p>{token}0x");
        let drawn = holes.fill(&cleaned);
        assert_eq!(
            drawn,
            "<p title=\"\">t</p><code class=\"math-source\">$a$</code>"
        );
    }

    #[test]
    fn budgets() {
        let one = "<math><annotation encoding=\"application/x-tex\">x</annotation></math>";
        let long = format!(
            "<math><annotation encoding=\"application/x-tex\">{}</annotation></math>",
            "y".repeat(TEX_MAX + 1)
        );
        let html = format!("{long}{}", one.repeat(3));
        let f = Formulas::collect(&html).1.unwrap();
        assert_eq!(f.formulas.len(), 4);
        let mut b = Budget::default();
        b.draw(&"y".repeat(TEX_MAX + 1), false);
        assert_eq!(b.spent, 0, "an undrawn formula is not charged");
        for _ in 0..FORMULAS_MAX {
            b.draw("x", false);
        }
        assert_eq!(b.drawn, FORMULAS_MAX);
        b.draw("x", false);
        assert_eq!(b.spent, TEX_BUDGET, "spent: later formulas are text");
    }

    #[test]
    fn js_lengths() {
        assert_eq!(js_len("ab"), 2);
        assert_eq!(js_len("é"), 1);
        assert_eq!(js_len("𝑥"), 2);
    }
}
