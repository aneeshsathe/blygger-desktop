//! The bracket grammar, defined once (the studio's `grammar.ts`, protocol
//! 0.4 §10.1, studio 0.47, label fixes from 0.48).
//!
//! Two forms, one `!` apart: an own-line directive `![[id]]` that
//! transcludes, and an inline link `[[id]]` that does not. Either may carry
//! a **label** after the id (decision #70): `[[id | text]]`,
//! `![[id | text]]`. The id is positional (the 26 characters right after the
//! brackets), so the first pipe after it separates, and everything from
//! there to the first `]]` is the label, pipes included. Whitespace around
//! the pipe is optional and trimmed; an empty label is no label; there is no
//! escape, so a label cannot contain `]]` and never spans lines. Resolution
//! ignores the label: on a link it is the anchor text, on a directive a note
//! for the author that never reaches the bake.
//!
//! The patterns are hand-scanned rather than regexes: the studio's
//! `(?:(?!\]\])[^\n])*` needs a lookahead, and the scan is simpler than its
//! lookahead-free equivalent. Each function names the pattern it matches.

use std::ops::Range;

use crate::ID_ALPHABET;
use crate::util::{is_js_ws, js_trim};

/// A matched `[[…]]` body: where it ends, its id, and its raw label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Body {
    /// Byte offset just past the closing `]]`.
    pub end: usize,
    pub id: Range<usize>,
    /// The label as written, pipe excluded (`m[2]`), untrimmed.
    pub raw_label: Option<Range<usize>>,
}

/// Which characters count as an id: the blyg alphabet (the protocol), or any
/// ASCII letter or digit (the native block model's looser reading).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ids {
    Strict,
    Loose,
}

impl Ids {
    fn accepts(self, b: u8) -> bool {
        match self {
            Ids::Strict => ID_ALPHABET.as_bytes().contains(&b),
            Ids::Loose => b.is_ascii_alphanumeric(),
        }
    }
}

/// `(ID)(?:[ \t]*\|((?:(?!\]\])[^\n])*))?\]\]` at byte `at` of `text` (just
/// after the opening `[[`).
pub(crate) fn body_at(text: &str, at: usize, ids: Ids) -> Option<Body> {
    let b = text.as_bytes();
    let id_end = at + 26;
    if id_end > b.len() || !b[at..id_end].iter().all(|&c| ids.accepts(c)) {
        return None;
    }
    let rest = &text[id_end..];
    if rest.starts_with("]]") {
        return Some(Body {
            end: id_end + 2,
            id: at..id_end,
            raw_label: None,
        });
    }
    let pad = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    if !rest[pad..].starts_with('|') {
        return None;
    }
    let from = id_end + pad + 1;
    let line = &text[from..text[from..].find('\n').map_or(text.len(), |n| from + n)];
    // The label is greedy up to, and never across, the first `]]`.
    let close = line.find("]]")?;
    Some(Body {
        end: from + close + 2,
        id: at..id_end,
        raw_label: Some(from..from + close),
    })
}

/// `label()`: a raw label trimmed, `None` when absent or empty.
pub fn label(raw: Option<&str>) -> Option<&str> {
    raw.map(js_trim).filter(|t| !t.is_empty())
}

/// One inline `[[id]]` or `[[id | label]]` link in a text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkRef {
    /// Byte range of the whole `[[…]]`, brackets included.
    pub range: Range<usize>,
    pub id: String,
    /// The author's label, trimmed (`label(m[2])`); `None` when absent or empty.
    pub label: Option<String>,
}

/// One own-line `![[id]]` or `![[id | label]]` directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectiveRef {
    pub id: String,
    /// The author's note, trimmed; never rendered.
    pub label: Option<String>,
}

