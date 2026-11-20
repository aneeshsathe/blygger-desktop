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
//!
//! It also draws formulas (`math-render.ts`) and finds them in Markdown
//! (`math.ts`), for the preview.

use std::cell::RefCell;
use std::rc::Rc;

use lol_html::html_content::{ContentType, EndTag};
use lol_html::{RewriteStrSettings, element, rewrite_str, text};

use std::sync::OnceLock;

use markdown_it::parser::inline::{InlineRule, InlineState};
use markdown_it::{Node, NodeValue, Renderer};
use regex::Regex;

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
    let drawn = render_tex(tex, display);
    match drawn.error {
        None => drawn.html,
        Some(_) => tex_as_text(tex, display),
    }
}

// ---------------------------------------------------------------------------
// Drawing (the studio's `math-render.ts`): TeX → MathML Core with its TeX
// in an `application/x-tex` annotation, through `math-core`, and checked
// against the studio's allowlist before anyone sees it, so a crate upgrade
// can't widen what reaches a page without a test failing.

/// `MATH_ELEMENTS`: what a formula may be made of.
pub const MATH_ELEMENTS: &[&str] = &[
    "math",
    "semantics",
    "annotation",
    "mrow",
    "mi",
    "mn",
    "mo",
    "ms",
    "mtext",
    "mspace",
    "mfrac",
    "msqrt",
    "mroot",
    "mstyle",
    "merror",
    "mpadded",
    "mphantom",
    "msub",
    "msup",
    "msubsup",
    "munder",
    "mover",
    "munderover",
    "mmultiscripts",
    "mprescripts",
    "mtable",
    "mtr",
    "mtd",
];
/// `MATH_ATTRIBUTES`.
pub const MATH_ATTRIBUTES: &[&str] = &[
    "display",
    "displaystyle",
    "scriptlevel",
    "mathvariant",
    "stretchy",
    "fence",
    "separator",
    "lspace",
    "rspace",
    "movablelimits",
    "accent",
    "accentunder",
    "form",
    "largeop",
    "symmetric",
    "minsize",
    "maxsize",
    "linethickness",
    "columnalign",
    "rowalign",
    "columnspan",
    "rowspan",
    "width",
    "height",
    "depth",
    "voffset",
    "encoding",
    "class",
];
/// Attributes that are lengths (`LENGTHS`).
const LENGTHS: &[&str] = &[
    "width",
    "height",
    "depth",
    "voffset",
    "lspace",
    "rspace",
    "minsize",
    "maxsize",
    "linethickness",
];
/// How far one `voffset` may move text, and all of a formula's together (em).
const VOFFSET_MAX: f64 = 2.;
const VOFFSET_TOTAL: f64 = 4.;

/// A length in em, by rough equivalence; `None` if it isn't one a formula
/// may write (`lengthEm`: `/^(-?\d*\.?\d+)(em|ex|px|pt|%)?$/`).
fn length_em(value: &str) -> Option<f64> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^(-?\d*\.?\d+)(em|ex|px|pt|%)?$").expect("re"));
    let m = re.captures(value.trim())?;
    let n: f64 = m[1].parse().ok()?;
    let per = match m.get(2).map(|u| u.as_str()) {
        None | Some("em") => 1.,
        Some("ex") => 0.5,
        Some("px") => 1. / 16.,
        Some("pt") => 1. / 12.,
        _ => 0.2,
    };
    Some(n * per)
}

/// `allowedLength`: at most 20em, at least -2em.
fn allowed_length(value: &str) -> bool {
    value == "-1width" || length_em(value).is_some_and(|em| (-2. ..=20.).contains(&em))
}

