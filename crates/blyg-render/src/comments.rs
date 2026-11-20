//! Comments to self: `/* … */` in a working copy (the studio's
//! `comments.ts`, studio 0.60). Studio-private grammar, like TK: the working
//! copy keeps a comment, and the server's publish removes it before anything
//! is hashed, rendered or stored, so the wire never sees one. The preview
//! leaves them out the same way (`previewComments`), and says what would
//! stop a publish (an unclosed comment, a stray `*/`, a `/*` attached to a
//! word) where the author is looking, as a `⚠` paragraph at the end.
//!
//! A comment opens at a line's start or after whitespace, never as `/**`,
//! never in code and never escaped (`\/*`). It closes at the first `*/`
//! outside code, outside a whole TK scope and not escaped. Removal tidies
//! the whitespace around it ([`close_up`]). Offsets are byte offsets where
//! the TypeScript has UTF-16 indices.

use crate::linemap::Mapped;
use crate::markdown::in_ranges;
use crate::util::{is_js_ws, js_trim};

/// A reason the working copy can't publish, at a byte offset of the text.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Issue {
    at: usize,
    reason: String,
}

/// `parseComments`, with each output byte's source offset (`None`: a
/// newline [`close_up`] put back).
struct Parsed {
    text: String,
    src: Vec<Option<usize>>,
    ranges: Vec<(usize, usize)>,
    errors: Vec<Issue>,
}

/// `codeRanges`, skipping the block parse when `text` can hold no code: a
/// code span needs a backtick, a fence a backtick or tilde, and an indented
/// block four columns of whitespace (a tab, or four spaces in a row). The
/// parse runs once per comment, so this keeps a long note-heavy draft fast.
fn code_of(text: &str) -> Vec<(usize, usize)> {
    if text.contains(['`', '~', '\t']) || text.contains("    ") {
        crate::markdown::code_ranges(text)
    } else {
        Vec::new()
    }
}

/// A token escaped by an odd run of backslashes (`escaped`).
fn escaped(text: &str, at: usize) -> bool {
    text.as_bytes()[..at]
        .iter()
        .rev()
        .take_while(|&&b| b == b'\\')
        .count()
        % 2
        == 1
}

fn ws_before(text: &str, at: usize) -> bool {
    text[..at].chars().next_back().is_some_and(is_js_ws)
}

/// `opens`: where a note would start, at a line's start or after a space,
/// and not as `/**` (so `src/*.ts` and `/**bold**/` are prose).
fn opens(text: &str, at: usize) -> bool {
    (at == 0 || ws_before(text, at)) && text.as_bytes().get(at + 2) != Some(&b'*')
}

/// `findOpener`.
fn find_opener(text: &str, from: usize, code: &[(usize, usize)]) -> Option<usize> {
    let mut at = text[from..].find("/*").map(|i| from + i);
    while let Some(a) = at {
        if opens(text, a) && !in_ranges(code, a) && !escaped(text, a) {
            break;
        }
        at = text[a + 2..].find("/*").map(|i| a + 2 + i);
    }
    at
}

/// `findCloser`: where a comment body ends, or `None`. A closer in code
/// doesn't close, and neither does one inside a whole TK scope, whose
/// generated output can say anything.
pub(crate) fn find_closer(body: &str) -> Option<usize> {
    let code = code_of(body);
    let mut at = 0;
    loop {
        let close = at + body[at..].find("*/")?;
        if let Some(scope) = body[at..].find("[TK]").map(|i| at + i)
            && scope < close
            && !in_ranges(&code, scope)
            && let Some(end) = body[scope..].find("[/TK]")
        {
            at = scope + end + 5;
            continue;
        }
        if !in_ranges(&code, close) && !escaped(body, close) {
            return Some(close);
        }
        at = close + 2;
    }
}