/// `LINK_INLINE`, `/(?<!!)\[\[BODY/g`: every link in `text`, left to right,
/// with matches as `matchAll` finds them. Code is not skipped here.
pub(crate) fn link_bodies(text: &str, ids: Ids) -> Vec<(usize, Body)> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = text[at..].find("[[") {
        let p = at + i;
        if p > 0 && text.as_bytes()[p - 1] == b'!' {
            at = p + 1;
            continue;
        }
        match body_at(text, p + 2, ids) {
            Some(body) => {
                at = body.end;
                out.push((p, body));
            }
            None => at = p + 1,
        }
    }
    out
}

/// `directiveInline()`, `/!\[\[BODY/g`: every `![[id]]` anywhere in `text`
/// (a `[TK]` instruction's source refs).
pub(crate) fn directive_bodies(text: &str) -> Vec<(usize, Body)> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = text[at..].find("![[") {
        let p = at + i;
        match body_at(text, p + 3, Ids::Strict) {
            Some(body) => {
                at = body.end;
                out.push((p, body));
            }
            None => at = p + 1,
        }
    }
    out
}

/// `DIRECTIVE_LINE`, `/^\s*!\[\[BODY\s*$/`: `line` (no `\n`) as a directive.
pub(crate) fn directive_line(line: &str, ids: Ids) -> Option<Body> {
    let start = line.len() - line.trim_start_matches(is_js_ws).len();
    if !line[start..].starts_with("![[") {
        return None;
    }
    let body = body_at(line, start + 3, ids)?;
    line[body.end..].chars().all(is_js_ws).then_some(body)
}

/// The links in `text`, in order, as the studio reads them
/// (`text.matchAll(linkInline())` with `label(m[2])`). Code is not skipped:
/// a caller that cares filters by `range.start` against [`code_ranges`].
///
/// [`code_ranges`]: crate::code_ranges
pub fn link_refs(text: &str) -> Vec<LinkRef> {
    link_bodies(text, Ids::Strict)
        .into_iter()
        .map(|(p, b)| LinkRef {
            range: p..b.end,
            id: text[b.id.clone()].to_string(),
            label: label(b.raw_label.map(|r| &text[r])).map(str::to_string),
        })
        .collect()
}

/// `line` (one line, no `\n`) as an own-line directive (`DIRECTIVE_LINE`).
pub fn directive_ref(line: &str) -> Option<DirectiveRef> {
    let b = directive_line(line, Ids::Strict)?;
    Some(DirectiveRef {
        id: line[b.id.clone()].to_string(),
        label: label(b.raw_label.map(|r| &line[r])).map(str::to_string),
    })
}

/// `labelText()`: text made safe to write as a label: one line (each run of
/// whitespace one space), trimmed, then every `]` followed by another `]`,
/// or by the end (where the closing `]]` will go), takes a space after it
/// (`.replace(/\](?=\]|$)/g, "] ")`, studio 0.48). So `a]]]b` is
/// `a] ] ]b` and `x]` is `x] `: no run of `]` can cut the link short.
pub fn label_text(text: &str) -> String {
    let mut one_line = String::with_capacity(text.len());
    let mut in_ws = false;
    for c in text.chars() {
        if is_js_ws(c) {
            if !in_ws {
                one_line.push(' ');
            }
            in_ws = true;
        } else {
            one_line.push(c);
            in_ws = false;
        }
    }
    let trimmed = js_trim(&one_line);
    let mut out = String::with_capacity(trimmed.len() + 4);
    let mut chars = trimmed.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(c);
        if c == ']' && matches!(chars.peek(), None | Some(']')) {
            out.push(' ');
        }
    }
    out
}

/// `linkSource()`: `[[id]]`, or `[[id | label]]` when `text` leaves a label
/// after [`label_text`]. What the picker and *copy [[id]]* write.
pub fn link_source(id: &str, text: Option<&str>) -> String {
    match text.map(label_text).filter(|l| !l.is_empty()) {
        Some(l) => format!("[[{id} | {l}]]"),
        None => format!("[[{id}]]"),
    }
}