/// `allowedMath`: only allowed element and attribute names, and bounded
/// lengths.
pub fn allowed_math(html: &str) -> bool {
    static TAG: OnceLock<Regex> = OnceLock::new();
    static ATTR: OnceLock<Regex> = OnceLock::new();
    let tag = TAG.get_or_init(|| Regex::new(r"</?([^\s/>]+)([^>]*)>").expect("re"));
    let attr = ATTR.get_or_init(|| {
        Regex::new(r#"([^\s=/]+)\s*(?:=\s*("[^"]*"|'[^']*'|[^\s>]+))?"#).expect("re")
    });
    let mut moved = 0.;
    for t in tag.captures_iter(html) {
        if !MATH_ELEMENTS.contains(&t[1].to_ascii_lowercase().as_str()) {
            return false;
        }
        for a in attr.captures_iter(&t[2]) {
            let name = a[1].to_ascii_lowercase();
            if !MATH_ATTRIBUTES.contains(&name.as_str()) {
                return false;
            }
            let raw = a.get(2).map_or("", |m| m.as_str());
            let value = raw.trim_matches(|c| c == '"' || c == '\'');
            if LENGTHS.contains(&name.as_str()) && !allowed_length(value) {
                return false;
            }
            if name == "voffset" {
                let em = length_em(value).map_or(f64::INFINITY, f64::abs);
                moved += em;
                if em > VOFFSET_MAX || moved > VOFFSET_TOTAL {
                    return false;
                }
            }
        }
    }
    true
}

/// Where the element opening at `start` closes: the index just past its
/// end tag (`closeOf`).
fn close_of(html: &str, start: usize, name: &str) -> Option<usize> {
    let open = format!("<{name}");
    let close = format!("</{name}>");
    let mut depth = 0usize;
    let mut at = start;
    loop {
        let o = html[at..].find(&open).map(|i| at + i);
        let c = html[at..].find(&close).map(|i| at + i);
        match (o, c) {
            (Some(o), c) if c.is_none_or(|c| o < c) => {
                let end = html[o..].find('>')? + o;
                if !html[..end].ends_with('/') {
                    depth += 1;
                } else if depth == 0 {
                    return Some(end + 1);
                }
                at = end + 1;
            }
            (_, Some(c)) => {
                depth = depth.checked_sub(1)?;
                at = c + close.len();
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => return None,
        }
    }
}

/// `withoutMenclose`: `menclose` is not MathML Core. A box draws plain, an
/// over- or underline as an accent; an enclosure whose loss changes what
/// the formula says is refused.
fn without_menclose(mut html: String) -> Result<String, String> {
    while let Some(at) = html.rfind("<menclose") {
        let undrawable = || "this formula could not be drawn".to_string();
        let end = close_of(&html, at, "menclose").ok_or_else(undrawable)?;
        let open_end = html[at..].find('>').ok_or_else(undrawable)? + at + 1;
        let notation = html[at..open_end]
            .split_once(" notation=\"")
            .and_then(|(_, r)| r.split_once('"'))
            .map_or("", |(n, _)| n)
            .to_string();
        let inner = if html[..open_end].ends_with("/>") {
            String::new()
        } else {
            html[open_end..end - "</menclose>".len()].to_string()
        };
        let replacement = match notation.as_str() {
            "top" => format!(
                "<mover accent=\"true\"><mrow>{inner}</mrow><mo stretchy=\"true\">\u{203E}</mo></mover>"
            ),
            "bottom" => format!(
                "<munder accentunder=\"true\"><mrow>{inner}</mrow><mo stretchy=\"true\">_</mo></munder>"
            ),
            "box" => inner,
            other => {
                let command = match other {
                    "updiagonalstrike" => "\\cancel",
                    "downdiagonalstrike" => "\\bcancel",
                    "updiagonalstrike downdiagonalstrike" => "\\xcancel",
                    "horizontalstrike" => "\\sout",
                    "actuarial" => "\\angl",
                    "phasorangle" => "\\phase",
                    "longdiv" => "\\longdiv",
                    _ => "this command",
                };
                return Err(format!(
                    "{command} needs a stylesheet most readers will not have, so the Studio does not publish it"
                ));
            }
        };
        html.replace_range(at..end, &replacement);
    }
    Ok(html)
}

/// A formula drawn, or the reason it can't be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drawn {
    pub html: String,
    pub error: Option<String>,
}

/// `refusedTex`: a refused formula as the preview shows it (publish refuses
/// before it would be baked).
pub fn refused_tex(tex: &str, display: bool, reason: &str) -> String {
    format!(
        "<code class=\"math-error\" title=\"{}\">{}</code>",
        escape_html(reason),
        escape_html(&written(tex, display))
    )
}

fn convert(tex: &str, display: bool) -> Result<String, String> {
    use math_core::{LatexToMathML, MathCoreConfig, MathDisplay};
    thread_local! {
        static CONVERTER: Option<LatexToMathML> = LatexToMathML::new(
            MathCoreConfig { annotation: true, ..Default::default() },
            Vec::new(),
        )
        .ok();
    }
    let mode = if display {
        MathDisplay::Block
    } else {
        MathDisplay::Inline
    };
    CONVERTER.with(|c| {
        let c = c.as_ref().ok_or("the formula renderer is unavailable")?;
        c.convert_with_local_state(tex, mode)
            .map(|r| r.mathml)
            .map_err(|e| e.error_message())
    })
}

/// `renderTex`: draw `tex`, or say why not (with the refusal's HTML).
pub fn render_tex(tex: &str, display: bool) -> Drawn {
    let failed = |reason: String| Drawn {
        html: refused_tex(tex, display, &reason),
        error: Some(reason),
    };
    if js_len(tex) > TEX_MAX {
        return failed(format!("a formula is limited to {TEX_MAX} characters"));
    }
    let html = match convert(tex, display).and_then(without_menclose) {
        Ok(h) => h,
        Err(e) => return failed(e),
    };
    // No formula restyles the page it lands on (§23.4).
    static STYLE: OnceLock<Regex> = OnceLock::new();
    let style = STYLE.get_or_init(|| Regex::new(r#"\sstyle="[^"]*""#).expect("re"));
    let html = style.replace_all(&html, "").into_owned();
    if !allowed_math(&html) {
        return failed(
            "this formula uses MathML the Studio does not publish yet, or moves its text further than a formula may"
                .into(),
        );
    }
    Drawn { html, error: None }
}

// ---------------------------------------------------------------------------
// Finding formulas in Markdown (the studio's `math.ts`): pandoc's
// `tex_math_dollars`. `$…$` is inline: the opening `$` has a non-space right
// after it, the closing one a non-space right before it and no digit right
// after it, so "$5 and $10" stays prose. `$$…$$` is displayed. `\$` is a
// dollar sign (markdown's escape runs first). Code is code: the backtick
// rule takes a code span before this one sees its `$`.

/// A formula in one inline source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MathSpan {
    pub tex: String,
    pub display: bool,
    pub end: usize,
}

/// For each byte of one inline source, the next dollar that could close an
/// inline formula and the next `$$`, both unescaped: one pass per source,
/// so finding formulas stays linear however many lone dollars there are.
#[derive(Debug, Default)]
pub(crate) struct Closers {
    inline: Vec<usize>,
    display: Vec<usize>,
}

const NONE: usize = usize::MAX;

/// JavaScript's `/\s/`, with no character (the ends) counting as space.
fn is_space(c: Option<char>) -> bool {
    c.is_none_or(crate::util::is_js_ws)
}

impl Closers {
    pub(crate) fn of(src: &str) -> Closers {
        let b = src.as_bytes();
        let n = b.len();
        let mut escaped = vec![false; n + 1];
        for k in 0..n {
            if b[k] == b'\\' && !escaped[k] {
                escaped[k + 1] = true;
            }
        }
        let mut inline = vec![NONE; n + 1];
        let mut display = vec![NONE; n + 1];
        let (mut next_inline, mut next_display) = (NONE, NONE);
        for j in (0..n).rev() {
            if b[j] == b'$' && !escaped[j] {
                let before = src[..j].chars().next_back();
                if !is_space(before) && !b.get(j + 1).is_some_and(u8::is_ascii_digit) {
                    next_inline = j;
                }
                if b.get(j + 1) == Some(&b'$') {
                    next_display = j;
                }
            }
            inline[j] = next_inline;
            display[j] = next_display;
        }
        Closers { inline, display }
    }

    /// `mathAt`: the formula opening at `pos` of `src`, if one does. A
    /// formula longer than TEX_MAX is not drawn, so a dollar further away
    /// than that doesn't close one.
    pub(crate) fn math_at(&self, src: &str, pos: usize) -> Option<MathSpan> {
        let b = src.as_bytes();
        if b.get(pos) != Some(&b'$') {
            return None;
        }
        let too_long = |a: usize, j: usize| j - a > TEX_MAX && js_len(&src[a..j]) > TEX_MAX;
        if b.get(pos + 1) == Some(&b'$') {
            let j = *self.display.get(pos + 2)?;
            if j == NONE || too_long(pos + 2, j) {
                return None;
            }
            let tex = &src[pos + 2..j];
            return (!crate::util::js_trim(tex).is_empty()).then(|| MathSpan {
                tex: tex.to_string(),
                display: true,
                end: j + 2,
            });
        }
        if is_space(src[pos + 1..].chars().next()) {
            return None;
        }
        let j = *self.inline.get(pos + 1)?;
        if j == NONE || too_long(pos + 1, j) {
            return None;
        }
        Some(MathSpan {
            tex: src[pos + 1..j].to_string(),
            display: false,
            end: j + 1,
        })
    }
}

impl markdown_it::parser::extset::InlineRootExt for Closers {}

/// A formula in the Markdown tree. The HTML render draws it (or refuses
/// it); the native stream shows it as code.
#[derive(Debug)]
pub struct MathFormula {
    pub tex: String,
    pub display: bool,
}

fn is_sentinel(c: char) -> bool {
    ('\u{E000}'..='\u{F8FF}').contains(&c)
}

impl MathFormula {
    /// The formula as written, for an error: each run of private-use
    /// placeholders (`SENTINELS`, a sentinel, what it numbers, a sentinel)
    /// shown as `…`.
    fn written_for_errors(&self) -> String {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| {
            Regex::new(r"[\x{E000}-\x{F8FF}]+[^\x{E000}-\x{F8FF}]*?[\x{E000}-\x{F8FF}]+|[\x{E000}-\x{F8FF}]+")
                .expect("re")
        });
        written(&re.replace_all(&self.tex, "…"), self.display)
    }

    /// Draw it: the HTML, and the publish error when it doesn't draw.
    pub(crate) fn draw(&self) -> (String, Option<MathError>) {
        // By the time a formula renders, a link, a mention or a generated
        // span inside it is a private sentinel. Refused whether or not it
        // draws (the studio's session 70, R1a).
        let (html, reason) = if self.tex.chars().any(is_sentinel) {
            let reason = "a formula cannot contain a link, a mention or a generated span";
            (
                refused_tex(&self.tex, self.display, reason),
                Some(reason.to_string()),
            )
        } else {
            let d = render_tex(&self.tex, self.display);
            (d.html, d.error)
        };
        let err = reason.map(|reason| MathError {
            formula: self.written_for_errors(),
            reason,
        });
        (html, err)
    }
}

impl NodeValue for MathFormula {
    fn render(&self, _: &Node, fmt: &mut dyn Renderer) {
        fmt.text_raw(&self.draw().0);
    }
}

/// A formula that doesn't draw, which publish refuses (the studio's
/// `MathError`, also the 400's `details.errors[]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MathError {
    /// As written, delimiters included.
    pub formula: String,
    pub reason: String,
}