/// `closeUp`: removal leaves what the author would have typed without the
/// comment. Returns how much of `before` to keep, whether a newline goes
/// after it, and how much of the start of `after` to drop.
fn close_up(before: &str, after: &str) -> (usize, bool, usize) {
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let line_before = &before[line_start..];
    let newline = after.find('\n');
    let line_after = newline.map_or(after, |n| &after[..n]);
    let starts_line = js_trim(line_before).is_empty();
    let ends_line = js_trim(line_after).is_empty();
    let lead_tabs = |s: &str| s.len() - s.trim_start_matches([' ', '\t']).len();
    let trail_tabs = |s: &str| s.len() - s.trim_end_matches([' ', '\t']).len();

    if starts_line && ends_line {
        // The comment had its lines to itself, so they go.
        let head = &before[..line_start];
        let mut skip = newline.map_or(after.len(), |n| n + 1);
        if head.is_empty() || ends_with_blank_line(head) {
            // `/^(?:[ \t]*\n)+/`
            loop {
                let rest = &after[skip..];
                let pad = lead_tabs(rest);
                if rest[pad..].starts_with('\n') {
                    skip += pad + 1;
                } else {
                    break;
                }
            }
        }
        if skip < after.len() {
            return (head.len(), false, skip);
        }
        let kept = head.trim_end_matches(is_js_ws).len();
        let nl = !js_trim(head).is_empty() && newline.is_some();
        return (kept, nl, skip);
    }
    if starts_line {
        return (before.len(), false, lead_tabs(after));
    }
    // Spaces left at a line's end would be a hard break, or just noise.
    if ends_line {
        return (before.len() - trail_tabs(before), false, lead_tabs(after));
    }
    if before.ends_with([' ', '\t']) {
        let pad = lead_tabs(after);
        if after[pad..].starts_with(['.', ',', ';', ':', '!', '?', ')', ']']) {
            return (before.len() - trail_tabs(before), false, pad);
        }
        return (before.len(), false, pad);
    }
    (before.len(), false, 0)
}

/// `/\n[ \t]*\n$/`
fn ends_with_blank_line(s: &str) -> bool {
    let Some(s) = s.strip_suffix('\n') else {
        return false;
    };
    s.trim_end_matches([' ', '\t']).ends_with('\n')
}

/// `lineOf`: the 1-based line of `at`.
fn line_of(text: &str, at: usize) -> usize {
    text[..at].matches('\n').count() + 1
}

/// The start of the last `n` UTF-16 units before `at`.
fn back_units(text: &str, at: usize, n: usize) -> usize {
    let mut units = 0;
    let mut start = at;
    for (i, c) in text[..at].char_indices().rev() {
        units += c.len_utf16();
        if units > n {
            break;
        }
        start = i;
    }
    start
}

/// `strayClosers`: a `*/` still in the text once comments are gone, at a
/// line's start or after whitespace.
fn stray_closers(text: &str) -> Vec<Issue> {
    let code = code_of(text);
    let mut issues = Vec::new();
    for (at, _) in text.match_indices("*/") {
        let start = at == 0 || ws_before(text, at);
        if !start || in_ranges(&code, at) || escaped(text, at) {
            continue;
        }
        let near = js_trim(&text[back_units(text, at, 30)..at])
            .rsplit('\n')
            .next()
            .unwrap_or("");
        let near = if near.is_empty() {
            String::new()
        } else {
            format!(", after \"{near}\"")
        };
        issues.push(Issue {
            at,
            reason: format!(
                "a */ with no comment to close{near}. Comments don't nest: this text would publish. Write \\*/ if you meant it literally"
            ),
        });
    }
    issues
}