/// `stripGeneratedGrammar()` (studio 0.47, ROADMAP row 67): generated text
/// with the bracket grammar taken out, for a client to apply to a model's
/// output before saving it as a scope's `[=]` text. Each inline `[[id]]` or
/// `![[id]]` becomes its label, or nothing, and the spacing around it is
/// tidied; a directive alone on its own line stays, since decision #60
/// makes that a real quote (which publish warns about).
pub fn strip_generated_grammar(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            if directive_line(line, Ids::Strict).is_some() {
                return line.to_string();
            }
            let stripped = strip_inline(line);
            if stripped == line {
                return stripped;
            }
            tidy(&stripped)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `/!?\[\[BODY/g` replaced by `label(raw) ?? ""`.
fn strip_inline(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut last = 0;
    let mut at = 0;
    while let Some(i) = line[at..].find("[[") {
        let p = at + i;
        match body_at(line, p + 2, Ids::Strict) {
            Some(b) => {
                let start = if p > last && line.as_bytes()[p - 1] == b'!' {
                    p - 1
                } else {
                    p
                };
                out.push_str(&line[last..start]);
                out.push_str(label(b.raw_label.map(|r| &line[r])).unwrap_or(""));
                last = b.end;
                at = b.end;
            }
            None => at = p + 1,
        }
    }
    out.push_str(&line[last..]);
    out
}

/// `.replace(/[ \t]{2,}/g, " ").replace(/ +([.,;:!?)])/g, "$1")
/// .replace(/\( +/g, "(").trimEnd()`.
fn tidy(s: &str) -> String {
    let mut a = String::with_capacity(s.len());
    let mut run = String::new();
    for c in s.chars() {
        if c == ' ' || c == '\t' {
            run.push(c);
            continue;
        }
        flush_run(&mut a, &mut run);
        a.push(c);
    }
    flush_run(&mut a, &mut run);
    let mut b = String::with_capacity(a.len());
    let mut spaces = 0usize;
    for c in a.chars() {
        if c == ' ' {
            spaces += 1;
            continue;
        }
        if !matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | ')') {
            b.extend(std::iter::repeat_n(' ', spaces));
        }
        spaces = 0;
        b.push(c);
    }
    b.extend(std::iter::repeat_n(' ', spaces));
    let mut c = String::with_capacity(b.len());
    let mut after_paren = false;
    for ch in b.chars() {
        if after_paren && ch == ' ' {
            continue;
        }
        after_paren = ch == '(';
        c.push(ch);
    }
    c.trim_end_matches(is_js_ws).to_string()
}