/// The inline rule (`mathInline`).
pub(crate) struct MathInline;

impl InlineRule for MathInline {
    const MARKER: char = '$';

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        if state.inline_ext.get::<Closers>().is_none() {
            let c = Closers::of(&state.src);
            state.inline_ext.insert(c);
        }
        let span = state
            .inline_ext
            .get::<Closers>()?
            .math_at(&state.src, state.pos)?;
        if span.end > state.pos_max {
            return None;
        }
        let len = span.end - state.pos;
        let mut node = Node::new(MathFormula {
            tex: span.tex,
            display: span.display,
        });
        node.srcmap = state.get_map(state.pos, span.end);
        Some((node, len))
    }
}

/// Every formula under `node` that doesn't draw, in document order.
pub(crate) fn math_errors(node: &Node) -> Vec<MathError> {
    let mut out = Vec::new();
    node.walk(|n, _| {
        if let Some(m) = n.cast::<MathFormula>()
            && let (_, Some(e)) = m.draw()
        {
            out.push(e);
        }
    });
    out
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

    fn ann(tex: &str) -> String {
        format!("<math><annotation encoding=\"application/x-tex\">{tex}</annotation></math>")
    }

    fn drawn(tex: &str, display: bool) -> String {
        let d = render_tex(tex, display);
        assert_eq!(d.error, None, "{tex}");
        d.html
    }

    #[test]
    fn a_formula_is_redrawn_from_its_annotation() {
        let html = "<p>a <math><semantics><mi onclick=\"x()\">q</mi><annotation encoding=\"application/x-tex\">x^2 &lt; y</annotation></semantics></math> b</p>";
        let out = through(html);
        assert_eq!(out, format!("<p>a {} b</p>", drawn("x^2 < y", false)));
        assert!(!out.contains("onclick") && !out.contains(">q<"), "{out}");
        assert!(
            out.contains("<annotation encoding=\"application/x-tex\">x^2 &lt; y</annotation>"),
            "{out}"
        );
    }

    #[test]
    fn a_formula_that_does_not_draw_is_its_tex_in_code() {
        assert_eq!(
            through(&ann("\\frac{")),
            "<code class=\"math-source\">$\\frac{$</code>"
        );
        assert_eq!(
            through(&ann("\\cancel{x}")),
            "<code class=\"math-source\">$\\cancel{x}$</code>"
        );
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
        assert_eq!(out, drawn("a", true));
        assert!(out.starts_with("<math display=\"block\">"), "{out}");
    }

    #[test]
    fn only_the_first_top_level_annotation_counts() {
        let out = through(
            "<math><annotation encoding=\"application/x-tex\">one</annotation>\
             <annotation encoding=\"application/x-tex\">two</annotation>\
             <math><annotation encoding=\"application/x-tex\">nested</annotation></math></math>",
        );
        assert_eq!(out, drawn("one", false));
        // A nested formula's annotation never stands in for the top-level one.
        let out = through(
            "<math><math><annotation encoding=\"application/x-tex\">inner</annotation></math></math>",
        );
        assert_eq!(out, "");
    }

    #[test]
    fn hostile_tex_stays_text() {
        let out = through(&ann(
            "x&lt;/annotation&gt;&lt;script&gt;alert(1)&lt;/script&gt;",
        ));
        assert!(!out.contains("<script"), "{out}");
        assert!(allowed_math(&out), "{out}");
        assert!(out.contains("&lt;/annotation&gt;&lt;script&gt;"), "{out}");
        // Attribute injection through `\text`.
        let out = through(&ann("\\text{&quot;&gt;&lt;img src=x onerror=alert(1)&gt;}"));
        assert!(!out.contains("<img"), "{out}");
        assert!(allowed_math(&out), "{out}");
        // A real closing tag ends the annotation; what follows is markup the
        // formula discards.
        let out = through(
            "<math><annotation encoding=\"application/x-tex\">x</annotation><script>alert(1)</script><mi onmouseover=\"x()\">y</mi></math>",
        );
        assert_eq!(out, drawn("x", false));
    }

    #[test]
    fn a_planted_token_is_never_substituted() {
        // The input cannot know the token, but it can try to look like one.
        let holes = hole_math(&format!("<p title=\"mfoo0x\">mfoo0x</p>{}", ann("a")));
        let token = holes.formulas.as_ref().unwrap().token.clone();
        assert!(!token.starts_with("mfoo"));
        let out = holes.fill(holes.html());
        assert_eq!(
            out,
            format!("<p title=\"mfoo0x\">mfoo0x</p>{}", drawn("a", false))
        );
        // A token the cleaner moved into a tag is not drawn there.
        let cleaned = format!("<p title=\"{token}0x\">t</p>{token}0x");
        assert_eq!(
            holes.fill(&cleaned),
            format!("<p title=\"\">t</p>{}", drawn("a", false))
        );
    }

    #[test]
    fn budgets() {
        let long = "y".repeat(TEX_MAX + 1);
        let mut b = Budget::default();
        assert_eq!(b.draw(&long, false), tex_as_text(&long, false));
        assert_eq!(b.spent, 0, "an undrawn formula is not charged");
        for _ in 0..FORMULAS_MAX {
            b.draw("x", false);
        }
        assert_eq!(b.drawn, FORMULAS_MAX);
        assert_eq!(b.draw("x", false), tex_as_text("x", false));
        assert_eq!(b.spent, TEX_BUDGET, "spent: later formulas are text");
        let mut b = Budget::default();
        let big = "x".repeat(TEX_MAX);
        for _ in 0..TEX_BUDGET / TEX_MAX {
            b.draw(&big, false);
        }
        assert_eq!(b.draw("y", false), tex_as_text("y", false), "100k drawn");
    }

    #[test]
    fn js_lengths() {
        assert_eq!(js_len("ab"), 2);
        assert_eq!(js_len("é"), 1);
        assert_eq!(js_len("𝑥"), 2);
    }

    #[test]
    fn drawing_keeps_to_the_allowlist() {
        for (tex, ok) in [
            ("x^2", true),
            ("\\frac{a}{b}", true),
            ("\\sqrt[3]{x}", true),
            ("\\sum_{i=0}^N x_i", true),
            ("\\begin{pmatrix}a&b\\\\c&d\\end{pmatrix}", true),
            ("\\boxed{x}", true),
            ("\\color{red}{x}", true),
            ("\\frac{", false),
            ("\\href{javascript:alert(1)}{x}", false),
            ("\\raisebox{-30em}{x}", false),
            ("\\hspace{9999em}", false),
            ("\\cancel{x}", false),
            ("\\texttt{x}", false),
        ] {
            let d = render_tex(tex, false);
            assert_eq!(d.error.is_none(), ok, "{tex}: {d:?}");
            if ok {
                assert!(allowed_math(&d.html), "{tex}: {}", d.html);
                assert!(!d.html.contains(" style="), "{tex}: {}", d.html);
                assert_eq!(d.html.matches("<annotation ").count(), 1, "{tex}");
            } else {
                assert!(d.html.starts_with("<code class=\"math-error\" title=\""));
            }
        }
        assert!(
            render_tex("\\cancel{x}", false)
                .error
                .unwrap()
                .starts_with("\\cancel needs a stylesheet")
        );
        assert!(
            render_tex(&"x".repeat(TEX_MAX + 1), false)
                .error
                .unwrap()
                .contains("limited to 4000")
        );
    }

    #[test]
    fn allowlist_and_lengths() {
        assert!(allowed_math("<math><mi>x</mi></math>"));
        assert!(!allowed_math("<math><mi onclick=\"x\">x</mi></math>"));
        assert!(!allowed_math("<math><script></script></math>"));
        assert!(!allowed_math("<math><mi style=\"x\">x</mi></math>"));
        assert!(allowed_math("<mspace width=\"20em\"/>"));
        assert!(!allowed_math("<mspace width=\"21em\"/>"));
        assert!(!allowed_math("<mspace width=\"-3em\"/>"));
        assert!(allowed_math(
            "<mpadded voffset=\"2em\"></mpadded><mpadded voffset=\"-2em\"></mpadded>"
        ));
        assert!(!allowed_math(
            "<mpadded voffset=\"2em\"></mpadded><mpadded voffset=\"2em\"></mpadded><mpadded voffset=\"1em\"></mpadded>"
        ));
        assert!(!allowed_math("<mspace width=\"1cm\"/>"));
    }

    fn spans(src: &str) -> Vec<(String, bool)> {
        let c = Closers::of(src);
        let mut out = Vec::new();
        let mut pos = 0;
        while pos < src.len() {
            if src.as_bytes()[pos] == b'\\' {
                pos += 2;
                continue;
            }
            match c.math_at(src, pos) {
                Some(s) => {
                    out.push((s.tex, s.display));
                    pos = s.end;
                }
                None => pos += 1,
            }
        }
        out
    }

    #[test]
    fn pandoc_dollars() {
        assert!(spans("$5 and $10").is_empty());
        assert!(spans("costs $5.").is_empty());
        assert!(spans("$x and y$2").is_empty(), "no digit after a closer");
        // The studio reads this as one formula (and temml refuses it).
        assert_eq!(spans("$x $ and $ x$"), [("x $ and $ x".to_string(), false)]);
        assert!(spans("\\$x$").is_empty());
        assert_eq!(spans("a $x^2$ b"), [("x^2".to_string(), false)]);
        assert_eq!(
            spans("$$\\frac{a}{b}$$"),
            [("\\frac{a}{b}".to_string(), true)]
        );
        assert_eq!(spans("$a\\$b$"), [("a\\$b".to_string(), false)]);
        assert!(spans("$$  $$").is_empty());
    }

    #[test]
    fn finding_formulas_is_linear() {
        // Lone dollars that close nothing: one pass, however many there are.
        let src = "$a ".repeat(200_000);
        let t = std::time::Instant::now();
        assert!(spans(&src).is_empty());
        assert!(t.elapsed().as_secs() < 5, "{:?}", t.elapsed());
        // The same through the Markdown engine.
        let t = std::time::Instant::now();
        assert!(!crate::render_markdown(&src).contains("<math"));
        assert!(t.elapsed().as_secs() < 5, "{:?}", t.elapsed());
    }

    #[test]
    fn markdown_formulas_code_and_money() {
        let html = crate::render_markdown("a $x^2$ b `$y$` and $5 and $10");
        assert_eq!(html.matches("<math").count(), 1, "{html}");
        assert!(html.contains("<code>$y$</code>"), "code is code: {html}");
        assert!(html.contains("$5 and $10"), "money is prose: {html}");
        let html = crate::render_markdown("```\n$x$\n```");
        assert!(!html.contains("<math"), "{html}");
    }

    #[test]
    fn a_bad_formula_is_a_publish_error_in_the_preview_stats() {
        let r = crate::render_preview(
            "fine $x$ and bad $\\frac{$",
            crate::Kind::Fragment,
            &crate::NoResolver,
            &crate::RenderOpts::default(),
        );
        assert_eq!(r.stats.math_errors.len(), 1, "{:?}", r.stats.math_errors);
        assert_eq!(r.stats.math_errors[0].formula, "$\\frac{$");
        assert!(r.html.contains("class=\"math-error\""), "{}", r.html);
        let thread = crate::render_preview(
            "bad $$\\frac{$$",
            crate::Kind::Thread,
            &crate::NoResolver,
            &crate::RenderOpts::default(),
        );
        assert_eq!(thread.stats.math_errors.len(), 1);
    }

    #[test]
    fn the_native_stream_shows_tex_as_code() {
        let blocks = crate::native_blocks("Area $\\pi r^2$ here", crate::Kind::Fragment);
        let crate::Block::Para(spans) = &blocks[0] else {
            panic!("{blocks:?}")
        };
        assert!(
            spans.iter().any(|s| s.code && s.text == "$\\pi r^2$"),
            "{spans:?}"
        );
    }
}
