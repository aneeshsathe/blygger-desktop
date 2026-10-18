//! The bracket grammar (protocol 0.4 §10.1, studio 0.47's `grammar.ts`)
//! for the crates that don't depend on blyg-render themselves (blyg-ai,
//! the extensions). The grammar lives in blyg-render; this re-exports it
//! and adds the few readings built on it that the rest of the app needs.
//!
//! Two forms, one `!` apart: an own-line directive `![[id]]` that
//! transcludes, and an inline link `[[id]]` that does not. Either may carry
//! a label after the id (decision #70): `[[id | text]]`, `![[id | text]]`.

use std::ops::Range;

pub use blyg_render::{
    DirectiveRef, LinkRef, directive_ref, label_text, link_refs, link_source,
    strip_generated_grammar,
};

/// `![[id]]`, or `![[id | label]]` with a label: what the picker writes for
/// a quote (studio 0.47's `paletteInsert`).
pub fn directive_source(id: &str, text: Option<&str>) -> String {
    format!("!{}", link_source(id, text))
}

/// Every `![[id]]` / `![[id | label]]` anywhere in `text` (a `[TK]`
/// instruction's source refs, `directiveInline`), ids deduplicated in
/// first-occurrence order.
pub fn source_ids(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut at = 0;
    while let Some(i) = text[at..].find("![[") {
        let p = at + i;
        // The `[[…]]` right after the `!`, read as a link at its start (a
        // reference never spans lines).
        let line_end = text[p..].find('\n').map_or(text.len(), |n| p + n);
        match link_refs(&text[p + 1..line_end]).into_iter().next() {
            Some(r) if r.range.start == 0 => {
                if !out.contains(&r.id) {
                    out.push(r.id);
                }
                at = p + 1 + r.range.end;
            }
            _ => at = p + 1,
        }
    }
    out
}

/// Where the label of link `r` (found in `text`) sits, trimmed as
/// `r.label` is; `None` without a label.
pub fn label_range(text: &str, r: &LinkRef) -> Option<Range<usize>> {
    let label = r.label.as_deref()?;
    // `[[id | label  ]]`: the label ends before trailing whitespace.
    let inner = &text[r.range.start..r.range.end - 2];
    let end = r.range.start + inner.trim_end_matches(is_js_ws).len();
    Some(end - label.len()..end)
}

/// `text` with each inline link that `f` maps to `Some` replaced by that
/// text; the rest is left as written. Code isn't skipped.
pub fn replace_links(text: &str, f: impl Fn(&LinkRef) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for r in link_refs(text) {
        if let Some(s) = f(&r) {
            out.push_str(&text[last..r.range.start]);
            out.push_str(&s);
            last = r.range.end;
        }
    }
    out.push_str(&text[last..]);
    out
}

/// `text` with every labelled link replaced by its label, the text a reader
/// sees (upstream's `plain-text.ts`); unlabelled links and directives are
/// left as written.
pub fn with_link_labels(text: &str) -> String {
    replace_links(text, |r| r.label.clone())
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

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "7c9wk2n4h6q1x8v0z3m5rjy2ke";

    #[test]
    fn source_ids_take_labelled_refs_by_id() {
        assert_eq!(
            source_ids(&format!(
                "summarise ![[{ID} | their post]] and ![[{ID}]], not [[{ID}]] or ![[{ID} x]]"
            )),
            vec![ID]
        );
        let other = "0123456789abcdefghjkmnpqrs";
        assert_eq!(
            source_ids(&format!("![[{other}|a]]![[{ID}]]")),
            vec![other, ID]
        );
        assert!(source_ids("![[short]] ![[").is_empty());
    }

    #[test]
    fn label_ranges_point_at_the_trimmed_label() {
        let t = format!("see [[{ID}   |  one two  ]] and [[{ID}]]");
        let rs = link_refs(&t);
        assert_eq!(&t[label_range(&t, &rs[0]).unwrap()], "one two");
        assert_eq!(label_range(&t, &rs[1]), None);
    }

    #[test]
    fn links_read_as_their_labels() {
        assert_eq!(
            with_link_labels(&format!("a [[{ID} | b]] [[{ID}]] ![[{ID} | c]]")),
            format!("a b [[{ID}]] ![[{ID} | c]]")
        );
        assert_eq!(directive_source(ID, Some("a")), format!("![[{ID} | a]]"));
        assert_eq!(directive_source(ID, Some(" ")), format!("![[{ID}]]"));
    }
}
