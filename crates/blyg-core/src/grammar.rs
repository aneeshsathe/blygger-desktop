//! The bracket grammar (protocol 0.4 §10.1), defined once for every crate
//! but the renderer: studio 0.47's `grammar.ts`.
//!
//! Two forms, one `!` apart: an own-line directive `![[id]]` that
//! transcludes, and an inline link `[[id]]` that does not. Either may carry
//! a **label** after the id (decision #70): `[[id | text]]`, `![[id | text]]`.
//! The id is positional (the 26 characters right after the brackets), so the
//! first pipe after it separates, and everything from there to the first
//! `]]` on the line is the label, pipes included. Whitespace before the pipe
//! is spaces or tabs; the label is trimmed, and an empty one is no label.
//! There is no escape, so a label can't contain `]]`. Resolution ignores
//! the label: on a link it is the anchor text, on a directive a note for the
//! author that never reaches the bake.
//!
//! Hand-written rather than regex (blyg-core has no regex dependency), but
//! each scanner matches upstream's pattern exactly:
//! - `BODY` = `([ID]{26})(?:[ \t]*\|((?:(?!\]\])[^\n])*))?\]\]`
//! - [`links`] = `(?<!!)\[\[BODY`, [`source_refs`] = `!\[\[BODY`,
//!   [`refs`] = `!?\[\[BODY`, [`directive_line`] = `^\s*!\[\[BODY\s*$`.

use std::ops::Range;

/// The Worker's id alphabet (Crockford-ish base32, lowercase).
pub const ID_ALPHABET: &str = "0123456789abcdefghjkmnpqrstvwxyz";
/// An item id's length.
pub const ID_LEN: usize = 26;

/// Whether `s` is exactly one item id.
pub fn is_id(s: &str) -> bool {
    s.len() == ID_LEN && s.bytes().all(|b| ID_ALPHABET.as_bytes().contains(&b))
}

/// JavaScript's WhiteSpace + LineTerminator set (`\s`, `String#trim`).
pub fn is_js_ws(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// One bracket reference found in some text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ref<'a> {
    /// The whole match, `!` (if any) through `]]`, as byte offsets.
    pub range: Range<usize>,
    /// The id's byte offsets.
    pub id_range: Range<usize>,
    pub id: &'a str,
    /// The label, trimmed; `None` when absent or empty (see [`label`]).
    pub label: Option<&'a str>,
    /// The trimmed label's byte offsets, with `label`.
    pub label_range: Option<Range<usize>>,
    /// `![[…]]` rather than `[[…]]`.
    pub bang: bool,
}

/// The part after `[[`: the id, then `]]` or an optional label and `]]`.
/// Returns the bytes consumed (through `]]`) and the raw label.
fn body(s: &str) -> Option<(usize, Option<&str>)> {
    let id = s.get(..ID_LEN)?;
    if !is_id(id) {
        return None;
    }
    let rest = &s[ID_LEN..];
    if rest.starts_with("]]") {
        return Some((ID_LEN + 2, None));
    }
    let t = rest.trim_start_matches([' ', '\t']).strip_prefix('|')?;
    let line = &t[..t.find('\n').unwrap_or(t.len())];
    let close = line.find("]]")?;
    let used = s.len() - t.len() + close + 2;
    Some((used, Some(&t[..close])))
}

/// A reference starting exactly at byte `at` (`[[` or `![[`), no context
/// checked (in particular not the `!` before a `[[`).
pub fn parse_at(text: &str, at: usize) -> Option<Ref<'_>> {
    let s = text.get(at..)?;
    let (bang, open) = if s.starts_with("![[") {
        (true, 3)
    } else if s.starts_with("[[") {
        (false, 2)
    } else {
        return None;
    };
    let (used, raw) = body(&s[open..])?;
    let id_start = at + open;
    let label = label(raw);
    let label_range = label.map(|l| {
        let from = l.as_ptr() as usize - text.as_ptr() as usize;
        from..from + l.len()
    });
    Some(Ref {
        range: at..id_start + used,
        id_range: id_start..id_start + ID_LEN,
        id: &text[id_start..id_start + ID_LEN],
        label,
        label_range,
        bang,
    })
}

/// Scan left to right like a global regex: at each position try `try_at`,
/// and after a match continue from its end.
fn scan<'a>(text: &'a str, mut try_at: impl FnMut(usize) -> Option<Ref<'a>>) -> Vec<Ref<'a>> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(off) = text[i..].find('[') {
        // A `!` just before the `[[` is part of the candidate.
        let at = i + off;
        let start = if at > i && text.as_bytes()[at - 1] == b'!' {
            at - 1
        } else {
            at
        };
        if let Some(r) = try_at(start).or_else(|| (start != at).then(|| try_at(at)).flatten()) {
            i = r.range.end;
            out.push(r);
        } else {
            i = at + 1;
        }
    }
    out
}

