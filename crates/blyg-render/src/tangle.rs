//! Tangling (studio 0.63, `src/tangle.ts`; blygger-spec
//! `docs/tangling-plan.md`, decisions #78 and #100): a `![[+ text]]` span in
//! a thread's working copy becomes a new item at publish, which the thread
//! then quotes in its place; an inline `[[+ text]]` placeholder becomes a new
//! item the thread links. The grammar is studio-private: the server rewrites
//! every span before it publishes, so nothing here reaches the wire.
//!
//! A span opens with `![[+` at the start of a line, followed by a space or a
//! newline, and closes with `]]` at the end of a line. It may run over
//! several lines and paragraphs; `[[id]]` links, `![[id]]` directives and
//! `[[+ …]]` placeholders inside it are stepped over when looking for the
//! closer. Like every bracket form it is inert in code, in TK scopes and in
//! comments to self (`/* … */`).
//!
//! Burrow uses this for the local pre-check before publish
//! ([`parse_tangles`]'s issues, said in the studio's words), the editor
//! preview (a span as a dashed "new item" quote, a placeholder as a dashed
//! link; `previewTangles` in the studio's `read-api.ts`), and the stub
//! chooser, which skips quotes inside a span (`stub-quote.ts`). Offsets are
//! byte offsets where the TypeScript has UTF-16 indices.

use crate::grammar::{Ids, body_at, directive_line, label_text, strip_generated_grammar};
use crate::linemap::{MapBuilder, Mapped};
use crate::markdown::{self, in_ranges};
use crate::util::{escape_html, is_js_ws, js_trim};

/// Spans one publish may mint (`TANGLE_MAX`): a fifth of the RECOMMENDED
/// 50-entry feed window (tangling plan §0).
pub const TANGLE_MAX: usize = 10;

/// What a span becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    /// A `![[+ …]]` block, quoted by the thread.
    Quote,
    /// An inline `[[+ text]]` placeholder, linked by the thread.
    Link,
}

/// One span in a working copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TangleSpan {
    pub kind: SpanKind,
    /// A quote: the start of the opener's line. A link: the placeholder's start.
    pub start: usize,
    /// A quote: the end of the closer's line (before its newline). A link:
    /// just past the placeholder.
    pub end: usize,
    /// A quote: the span's text, trimmed (what the new item holds). A link:
    /// the anchor text.
    pub text: String,
}

/// A reason the working copy can't publish, at a byte offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TangleIssue {
    pub at: usize,
    pub reason: String,
}

/// Every span in `md`, and every reason it can't publish.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tangles {
    /// In source order.
    pub spans: Vec<TangleSpan>,
    /// Any issue means publish refuses before anything is minted.
    pub issues: Vec<TangleIssue>,
}

const OPENER: &str = "![[+";

fn line_start(md: &str, at: usize) -> usize {
    md[..at].rfind('\n').map_or(0, |i| i + 1)
}

fn line_end(md: &str, at: usize) -> usize {
    md[at..].find('\n').map_or(md.len(), |i| at + i)
}

/// `PLACEHOLDER` at byte `p` (the `[[`): `\[\[\+[ \t]+((?:(?!\]\])[^\n])*?)\]\]`.
/// Returns the match's end and the raw anchor text.
fn placeholder_at(md: &str, p: usize) -> Option<(usize, &str)> {
    let rest = md.get(p..)?;
    let after = rest.strip_prefix("[[+")?;
    let text_from = after.trim_start_matches([' ', '\t']);
    if text_from.len() == after.len() {
        return None;
    }
    let line = &text_from[..text_from.find('\n').unwrap_or(text_from.len())];
    let close = line.find("]]")?;
    let start = md.len() - text_from.len();
    Some((start + close + 2, &md[start..start + close]))
}

