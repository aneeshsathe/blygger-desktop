//! A formula's TeX as the text a reader would say: `e^(iπ)`, `∫₀¹`,
//! `1/3` (ux 0.28, moved here in ux 0.29 so the picker's rows say it too).
//! Plain text shows a formula this way wherever it is read, not written:
//! cards, list rows, pickers. Labels written into posts keep the TeX
//! (`model::plain_title`), as the studio writes them.
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