/// Every inline link, `[[id]]` / `[[id | label]]` not preceded by `!`
/// (`LINK_INLINE`).
pub fn links(text: &str) -> Vec<Ref<'_>> {
    scan(text, |at| {
        (!text[..at].ends_with('!'))
            .then(|| parse_at(text, at))
            .flatten()
            .filter(|r| !r.bang)
    })
}

/// Every `![[id]]` / `![[id | label]]` anywhere (a TK instruction's source
/// refs, `directiveInline`).
pub fn source_refs(text: &str) -> Vec<Ref<'_>> {
    scan(text, |at| parse_at(text, at).filter(|r| r.bang))
}

/// Every reference of either form, anywhere (`!?\[\[BODY`).
pub fn refs(text: &str) -> Vec<Ref<'_>> {
    scan(text, |at| parse_at(text, at))
}

/// A whole line that is a transclusion directive (`DIRECTIVE_LINE`):
/// JavaScript whitespace around it is allowed, nothing else.
pub fn directive_line(line: &str) -> Option<Ref<'_>> {
    let at = line.len() - line.trim_start_matches(is_js_ws).len();
    let r = parse_at(line, at).filter(|r| r.bang)?;
    line[r.range.end..].chars().all(is_js_ws).then_some(r)
}

/// The ids of every `![[id]]` directive line in `md`, in order.
pub fn directive_ids(md: &str) -> Vec<&str> {
    md.split('\n')
        .filter_map(directive_line)
        .map(|r| r.id)
        .collect()
}

/// `text` with every labelled inline link replaced by its label (the text
/// a reader sees, upstream's `plain-text.ts`); unlabelled links and
/// directives are left as written.
pub fn with_link_labels(text: &str) -> String {
    replace_links(text, |r| r.label.map(str::to_string))
}

/// `text` with each inline link that `f` maps to `Some` replaced by that
/// text; the rest is left as written.
pub fn replace_links(text: &str, f: impl Fn(&Ref) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for r in links(text) {
        if let Some(s) = f(&r) {
            out.push_str(&text[last..r.range.start]);
            out.push_str(&s);
            last = r.range.end;
        }
    }
    out.push_str(&text[last..]);
    out
}

/// The label a match carries, trimmed; `None` when absent or empty.
pub fn label(raw: Option<&str>) -> Option<&str> {
    raw.map(|s| s.trim_matches(is_js_ws))
        .filter(|s| !s.is_empty())
}

/// Text made safe to write as a label: one line, no `]]` (the grammar has
/// no escape, so §10.1 says a label that would contain one is rewritten).
pub fn label_text(text: &str) -> String {
    let one_line = text
        .split(is_js_ws)
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>();
    // `\s+` → " " then trim: runs of whitespace become one space.
    one_line
        .join(" ")
        .replace("]]", "] ]")
        .trim_matches(is_js_ws)
        .to_string()
}

/// `[[id]]`, or `[[id | label]]` with a label (`linkSource`).
pub fn link_source(id: &str, text: Option<&str>) -> String {
    let l = text.map(label_text).unwrap_or_default();
    if l.is_empty() {
        format!("[[{id}]]")
    } else {
        format!("[[{id} | {l}]]")
    }
}

/// `![[id]]`, or `![[id | label]]` with a label (what upstream's picker
/// writes for a quote or a source).
pub fn directive_source(id: &str, text: Option<&str>) -> String {
    format!("!{}", link_source(id, text))
}

