//! TK instructed-generation scopes, `[TK]instruction[=]output[/TK]` (the
//! Worker's `tk.ts`): the parser, the studio preview strip, and the
//! sentinel technique that wraps generated spans in `blyg-tk-gen` after
//! rendering.
//!
//! Offsets are byte offsets into the working copy (tk.ts uses UTF-16 code
//! units; every offset is only used to slice the same string, so the results
//! are identical).

use regex::Regex;
use std::sync::OnceLock;

use crate::linemap::{MapBuilder, Mapped};
use crate::markdown;
use crate::util::{escape_html, is_js_ws, js_trim};

/// One parsed scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TkScope {
    /// Offset of the opening `[TK]`.
    pub start: usize,
    /// Offset just after the closing `[/TK]`.
    pub end: usize,
    /// The instruction, trimmed.
    pub instruction: String,
    /// Text between `[=]` and `[/TK]`; `None` while ungenerated.
    pub output: Option<String>,
    /// De-duplicated `![[id]]` source references anywhere in the scope, in order.
    pub source_ids: Vec<String>,
    /// Offset right after `[=]`, when present.
    pub output_start: Option<usize>,
    /// Alone in its own paragraph: renders as `div.blyg-tk-gen`, not `span`.
    pub block: bool,
    /// Set for an `impyrt` scope: text generated elsewhere and pasted in.
    pub imported: Option<Imported>,
}

/// An `impyrt` scope's provenance: the model, only when the author named one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    pub model: Option<String>,
}

/// A malformed scope (`nested TK scopes are not supported`,
/// `unterminated scope (missing [/TK])`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TkError {
    pub at: usize,
    pub reason: String,
}

/// Every `![[id]]` or `![[id | label]]` in the instruction, inline or on its
/// own line (`SOURCE_REF = directiveInline()`), deduplicated in order.
fn extract_source_ids(scope_text: &str) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for (_, b) in crate::grammar::directive_bodies(scope_text) {
        let id = &scope_text[b.id];
        if !ids.iter().any(|x| x == id) {
            ids.push(id.to_string());
        }
    }
    ids
}

/// `BLANK_BEFORE = /(^\s*|\n[ \t]*\n)[ \t]*$/` on the text before the scope:
/// a blank line, or only whitespace back to the document's start (studio 0.36).
fn blank_before(before: &str) -> bool {
    if before.chars().all(is_js_ws) {
        return true;
    }
    let t = before.trim_end_matches([' ', '\t']);
    let Some(t) = t.strip_suffix('\n') else {
        return false;
    };
    let t = t.trim_end_matches([' ', '\t']);
    t.ends_with('\n')
}

/// `BLANK_AFTER = /^[ \t]*(\n[ \t]*\n|\s*$)/` on the text after the scope:
/// a blank line, or only whitespace to the document's end (studio 0.36).
fn blank_after(after: &str) -> bool {
    if after.chars().all(is_js_ws) {
        return true;
    }
    let t = after.trim_start_matches([' ', '\t']);
    let Some(t) = t.strip_prefix('\n') else {
        return false;
    };
    t.trim_start_matches([' ', '\t']).starts_with('\n')
}

/// `IMPYRT_INLINE = /^\s*impyrt(?:[ \t]+([^\s=]+))?[ \t]*=/i`
fn impyrt_inline() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(&format!(
            r"^(?i)[{ws}]*impyrt(?:[ \t]+([^{ws}=]+))?[ \t]*=",
            ws = crate::embeds::JS_WS
        ))
        .expect("impyrt inline re")
    })
}

/// `IMPYRT_INSTRUCTION = /^impyrt(?:[ \t]+(\S+))?$/i`
fn impyrt_instruction() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(&format!(
            r"^(?i)impyrt(?:[ \t]+([^{ws}]+))?$",
            ws = crate::embeds::JS_WS
        ))
        .expect("impyrt instruction re")
    })
}

