//! --- ux 0.28 --- Formulas in cards, as text. The Reader draws a formula
//! (MathML); a card is plain text, where `$e^{i\pi}$` or `\int_0^1` read as
//! markup. Here TeX becomes the text a reader would say: `e^(iπ)`, `∫₀¹`,
//! `1/3`. Stream cards ([`super::stream::post_blocks`]) and the Agentic
//! feed's cards ([`super::agentic::card`]) both use it.
//!
//! It isn't a TeX renderer: commands it doesn't know keep their name
//! without the backslash, and the layout (fractions, roots, scripts) is
//! written on one line.

/// A formula's TeX as one line of readable text.
pub fn tex_text(tex: &str) -> String {
    let chars: Vec<char> = tex.chars().collect();
    let mut i = 0;
    let out = run(&chars, &mut i, false);
    collapse(&out)
}

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

/// Spaces collapsed, ends trimmed.
fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Read until the end, or a `}` closing the group we're in (`group`).
fn run(c: &[char], i: &mut usize, group: bool) -> String {
    let mut out = String::new();
    while *i < c.len() {
        let ch = c[*i];
        match ch {
            '}' if group => {
                *i += 1;
                return out;
            }
            '{' => {
                *i += 1;
                out.push_str(&run(c, i, true));
            }
            '}' => *i += 1,
            '^' | '_' => {
                *i += 1;
                let arg = arg(c, i);
                out.push_str(&script(&arg, ch == '^'));
            }
            '\\' => {
                *i += 1;
                out.push_str(&command(c, i));
            }
            '~' => {
                *i += 1;
                out.push(' ');
            }
            _ => {
                *i += 1;
                out.push(ch);
            }
        }
    }
    out
}

/// One argument: a `{group}`, a command, or one character.
fn arg(c: &[char], i: &mut usize) -> String {
    while *i < c.len() && c[*i] == ' ' {
        *i += 1;
    }
    match c.get(*i) {
        Some('{') => {
            *i += 1;
            collapse(&run(c, i, true))
        }
        Some('\\') => {
            *i += 1;
            command(c, i)
        }
        Some(&ch) => {
            *i += 1;
            ch.to_string()
        }
        None => String::new(),
    }
}

/// After a backslash: the command's text.
fn command(c: &[char], i: &mut usize) -> String {
    let start = *i;
    while *i < c.len() && c[*i].is_ascii_alphabetic() {
        *i += 1;
    }
    if *i == start {
        // `\,` `\;` `\!` `\ ` spacing, `\\` a line break, `\{` a brace.
        let Some(&ch) = c.get(*i) else {
            return String::new();
        };
        *i += 1;
        return match ch {
            ',' | ';' | ':' | ' ' | '\\' => " ".into(),
            '!' => String::new(),
            other => other.to_string(),
        };
    }
    let name: String = c[start..*i].iter().collect();
    match name.as_str() {
        "frac" | "dfrac" | "tfrac" => {
            let (a, b) = (arg(c, i), arg(c, i));
            format!("{}/{}", wrapped(&a), wrapped(&b))
        }
        "sqrt" => format!("√{}", wrapped(&arg(c, i))),
        "text" | "textrm" | "textit" | "textbf" | "mathrm" | "mathbf" | "mathit" | "mathsf"
        | "mathtt" | "mathcal" | "mathbb" | "operatorname" | "boldsymbol" | "vec" | "hat"
        | "bar" | "overline" | "tilde" | "dot" => arg(c, i),
        "left" | "right" | "big" | "Big" | "bigg" | "Bigg" | "displaystyle" | "limits" => {
            String::new()
        }
        "quad" | "qquad" => " ".into(),
        n => symbol(n).map_or_else(|| n.to_string(), str::to_string),
    }
}

/// A fraction's or root's part: bare when it's one term, else in brackets.
fn wrapped(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() <= 1
        || s.chars()
            .all(|c| c.is_alphanumeric() || "²³¹⁰⁴⁵⁶⁷⁸⁹".contains(c))
    {
        s.to_string()
    } else {
        format!("({s})")
    }
}