/// `attachedOpeners`: a `/*` attached to a word with a `*/` after it in
/// the same paragraph reads as a note written tight, which would publish.
fn attached_openers(text: &str) -> Vec<Issue> {
    let code = code_of(text);
    let mut issues = Vec::new();
    for (at, _) in text.match_indices("/*") {
        if opens(text, at)
            || text.as_bytes().get(at + 2) == Some(&b'*')
            || in_ranges(&code, at)
            || escaped(text, at)
        {
            continue;
        }
        let paragraph_end = blank_line_at(&text[at..]).map_or(text.len(), |b| at + b);
        let paragraph = &text[(at + 2).min(paragraph_end)..paragraph_end];
        let Some(close) = paragraph.find("*/") else {
            continue;
        };
        if in_ranges(&code, at + 2 + close) {
            continue;
        }
        let word = text[..at].rsplit(is_js_ws).next().unwrap_or("");
        issues.push(Issue {
            at,
            reason: format!(
                "a /* attached to \"{word}\" with a */ after it. A comment opens only at a line's start or after a space, so this would publish. Put a space before /*, or write \\/* if you meant it literally"
            ),
        });
    }
    issues
}

/// `/\n[ \t]*\n/`'s first match in `s`.
fn blank_line_at(s: &str) -> Option<usize> {
    s.match_indices('\n')
        .map(|(i, _)| i)
        .find(|&i| s[i + 1..].trim_start_matches([' ', '\t']).starts_with('\n'))
}

/// `parseComments`: every comment in a working copy, the text without
/// them, and what would stop a publish. Code is judged on the text as it
/// will publish (earlier comments removed), so a stray backtick in a note
/// can't hide the next comment's opener.
fn parse(md: &str) -> Parsed {
    let mut kept = String::new();
    let mut src: Vec<Option<usize>> = Vec::new();
    let mut r = 0; // rest = &md[r..]
    let mut ranges = Vec::new();
    loop {
        let rest = &md[r..];
        let scan = format!("{kept}{rest}");
        let code = if rest.contains("/*") {
            code_of(&scan)
        } else {
            Vec::new()
        };
        let Some(open) = find_opener(&scan, kept.len(), &code) else {
            break;
        };
        let source = r + (open - kept.len());
        let body = &scan[open + 2..];
        let close = find_closer(body);
        let after = match close {
            Some(c) => &body[c + 2..],
            None => "",
        };
        let (head, nl, skip) = close_up(&scan[..open], after);
        // The kept head: a prefix of `kept + md[r..source]`.
        if head <= kept.len() {
            kept.truncate(head);
            src.truncate(head);
        } else {
            let to = r + (head - kept.len());
            kept.push_str(&md[r..to]);
            src.extend((r..to).map(Some));
        }
        if nl {
            kept.push('\n');
            src.push(None);
        }
        let Some(_) = close else {
            ranges.push((source, md.len()));
            return Parsed {
                text: kept,
                src,
                ranges,
                errors: vec![Issue {
                    at: source,
                    reason: format!(
                        "a comment opened on line {} is never closed with */. Close it, or write \\/* if you meant it literally",
                        line_of(md, source)
                    ),
                }],
            };
        };
        let after_at = md.len() - after.len();
        ranges.push((source, after_at));
        r = after_at + skip;
    }
    kept.push_str(&md[r..]);
    src.extend((r..md.len()).map(Some));
    let mut errors = stray_closers(&kept);
    errors.extend(attached_openers(&kept));
    errors.sort_by_key(|e| e.at);
    Parsed {
        text: kept,
        src,
        ranges,
        errors,
    }
}

/// Where comments sit in `md` (`parseComments`'s ranges): `[start, end)`,
/// the opener to just past the closer, or to the end when unclosed.
pub(crate) fn ranges(md: &str) -> Vec<(usize, usize)> {
    if !md.contains("/*") {
        return Vec::new();
    }
    parse(md).ranges
}