fn flush_run(out: &mut String, run: &mut String) {
    if run.chars().count() >= 2 {
        out.push(' ');
    } else {
        out.push_str(run);
    }
    run.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "7c9wk2n4h6q1x8v0z3m5rjy2ke";

    fn links(text: &str) -> Vec<(String, Option<String>)> {
        link_refs(text)
            .into_iter()
            .map(|l| (l.id, l.label))
            .collect()
    }

    fn one(label: Option<&str>) -> Vec<(String, Option<String>)> {
        vec![(ID.to_string(), label.map(str::to_string))]
    }

    // labels.test.ts, "the grammar".
    #[test]
    fn reads_labels() {
        for (text, expected) in [
            (format!("[[{ID}]]"), None),
            (
                format!("[[{ID} | the opening fragment]]"),
                Some("the opening fragment"),
            ),
            (format!("[[{ID}|tight]]"), Some("tight")),
            (format!("[[{ID}   |   padded   ]]"), Some("padded")),
            (format!("[[{ID} | either | or]]"), Some("either | or")),
            (
                format!("[[{ID} | a ] single bracket]]"),
                Some("a ] single bracket"),
            ),
            (
                format!("[[{ID} | [nested] brackets]]"),
                Some("[nested] brackets"),
            ),
            (format!("[[{ID} |]]"), None),
            (format!("[[{ID} |   ]]"), None),
        ] {
            assert_eq!(links(&text), one(expected), "{text}");
        }
        assert_eq!(links(&format!("[[{ID} | cut]] here]]")), one(Some("cut")));
        assert_eq!(
            links(&format!("see [[{ID} | one]] and [[{ID}]] too")),
            vec![(ID.to_string(), Some("one".into())), (ID.to_string(), None)]
        );
    }

    #[test]
    fn reads_nothing_that_is_not_an_id_first() {
        assert!(links(&format!("[[a label | {ID}]]")).is_empty());
        assert!(links(&format!("[[{} | short id]]", &ID[1..])).is_empty());
        assert!(links(&format!("[[{ID}x]]")).is_empty());
        assert!(links("[[just words]]").is_empty());
        assert!(links(&format!("![[{ID} | a quote]]")).is_empty());
        assert!(links(&format!("[[{ID} | no close\n]]")).is_empty());
        assert!(links(&format!("[[{ID} ]]")).is_empty());
    }

    #[test]
    fn reads_a_labelled_directive_line_only_whole() {
        assert_eq!(
            directive_ref(&format!("![[{ID} | the source]]")),
            Some(DirectiveRef {
                id: ID.into(),
                label: Some("the source".into())
            })
        );
        assert_eq!(
            directive_ref(&format!("  ![[{ID}]]  ")),
            Some(DirectiveRef {
                id: ID.into(),
                label: None
            })
        );
        assert_eq!(directive_ref(&format!("text ![[{ID} | inline]]")), None);
        assert_eq!(directive_ref(&format!("![[{ID} | a]] b]]")), None);
    }

    #[test]
    fn writes_the_long_form() {
        assert_eq!(link_source(ID, None), format!("[[{ID}]]"));
        assert_eq!(
            link_source(ID, Some("A title")),
            format!("[[{ID} | A title]]")
        );
        assert_eq!(link_source(ID, Some("  ")), format!("[[{ID}]]"));
        assert_eq!(label_text("two\nlines"), "two lines");
        assert_eq!(label_text("ends ]] early"), "ends ] ] early");
        assert_eq!(label_text("a]]]b"), "a] ] ]b");
        assert_eq!(label_text("x]]]"), "x] ] ] ");
        assert_eq!(
            links(&link_source(ID, Some("ends ]] early"))),
            one(Some("ends ] ] early"))
        );
        // label-grammar-edge-cases.test.ts: runs of `]` round-trip whole.
        for text in ["a]]]b", "a]]]]b", "x]]]"] {
            let src = link_source(ID, Some(text));
            let refs = link_refs(&src);
            assert_eq!(refs.len(), 1, "{src}");
            assert_eq!(refs[0].range, 0..src.len(), "{src}");
            assert!(!refs[0].label.as_deref().unwrap().contains("]]"), "{src}");
        }
    }

    // tk-generate.test.ts, "generated text is not grammar".
    #[test]
    fn strips_generated_grammar() {
        for (input, expected) in [
            (
                format!("It is worth reading in full ![[{ID}]]"),
                "It is worth reading in full".to_string(),
            ),
            (
                format!("As argued in [[{ID}]], the order matters."),
                "As argued in, the order matters.".into(),
            ),
            (
                format!("As argued in [[{ID} | the earlier post]], yes."),
                "As argued in the earlier post, yes.".into(),
            ),
            (
                format!("see (![[{ID} | that]]) here"),
                "see (that) here".into(),
            ),
            (
                "No grammar here at all.".into(),
                "No grammar here at all.".into(),
            ),
            (
                format!("Above.\n\n![[{ID}]]\n\nBelow."),
                format!("Above.\n\n![[{ID}]]\n\nBelow."),
            ),
        ] {
            assert_eq!(strip_generated_grammar(&input), expected, "{input:?}");
        }
    }
}
