//! `[[id]]` plain internal links (protocol 0.3 §16.2): the Worker's
//! `transclusion.ts` `resolveInternalLinks` with the preview's unresolved
//! placeholder (`previewInternalLinks`), `applyInternalLinks` and
//! `remapRanges`, including its patch 9 (a link Markdown renders inside
//! `<code>` is literal text: never resolved, never an error).
//!
//! A link is inline and bakes nothing: no `transclusions[]` entry, no
//! mention, no self or cycle check. It resolves in the same order as a
//! quote (your published item, then an imported blyg item) and becomes an
//! anchor whose text is a short excerpt of the target in quotes.

use regex::Regex;
use std::collections::HashSet;
use std::sync::OnceLock;

use crate::linemap::Mapped;
use crate::markdown;
use crate::transclusion::{Found, Resolution, Resolver, Unresolved, UnresolvedReason};
use crate::util::{escape_html, is_js_ws};
use crate::{ID_ALPHABET, ItemKind};

/// The token a link becomes before rendering (the Worker's patched U+0004).
const SENTINEL: char = '\u{4}';
/// Marks each candidate in the "is this code?" probe; never leaves it.
const PROBE: char = '\u{5}';

fn link_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(&format!(r"\[\[([{ID_ALPHABET}]{{26}})\]\]")).expect("link re"))
}

/// Any 26 ASCII letters and digits, either case: what the native block
/// model accepts (it only decides what to draw as a link, like its looser
/// directive grammar).
fn loose_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[\[([0-9A-Za-z]{26})\]\]").expect("loose link re"))
}

/// `(?<!!)\[\[(id)\]\]` over `text`: `(start, end)` of each match.
fn matches(text: &str) -> Vec<(usize, usize)> {
    matches_with(text, link_re())
}

fn matches_with(text: &str, re: &Regex) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(m) = re.find_at(text, at) {
        if text[..m.start()].ends_with('!') {
            at = m.start() + 1;
            continue;
        }
        out.push((m.start(), m.end()));
        at = m.end();
    }
    out
}

/// `codeMatches`: which matches Markdown puts inside `<code>`, found by
/// rendering a copy with each match swapped for a numbered marker (for
/// `html` input, already rendered, by scanning it as it is).
fn code_matches(text: &str, ms: &[(usize, usize)], html: bool) -> HashSet<usize> {
    static CODE: OnceLock<Regex> = OnceLock::new();
    static MARK: OnceLock<Regex> = OnceLock::new();
    let mut in_code = HashSet::new();
    if ms.is_empty() {
        return in_code;
    }
    let mut probe = String::with_capacity(text.len());
    let mut last = 0;
    for (i, &(a, b)) in ms.iter().enumerate() {
        probe.push_str(&text[last..a]);
        probe.push(PROBE);
        probe.push_str(&i.to_string());
        probe.push(PROBE);
        last = b;
    }
    probe.push_str(&text[last..]);
    let html = if html {
        probe
    } else {
        markdown::render(&probe, None).0
    };
    let code = CODE.get_or_init(|| Regex::new(r"(?s)<code\b[^>]*>.*?</code>").expect("code re"));
    let mark = MARK.get_or_init(|| Regex::new("\u{5}([0-9]+)\u{5}").expect("mark re"));
    for c in code.find_iter(&html) {
        for hit in mark.captures_iter(c.as_str()) {
            if let Ok(i) = hit[1].parse() {
                in_code.insert(i);
            }
        }
    }
    in_code
}

/// `[[id]]` links in `text` (outside code) swapped for `U+0004 n U+0004`
/// tokens, with the ids in token order. For the native block model, which
/// draws the links itself.
pub(crate) fn tokenize(text: &str) -> (String, Vec<String>) {
    if !text.contains("[[") {
        return (text.to_string(), Vec::new());
    }
    let ms = matches_with(text, loose_re());
    let in_code = code_matches(text, &ms, false);
    let in_url = url_matches(text, &ms, false);
    let mut ids = Vec::new();
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (i, &(a, b)) in ms.iter().enumerate() {
        if in_code.contains(&i) || in_url.contains(&i) {
            continue;
        }
        out.push_str(&text[last..a]);
        out.push(SENTINEL);
        out.push_str(&ids.len().to_string());
        out.push(SENTINEL);
        ids.push(text[a + 2..b - 2].to_string());
        last = b;
    }
    out.push_str(&text[last..]);
    (out, ids)
}

