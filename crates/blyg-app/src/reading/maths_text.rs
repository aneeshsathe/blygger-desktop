//! --- ux 0.28 --- Formulas in cards, as text. The Reader draws a formula
//! (MathML); a card is plain text, where `$e^{i\pi}$` or `\int_0^1` read as
//! markup. Here TeX becomes the text a reader would say: `e^(iπ)`, `∫₀¹`,
//! `1/3`. Stream cards ([`super::stream::post_blocks`]) and the Agentic
//! feed's cards ([`super::agentic::card`]) both use it.
//!
//! It isn't a TeX renderer: commands it doesn't know keep their name
//! without the backslash, and the layout (fractions, roots, scripts) is
//! written on one line.

pub use blyg_core::maths_text::tex_text;

/// Text with formulas in it (`$…$`, `$$…$$`, as `blyg_core::plain_title`
/// finds them; money stays money): each formula as [`tex_text`].
pub fn plain_maths(text: &str) -> String {
    let mut out = String::new();
    let mut at = 0;
    for (a, b, tex) in blyg_render::math::formula_spans(text) {
        out.push_str(&text[at..a]);
        out.push_str(&tex_text(&tex));
        at = b;
    }
    out.push_str(&text[at..]);
    out
}

/// Markdown with each formula held aside ([`hold_maths`]): the plain-text
/// passes (`blyg_core::plain_text`, `plain_title`) run on `md`, and
/// [`Held::back`] puts the formulas back, as text, into what they made.
pub struct Held {
    pub md: String,
    said: Vec<String>,
}

impl Held {
    /// `s` (made from [`Held::md`]) with each formula as text.
    pub fn back(&self, s: &str) -> String {
        let mut s = s.to_string();
        for (n, t) in self.said.iter().enumerate() {
            s = s.replace(&token(n), t);
        }
        s
    }
}

/// Swap each formula in `md` for a token the plain-text passes leave
/// alone, so its marks (`_`, `*`, `^`) aren't read as Markdown.
pub fn hold_maths(md: &str) -> Held {
    let mut said = Vec::new();
    let mut held = String::with_capacity(md.len());
    let mut lines = md.split_inclusive('\n');
    while let Some(line) = lines.next() {
        // A display formula across lines: `$$` alone, then its TeX, then `$$`.
        if line.trim() == "$$" {
            let mut body = String::new();
            let mut closed = false;
            for l in lines.by_ref() {
                if l.trim() == "$$" {
                    closed = true;
                    break;
                }
                body.push_str(l);
            }
            if closed {
                held.push_str(&token(said.len()));
                held.push('\n');
                said.push(tex_text(&body));
            } else {
                held.push_str(line);
                held.push_str(&body);
            }
            continue;
        }
        let mut at = 0;
        for (a, b, tex) in blyg_render::math::formula_spans(line) {
            held.push_str(&line[at..a]);
            held.push_str(&token(said.len()));
            said.push(tex_text(&tex));
            at = b;
        }
        held.push_str(&line[at..]);
    }
    Held { md: held, said }
}

/// A formula's stand-in: private-use characters around its number, which
/// no Markdown rule touches.
fn token(n: usize) -> String {
    format!("\u{E000}{n}\u{E001}")
}