/// `previewComments`: `md` (the text of `doc`, when there is one) without
/// its comments, with what would stop a publish said in `⚠` paragraphs at
/// the end; each line mapped to the line it came from. `None` when the text
/// holds no comment tokens at all.
pub(crate) fn preview(md: &str, doc: Option<&Mapped>) -> Option<Mapped> {
    if !md.contains("/*") && !md.contains("*/") {
        return None;
    }
    let identity;
    let doc = match doc {
        Some(d) => d,
        None => {
            identity = Mapped::identity(md);
            &identity
        }
    };
    let mut p = parse(md);
    // Each output line maps to the line of its first byte's source; an empty
    // line put back by `close_up` to the line before it, and a last empty
    // line to the source line after the newline that opened it.
    let mut lines = Vec::new();
    let mut line = doc.lines.first().copied().unwrap_or(0);
    for start in std::iter::once(0).chain(p.text.match_indices('\n').map(|(i, _)| i + 1)) {
        line = match (
            p.src.get(start),
            start.checked_sub(1).and_then(|i| p.src[i]),
        ) {
            (Some(Some(s)), _) => doc.line_at(*s),
            (None, Some(s)) => doc.line_at((s + 1).min(md.len())),
            _ => line,
        };
        lines.push(line);
    }
    if !p.errors.is_empty() {
        let last = doc.lines.last().copied().unwrap_or(0);
        let notes: Vec<String> = p
            .errors
            .iter()
            .map(|e| {
                // `/[*\\]/g` → `\$&`: Markdown shows the tokens as written.
                let escaped: String = e
                    .reason
                    .chars()
                    .flat_map(|c| {
                        (c == '*' || c == '\\')
                            .then_some('\\')
                            .into_iter()
                            .chain([c])
                    })
                    .collect();
                format!("⚠ {escaped}.")
            })
            .collect();
        let notes = format!("\n\n{}", notes.join("\n\n"));
        lines.extend(notes.matches('\n').map(|_| last));
        p.text.push_str(&notes);
    }
    Some(Mapped::from_lines(p.text, lines))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(md: &str) -> String {
        preview(md, None).map_or_else(|| md.to_string(), |m| m.text)
    }

    #[test]
    fn comments_go_and_the_whitespace_closes_up() {
        // The studio's comment_to_self fixture.
        assert_eq!(
            text("Kept /* a note to self */ text.\n\n/* a whole\nparagraph note */\n\nAfter."),
            "Kept text.\n\nAfter."
        );
        assert_eq!(text("a /* x */, b"), "a, b");
        assert_eq!(text("end /* x */\nnext"), "end\nnext");
        assert_eq!(text("/* only */"), "");
        assert_eq!(text("a\n\n/* x */\n"), "a\n");
        // Not a comment: attached, `/**`, code, escaped.
        for md in ["src/*.ts", "/**bold**/", "`/* x */`", "\\/* x */ y"] {
            assert_eq!(parse(md).ranges, vec![], "{md}");
        }
    }

    #[test]
    fn a_closer_in_a_tk_scope_or_code_does_not_close() {
        let md = "/* [TK]a */ b[=]c */ d[/TK] still */ after";
        assert_eq!(text(md), "after");
        assert_eq!(text("/* `*/` still */ after"), "after");
    }

    #[test]
    fn what_would_stop_a_publish_is_said_at_the_end() {
        assert_eq!(
            text("a\n\n/* never closed"),
            "a\n\n⚠ a comment opened on line 3 is never closed with \\*/. Close it, or write \\\\/\\* if you meant it literally."
        );
        let t = text("one */ two");
        assert!(
            t.starts_with("one */ two\n\n⚠ a \\*/ with no comment to close, after \"one\""),
            "{t}"
        );
        let t = text("word/*note*/");
        assert!(t.contains("⚠ a /\\* attached to \"word\""), "{t}");
    }

    #[test]
    fn lines_map_to_the_source() {
        let md = "a\n/* one\ntwo */\nb\n\nc /* x */ d";
        let m = preview(md, None).unwrap();
        assert_eq!(m.text, "a\nb\n\nc d");
        assert_eq!(m.lines, vec![0, 3, 4, 5]);
    }
}