/// Split `s` around the tokens [`tokenize`] made: `Err(id)` for a link,
/// `Ok(text)` for the text between.
pub(crate) fn split_tokens<'a>(s: &'a str, ids: &'a [String]) -> Vec<Result<&'a str, &'a str>> {
    let mut out = Vec::new();
    let mut parts = s.split(SENTINEL);
    if let Some(first) = parts.next()
        && !first.is_empty()
    {
        out.push(Ok(first));
    }
    // After the first piece, pieces alternate: token number, then text.
    let mut token = true;
    for p in parts {
        if token {
            match p.parse::<usize>().ok().and_then(|i| ids.get(i)) {
                Some(id) => out.push(Err(id.as_str())),
                None if !p.is_empty() => out.push(Ok(p)),
                None => {}
            }
        } else if !p.is_empty() {
            out.push(Ok(p));
        }
        token = !token;
    }
    out
}

/// Patch 12 `urlMatches`: which matches are part of a URL the author wrote
/// as a link destination, a CommonMark autolink `<https://x.test/[[id]]>`,
/// and so stay literal. In `html` input (a rendered block), a match inside
/// an `<a>` whose `href` carries the same `[[id]]` percent-encoded.
fn url_matches(text: &str, ms: &[(usize, usize)], html: bool) -> HashSet<usize> {
    static AUTOLINK: OnceLock<Regex> = OnceLock::new();
    static ANCHOR: OnceLock<Regex> = OnceLock::new();
    let mut in_url = HashSet::new();
    if ms.is_empty() {
        return in_url;
    }
    let mut spans: Vec<(usize, usize, &str)> = Vec::new();
    if html {
        let re = ANCHOR.get_or_init(|| {
            Regex::new(r#"(?s)<a\b[^>]*\bhref="([^"]*)"[^>]*>.*?</a>"#).expect("anchor re")
        });
        for c in re.captures_iter(text) {
            let m = c.get(0).expect("match");
            spans.push((m.start(), m.end(), c.get(1).map_or("", |h| h.as_str())));
        }
    } else {
        let re = AUTOLINK.get_or_init(|| {
            Regex::new(r"<[A-Za-z][A-Za-z0-9+.-]{1,31}:[^<>\x00-\x20]*>").expect("autolink re")
        });
        let mut at = 0;
        while let Some(m) = re.find_at(text, at) {
            // `(?<!\\)`: an escaped `<` starts no autolink.
            if text[..m.start()].ends_with('\\') {
                at = m.start() + 1;
                continue;
            }
            spans.push((m.start(), m.end(), ""));
            at = m.end();
        }
    }
    for (i, &(a, b)) in ms.iter().enumerate() {
        let id = &text[a + 2..b - 2];
        let encoded = format!("%5B%5B{id}%5D%5D");
        if spans
            .iter()
            .any(|&(s, e, href)| a > s && b < e && (!html || href.contains(&encoded)))
        {
            in_url.insert(i);
        }
    }
    in_url
}

/// A text's links swapped for tokens: what replaces them, and what failed.
pub(crate) struct Links {
    /// Token → the anchor (or unresolved marker) HTML.
    replacements: Vec<(String, String)>,
    pub resolved: usize,
    pub unresolved: Vec<Unresolved>,
    /// `(offset in the input, input length, output length)` per substitution.
    edits: Vec<(usize, usize, usize)>,
}

/// `normalizedOrigin(siteOrigin)`: the base for links to your own items.
fn our_origin(mount: &str) -> String {
    if mount.is_empty() || mount.ends_with('/') {
        mount.to_string()
    } else {
        format!("{mount}/")
    }
}

/// importer/util.ts `blygItemUrl`.
fn item_url(origin: &str, kind: ItemKind, id: &str, page: Option<&str>) -> String {
    match page.filter(|p| !p.is_empty()) {
        Some(p) => format!("{origin}{}", p.strip_prefix('/').unwrap_or(p)),
        None => {
            let seg = if kind == ItemKind::Thread { "t" } else { "f" };
            format!("{origin}{seg}/{id}/")
        }
    }
}

/// transclusion.ts `anchorText`: the target's excerpt in curly quotes.
fn anchor_text(f: &Found) -> String {
    let excerpt = excerpt_from_html(&f.content_html, 60);
    if excerpt.is_empty() {
        format!("a {}", f.kind.as_str())
    } else {
        format!("“{excerpt}”")
    }
}

fn reason(r: Resolution) -> Result<Found, UnresolvedReason> {
    match r {
        Resolution::Found(f) => Ok(f),
        Resolution::NotFound => Err(UnresolvedReason::UnknownItem),
        Resolution::Ambiguous => Err(UnresolvedReason::Ambiguous),
        Resolution::RssNotQuotable => Err(UnresolvedReason::RssNotQuotable),
        Resolution::ReservedVersion => Err(UnresolvedReason::ReservedVersion),
        Resolution::Unavailable(r) => Err(r),
    }
}

/// `previewInternalLinks` over the document `doc` (Markdown). `mount` is the
/// blyg's mount, the base of links to your own items. `seq` numbers tokens
/// across every text of one render, so their maps can be merged.
pub(crate) fn resolve(
    doc: &Mapped,
    resolver: &dyn Resolver,
    mount: &str,
    seq: &mut usize,
) -> (Mapped, Links) {
    let (text, links) = substitute(
        &doc.text,
        false,
        &|at| doc.line_at(at),
        resolver,
        mount,
        seq,
    );
    // Links are single-line, so every line keeps its source line.
    let doc = match text {
        Some(t) => doc.with_text(t),
        None => doc.clone(),
    };
    (doc, links)
}

/// `previewInternalLinks(…, {html: true})`: over a generated block's rendered
/// HTML (patch 12: the preview resolves these as publish does). Every
/// unresolved link reports the block's source `line`.
pub(crate) fn resolve_html(
    html: &str,
    line: usize,
    resolver: &dyn Resolver,
    mount: &str,
    seq: &mut usize,
) -> (Option<String>, Links) {
    substitute(html, true, &|_| line, resolver, mount, seq)
}

/// `resolveInternalLinks`: the new text (`None` when nothing matched).
fn substitute(
    text: &str,
    html: bool,
    line_of: &dyn Fn(usize) -> usize,
    resolver: &dyn Resolver,
    mount: &str,
    seq: &mut usize,
) -> (Option<String>, Links) {
    let mut links = Links {
        replacements: Vec::new(),
        resolved: 0,
        unresolved: Vec::new(),
        edits: Vec::new(),
    };
    let ms = if text.contains("[[") {
        matches(text)
    } else {
        Vec::new()
    };
    if ms.is_empty() {
        return (None, links);
    }
    let in_code = code_matches(text, &ms, html);
    let in_url = url_matches(text, &ms, html);
    let origin = our_origin(mount);
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (i, &(a, b)) in ms.iter().enumerate() {
        if in_code.contains(&i) || in_url.contains(&i) {
            continue;
        }
        out.push_str(&text[last..a]);
        last = b;
        let token = format!("{SENTINEL}{seq}{SENTINEL}");
        *seq += 1;
        let id = &text[a + 2..b - 2];
        let html = match reason(resolver.resolve_link(id)) {
            Ok(f) => {
                let href = match &f.origin {
                    Some(o) => item_url(o, f.kind, &f.id, f.page.as_deref()),
                    None => item_url(&origin, f.kind, &f.id, None),
                };
                links.resolved += 1;
                format!(
                    "<a href=\"{}\">{}</a>",
                    escape_html(&href),
                    escape_html(&anchor_text(&f))
                )
            }
            Err(r) => {
                let html = format!(
                    "<span class=\"blyg-link-unresolved\" style=\"color:#b3412b\">⚠ {}</span>",
                    escape_html(&r.to_string())
                );
                links.unresolved.push(Unresolved {
                    line: line_of(a),
                    directive: text[a..b].to_string(),
                    reason: r,
                });
                html
            }
        };
        links.edits.push((a, b - a, token.len()));
        out.push_str(&token);
        links.replacements.push((token, html));
    }
    out.push_str(&text[last..]);
    (Some(out), links)
}

impl Links {
    /// `remapRanges`: carry `[start, end)` offsets in the input over to `doc`.
    pub fn remap(&self, ranges: &[(usize, usize)]) -> Vec<(usize, usize)> {
        if self.edits.is_empty() {
            return ranges.to_vec();
        }
        let map = |pos: usize| -> usize {
            let mut delta: isize = 0;
            for &(at, from, to) in &self.edits {
                if at >= pos {
                    break;
                }
                let (from, to) = (from as isize, to as isize);
                let into = (pos - at) as isize;
                delta += if at as isize + from <= pos as isize {
                    to - from
                } else {
                    into.min(to) - into
                };
            }
            (pos as isize + delta) as usize
        };
        ranges.iter().map(|&(a, b)| (map(a), map(b))).collect()
    }

    /// `applyInternalLinks`: splice the anchors into rendered HTML. A token
    /// inside a tag (an image's alt) becomes the anchor's text, not markup.
    pub fn apply(&self, html: String) -> String {
        if self.replacements.is_empty() || !html.contains(SENTINEL) {
            return html;
        }
        let mut out = String::with_capacity(html.len());
        let mut rest = html.as_str();
        while let Some(lt) = rest.find('<') {
            let Some(gt) = rest[lt..].find('>').map(|g| g + lt) else {
                break;
            };
            out.push_str(&rest[..lt]);
            let tag = &rest[lt..=gt];
            if tag.contains(SENTINEL) {
                let mut t = tag.to_string();
                for (token, anchor) in &self.replacements {
                    let text = strip_tags(anchor).replace('"', "&quot;");
                    t = t.replace(token.as_str(), &text);
                }
                t.retain(|c| c != SENTINEL);
                out.push_str(&t);
            } else {
                out.push_str(tag);
            }
            rest = &rest[gt + 1..];
        }
        out.push_str(rest);
        // Patch 12: a token inside an `<a>`'s content (`[see [[id]]](url)`)
        // must not become a second `<a>`: it keeps the replacement minus its
        // own `<a>`/`</a>` (the unresolved `<span>` stays as it is).
        if out.contains(SENTINEL) {
            out = self.unnest(&out);
        }
        for (token, anchor) in &self.replacements {
            out = out.replace(token.as_str(), anchor);
        }
        out
    }

    /// `/<[^>]*>|[^<]+/g` with `<a>` depth: tokens in link text become the
    /// anchor's content only.
    fn unnest(&self, html: &str) -> String {
        static OPEN: OnceLock<Regex> = OnceLock::new();
        static CLOSE: OnceLock<Regex> = OnceLock::new();
        static A_TAG: OnceLock<Regex> = OnceLock::new();
        let open = OPEN.get_or_init(|| Regex::new(r"(?i)^<a\b").expect("open re"));
        let close = CLOSE.get_or_init(|| Regex::new(r"(?i)^</a\s*>").expect("close re"));
        let a_tag = A_TAG.get_or_init(|| Regex::new(r"(?i)</?a\b[^>]*>").expect("a re"));
        let mut out = String::with_capacity(html.len());
        let mut depth = 0usize;
        let mut rest = html;
        while !rest.is_empty() {
            if rest.starts_with('<') {
                match rest.find('>') {
                    Some(gt) => {
                        let tag = &rest[..=gt];
                        if open.is_match(tag) {
                            depth += 1;
                        } else if close.is_match(tag) {
                            depth = depth.saturating_sub(1);
                        }
                        out.push_str(tag);
                        rest = &rest[gt + 1..];
                    }
                    // A `<` that opens no tag matches neither alternative.
                    None => {
                        out.push('<');
                        rest = &rest[1..];
                    }
                }
                continue;
            }
            let end = rest.find('<').unwrap_or(rest.len());
            let part = &rest[..end];
            if depth > 0 && part.contains(SENTINEL) {
                let mut t = part.to_string();
                for (token, anchor) in &self.replacements {
                    t = t.replace(token.as_str(), &a_tag.replace_all(anchor, ""));
                }
                out.push_str(&t);
            } else {
                out.push_str(part);
            }
            rest = &rest[end..];
        }
        out
    }
}

/// `/<[^>]*>/g` → "".
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(lt) = rest.find('<') {
        let Some(gt) = rest[lt..].find('>').map(|g| g + lt) else {
            break;
        };
        out.push_str(&rest[..lt]);
        rest = &rest[gt + 1..];
    }
    out.push_str(rest);
    out
}