/// Linear token scan, exactly as tk.ts `parseScopes`: no bracket balancing,
/// no nesting (a nested `[TK]` skips the whole outer scope), and an
/// unterminated scope stops the scan. A `[TK]`, `[=]` or `[/TK]` inside
/// code is an example of the grammar, not a token (studio#3, #68). `[TK]impyrt=…[/TK]` and
/// `[TK]impyrt <model>=…[/TK]` (or the same with `[=]`) mark pasted
/// generated text (decision #37): its instruction reads `impyrt`, and it
/// declares no sources.
pub fn parse_scopes(md: &str) -> (Vec<TkScope>, Vec<TkError>) {
    let mut scopes = Vec::new();
    let mut errors = Vec::new();
    let code = if md.contains("[TK]") {
        markdown::code_ranges(md)
    } else {
        Vec::new()
    };
    // Every token is looked for outside code, not only the opening one: a
    // `[TK]` or `[/TK]` quoted in code inside a scope is text, not a nested
    // scope or an early close (studio 0.41.1, #68).
    let find = |token: &str, from: usize| -> Option<usize> {
        let mut from = from;
        loop {
            let at = md[from..].find(token).map(|p| p + from)?;
            if !markdown::in_ranges(&code, at) {
                return Some(at);
            }
            from = at + token.len();
        }
    };
    let mut i = 0;
    while let Some(tk) = find("[TK]", i) {
        let Some(close) = find("[/TK]", tk + 4) else {
            errors.push(TkError {
                at: tk,
                reason: "unterminated scope (missing [/TK])".into(),
            });
            break;
        };
        if let Some(nested) = find("[TK]", tk + 4)
            && nested < close
        {
            errors.push(TkError {
                at: nested,
                reason: "nested TK scopes are not supported".into(),
            });
            i = close + 5;
            continue;
        }
        let eq = find("[=]", tk + 4).filter(|&e| e < close);
        let instr_end = eq.unwrap_or(close);
        let end = close + 5;
        let mut instruction = js_trim(&md[tk + 4..instr_end]).to_string();
        let mut output_start = eq.map(|e| e + 3);
        let mut imported = None;
        if eq.is_none() {
            if let Some(c) = impyrt_inline().captures(&md[tk + 4..close]) {
                output_start = Some(tk + 4 + c[0].len());
                instruction = "impyrt".into();
                imported = Some(Imported {
                    model: c.get(1).map(|m| m.as_str().to_string()),
                });
            }
        } else if let Some(c) = impyrt_instruction().captures(&instruction) {
            imported = Some(Imported {
                model: c.get(1).map(|m| m.as_str().to_string()),
            });
            instruction = "impyrt".into();
        }
        scopes.push(TkScope {
            start: tk,
            end,
            instruction,
            output: output_start.map(|o| md[o..close].to_string()),
            source_ids: if imported.is_some() {
                Vec::new()
            } else {
                // The instruction's own refs (studio 0.32.2, upstream #60): a
                // directive the output adds is not a source.
                extract_source_ids(&md[tk + 4..instr_end])
            },
            output_start,
            block: blank_before(&md[..tk]) && blank_after(&md[end..]),
            imported,
        });
        i = end;
    }
    (scopes, errors)
}

/// A span of preview text standing in for a scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Span {
    pub start: usize,
    pub end: usize,
    pub block: bool,
}

/// tk.ts `previewStrip`: every scope becomes its output, or the visible
/// `⚠ ungenerated — <instruction>` placeholder.
pub(crate) fn preview_strip(src: &Mapped, scopes: &[TkScope]) -> (Mapped, Vec<Span>) {
    let mut b = MapBuilder::new(src);
    let mut spans = Vec::with_capacity(scopes.len());
    let mut last = 0;
    let mut out_len = 0;
    for s in scopes {
        b.copy(last, s.start);
        out_len += s.start - last;
        let span_start = out_len;
        match (&s.output, s.output_start) {
            (Some(out), Some(os)) => {
                b.copy(os, os + out.len());
                out_len += out.len();
            }
            _ => {
                let instr = if s.instruction.is_empty() {
                    "(no instruction)"
                } else {
                    s.instruction.as_str()
                };
                let text = format!("⚠ ungenerated — {instr}");
                b.insert(&text, src.line_at(s.start));
                out_len += text.len();
            }
        }
        spans.push(Span {
            start: span_start,
            end: out_len,
            block: s.block,
        });
        last = s.end;
    }
    b.copy(last, src.text.len());
    (b.finish(), spans)
}

// Private Use Area characters, as tk.ts uses them (U+E000–U+E002). Markdown
// treats them as ordinary text, so linkify and link destinations take them
// as URL characters: `apply_wrappers` drops their percent-encoded form from
// an `href`, and drops them inside any tag.
pub(crate) const BLOCK_SENTINEL: char = '\u{E000}';
pub(crate) const INLINE_OPEN: char = '\u{E001}';
pub(crate) const INLINE_CLOSE: char = '\u{E002}';
/// `encodeURIComponent` of the inline sentinels.
const ENCODED_INLINE: [&str; 2] = ["%EE%80%81", "%EE%80%82"];

fn is_sentinel(c: char) -> bool {
    matches!(c, '\u{E000}'..='\u{E002}')
}

/// The Worker's `annotateGenerated` with every span highlighted (studio
/// preview): block spans become a one-line placeholder token whose
/// independently rendered HTML is spliced in later; inline spans are wrapped
/// in sentinels so Markdown still parses across their boundaries.
pub(crate) struct Annotated {
    pub doc: Mapped,
    /// (token, block span text, source line of the span)
    pub blocks: Vec<(String, String, usize)>,
    pub has_inline: bool,
}