/// Generated text with the bracket grammar taken out (`stripGeneratedGrammar`):
/// generated output is not grammar, so a model that echoes `[[id]]` or an
/// inline `![[id]]` would publish literal brackets. Each inline form becomes
/// its label, or nothing. A directive alone on its own line stays: that is a
/// real quote, which publish warns about when the instruction didn't ask.
pub fn strip_generated_grammar(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            if directive_line(line).is_some() {
                return line.to_string();
            }
            let found = refs(line);
            if found.is_empty() {
                return line.to_string();
            }
            let mut out = String::with_capacity(line.len());
            let mut last = 0;
            for r in &found {
                out.push_str(&line[last..r.range.start]);
                out.push_str(r.label.unwrap_or(""));
                last = r.range.end;
            }
            out.push_str(&line[last..]);
            if out == line {
                return out;
            }
            tidy(&out)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `.replace(/[ \t]{2,}/g, " ").replace(/ +([.,;:!?)])/g, "$1")
/// .replace(/\( +/g, "(").trimEnd()`.
fn tidy(s: &str) -> String {
    let mut a = String::with_capacity(s.len());
    let mut run = 0;
    for c in s.chars() {
        if c == ' ' || c == '\t' {
            run += 1;
            if run == 1 {
                a.push(c);
            } else if run == 2 {
                a.pop();
                a.push(' ');
            }
        } else {
            run = 0;
            a.push(c);
        }
    }
    let mut b = String::with_capacity(a.len());
    let mut spaces = 0;
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

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "7c9wk2n4h6q1x8v0z3m5rjy2ke";

    fn link(text: &str) -> Vec<(&str, Option<&str>)> {
        links(text).into_iter().map(|r| (r.id, r.label)).collect()
    }

    // test/labels.test.ts "the grammar".
    #[test]
    fn reads_links_and_their_labels() {
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
            assert_eq!(link(&text), vec![(ID, expected)], "{text}");
        }
    }

    #[test]
    fn a_label_ends_at_the_first_close() {
        assert_eq!(
            link(&format!("[[{ID} | cut]] here]]")),
            vec![(ID, Some("cut"))]
        );
        let t = format!("see [[{ID} | one]] and [[{ID}]] too");
        assert_eq!(link(&t), vec![(ID, Some("one")), (ID, None)]);
        let r = &links(&t)[0];
        assert_eq!(&t[r.range.clone()], format!("[[{ID} | one]]"));
        assert_eq!(&t[r.id_range.clone()], ID);
        assert_eq!(&t[r.label_range.clone().unwrap()], "one");
        assert_eq!(links(&t)[1].label_range, None);
    }

    #[test]
    fn reads_nothing_that_is_not_an_id_first() {
        assert!(link(&format!("[[a label | {ID}]]")).is_empty());
        assert!(link(&format!("[[{} | short id]]", &ID[1..])).is_empty());
        assert!(link(&format!("[[{ID}x]]")).is_empty());
        assert!(link("[[just words]]").is_empty());
        assert!(link(&format!("[[{ID} ]]")).is_empty(), "space, no pipe");
        assert!(
            link(&format!("[[{ID} | no close\n]]")).is_empty(),
            "one line"
        );
        assert!(link(&format!("[[{}]]", ID.to_uppercase())).is_empty());
    }

    #[test]
    fn a_directive_is_never_a_link() {
        assert!(link(&format!("![[{ID} | a quote]]")).is_empty());
        assert!(link(&format!("![[{ID}]]")).is_empty());
        // `[[[id]]]`: the inner `[[id]]` is a link, as the regex finds it.
        assert_eq!(link(&format!("[[[{ID}]]]")), vec![(ID, None)]);
        let t = format!("summarise ![[{ID} | their post]] and [[{ID}]]");
        let s = source_refs(&t);
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].id, s[0].label), (ID, Some("their post")));
        assert_eq!(refs(&format!("![[{ID}]] [[{ID}]]")).len(), 2);
    }

    #[test]
    fn reads_a_labelled_directive_line_and_only_a_whole_line() {
        let t = format!("![[{ID} | the source]]");
        let d = directive_line(&t).unwrap();
        assert_eq!((d.id, d.label), (ID, Some("the source")));
        let t = format!("  ![[{ID}]]  ");
        let d = directive_line(&t).unwrap();
        assert_eq!((d.id, d.label), (ID, None));
        assert!(directive_line(&format!("\u{A0}![[{ID}]]\r")).is_some());
        assert!(directive_line(&format!("text ![[{ID} | inline]]")).is_none());
        assert!(directive_line(&format!("![[{ID}]] after")).is_none());
        assert!(directive_line(&format!("[[{ID}]]")).is_none());
        assert_eq!(
            directive_ids(&format!("a\n![[{ID} | x]]\nb ![[{ID}]]")),
            vec![ID]
        );
    }

    #[test]
    fn links_read_as_their_labels() {
        assert_eq!(
            with_link_labels(&format!("a [[{ID} | b]] [[{ID}]] ![[{ID} | c]]")),
            format!("a b [[{ID}]] ![[{ID} | c]]")
        );
    }

    #[test]
    fn writes_the_long_form() {
        assert_eq!(link_source(ID, None), format!("[[{ID}]]"));
        assert_eq!(
            link_source(ID, Some("A title")),
            format!("[[{ID} | A title]]")
        );
        assert_eq!(link_source(ID, Some("  ")), format!("[[{ID}]]"));
        assert_eq!(directive_source(ID, Some("a")), format!("![[{ID} | a]]"));
        assert_eq!(label_text("two\nlines"), "two lines");
        assert_eq!(label_text("  a \t b  "), "a b");
        assert_eq!(label_text("ends ]] early"), "ends ] ] early");
        let s = link_source(ID, Some("ends ]] early"));
        assert_eq!(link(&s), vec![(ID, Some("ends ] ] early"))]);
    }

    // test/tk-generate.test.ts "generated text is not grammar".
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
            (
                format!("Kept:  two spaces, [[{ID} | x]]  here ."),
                "Kept: two spaces, x here.".into(),
            ),
            (
                "untouched  (  spacing )".into(),
                "untouched  (  spacing )".into(),
            ),
        ] {
            assert_eq!(strip_generated_grammar(&input), expected, "{input:?}");
        }
    }
}
