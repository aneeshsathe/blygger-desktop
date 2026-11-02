//! Labelled links, `[[id | label]]`, and writing them bare.
//!
//! Burrow's `[[` picker writes labelled links. Every studio Burrow syncs
//! with (0.49.12 or later) understands them; these helpers remain for the
//! "your blyg needs updating" path, where a post holding them offers to
//! write them bare ([`strip_labels`]) and the picker writes the bare form
//! ([`picker_label`]).

/// One labelled reference outside code: the bytes it spans (the `!`
/// included for a directive), and the bare form it rewrites to.
fn labelled(text: &str) -> Vec<(std::ops::Range<usize>, String)> {
    use crate::grammar::link_refs;
    let code = blyg_render::code_ranges(text);
    let in_code = |at: usize| code.iter().any(|&(s, e)| s <= at && at < e);
    let mut out: Vec<(std::ops::Range<usize>, String)> = link_refs(text)
        .into_iter()
        .filter(|r| r.label.is_some() && !in_code(r.range.start))
        .map(|r| (r.range, format!("[[{}]]", r.id)))
        .collect();
    // `![[id | label]]`, own-line or inline (a `[TK]` instruction's source):
    // the `[[…]]` right after the `!`, read as a link at its start.
    let mut at = 0;
    while let Some(i) = text[at..].find("![[") {
        let p = at + i;
        let line_end = text[p..].find('\n').map_or(text.len(), |n| p + n);
        match link_refs(&text[p + 1..line_end]).into_iter().next() {
            Some(r) if r.range.start == 0 => {
                if r.label.is_some() && !in_code(p) {
                    out.push((p..p + 1 + r.range.end, format!("![[{}]]", r.id)));
                }
                at = p + 1 + r.range.end;
            }
            _ => at = p + 1,
        }
    }
    out.sort_by_key(|(r, _)| r.start);
    out
}

/// Whether `text` holds a labelled link or directive, `[[id | label]]` or
/// `![[id | label]]`, outside code and with a label that isn't empty: what
/// a studio older than 0.47 can't show.
pub fn has_labels(text: &str) -> bool {
    !labelled(text).is_empty()
}

/// `text` with every labelled link and directive outside code written bare,
/// `[[id]]` / `![[id]]`, which every studio understands. Code is untouched.
pub fn strip_labels(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (r, bare) in labelled(text) {
        out.push_str(&text[last..r.start]);
        out.push_str(&bare);
        last = r.end;
    }
    out.push_str(&text[last..]);
    out
}

/// The label a picker writes: none on a blyg below the floor (`below`), so a
/// new link goes in bare and nothing waits on it.
pub fn picker_label(below: bool, label: &str) -> Option<&str> {
    (!below).then_some(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "7c9wk2n4h6q1x8v0z3m5rjy2ke";

    #[test]
    fn labelled_links_and_directives_count() {
        assert!(has_labels(&format!("see [[{ID} | that post]] now")));
        assert!(has_labels(&format!("see [[{ID}|x]]")));
        assert!(has_labels(&format!("intro\n\n![[{ID} | their post]]\n")));
        assert!(has_labels(&format!(
            "[TK] summarise ![[{ID} | it]] [=] out [/TK]"
        )));
        // Generated output is text like any other.
        assert!(has_labels(&format!(
            "[TK] link it [=] as [[{ID} | they said]] [/TK]"
        )));
    }

    #[test]
    fn bare_empty_and_code_dont_count() {
        assert!(!has_labels(&format!("see [[{ID}]] and\n![[{ID}]]\n")));
        // An empty label is no label.
        assert!(!has_labels(&format!("see [[{ID} | ]] and [[{ID} |   ]]")));
        assert!(!has_labels(&format!("![[{ID} |]]")));
        // Not an id: not a link at all.
        assert!(!has_labels("see [[short | label]]"));
        // Inline code, fenced code, indented code.
        assert!(!has_labels(&format!("`[[{ID} | x]]`")));
        assert!(!has_labels(&format!(
            "```\n![[{ID} | x]]\n[[{ID} | y]]\n```\n"
        )));
        assert!(!has_labels(&format!("text\n\n    [[{ID} | x]]\n")));
        assert!(has_labels(&format!("`code` then [[{ID} | x]]")));
    }

    #[test]
    fn strip_writes_them_bare_and_leaves_code() {
        let t = format!("a [[{ID} | one]] b [[{ID}]] `[[{ID} | code]]`\n![[{ID} | quote]]\n");
        let bare = strip_labels(&t);
        assert_eq!(
            bare,
            format!("a [[{ID}]] b [[{ID}]] `[[{ID} | code]]`\n![[{ID}]]\n")
        );
        assert!(!has_labels(&bare));
        assert_eq!(strip_labels("nothing here"), "nothing here");
    }

    #[test]
    fn the_picker_writes_bare_below_the_floor() {
        assert_eq!(picker_label(true, "Hello"), None);
        assert_eq!(picker_label(false, "Hello"), Some("Hello"));
    }
}