/// markdown.ts `decodeEntities`, in its order (`&amp;` last).
fn decode_entities(s: &str) -> String {
    [
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&apos;", "'"),
        ("&nbsp;", " "),
        ("&hellip;", "…"),
        ("&mdash;", "—"),
        ("&ndash;", "–"),
        ("&ldquo;", "\""),
        ("&rdquo;", "\""),
        ("&lsquo;", "'"),
        ("&rsquo;", "'"),
        ("&amp;", "&"),
    ]
    .iter()
    .fold(s.to_string(), |acc, (from, to)| acc.replace(from, to))
}

fn script_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?is)<script[^>]*>.*?</script>|<style[^>]*>.*?</style>").expect("script re")
    })
}

/// markdown.ts `BLOCK_BOUNDARY`.
fn boundary_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(&format!(
            r"(?i)</(?:p|h[1-6]|li|blockquote|pre|div|tr|section|article)>|<br[{}]*/?>",
            crate::embeds::JS_WS
        ))
        .expect("boundary re")
    })
}

/// `s.replace(/\s+/g, " ").trim()`, with JavaScript's `\s`.
fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut ws = false;
    for c in s.chars() {
        if is_js_ws(c) {
            ws = true;
        } else {
            if ws && !out.is_empty() {
                out.push(' ');
            }
            ws = false;
            out.push(c);
        }
    }
    out
}

