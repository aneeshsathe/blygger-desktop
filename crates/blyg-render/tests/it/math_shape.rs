//! Formulas are compared by shape, not bytes. The studio draws MathML with
//! temml 0.14 (`math-render.ts`), which no Rust crate reproduces byte for
//! byte, so `shape` replaces each top-level `<math>` with a canonical stand-in
//! that keeps what parity can promise:
//!
//! - inline or display (`display="block"`);
//! - the formula's `application/x-tex` annotation, which is the author's TeX
//!   (temml's `annotate`; the sanitizer redraws from it), entity-decoded;
//! - that every element and attribute inside is on the studio's allowlist
//!   (`MATH_ELEMENTS`, `MATH_ATTRIBUTES`) and carries no `style`.
//!
//! A refused formula's `<code class="math-error" title="…">` keeps its text
//! and loses its title, temml's parse message. Everything around a formula
//! is still compared byte for byte. `docs/RENDER.md` ("Formulas") explains it.

use regex::Regex;
use std::sync::LazyLock;

/// `math-render.ts`'s `MATH_ELEMENTS` (studio 0.64.1).
const MATH_ELEMENTS: &[&str] = &[
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

/// `math-render.ts`'s `MATH_ATTRIBUTES` (studio 0.64.1).
const MATH_ATTRIBUTES: &[&str] = &[
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

static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<(/?)([^\s/>]+)([^>]*)>").unwrap());
static ATTR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"([^\s=/]+)\s*(?:=\s*("[^"]*"|'[^']*'|[^\s>]+))?"#).unwrap());
static ANNOTATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<annotation encoding="application/x-tex">(.*?)</annotation>"#).unwrap()
});
static MATH_ERROR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<code class="math-error" title="([^"]*)">"#).unwrap());

/// `html` with every top-level formula replaced by its stand-in, and the
/// problems found in the formulas (none for a well-formed one).
pub fn shape(html: &str) -> (String, Vec<String>) {
    let mut problems = Vec::new();
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(at) = find_open(rest) {
        out.push_str(&rest[..at]);
        let Some(len) = element_len(&rest[at..]) else {
            problems.push(format!("unclosed <math> at {:?}", clip(&rest[at..])));
            out.push_str(&rest[at..]);
            rest = "";
            break;
        };
        let formula = &rest[at..at + len];
        out.push_str(&stand_in(formula, &mut problems));
        rest = &rest[at + len..];
    }
    out.push_str(rest);
    let mut shaped = String::with_capacity(out.len());
    let mut last = 0;
    for m in MATH_ERROR.captures_iter(&out) {
        let whole = m.get(0).unwrap();
        if m[1].trim().is_empty() {
            problems.push("a refused formula's math-error code has no reason".into());
        }
        shaped.push_str(&out[last..whole.start()]);
        shaped.push_str("<code class=\"math-error\" title=\"…\">");
        last = whole.end();
    }
    shaped.push_str(&out[last..]);
    (shaped, problems)
}

fn find_open(s: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = s[from..].find("<math") {
        let at = from + i;
        match s.as_bytes().get(at + 5) {
            Some(b'>' | b' ' | b'\n' | b'\t' | b'/') => return Some(at),
            _ => from = at + 5,
        }
    }
    None
}

/// The length of the `<math>` element at the start of `s`, end tag included.
fn element_len(s: &str) -> Option<usize> {
    let mut depth = 0usize;
    for m in TAG.captures_iter(s) {
        if !m[2].eq_ignore_ascii_case("math") {
            continue;
        }
        let whole = m.get(0).unwrap();
        if &m[1] == "/" {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(whole.end());
            }
        } else if whole.as_str().ends_with("/>") {
            if depth == 0 {
                return Some(whole.end());
            }
        } else {
            depth += 1;
        }
    }
    None
}