/// `NESTED` (sticky) at byte `i`: a `[[id]]` link, `![[id]]` directive
/// (label allowed) or `[[+ …]]` placeholder; its end.
fn nested_at(md: &str, i: usize) -> Option<usize> {
    let p = if md[i..].starts_with("![[") {
        i + 1
    } else if md[i..].starts_with("[[") {
        i
    } else {
        return None;
    };
    if let Some(body) = body_at(md, p + 2, Ids::Strict) {
        return Some(body.end);
    }
    placeholder_at(md, p).map(|(end, _)| end)
}

/// `OWN_MEDIA`, `\]\([^)\s]*\/?media\/[0-9a-z]{26}[/.]`: a markdown image or
/// link to this blyg's `media/` path (tangling plan §2.5).
fn has_own_media(text: &str) -> bool {
    let mut at = 0;
    while let Some(i) = text[at..].find("](") {
        let from = at + i + 2;
        let run_end = text[from..]
            .find(|c: char| c == ')' || is_js_ws(c))
            .map_or(text.len(), |n| from + n);
        let run = &text.as_bytes()[from..run_end];
        let mut k = 0;
        while let Some(m) = find_bytes(&run[k..], b"media/") {
            let s = k + m + 6;
            let id_ok = run.len() > s + 26
                && run[s..s + 26]
                    .iter()
                    .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase())
                && matches!(run[s + 26], b'/' | b'.');
            if id_ok {
                return true;
            }
            k += m + 1;
        }
        at = from;
    }
    false
}