/// markdown.ts `plainTextFromHtml`.
pub fn plain_text_from_html(html: &str) -> String {
    let s = script_re().replace_all(html, " ");
    let s = boundary_re().replace_all(&s, " ");
    collapse_ws(&decode_entities(&strip_nonempty_tags(&s)))
}

/// markdown.ts `BLOCK_SEP`: block boundaries are marked before any
/// whitespace collapses, so only they survive as line breaks.
const BLOCK_SEP: &str = "\u{0}";

/// markdown.ts `normalizeBlocks`: whitespace inside each segment collapses,
/// empty segments drop, and the rest join with `\n`.
fn normalize_blocks<'a>(segments: impl Iterator<Item = &'a str>) -> String {
    segments
        .map(collapse_ws)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// markdown.ts `selectionText` (protocol 0.3 §16.4), the selection
/// normalizer for partial quotes. Like [`plain_text_from_html`], except that
/// a block boundary (`</p>`, `</li>`, `<br>`, …) becomes a line break instead
/// of a space. Whitespace inside a block collapses to single spaces, so an
/// author's soft wraps never matter, and empty blocks drop. A partial quote
/// resolves when the quote's `selection_text` is a substring of the target's.
pub fn selection_text(html: &str) -> String {
    let s = script_re().replace_all(html, " ");
    let s = boundary_re().replace_all(&s, BLOCK_SEP);
    let s = decode_entities(&strip_nonempty_tags(&s));
    normalize_blocks(s.split(BLOCK_SEP))
}

/// markdown.ts `normalizeSelection`: [`selection_text`]'s rule for text that
/// is already text, such as a selection whose block boundaries are newlines.
pub fn normalize_selection(text: &str) -> String {
    normalize_blocks(text.split('\n'))
}

/// `/<[^>]+>/g` → "" (unlike [`strip_tags`], `<>` stays).
fn strip_nonempty_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(lt) = rest.find('<') {
        match rest[lt..].find('>').map(|g| g + lt) {
            Some(gt) if gt > lt + 1 => {
                out.push_str(&rest[..lt]);
                rest = &rest[gt + 1..];
            }
            Some(_) => {
                out.push_str(&rest[..=lt]);
                rest = &rest[lt + 1..];
            }
            None => break,
        }
    }
    out.push_str(rest);
    out
}