fn stand_in(formula: &str, problems: &mut Vec<String>) -> String {
    let open_end = formula.find('>').unwrap_or(formula.len());
    let display = formula[..open_end].contains("display=\"block\"");
    for m in TAG.captures_iter(formula) {
        let name = m[2].to_ascii_lowercase();
        if !MATH_ELEMENTS.contains(&name.as_str()) {
            problems.push(format!("<{name}> is not on the MathML allowlist"));
        }
        for a in ATTR.captures_iter(&m[3]) {
            let attr = a[1].to_ascii_lowercase();
            if !MATH_ATTRIBUTES.contains(&attr.as_str()) {
                problems.push(format!(
                    "{attr}= on <{name}> is not on the MathML allowlist"
                ));
            }
        }
    }
    let annotations: Vec<_> = ANNOTATION.captures_iter(formula).collect();
    let tex = match annotations.as_slice() {
        [one] => html_escape::decode_html_entities(&one[1]).into_owned(),
        [] => {
            problems.push(format!(
                "formula without an x-tex annotation: {}",
                clip(formula)
            ));
            String::new()
        }
        _ => {
            problems.push(format!(
                "formula with several x-tex annotations: {}",
                clip(formula)
            ));
            String::new()
        }
    };
    format!(
        "<math display=\"{}\">{}</math>",
        if display { "block" } else { "inline" },
        html_escape::encode_text(&tex)
    )
}

fn clip(s: &str) -> &str {
    let mut end = s.len().min(120);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[test]
fn shape_keeps_display_and_tex_and_drops_the_drawing() {
    let temml = "<p>a <math><semantics><msup><mi>x</mi><mn class=\"tml-sml-pad\">2</mn></msup><annotation encoding=\"application/x-tex\">x^2 &lt; y</annotation></semantics></math> b <math display=\"block\" class=\"tml-display\"><semantics><mi>y</mi><annotation encoding=\"application/x-tex\">y</annotation></semantics></math></p>";
    let other = "<p>a <math><mrow><msup><mi>x</mi><mn>2</mn></msup></mrow><semantics><annotation encoding=\"application/x-tex\">x^2 &#60; y</annotation></semantics></math> b <math display=\"block\"><semantics><mi>y</mi><annotation encoding=\"application/x-tex\">y</annotation></semantics></math></p>";
    let (a, pa) = shape(temml);
    let (b, pb) = shape(other);
    assert!(pa.is_empty() && pb.is_empty(), "{pa:?} {pb:?}");
    assert_eq!(a, b);
    assert_eq!(
        a,
        "<p>a <math display=\"inline\">x^2 &lt; y</math> b <math display=\"block\">y</math></p>"
    );
}

#[test]
fn shape_bites_on_tex_display_and_allowlist() {
    let base = shape("<math><semantics><mi>x</mi><annotation encoding=\"application/x-tex\">x</annotation></semantics></math>").0;
    let other_tex = shape("<math><semantics><mi>x</mi><annotation encoding=\"application/x-tex\">x </annotation></semantics></math>").0;
    let block = shape("<math display=\"block\"><semantics><mi>x</mi><annotation encoding=\"application/x-tex\">x</annotation></semantics></math>").0;
    assert_ne!(base, other_tex);
    assert_ne!(base, block);
    let (_, styled) = shape(
        "<math><semantics><mi style=\"color:red\">x</mi><annotation encoding=\"application/x-tex\">x</annotation></semantics></math>",
    );
    assert!(!styled.is_empty(), "{styled:?}");
    let (_, foreign) = shape(
        "<math><semantics><menclose><mi>x</mi></menclose><annotation encoding=\"application/x-tex\">x</annotation></semantics></math>",
    );
    assert!(!foreign.is_empty(), "{foreign:?}");
    let (_, bare) = shape("<math><mi>x</mi></math>");
    assert_eq!(bare.len(), 1, "{bare:?}");
    let (_, untitled) = shape("<code class=\"math-error\" title=\"\">$x$</code>");
    assert_eq!(untitled.len(), 1);
    assert_eq!(
        shape("<code class=\"math-error\" title=\"one\">$x$</code>").0,
        shape("<code class=\"math-error\" title=\"two\">$x$</code>").0
    );
}