fn find_bytes(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// A token escaped by an odd run of backslashes (`comments.ts` `escaped`).
fn escaped(text: &str, at: usize) -> bool {
    text.as_bytes()[..at]
        .iter()
        .rev()
        .take_while(|&&b| b == b'\\')
        .count()
        % 2
        == 1
}

/// Where comments to self (`/* … */`) sit (`comments.ts` `parseComments`'s
/// ranges). A comment opens at a line's start or after whitespace, never as
/// `/**`, and not in code; it closes at the first `*/` outside code and
/// outside a whole TK scope. An unclosed one runs to the end. Code is read
/// on the source rather than on the text with earlier comments removed,
/// which differs only for a comment holding a stray backtick.
fn comment_ranges(md: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if !md.contains("/*") {
        return out;
    }
    let code = markdown::code_ranges(md);
    let opens = |at: usize| {
        let before_ok = at == 0
            || md[..at]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
        before_ok && md.as_bytes().get(at + 2) != Some(&b'*')
    };
    let mut from = 0;
    loop {
        let mut open = md[from..].find("/*").map(|i| from + i);
        while let Some(at) = open {
            if opens(at) && !in_ranges(&code, at) && !escaped(md, at) {
                break;
            }
            open = md[at + 2..].find("/*").map(|i| at + 2 + i);
        }
        let Some(open) = open else { break };
        let body = &md[open + 2..];
        match find_closer(body) {
            None => {
                out.push((open, md.len()));
                break;
            }
            Some(close) => {
                let end = open + 2 + close + 2;
                out.push((open, end));
                from = end;
            }
        }
    }
    out
}

/// `findCloser`: where a comment body ends, or `None`.
fn find_closer(body: &str) -> Option<usize> {
    let code = markdown::code_ranges(body);
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

/// `parseTangles`: every span in a working copy, and every reason it can't
/// publish (tangling plan §1.4), in the studio's words.
pub fn parse_tangles(md: &str) -> Tangles {
    let mut t = Tangles::default();
    if !md.contains("[[+") {
        return t;
    }
    let scopes: Vec<(usize, usize)> = if md.contains("[TK]") {
        crate::tk::parse_scopes(md)
            .0
            .iter()
            .map(|s| (s.start, s.end))
            .collect()
    } else {
        Vec::new()
    };
    let mut inert = markdown::code_ranges(md);
    inert.extend(scopes.iter().copied());
    inert.extend(comment_ranges(md));
    let issue = |t: &mut Tangles, at: usize, reason: &str| {
        t.issues.push(TangleIssue {
            at,
            reason: reason.into(),
        })
    };

    let mut from = 0;
    while let Some(i) = md[from..].find(OPENER) {
        let at = from + i;
        from = at + OPENER.len();
        if in_ranges(&inert, at) {
            continue;
        }
        let start = line_start(md, at);
        if !js_trim(&md[start..at]).is_empty() {
            issue(&mut t, at, "a new-item span must start its own line");
            continue;
        }
        if let Some(c) = md[at + OPENER.len()..].chars().next()
            && !matches!(c, ' ' | '\t' | '\n')
        {
            issue(&mut t, at, "write a space after ![[+");
            continue;
        }
        // Look for the closer, stepping over links, directives and inert text.
        let mut i = at + OPENER.len();
        let mut close = None;
        let mut nested = None;
        while i < md.len() {
            if in_ranges(&inert, i) {
                i += md[i..].chars().next().map_or(1, char::len_utf8);
                continue;
            }
            if md[i..].starts_with(OPENER) {
                nested = Some(i);
                break;
            }
            if let Some(end) = nested_at(md, i) {
                i = end;
                continue;
            }
            if md[i..].starts_with("]]") {
                close = Some(i);
                break;
            }
            i += md[i..].chars().next().map_or(1, char::len_utf8);
        }
        if let Some(n) = nested {
            issue(
                &mut t,
                at,
                "a new-item span cannot contain another; tangle again inside the new item once it exists",
            );
            from = n;
            continue;
        }
        let Some(close) = close else {
            issue(&mut t, at, "this new-item span has no closing ]]");
            break;
        };
        from = close + 2;
        let end = line_end(md, close);
        if !js_trim(&md[close + 2..end]).is_empty() {
            issue(
                &mut t,
                close,
                "a new-item span's closing ]] must end its line",
            );
            continue;
        }
        let text = js_trim(&md[at + OPENER.len()..close]);
        if text.is_empty() {
            issue(&mut t, at, "this new-item span is empty");
            continue;
        }
        if scopes.iter().any(|&(s, _)| s > at && s < close) {
            issue(
                &mut t,
                at,
                "a new-item span cannot contain a TK scope; generate first, or move the scope out of the span",
            );
            continue;
        }
        if has_own_media(text) {
            issue(
                &mut t,
                at,
                "a new-item span cannot contain an attached image yet; attach it to the new item after it is published",
            );
            continue;
        }
        t.spans.push(TangleSpan {
            kind: SpanKind::Quote,
            start,
            end,
            text: text.to_string(),
        });
    }

    // Inline placeholders, `matchAll(PLACEHOLDER)`: not preceded by `!`.
    let blocks: Vec<(usize, usize)> = t.spans.iter().map(|s| (s.start, s.end)).collect();
    let mut at = 0;
    while let Some(i) = md[at..].find("[[+") {
        let p = at + i;
        let Some((end, raw)) = (p == 0 || md.as_bytes()[p - 1] != b'!')
            .then(|| placeholder_at(md, p))
            .flatten()
        else {
            at = p + 1;
            continue;
        };
        at = end;
        if in_ranges(&inert, p) {
            continue;
        }
        if blocks.iter().any(|&(s, e)| p > s && p < e) {
            issue(
                &mut t,
                p,
                "a [[+ …]] placeholder cannot sit inside a ![[+ …]] span; add it to the new item once it exists",
            );
            continue;
        }
        let text = js_trim(raw);
        if text.is_empty() {
            issue(&mut t, p, "this [[+ …]] placeholder is empty");
            continue;
        }
        t.spans.push(TangleSpan {
            kind: SpanKind::Link,
            start: p,
            end,
            text: text.to_string(),
        });
    }
    t.spans.sort_by_key(|s| s.start);
    if t.spans.len() > TANGLE_MAX {
        let at = t.spans[TANGLE_MAX].start;
        let n = t.spans.len();
        issue(
            &mut t,
            at,
            &format!("one publish can make at most {TANGLE_MAX} new items; this has {n}"),
        );
    }
    t
}

/// `tangleLabel`: the label the server writes into the thread for a minted
/// item, and the name its confirm step lists (tangling plan §2.3): the
/// span's first eight words, with `…` when cut; links become their labels
/// and directive lines are left out. A placeholder's label is its text,
/// with " (linked)" added in the confirm step's list.
pub fn tangle_label(span: &TangleSpan) -> String {
    if span.kind == SpanKind::Link {
        return label_text(&span.text);
    }
    let kept: Vec<&str> = span
        .text
        .split('\n')
        .filter(|l| directive_line(l, Ids::Strict).is_none())
        .collect();
    let prose = strip_generated_grammar(&kept.join("\n"));
    let words: Vec<&str> = prose.split(is_js_ws).filter(|w| !w.is_empty()).collect();
    let cut = words.len() > 8;
    let mut s = words[..words.len().min(8)].join(" ");
    if cut {
        s.push('…');
    }
    label_text(&s)
}

/// The names the studio's 409 lists (`details.parts`): a placeholder's
/// label has " (linked)" appended.
pub fn confirm_labels(spans: &[TangleSpan]) -> Vec<String> {
    spans
        .iter()
        .map(|s| match s.kind {
            SpanKind::Link => format!("{} (linked)", tangle_label(s)),
            SpanKind::Quote => tangle_label(s),
        })
        .collect()
}

/// Where the quote spans are (`[start, end)`): out of bounds for the stub
/// chooser, as code is.
pub fn quote_span_ranges(md: &str) -> Vec<(usize, usize)> {
    if !md.contains("![[+") {
        return Vec::new();
    }
    parse_tangles(md)
        .spans
        .into_iter()
        .filter(|s| s.kind == SpanKind::Quote)
        .map(|s| (s.start, s.end))
        .collect()
}

// ------------------------------------------------------------- preview

/// A line no author writes, standing in for span `i` while the rest renders.
fn token(i: usize) -> String {
    format!("\u{E010}tangle{i}\u{E010}")
}

/// A thread's working copy with each span replaced by a token, keeping its
/// line map (`previewTangles`): a quoted span becomes a block of its own; a
/// placeholder stays in its sentence.
pub(crate) struct Preview {
    pub doc: Mapped,
    pub spans: Vec<TangleSpan>,
}

pub(crate) fn preview(md: &str) -> Option<Preview> {
    if !md.contains("[[+") {
        return None;
    }
    let spans = parse_tangles(md).spans;
    if spans.is_empty() {
        return None;
    }
    let src = Mapped::identity(md);
    let mut b = MapBuilder::new(&src);
    let mut last = 0;
    for (i, span) in spans.iter().enumerate() {
        b.copy(last, span.start);
        let line = src.line_at(span.start);
        match span.kind {
            SpanKind::Link => b.insert(&token(i), line),
            SpanKind::Quote => b.insert(&format!("\n{}\n\n", token(i)), line),
        }
        last = span.end;
    }
    b.copy(last, md.len());
    Some(Preview {
        doc: b.finish(),
        spans,
    })
}

/// The rendered thread with each token drawn (`previewTangles`' `finish`):
/// a quoted span as `blockquote.blyg-tangle-new` holding `quotes[i]` (the
/// span rendered on its own) under a "new item" label, a placeholder as
/// `span.blyg-tangle-link`. A token's paragraph keeps its attributes
/// (`data-line`) on the blockquote.
pub(crate) fn finish(html: &str, spans: &[TangleSpan], quotes: &[String]) -> String {
    let mut out = html.to_string();
    for (i, span) in spans.iter().enumerate() {
        let tok = token(i);
        if span.kind == SpanKind::Quote {
            let closed = format!("{tok}</p>");
            if let Some(t) = out.find(&closed)
                && let Some(p) = out[..t].rfind("<p")
                && out[p..t].ends_with('>')
                && !out[p + 2..t - 1].contains(['<', '>'])
            {
                let attrs = out[p + 2..t - 1].to_string();
                let quote = quotes.get(i).map(String::as_str).unwrap_or("");
                out.replace_range(
                    p..t + closed.len(),
                    &format!(
                        "<blockquote class=\"blyg-tangle-new\"{attrs}><p class=\"blyg-tangle-label\">new item</p>{quote}</blockquote>"
                    ),
                );
                continue;
            }
        }
        out = out.replace(
            &tok,
            &format!(
                "<span class=\"blyg-tangle-link\" title=\"new item, linked\">{}</span>",
                escape_html(&span.text)
            ),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "7c9wk2n4h6q1x8v0z3m5rjy2ke";

    fn reasons(md: &str) -> Vec<String> {
        parse_tangles(md)
            .issues
            .into_iter()
            .map(|i| i.reason)
            .collect()
    }

    #[test]
    fn a_span_runs_from_a_line_start_to_a_line_end() {
        let md = "Intro.\n\n![[+ First part.\n\nSecond paragraph of it.]]\n\nOutro, with [[+ an idea]].\n";
        let t = parse_tangles(md);
        assert!(t.issues.is_empty(), "{:?}", t.issues);
        assert_eq!(t.spans.len(), 2);
        let q = &t.spans[0];
        assert_eq!(q.kind, SpanKind::Quote);
        assert_eq!(q.text, "First part.\n\nSecond paragraph of it.");
        assert_eq!(
            &md[q.start..q.end],
            "![[+ First part.\n\nSecond paragraph of it.]]"
        );
        let l = &t.spans[1];
        assert_eq!(l.kind, SpanKind::Link);
        assert_eq!(l.text, "an idea");
        assert_eq!(&md[l.start..l.end], "[[+ an idea]]");
        assert_eq!(
            confirm_labels(&t.spans),
            ["First part. Second paragraph of it.", "an idea (linked)"]
        );
    }

    #[test]
    fn links_and_directives_inside_a_span_are_stepped_over() {
        let md = format!("![[+ see [[{ID} | this]] and [[{ID}]]\n![[{ID} | a quote]]\nend]]\n");
        let t = parse_tangles(&md);
        assert!(t.issues.is_empty(), "{:?}", t.issues);
        assert_eq!(t.spans.len(), 1);
        // Labels: links read as their labels (or nothing); directive lines go.
        assert_eq!(tangle_label(&t.spans[0]), "see this and end");
    }

    #[test]
    fn spans_are_inert_in_code_tk_and_comments() {
        for md in [
            "`![[+ x]]`",
            "```\n![[+ x]]\n```\n",
            "    ![[+ x]]\n",
            "[TK]write ![[+ x]] here[/TK]",
            "/* ![[+ x]] */",
            "/*\n[[+ an idea]]\n*/",
            "`[[+ code]]`",
        ] {
            let t = parse_tangles(md);
            assert_eq!(t, Tangles::default(), "{md:?}");
        }
    }

    #[test]
    fn the_studios_refusals() {
        assert_eq!(
            reasons("text ![[+ x]]"),
            ["a new-item span must start its own line"]
        );
        assert_eq!(reasons("![[+x]]"), ["write a space after ![[+"]);
        assert_eq!(
            reasons("![[+ x\n\nmore"),
            ["this new-item span has no closing ]]"]
        );
        assert_eq!(
            reasons("![[+ x]] trailing"),
            ["a new-item span's closing ]] must end its line"]
        );
        assert_eq!(reasons("![[+ ]]"), ["this new-item span is empty"]);
        assert_eq!(
            reasons("![[+ a\n![[+ b]]"),
            [
                "a new-item span cannot contain another; tangle again inside the new item once it exists"
            ]
        );
        assert_eq!(
            reasons("![[+ a [TK]x[/TK] b]]"),
            [
                "a new-item span cannot contain a TK scope; generate first, or move the scope out of the span"
            ]
        );
        let media = "![[+ ![alt](media/0123456789abcdefghjkmnpqrs.webp)]]";
        assert_eq!(
            reasons(media),
            [
                "a new-item span cannot contain an attached image yet; attach it to the new item after it is published"
            ]
        );
        assert_eq!(
            reasons("![[+ a [[+ b]] c]]"),
            [
                "a [[+ …]] placeholder cannot sit inside a ![[+ …]] span; add it to the new item once it exists"
            ]
        );
        assert_eq!(
            reasons("x [[+  ]] y"),
            ["this [[+ …]] placeholder is empty"]
        );
    }

    #[test]
    fn at_most_ten_spans() {
        let ten: String = (0..10).map(|i| format!("[[+ idea {i}]] ")).collect();
        assert!(parse_tangles(&ten).issues.is_empty());
        let eleven = format!("{ten}\n\n![[+ one more]]\n");
        assert_eq!(
            reasons(&eleven),
            ["one publish can make at most 10 new items; this has 11"]
        );
    }

    #[test]
    fn a_placeholder_needs_a_space_and_one_line() {
        assert!(parse_tangles("x [[+y]] z").spans.is_empty());
        assert!(parse_tangles("x [[+ y\nz]]").spans.is_empty());
        assert!(parse_tangles("x ![[+ y]]").spans.is_empty());
    }

    #[test]
    fn long_spans_are_labelled_by_their_first_eight_words() {
        let t = parse_tangles("![[+ one two three four five six seven eight nine ten]]");
        assert_eq!(
            tangle_label(&t.spans[0]),
            "one two three four five six seven eight…"
        );
    }

    #[test]
    fn the_preview_draws_spans_in_place() {
        let md = "Intro.\n![[+ The part.]]\nOutro [[+ idea <b>]].";
        let p = preview(md).expect("spans");
        assert_eq!(
            p.doc.text,
            format!("Intro.\n\n{}\n\n\nOutro {}.", token(0), token(1))
        );
        // The token's lines keep the span's source line.
        assert_eq!(p.doc.lines, [0, 1, 1, 1, 1, 2]);
        let html = format!(
            "<p data-line=\"0\">Intro.</p>\n<p data-line=\"1\">{}</p>\n<p data-line=\"2\">Outro {}.</p>\n",
            token(0),
            token(1)
        );
        assert_eq!(
            finish(
                &html,
                &p.spans,
                &["<p>The part.</p>\n".into(), String::new()]
            ),
            "<p data-line=\"0\">Intro.</p>\n\
             <blockquote class=\"blyg-tangle-new\" data-line=\"1\"><p class=\"blyg-tangle-label\">new item</p><p>The part.</p>\n</blockquote>\n\
             <p data-line=\"2\">Outro <span class=\"blyg-tangle-link\" title=\"new item, linked\">idea &lt;b&gt;</span>.</p>\n"
        );
    }

    #[test]
    fn a_thread_preview_shows_a_span_as_a_dashed_new_item() {
        use crate::{Kind, NoResolver, RenderOpts, render_preview};
        let md = "Intro.\n\n![[+ The *part*.\n\nIts second line.]]\n\nOutro, [[+ an idea]].\n";
        let out = render_preview(md, Kind::Thread, &NoResolver, &RenderOpts::default());
        assert!(
            out.html.contains(
                "<blockquote class=\"blyg-tangle-new\" data-line=\"2\"><p class=\"blyg-tangle-label\">new item</p><p>The <em>part</em>.</p>\n<p>Its second line.</p>"
            ),
            "{}",
            out.html
        );
        assert!(
            out.html.contains(
                "<p data-line=\"6\">Outro, <span class=\"blyg-tangle-link\" title=\"new item, linked\">an idea</span>.</p>"
            ),
            "{}",
            out.html
        );
        assert!(!out.html.contains('\u{E010}'));
        assert_eq!(out.line_map, [(0, 0), (2, 1), (6, 2)]);
        // A fragment can't tangle: its preview shows the text as written.
        let frag = render_preview(md, Kind::Fragment, &NoResolver, &RenderOpts::default());
        assert!(frag.html.contains("![[+ The"), "{}", frag.html);
    }
}