/// A raised or lowered part: Unicode's marks when every character has one,
/// else `^x` / `^(…)`.
fn script(s: &str, up: bool) -> String {
    let s = s.trim();
    let mapped: Option<String> = s
        .chars()
        .map(|c| if up { sup(c) } else { sub(c) })
        .collect();
    match mapped {
        Some(m) if !m.is_empty() => m,
        _ => {
            let mark = if up { '^' } else { '_' };
            if s.chars().count() <= 1 {
                format!("{mark}{s}")
            } else {
                format!("{mark}({s})")
            }
        }
    }
}

fn sup(c: char) -> Option<char> {
    Some(match c {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        '+' => '⁺',
        '-' => '⁻',
        '=' => '⁼',
        '(' => '⁽',
        ')' => '⁾',
        'n' => 'ⁿ',
        'i' => 'ⁱ',
        'x' => 'ˣ',
        'y' => 'ʸ',
        'k' => 'ᵏ',
        'm' => 'ᵐ',
        't' => 'ᵗ',
        'T' => 'ᵀ',
        '′' => '′',
        _ => return None,
    })
}

fn sub(c: char) -> Option<char> {
    Some(match c {
        '0' => '₀',
        '1' => '₁',
        '2' => '₂',
        '3' => '₃',
        '4' => '₄',
        '5' => '₅',
        '6' => '₆',
        '7' => '₇',
        '8' => '₈',
        '9' => '₉',
        '+' => '₊',
        '-' => '₋',
        '=' => '₌',
        '(' => '₍',
        ')' => '₎',
        'a' => 'ₐ',
        'e' => 'ₑ',
        'i' => 'ᵢ',
        'j' => 'ⱼ',
        'k' => 'ₖ',
        'n' => 'ₙ',
        'm' => 'ₘ',
        'x' => 'ₓ',
        _ => return None,
    })
}

/// A command's symbol.
fn symbol(name: &str) -> Option<&'static str> {
    Some(match name {
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "epsilon" | "varepsilon" => "ε",
        "zeta" => "ζ",
        "eta" => "η",
        "theta" | "vartheta" => "θ",
        "iota" => "ι",
        "kappa" => "κ",
        "lambda" => "λ",
        "mu" => "μ",
        "nu" => "ν",
        "xi" => "ξ",
        "pi" => "π",
        "rho" => "ρ",
        "sigma" => "σ",
        "tau" => "τ",
        "upsilon" => "υ",
        "phi" | "varphi" => "φ",
        "chi" => "χ",
        "psi" => "ψ",
        "omega" => "ω",
        "Gamma" => "Γ",
        "Delta" => "Δ",
        "Theta" => "Θ",
        "Lambda" => "Λ",
        "Xi" => "Ξ",
        "Pi" => "Π",
        "Sigma" => "Σ",
        "Phi" => "Φ",
        "Psi" => "Ψ",
        "Omega" => "Ω",
        "int" => "∫",
        "iint" => "∬",
        "oint" => "∮",
        "sum" => "∑",
        "prod" => "∏",
        "infty" => "∞",
        "partial" => "∂",
        "nabla" => "∇",
        "cdot" => "·",
        "times" => "×",
        "div" => "÷",
        "pm" => "±",
        "mp" => "∓",
        "le" | "leq" => "≤",
        "ge" | "geq" => "≥",
        "ne" | "neq" => "≠",
        "approx" => "≈",
        "equiv" => "≡",
        "sim" => "∼",
        "propto" => "∝",
        "to" | "rightarrow" => "→",
        "leftarrow" => "←",
        "Rightarrow" | "implies" => "⇒",
        "Leftrightarrow" | "iff" => "⇔",
        "mapsto" => "↦",
        "in" => "∈",
        "notin" => "∉",
        "subset" => "⊂",
        "subseteq" => "⊆",
        "cup" => "∪",
        "cap" => "∩",
        "forall" => "∀",
        "exists" => "∃",
        "emptyset" | "varnothing" => "∅",
        "ldots" | "dots" | "cdots" => "…",
        "circ" => "∘",
        "prime" => "′",
        "hbar" => "ℏ",
        "ell" => "ℓ",
        "langle" => "⟨",
        "rangle" => "⟩",
        "neg" | "lnot" => "¬",
        "land" | "wedge" => "∧",
        "lor" | "vee" => "∨",
        _ => return None,
    })
}