/// markdown.ts `excerptFromHtml`: the first `n` UTF-16 units, ellipsized.
pub fn excerpt_from_html(html: &str, n: usize) -> String {
    let text = plain_text_from_html(html);
    if text.encode_utf16().count() <= n {
        return text;
    }
    let mut units = 0;
    let mut end = 0;
    for (i, c) in text.char_indices() {
        units += c.len_utf16();
        if units > n {
            break;
        }
        end = i + c.len_utf8();
    }
    let cut = text[..end].trim_end_matches(is_js_ws);
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "01j9zq3k4m5n6p7q8r9s0t1v2w";

    #[test]
    fn grammar_skips_directives() {
        let t = format!("![[{A}]] [[{A}]] [[[{A}]]]");
        let m = matches(&t);
        assert_eq!(m.len(), 2);
        assert_eq!(&t[m[0].0..m[0].1], format!("[[{A}]]"));
    }

    #[test]
    fn code_is_literal() {
        let t = format!("`[[{A}]]` and [[{A}]]");
        let m = matches(&t);
        assert_eq!(code_matches(&t, &m, false), HashSet::from([0]));
    }

    #[test]
    fn excerpt_counts_utf16_units() {
        let html = format!("<p>{}</p>", "😀".repeat(40));
        let e = excerpt_from_html(&html, 60);
        assert_eq!(e, format!("{}…", "😀".repeat(30)));
        assert_eq!(
            plain_text_from_html("<p>a &amp;lt; b</p><p>c<br>d</p>"),
            "a &lt; b c d"
        );
    }

    #[test]
    fn remap_shifts_offsets_past_tokens() {
        let l = Links {
            replacements: Vec::new(),
            resolved: 0,
            unresolved: Vec::new(),
            edits: vec![(2, 30, 3)],
        };
        assert_eq!(l.remap(&[(0, 1), (40, 50)]), vec![(0, 1), (13, 23)]);
    }

    // Ported from studio v0.8.3 test/selection.test.ts: the selection
    // normalizer's rule, independent of any call site.
    mod selection {
        use super::super::{normalize_selection, selection_text};
        use crate::markdown;

        fn md(s: &str) -> String {
            markdown::render(s, None).0
        }

        #[test]
        fn blocks_become_line_breaks() {
            assert_eq!(
                selection_text("<p>first</p>\n<p>second</p>"),
                "first\nsecond"
            );
            // Soft wraps inside a paragraph collapse; they are not breaks.
            assert_eq!(selection_text("<p>one\ntwo\nthree</p>"), "one two three");
            assert_eq!(selection_text("<p>a   b\t\tc</p>"), "a b c");
            assert_eq!(selection_text("<p>a</p><p></p><p>  </p><p>b</p>"), "a\nb");
            assert_eq!(selection_text("<p>a<br>b</p>"), "a\nb");
            assert_eq!(selection_text("<p>a<br />b</p>"), "a\nb");
            assert_eq!(
                selection_text("<ul><li>one</li><li>two</li></ul>"),
                "one\ntwo"
            );
            assert!(!selection_text("<p>evil.</p><p>Next</p>").contains("evil.Next"));
        }

        #[test]
        fn markup_and_entities() {
            assert_eq!(
                selection_text("<p>the <em>thin</em> layer</p>"),
                "the thin layer"
            );
            assert_eq!(
                selection_text("<p>a &amp; b &lt;c&gt; &quot;d&quot;</p>"),
                "a & b <c> \"d\""
            );
            assert_eq!(
                selection_text("<blockquote><p>inner</p></blockquote><p>after</p>"),
                "inner\nafter"
            );
            assert_eq!(
                selection_text("<p>a</p><script>var x = 1;</script><p>b</p>"),
                "a\nb"
            );
            assert_eq!(selection_text("<p></p>"), "");
            assert_eq!(selection_text(""), "");
        }

        #[test]
        fn normalize_selection_agrees() {
            assert_eq!(normalize_selection("  a  b \n\n c\t"), "a b\nc");
            assert_eq!(
                normalize_selection("first\nsecond"),
                selection_text("<p>first</p><p>second</p>")
            );
        }

        #[test]
        fn both_sides_converge() {
            let contains = |target: &str, quote: &str| {
                selection_text(&md(target)).contains(&selection_text(&md(quote)))
            };
            let s = "Stigmergy is what a protocol looks like from inside.";
            assert!(contains(&format!("{s}\n\nAnd the rest."), s));
            assert!(contains(
                s,
                "Stigmergy is what a protocol\nlooks like from inside."
            ));
            assert!(contains(
                "First paragraph here.\n\nSecond paragraph here.\n\nThird.",
                "First paragraph here.\n\nSecond paragraph here."
            ));
            assert!(!contains(
                "First paragraph here.\n\nSecond paragraph here.",
                "First paragraph here. Second paragraph here."
            ));
            assert!(contains(
                "the *thin* layer where coordination happens",
                "the thin layer where coordination happens"
            ));
        }
    }
}