/// A piece of a block span's text: generated prose, or an own-line
/// directive (with its attached `>` run) that publish bakes as a quote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Piece {
    Generated(std::ops::Range<usize>),
    Directive(std::ops::Range<usize>),
}

/// Decision #60: an own-line `![[id]]` left in TK output is a transclusion
/// directive once publish consumes the scope. Split a block span's text at
/// such lines (outside code), so the thread walk bakes them as quotes and
/// only the prose around them is disclosed as generated. Whitespace-only
/// prose is dropped. A span with no directive is one `Generated` piece.
pub(crate) fn split_directives(text: &str) -> Vec<Piece> {
    if !text.contains("![[") {
        return vec![Piece::Generated(0..text.len())];
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let literal = crate::transclusion::literal_lines(text, &lines);
    let mut starts = Vec::with_capacity(lines.len());
    let mut at = 0;
    for l in &lines {
        starts.push(at);
        at += l.len() + 1;
    }
    let push_prose = |from: Option<usize>, to: usize, pieces: &mut Vec<Piece>| {
        if let Some(from) = from
            && !text[from..to].chars().all(is_js_ws)
        {
            pieces.push(Piece::Generated(from..to));
        }
    };
    let mut pieces = Vec::new();
    let mut prose: Option<usize> = None;
    let mut k = 0;
    while k < lines.len() {
        if literal[k] || !crate::transclusion::is_directive_line(lines[k]) {
            prose.get_or_insert(starts[k]);
            k += 1;
            continue;
        }
        push_prose(prose.take(), starts[k], &mut pieces);
        let (_, next) = crate::transclusion::attached_quote(&lines, k);
        pieces.push(Piece::Directive(
            starts[k]..starts[next - 1] + lines[next - 1].len(),
        ));
        k = next;
    }
    push_prose(prose.take(), text.len(), &mut pieces);
    pieces
}

/// With `quotes` (threads), a block span's own-line directives stay in the
/// document as directives ([`split_directives`]), and each run of prose
/// around them becomes a generated block of its own. The studio's preview
/// shows such a directive as literal text inside the block; publish, by
/// decision #60, bakes it (see docs/RENDER.md).
pub(crate) fn annotate(stripped: &Mapped, spans: &[Span], quotes: bool) -> Annotated {
    let mut b = MapBuilder::new(stripped);
    let mut blocks = Vec::new();
    let mut has_inline = false;
    let mut last = 0;
    for (i, span) in spans.iter().enumerate() {
        b.copy(last, span.start);
        let line = stripped.line_at(span.start);
        let text = &stripped.text[span.start..span.end];
        let whole = [Piece::Generated(0..text.len())];
        let pieces = if span.block && quotes {
            split_directives(text)
        } else {
            whole.to_vec()
        };
        if span.block && pieces != whole {
            for (k, piece) in pieces.iter().enumerate() {
                let (Piece::Directive(r) | Piece::Generated(r)) = piece;
                // Prose after a directive starts on the blank line below it.
                let lead =
                    text[r.clone()].len() - text[r.clone()].trim_start_matches(is_js_ws).len();
                let at = stripped.line_at(span.start + r.start + lead);
                if k > 0 {
                    // The line the separator opens is the next piece's.
                    b.insert("\n\n", at);
                }
                match piece {
                    Piece::Directive(r) => b.copy(span.start + r.start, span.start + r.end),
                    Piece::Generated(r) => {
                        let token = format!("{BLOCK_SENTINEL}{i}.{k}{BLOCK_SENTINEL}");
                        b.insert(&token, at);
                        blocks.push((token, js_trim(&text[r.clone()]).to_string(), at));
                    }
                }
            }
        } else if span.block {
            let token = format!("{BLOCK_SENTINEL}{i}{BLOCK_SENTINEL}");
            b.insert(&token, line);
            blocks.push((token, text.to_string(), line));
        } else {
            has_inline = true;
            b.insert(&INLINE_OPEN.to_string(), line);
            b.copy(span.start, span.end);
            b.insert(&INLINE_CLOSE.to_string(), line);
        }
        last = span.end;
    }
    b.copy(last, stripped.text.len());
    Annotated {
        doc: b.finish(),
        blocks,
        has_inline,
    }
}

/// Ids an own-line `![[id]]` in a scope's output names without its
/// instruction naming them (studio tk.ts `unrequestedOutputDirectives`,
/// decision #60). Publishing a thread bakes each as a quote and notifies its
/// origin, which is a surprise when a model echoed the directive, so a
/// client warns first. `impyrt` and ungenerated scopes are skipped, and so
/// is a line inside code, which never bakes (the studio does not skip it).
pub fn unrequested_output_directives(md: &str) -> Vec<String> {
    let (scopes, _) = parse_scopes(md);
    let mut found: Vec<String> = Vec::new();
    for s in &scopes {
        if s.imported.is_some() {
            continue;
        }
        let Some(out) = &s.output else { continue };
        for piece in split_directives(out) {
            let Piece::Directive(r) = piece else { continue };
            let line = out[r].split('\n').next().unwrap_or_default();
            let Some(id) = crate::transclusion::directive_id(line) else {
                continue;
            };
            if !s.source_ids.iter().any(|x| x == id) && !found.iter().any(|x| x == id) {
                found.push(id.to_string());
            }
        }
    }
    found
}

/// A rendered block span, for [`apply_wrappers`].
pub(crate) struct Block {
    pub token: String,
    /// The complete `<div class="blyg-tk-gen">…</div>`.
    pub html: String,
    /// The span's raw text, spliced in (escaped) if Markdown did not leave
    /// the token in a paragraph of its own.
    pub literal: String,
}

/// The Worker's `applyGeneratedWrappers`. When top-level blocks carry
/// `data-line`, the placeholder paragraph is `<p data-line="N">token</p>` and
/// the attribute moves onto the `<div>`.
pub(crate) fn apply_wrappers(html: String, blocks: &[Block], has_inline: bool) -> String {
    // Every step below only touches sentinels.
    if !html.contains(is_sentinel) && !(has_inline && html.contains("%EE%80%8")) {
        return html;
    }
    let mut out = html;
    for b in blocks {
        // Spliced literally: generated text's `$&` stays `$&`.
        let plain = format!("<p>{}</p>", b.token);
        if let Some(at) = out.find(&plain) {
            out.replace_range(at..at + plain.len(), &b.html);
            continue;
        }
        // `<p data-line="N">token</p>`
        let tail = format!(">{}</p>", b.token);
        let Some(t) = out.find(&tail) else { continue };
        let Some(p) = out[..t].rfind("<p ") else {
            continue;
        };
        let attrs = &out[p + 2..t];
        if !attrs.starts_with(" data-line=\"") || attrs.contains('<') {
            continue;
        }
        let with_attrs = b.html.replacen(
            "<div class=\"blyg-tk-gen\"",
            &format!("<div class=\"blyg-tk-gen\"{attrs}"),
            1,
        );
        out.replace_range(p..t + tail.len(), &with_attrs);
    }
    if has_inline {
        // Markers linkify or a link destination took into a URL were
        // percent-encoded there: drop them, the URL is the generated text.
        for enc in ENCODED_INLINE {
            out = out.replace(enc, "");
        }
        // `/(<[^>]*>)/`: in a tag (an image's alt) a marker is dropped; in
        // text it becomes the span.
        let mut wrapped = String::with_capacity(out.len() + 64);
        let mut rest = out.as_str();
        let text = |t: &str, w: &mut String| {
            for c in t.chars() {
                match c {
                    INLINE_OPEN => w.push_str("<span class=\"blyg-tk-gen\">"),
                    INLINE_CLOSE => w.push_str("</span>"),
                    c => w.push(c),
                }
            }
        };
        while let Some(lt) = rest.find('<') {
            let Some(gt) = rest[lt..].find('>').map(|g| g + lt) else {
                break;
            };
            text(&rest[..lt], &mut wrapped);
            wrapped.extend(
                rest[lt..=gt]
                    .chars()
                    .filter(|&c| c != INLINE_OPEN && c != INLINE_CLOSE),
            );
            rest = &rest[gt + 1..];
        }
        text(rest, &mut wrapped);
        out = wrapped;
    }
    // Not in the Worker, which leaves these in: a block token Markdown did
    // not leave in a paragraph of its own becomes the span's escaped text,
    // and no other marker ever ships.
    for b in blocks {
        if out.contains(&b.token) {
            let span = format!(
                "<span class=\"blyg-tk-gen\">{}</span>",
                escape_html(&b.literal)
            );
            out = out.replace(&b.token, &span);
        }
    }
    out.retain(|c| !is_sentinel(c));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_like_tk_ts() {
        let (s, e) = parse_scopes("a [TK]x[=]y[/TK] b");
        assert!(e.is_empty());
        assert_eq!(s[0].instruction, "x");
        assert_eq!(s[0].output.as_deref(), Some("y"));
        assert!(!s[0].block);

        let (s, _) = parse_scopes("[TK] do it [/TK]\n\nnext");
        assert!(s[0].block);
        assert_eq!(s[0].output, None);

        let (s, e) = parse_scopes("A [TK]o [TK]i[/TK] t[/TK] B");
        assert!(s.is_empty() || s.len() == 1);
        assert_eq!(e[0].reason, "nested TK scopes are not supported");

        let (_, e) = parse_scopes("x [TK]open");
        assert_eq!(e[0].reason, "unterminated scope (missing [/TK])");
    }
}
