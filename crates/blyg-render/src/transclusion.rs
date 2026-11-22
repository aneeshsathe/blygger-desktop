//! Transclusion `![[id]]` (threads only): the directive grammar, resolution
//! through a [`Resolver`], the baked `blockquote.blyg-transclusion` markup
//! and its provenance line (the Worker's `transclusion.ts`
//! `previewTransclusions` and `pages.ts` `transclusionProvenance` +
//! `injectProvenance`).

use regex::Regex;
use std::fmt;
use std::sync::OnceLock;

use crate::ItemKind;
use crate::grammar::{self, Ids};
use crate::linemap::Mapped;
use crate::markdown::{self, MdStats};
use crate::util::{absolutize_html, blyg_item_url, escape_html, is_js_ws, js_trim};

/// A resolved quote: the snapshot to bake and the provenance to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The source blyg's origin (e.g. `https://blyg.example.com/`) for an
    /// imported item; `None` for your own.
    pub origin: Option<String>,
    pub id: String,
    /// The version whose HTML is baked (for a retained tombstone, the pinned one).
    pub version: u32,
    /// The target's authored kind (threads nest).
    pub kind: ItemKind,
    /// The snapshot's stored `content_html`, inserted verbatim. Resolvers must
    /// supply HTML that was sanitised when it was stored (as the reference
    /// importer does); the preview page's CSP is the backstop.
    pub content_html: String,
    /// For an imported item: the source blyg's display name (the provenance
    /// line reads "from <em>name</em>"; without it, the origin's host).
    pub author: Option<String>,
    /// For an imported item: the origin's own `page` path, when it declared one.
    pub page: Option<String>,
}

/// What a [`Resolver`] says about an id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Found(Found),
    /// No local item and no imported blyg item with this id.
    NotFound,
    /// Imported from more than one origin.
    Ambiguous,
    /// Imported from a plain RSS (L0) feed, which is not quotable.
    RssNotQuotable,
    /// `![[id@vN]]`: reserved. The renderer detects these itself and never
    /// asks; a resolver may return it for completeness.
    ReservedVersion,
    /// Any other reason it cannot be quoted (draft, withdrawn, circular, …).
    Unavailable(UnresolvedReason),
}

/// Resolves quote ids from the local store (your published items and your
/// imported reading items). Publishing never fetches, and neither does this.
pub trait Resolver {
    fn resolve(&self, id: &str) -> Resolution;

    /// Resolve the target of a `[[id]]` link. A link bakes nothing, so a
    /// resolver's cycle or depth guards for quotes need not apply; the
    /// default is [`Resolver::resolve`].
    fn resolve_link(&self, id: &str) -> Resolution {
        self.resolve(id)
    }

    /// What a quote nested inside a baked quote names on its provenance line
    /// (pages.ts `injectNestedProvenance`): the item `id` from `origin`
    /// (`None`: your own), as far as the store holds it. The default asks
    /// [`Resolver::resolve`]; a resolver whose `resolve` is costly (it renders)
    /// or guarded (cycles) should answer from its store directly.
    fn nested_source(&self, origin: Option<&str>, id: &str) -> NestedSource {
        match self.resolve(id) {
            Resolution::Found(f) if f.origin.as_deref() == origin => NestedSource {
                kind: Some(f.kind),
                page: f.page,
                title: f.author,
            },
            _ => NestedSource::default(),
        }
    }

    /// --- contacts --- The contact an `@handle` names (any case), when the
    /// blyg rewrites handles at publish. The default knows nobody, so every
    /// `@handle` stays literal prose, as on a blyg without contacts.
    fn contact(&self, handle: &str) -> Option<crate::HandleCard> {
        let _ = handle;
        None
    }
}

/// A nested quote's source, for its provenance line ([`Resolver::nested_source`]).
/// Everything is optional: an unknown kind reads "fragment", a missing title
/// gives the origin's host, as the studio's line degrades.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NestedSource {
    /// The item's authored kind, when the store holds it.
    pub kind: Option<ItemKind>,
    /// For an imported item: the origin's own `page` path, when declared.
    pub page: Option<String>,
    /// For an imported item: the source blyg's display name.
    pub title: Option<String>,
}

/// A resolver that knows nothing (every quote is unresolved).
pub struct NoResolver;

impl Resolver for NoResolver {
    fn resolve(&self, _id: &str) -> Resolution {
        Resolution::NotFound
    }
}

/// Why a directive did not resolve. `Display` gives the Worker's exact reason
/// string (what the unresolved marker shows); [`UnresolvedReason::human`]
/// gives a sentence for status bars.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnresolvedReason {
    UnknownItem,
    Draft,
    Withdrawn,
    Ambiguous,
    SourceWithdrawn,
    RssNotQuotable,
    SelfQuote,
    Circular,
    ReservedVersion,
    /// A partial quote (§16.4) whose attached blockquote has no text.
    EmptyQuote,
    /// A partial quote whose passage is not in the version that would be
    /// baked.
    QuoteNotFound {
        version: u32,
    },
    /// A directive line in a fragment (§10.1, decision #102): a publish
    /// error. Only threads quote; readers show an old one as literal text.
    DirectiveInFragment,
    Other(String),
}

impl fmt::Display for UnresolvedReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            UnresolvedReason::UnknownItem => "unknown item",
            UnresolvedReason::Draft => "item is a draft, not published",
            UnresolvedReason::Withdrawn => "item is withdrawn",
            UnresolvedReason::Ambiguous => "ambiguous id: imported from more than one origin",
            UnresolvedReason::SourceWithdrawn => "source withdrawn by origin",
            UnresolvedReason::RssNotQuotable => "source is a plain RSS (L0) item, not a blyg item",
            UnresolvedReason::SelfQuote => "a thread cannot transclude itself",
            UnresolvedReason::Circular => {
                "circular transclusion: that thread already quotes this one"
            }
            UnresolvedReason::ReservedVersion => {
                "explicit-version references (@vN) are reserved, not supported in v0.1"
            }
            UnresolvedReason::EmptyQuote => "the attached blockquote is empty",
            UnresolvedReason::QuoteNotFound { version } => {
                return write!(
                    f,
                    "quoted passage not found in the target's version {version}"
                );
            }
            // The studio's publish refusal (model.ts publishScoped, 0.67, #161).
            UnresolvedReason::DirectiveInFragment => {
                "a fragment can't quote another item: make this a thread, or link it with [[id]]"
            }
            UnresolvedReason::Other(s) => s,
        };
        f.write_str(s)
    }
}

impl UnresolvedReason {
    /// A plain-language explanation, for the status bar and publish warnings.
    pub fn human(&self) -> String {
        match self {
            UnresolvedReason::UnknownItem => "it isn't in your posts or your reading list".into(),
            UnresolvedReason::Draft => "that post is still a draft".into(),
            UnresolvedReason::Withdrawn => "that post has been withdrawn".into(),
            UnresolvedReason::Ambiguous => {
                "more than one blyg you read has an item with this id".into()
            }
            UnresolvedReason::SourceWithdrawn => "its author withdrew it".into(),
            UnresolvedReason::RssNotQuotable => {
                "it comes from a plain RSS feed, which can't be quoted".into()
            }
            UnresolvedReason::SelfQuote => "a thread can't quote itself".into(),
            UnresolvedReason::Circular => "that thread already quotes this one".into(),
            UnresolvedReason::ReservedVersion => {
                "quoting a specific version (@vN) isn't supported yet".into()
            }
            UnresolvedReason::EmptyQuote => "the quoted passage under it is empty".into(),
            UnresolvedReason::QuoteNotFound { version } => {
                format!("the quoted passage isn't in version {version} of that post")
            }
            UnresolvedReason::DirectiveInFragment => {
                "a fragment can't quote: make this a thread, or link it with [[id]]"
                    .into()
            }
            UnresolvedReason::Other(s) => s.clone(),
        }
    }
}

/// A directive that did not resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unresolved {
    /// Source line (0-based).
    pub line: usize,
    /// The directive as written, trimmed.
    pub directive: String,
    pub reason: UnresolvedReason,
}

/// A resolved quote, in directive order (the wire's `transclusions[]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quote {
    pub id: String,
    pub version: u32,
    pub origin: Option<String>,
    /// Source line (0-based).
    pub line: usize,
    /// For a partial quote (§16.4): the passage, as the wire's `selector`.
    /// `None` for a whole quote.
    pub selector: Option<TextQuoteSelector>,
}

/// Protocol 0.3 §16.4's `selector`, the W3C text-quote shape (types.ts
/// `TextQuoteSelector`). `exact` is the selection in [`selection_text`]
/// form; `prefix` and `suffix` are up to [`SELECTOR_CONTEXT`] UTF-16 units of
/// the target's text either side of the first match, omitted when empty.
///
/// [`selection_text`]: crate::selection_text
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextQuoteSelector {
    pub exact: String,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
}

/// transclusion.ts `SELECTOR_CONTEXT`: how much context either side of the
/// match a selector records.
pub const SELECTOR_CONTEXT: usize = 32;

/// transclusion.ts `QUOTE_LINE` (`/^\s*>/`).
pub(crate) fn is_quote_line(line: &str) -> bool {
    line.trim_start_matches(is_js_ws).starts_with('>')
}

/// directives.ts `parser`: markdown-it's default block rules (no HTML),
/// used only to measure a blockquote. Its own engine, as in the studio, so
/// a block rule added to the renderer can't move where a quote ends.
fn extent_engine() -> &'static markdown_it::MarkdownIt {
    static MD: OnceLock<markdown_it::MarkdownIt> = OnceLock::new();
    MD.get_or_init(|| {
        use markdown_it::plugins::cmark::block;
        let mut md = markdown_it::MarkdownIt::new();
        block::code::add(&mut md);
        block::fence::add(&mut md);
        block::blockquote::add(&mut md);
        block::hr::add(&mut md);
        block::list::add(&mut md);
        block::reference::add(&mut md);
        block::heading::add(&mut md);
        block::lheading::add(&mut md);
        block::paragraph::add(&mut md);
        markdown_it::plugins::extra::tables::add(&mut md);
        md
    })
}

/// directives.ts `quoteExtent`: how many of `lines` (the first a quote
/// line) the blockquote opening there spans, CommonMark's lazy continuation
/// included (§10.1, decision #76). A quote line's leading whitespace is
/// ignored, and an unprefixed directive or reserved line is never lazy text.
fn quote_extent(lines: &[&str]) -> usize {
    use markdown_it::plugins::cmark::block::blockquote::Blockquote;
    let end = lines
        .iter()
        .position(|l| js_trim(l).is_empty())
        .unwrap_or(lines.len());
    let src = lines[..end]
        .iter()
        .map(|l| {
            if is_quote_line(l) {
                l.trim_start_matches(is_js_ws)
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let root = extent_engine().parse(&src);
    let starts = markdown::line_starts(&src);
    let mut n = root
        .children
        .iter()
        .find(|c| c.is::<Blockquote>())
        .and_then(|q| q.srcmap)
        .map(|m| m.get_byte_offsets())
        .filter(|&(s, _)| markdown::line_index(&starts, s) == 0)
        .map_or(1, |(s, e)| {
            (markdown::line_index(&starts, e.saturating_sub(1).max(s)) + 1).min(end)
        });
    if let Some(k) = (1..n).find(|&k| !is_quote_line(lines[k]) && is_directive_line(lines[k])) {
        n = k;
    }
    n.max(1)
}

/// directives.ts `attachedQuote`: the blockquote directly after the
/// directive at `i` (no blank line between) and the index to resume at.
/// Its extent is CommonMark's, lazy continuation lines included
/// ([`quote_extent`]). Each `>` line loses its marker and at most one
/// following whitespace character; a lazy line is kept as written. A line
/// empty after its marker is a paragraph break inside the selection.
pub(crate) fn attached_quote(lines: &[&str], i: usize) -> (Option<String>, usize) {
    if i + 1 >= lines.len() || !is_quote_line(lines[i + 1]) {
        return (None, i + 1);
    }
    let n = quote_extent(&lines[i + 1..]);
    let run: Vec<&str> = lines[i + 1..i + 1 + n]
        .iter()
        .map(|l| {
            if !is_quote_line(l) {
                return *l;
            }
            let rest = &l.trim_start_matches(is_js_ws)[1..];
            match rest.chars().next() {
                Some(c) if is_js_ws(c) => &rest[c.len_utf8()..],
                _ => rest,
            }
        })
        .collect();
    (Some(run.join("\n")), i + 1 + n)
}

/// transclusion.ts `selectionFromQuote`: the selection a quote run denotes,
/// its Markdown rendered and then normalized by [`selection_text`], so both
/// sides are compared as HTML flattened by the same rule.
///
/// [`selection_text`]: crate::selection_text
pub fn selection_from_quote(quote_md: &str) -> String {
    crate::links::selection_text(&markdown::render(quote_md, None).0)
}

/// transclusion.ts `locateSelection`: where `selection` occurs in the
/// target's [`selection_text`], as a selector with context from the first
/// match. `None` when it is not a substring (a publish error).
///
/// [`selection_text`]: crate::selection_text
pub fn locate_selection(target_html: &str, selection: &str) -> Option<TextQuoteSelector> {
    let hay = crate::links::selection_text(target_html);
    let at = hay.find(selection)?;
    let end = at + selection.len();
    // `slice` counts UTF-16 units; whole characters only here, so an astral
    // character straddling the limit is left out rather than split.
    let mut units = 0;
    let prefix_start = hay[..at]
        .char_indices()
        .rev()
        .take_while(|(_, c)| {
            units += c.len_utf16();
            units <= SELECTOR_CONTEXT
        })
        .last()
        .map_or(at, |(k, _)| k);
    let mut units = 0;
    let suffix_end = hay[end..]
        .char_indices()
        .take_while(|(_, c)| {
            units += c.len_utf16();
            units <= SELECTOR_CONTEXT
        })
        .last()
        .map_or(end, |(k, c)| end + k + c.len_utf8());
    let nonempty = |s: &str| (!s.is_empty()).then(|| s.to_string());
    Some(TextQuoteSelector {
        exact: selection.to_string(),
        prefix: nonempty(&hay[prefix_start..at]),
        suffix: nonempty(&hay[end..suffix_end]),
    })
}

/// The id of an own-line `![[id]]` or `![[id | label]]` directive
/// (`DIRECTIVE_LINE`), if `line` is one. The label is the author's note and
/// never reaches the bake (decision #70).
pub(crate) fn directive_id(line: &str) -> Option<&str> {
    grammar::directive_line(line, Ids::Strict).map(|b| &line[b.id])
}

/// Whether the walk would take `line` as a directive (resolved, or refused
/// as a reserved `@vN` form, labelled or not) rather than prose.
pub(crate) fn is_directive_line(line: &str) -> bool {
    directive_id(line).is_some() || grammar::reserved_line(line)
}

/// A baked quote is never inside a `blyg-tk-gen` wrapper (§5.7, #102): an
/// inline generated span that a quote interrupts closes before the quote and
/// reopens after it. `open` says whether a span was left open by the prose
/// before this chunk, and is updated for the prose after it. The span keeps
/// one `generated[]` entry; only its wrapper is divided.
fn balance_inline(chunk: &str, open: &std::cell::Cell<bool>) -> String {
    use crate::tk::{INLINE_CLOSE, INLINE_OPEN};
    let was_open = open.get();
    if !was_open && !chunk.contains([INLINE_OPEN, INLINE_CLOSE]) {
        return chunk.to_string();
    }
    let mut out = String::with_capacity(chunk.len() + 6);
    let lead = chunk.len() - chunk.trim_start_matches(is_js_ws).len();
    out.push_str(&chunk[..lead]);
    let mut o = was_open;
    if was_open && lead < chunk.len() {
        out.push(INLINE_OPEN);
    }
    for c in chunk[lead..].chars() {
        match c {
            INLINE_OPEN => o = true,
            INLINE_CLOSE => o = false,
            _ => {}
        }
    }
    let body = &chunk[lead..];
    let trimmed = body.trim_end_matches(is_js_ws);
    out.push_str(trimmed);
    if o && !trimmed.is_empty() {
        out.push(INLINE_CLOSE);
    }
    out.push_str(&body[trimmed.len()..]);
    open.set(o);
    out
}

pub(crate) struct WalkResult {
    pub html: String,
    pub quotes: Vec<(Quote, Found)>,
    pub unresolved: Vec<Unresolved>,
    pub md: MdStats,
    /// Source line of each top-level block, in document order.
    pub block_lines: Vec<usize>,
}

fn data_line(on: bool, line: usize) -> String {
    if on {
        format!(" data-line=\"{line}\"")
    } else {
        String::new()
    }
}

/// Per line, whether it can never be a directive: the Worker's walk skips a
/// line whose start lies in `codeRanges` (a code block, or a code span
/// running across lines). A directive on its own line in TK output is,
/// provisionally upstream, a real quote (v0.4-plan §9.2).
pub(crate) fn literal_lines(text: &str, lines: &[&str]) -> Vec<bool> {
    // Only a line holding `![[` could be a directive; skip the extra parse.
    if !text.contains("![[") {
        return vec![false; lines.len()];
    }
    let code = markdown::code_ranges(text);
    let mut start = 0;
    lines
        .iter()
        .map(|line| {
            let literal = markdown::in_ranges(&code, start);
            start += line.len() + 1;
            literal
        })
        .collect()
}

/// transclusion.ts `walk` with the preview's unresolved placeholder. Lines
/// in code stay prose.
pub(crate) fn walk(
    doc: &Mapped,
    resolver: &dyn Resolver,
    self_id: Option<&str>,
    lines_on: bool,
) -> WalkResult {
    let mut parts: Vec<String> = Vec::new();
    let mut quotes = Vec::new();
    let mut unresolved = Vec::new();
    let mut md = MdStats::default();
    let mut block_lines = Vec::new();
    let mut prose_start: Option<usize> = None; // first line index of pending prose
    let lines: Vec<&str> = doc.text.split('\n').collect();
    let literal = literal_lines(&doc.text, &lines);
    // Whether an inline generated span is open across a quote (§5.7, #102).
    let span_open = std::cell::Cell::new(false);

    let flush = |from: Option<usize>,
                 to: usize,
                 parts: &mut Vec<String>,
                 md: &mut MdStats,
                 block_lines: &mut Vec<usize>| {
        if let Some(from) = from {
            let chunk = balance_inline(&lines[from..to].join("\n"), &span_open);
            let map = |j: usize| doc.lines[from + j];
            let (html, stats) = markdown::render(&chunk, if lines_on { Some(&map) } else { None });
            md.images += stats.images;
            md.videos += stats.videos;
            md.math_errors.extend(stats.math_errors); // --- maths ---
            block_lines.extend(stats.block_lines);
            parts.push(html);
        }
    };

    // Lines before this belong to a directive's attached quote.
    let mut resume = 0;
    for (k, line) in lines.iter().enumerate() {
        if k < resume {
            continue;
        }
        let src_line = doc.lines[k];
        let fail = |reason: UnresolvedReason,
                    parts: &mut Vec<String>,
                    unresolved: &mut Vec<Unresolved>,
                    block_lines: &mut Vec<usize>| {
            parts.push(format!(
                "<blockquote class=\"blyg-transclusion unresolved\"{}><p>⚠ unresolvable: {}</p></blockquote>",
                data_line(lines_on, src_line),
                escape_html(&reason.to_string())
            ));
            if lines_on {
                block_lines.push(src_line);
            }
            unresolved.push(Unresolved {
                line: src_line,
                directive: js_trim(line).to_string(),
                reason,
            });
        };
        if literal[k] {
            prose_start.get_or_insert(k);
            continue;
        }
        if grammar::reserved_line(line) {
            flush(prose_start.take(), k, &mut parts, &mut md, &mut block_lines);
            fail(
                UnresolvedReason::ReservedVersion,
                &mut parts,
                &mut unresolved,
                &mut block_lines,
            );
            continue;
        }
        let Some(body) = grammar::directive_line(line, Ids::Strict) else {
            prose_start.get_or_insert(k);
            continue;
        };
        let id = &line[body.id.clone()];
        // The author's name for the quoted item (§10.1, §10.2, #81), baked
        // verbatim and escaped as `data-blyg-label`; plain text (#102).
        let label = grammar::label(body.raw_label.clone().map(|r| &line[r]));
        flush(prose_start.take(), k, &mut parts, &mut md, &mut block_lines);
        // The attached quote (§16.4) is part of the directive whether or not
        // the target resolves, so a failure still consumes it rather than
        // leaving it to render as the author's own quotation.
        let (quote_md, next) = attached_quote(&lines, k);
        resume = next;
        let found = match resolver.resolve(id) {
            Resolution::Found(f) => {
                if f.origin.is_none()
                    && f.kind == ItemKind::Thread
                    && self_id == Some(f.id.as_str())
                {
                    Err(UnresolvedReason::SelfQuote)
                } else {
                    Ok(f)
                }
            }
            Resolution::NotFound => Err(UnresolvedReason::UnknownItem),
            Resolution::Ambiguous => Err(UnresolvedReason::Ambiguous),
            Resolution::RssNotQuotable => Err(UnresolvedReason::RssNotQuotable),
            Resolution::ReservedVersion => Err(UnresolvedReason::ReservedVersion),
            Resolution::Unavailable(r) => Err(r),
        };
        // Partial (§16.4): the passage must be in the version being baked.
        let found = found.and_then(|f| match &quote_md {
            None => Ok((f, None)),
            Some(q) => {
                let selection = selection_from_quote(q);
                if selection.is_empty() {
                    return Err(UnresolvedReason::EmptyQuote);
                }
                match locate_selection(&f.content_html, &selection) {
                    Some(sel) => Ok((f, Some(sel))),
                    None => Err(UnresolvedReason::QuoteNotFound { version: f.version }),
                }
            }
        });
        match found {
            Err(reason) => fail(reason, &mut parts, &mut unresolved, &mut block_lines),
            Ok((f, selector)) => {
                let origin_attr = f
                    .origin
                    .as_ref()
                    .map(|o| format!(" data-blyg-origin=\"{}\"", escape_html(o)))
                    .unwrap_or_default();
                let label_attr = label
                    .map(|l| format!(" data-blyg-label=\"{}\"", escape_html(l)))
                    .unwrap_or_default();
                let (class, body) = match &selector {
                    // The bake is the selection's plain text in paragraphs
                    // (plan §7.3 P4), not a carved range of the target's HTML.
                    Some(sel) => (
                        "blyg-transclusion blyg-partial",
                        sel.exact
                            .split('\n')
                            .map(|p| format!("<p>{}</p>", escape_html(p)))
                            .collect::<Vec<_>>()
                            .join("\n"),
                    ),
                    // A non-conformant source's relative URLs resolve
                    // against the source's identity origin before the bake
                    // (§10.2, #102), so this item's HTML meets §5.2.
                    None => (
                        "blyg-transclusion",
                        match &f.origin {
                            Some(o) => absolutize_html(&f.content_html, o),
                            None => f.content_html.clone(),
                        },
                    ),
                };
                parts.push(format!(
                    "<blockquote class=\"{class}\" data-blyg-id=\"{}\" data-blyg-version=\"{}\"{origin_attr}{label_attr}{}>\n{body}\n</blockquote>",
                    escape_html(&f.id),
                    f.version,
                    data_line(lines_on, src_line),
                ));
                if lines_on {
                    block_lines.push(src_line);
                }
                quotes.push((
                    Quote {
                        id: f.id.clone(),
                        version: f.version,
                        origin: f.origin.clone(),
                        line: src_line,
                        selector,
                    },
                    f,
                ));
            }
        }
    }
    flush(
        prose_start.take(),
        lines.len(),
        &mut parts,
        &mut md,
        &mut block_lines,
    );
    WalkResult {
        html: parts.join("\n"),
        quotes,
        unresolved,
        md,
        block_lines,
    }
}

/// `new URL(origin).host`, for the provenance label.
fn host_of(origin: &str) -> String {
    let rest = origin.split_once("://").map_or(origin, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = host.to_ascii_lowercase();
    let scheme = origin
        .split_once("://")
        .map(|(s, _)| s.to_ascii_lowercase());
    match (scheme.as_deref(), host.rsplit_once(':')) {
        (Some("https"), Some((h, "443"))) | (Some("http"), Some((h, "80"))) => h.to_string(),
        _ => host,
    }
}

/// pages.ts `transclusionProvenance` for one direct quote. A partial quote
/// reads "excerpt of vN", a whole one "snapshot of vN".
pub(crate) fn provenance_line(f: &Found, partial: bool, mount: &str) -> String {
    let (href, label) = match &f.origin {
        Some(origin) => {
            let href = blyg_item_url(origin, f.kind, &f.id, f.page.as_deref());
            let label = match f.author.as_deref().filter(|a| !a.is_empty()) {
                Some(name) => format!("from <em>{}</em> ↗", escape_html(name)),
                None => format!("from {} ↗", escape_html(&host_of(origin))),
            };
            (href, label)
        }
        None => {
            let seg = if f.kind == ItemKind::Thread { "t" } else { "f" };
            (
                format!("{mount}/{seg}/{}/", f.id),
                format!("{} ↗", f.kind.as_str()),
            )
        }
    };
    format!(
        "<p class=\"provenance\"><a href=\"{}\">{label}</a> · {} v{}</p>",
        crate::util::escape_href(&href),
        if partial { "excerpt of" } else { "snapshot of" },
        f.version
    )
}

/// Whether an opening `<blockquote …>` tag is a baked quote: its class list
/// has the `blyg-transclusion` token (so `blyg-transclusion blyg-partial`
/// counts, as in pages.ts `injectProvenance`) and not `unresolved` (the
/// preview's marker, which has no provenance line).
fn is_baked_quote(tag: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r#"\bclass="([^"]*)""#).expect("class re"));
    re.captures(tag).is_some_and(|c| {
        let has = |t: &str| c[1].split(is_js_ws).any(|x| x == t);
        has("blyg-transclusion") && !has("unresolved")
    })
}

/// The `<blockquote …>` / `</blockquote>` tags, as pages.ts scans them.
fn blockquote_tags() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<blockquote\b[^>]*>|</blockquote>").expect("bq re"))
}

/// pages.ts `clampForeign`: whitespace runs as one space, trimmed, and cut
/// to `max` UTF-16 units with an ellipsis (whole characters; the studio cuts
/// at a grapheme boundary).
fn clamp_foreign(raw: &str, max: usize) -> String {
    let flat = raw
        .split(is_js_ws)
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if flat.encode_utf16().count() <= max {
        return flat;
    }
    let mut units = 0;
    let mut end = 0;
    for (i, c) in flat.char_indices() {
        units += c.len_utf16();
        if units > max - 1 {
            break;
        }
        end = i + c.len_utf8();
    }
    format!("{}…", &flat[..end])
}

/// pages.ts `bakedLabel`: a baked quote's `data-blyg-label` (decision #81),
/// decoded and flattened to one short line; `None` when the tag has none.
fn baked_label(tag: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r#"\sdata-blyg-label="([^"]*)""#).expect("label re"));
    let raw = re.captures(tag)?;
    let text = clamp_foreign(&html_escape::decode_html_entities(&raw[1]), 120);
    (!text.is_empty()).then_some(text)
}

/// pages.ts `labelledProvenance`: the quoter's label becomes the line's
/// anchor text, and what the anchor said (the source blyg, or the local
/// kind) stays beside it as plain text: `<a>label ↗</a> · fragment · …`.
fn labelled_provenance(line: &str, label: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(?s)(<a\b[^>]*>)(.*?)(?: ↗)?</a>").expect("anchor re"));
    re.replace(line, |c: &regex::Captures| {
        format!("{}{} ↗</a> · {}", &c[1], escape_html(label), &c[2])
    })
    .into_owned()
}

/// pages.ts `injectProvenance`: one provenance paragraph inside each
/// top-level baked quote (whole or partial), in order; nested quotes get
/// theirs from [`inject_nested_provenance`]. A labelled quote's line
/// anchors on the label ([`labelled_provenance`], studio 0.59).
pub(crate) fn inject_provenance(html: &str, provenance: &[String]) -> String {
    let mut out =
        String::with_capacity(html.len() + provenance.iter().map(|p| p.len() + 1).sum::<usize>());
    let mut last = 0;
    let mut depth = 0usize;
    let mut in_transclusion = false;
    let mut label: Option<String> = None;
    let mut i = 0;
    for m in blockquote_tags().find_iter(html) {
        if m.as_str().starts_with("</") {
            depth = depth.saturating_sub(1);
            if depth == 0 && in_transclusion {
                out.push_str(&html[last..m.start()]);
                if let Some(p) = provenance.get(i) {
                    out.push('\n');
                    match &label {
                        Some(l) => out.push_str(&labelled_provenance(p, l)),
                        None => out.push_str(p),
                    }
                }
                i += 1;
                last = m.start();
                in_transclusion = false;
            }
        } else {
            if depth == 0 {
                label = baked_label(m.as_str());
                in_transclusion = is_baked_quote(m.as_str());
            }
            depth += 1;
        }
    }
    out.push_str(&html[last..]);
    out
}

/// One open `<blockquote>` while [`inject_nested_provenance`] scans.
struct Layer {
    transclusion: bool,
    id: Option<String>,
    version: Option<String>,
    origin: Option<String>,
    partial: bool,
    nested: bool,
    label: Option<String>,
}

/// A nested layer's provenance anchor: its source blyg (or your own kind),
/// linked where the store says the item lives.
fn nested_link(layer_origin: Option<&str>, id: &str, src: NestedSource, mount: &str) -> String {
    let kind = src.kind.unwrap_or(ItemKind::Fragment);
    let (href, label) = match layer_origin {
        Some(origin) => (
            blyg_item_url(origin, kind, id, src.page.as_deref()),
            match src.title.filter(|t| !t.is_empty()) {
                Some(t) => format!("from <em>{}</em> ↗", escape_html(&clamp_foreign(&t, 60))),
                None => format!("from {} ↗", escape_html(&host_of(origin))),
            },
        ),
        None => {
            let seg = if kind == ItemKind::Thread { "t" } else { "f" };
            (
                format!("{mount}/{seg}/{id}/"),
                format!("{} ↗", kind.as_str()),
            )
        }
    };
    format!(
        "<a href=\"{}\">{label}</a>",
        crate::util::escape_href(&href)
    )
}

/// pages.ts `injectNestedProvenance` (studio 0.49, in the preview since
/// 0.64): a `p.provenance.nested` line in every baked quote nested inside
/// another, at any depth, read from the nested quote's own `data-blyg-*`
/// attributes. An absent `data-blyg-origin` means the origin of the layer
/// that baked it, so the origin is carried down. Presentation only.
pub(crate) fn inject_nested_provenance(html: &str, resolver: &dyn Resolver, mount: &str) -> String {
    if !html.contains("data-blyg-id=") {
        return html.to_string();
    }
    static ATTRS: OnceLock<[Regex; 4]> = OnceLock::new();
    let attrs = ATTRS.get_or_init(|| {
        [
            "class",
            "data-blyg-id",
            "data-blyg-version",
            "data-blyg-origin",
        ]
        .map(|n| Regex::new(&format!(r#"\b{n}="([^"]*)""#)).expect("attr re"))
    });
    let attr = |tag: &str, k: usize| attrs[k].captures(tag).map(|c| c[1].to_string());
    let mut stack: Vec<Layer> = Vec::new();
    let mut closes: Vec<(usize, Layer)> = Vec::new();
    for m in blockquote_tags().find_iter(html) {
        let tag = m.as_str();
        if tag.starts_with("</") {
            if let Some(layer) = stack.pop()
                && layer.transclusion
                && layer.nested
                && layer.id.is_some()
            {
                closes.push((m.start(), layer));
            }
            continue;
        }
        let class = attr(tag, 0).unwrap_or_default();
        let has = |t: &str| class.split(is_js_ws).any(|x| x == t);
        // The preview's unresolved marker is not a quote (and holds none).
        let transclusion = has("blyg-transclusion") && !has("unresolved");
        let enclosing = stack.iter().rev().find(|l| l.transclusion);
        let layer = Layer {
            transclusion,
            id: attr(tag, 1),
            version: attr(tag, 2),
            origin: attr(tag, 3).or_else(|| enclosing.and_then(|l| l.origin.clone())),
            partial: has("blyg-partial"),
            nested: enclosing.is_some(),
            label: if transclusion { baked_label(tag) } else { None },
        };
        stack.push(layer);
    }
    if closes.is_empty() {
        return html.to_string();
    }
    let mut links: std::collections::HashMap<(Option<String>, String), String> =
        std::collections::HashMap::new();
    let mut out = String::with_capacity(html.len() + closes.len() * 120);
    let mut last = 0;
    for (at, layer) in &closes {
        let id = layer.id.as_deref().unwrap_or_default();
        let origin = layer.origin.as_deref();
        let link = links
            .entry((layer.origin.clone(), id.to_string()))
            .or_insert_with(|| nested_link(origin, id, resolver.nested_source(origin, id), mount));
        let line = format!(
            "<p class=\"provenance nested\">{link} · {} v{}</p>",
            if layer.partial {
                "excerpt of"
            } else {
                "snapshot of"
            },
            escape_html(layer.version.as_deref().unwrap_or("?"))
        );
        out.push_str(&html[last..*at]);
        out.push('\n');
        match &layer.label {
            Some(l) => out.push_str(&labelled_provenance(&line, l)),
            None => out.push_str(&line),
        }
        last = *at;
    }
    out.push_str(&html[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grammar() {
        assert!(directive_id("  ![[01j9zq3k4m5n6p7q8r9s0t1v2w]]\u{A0}").is_some());
        assert!(directive_id("![[01J9ZQ3K4M5N6P7Q8R9S0T1V2W]]").is_none());
        assert!(directive_id("see ![[01j9zq3k4m5n6p7q8r9s0t1v2w]]").is_none());
        assert_eq!(
            directive_id("\u{A0}![[01j9zq3k4m5n6p7q8r9s0t1v2w | a | note]] "),
            Some("01j9zq3k4m5n6p7q8r9s0t1v2w")
        );
        assert!(directive_id("![[01j9zq3k4m5n6p7q8r9s0t1v2w | a]] b]]").is_none());
        assert!(grammar::reserved_line(
            "![[01j9zq3k4m5n6p7q8r9s0t1v2w@v12]]"
        ));
        // Decision #102: the labelled form is reserved too; inline is text.
        assert!(grammar::reserved_line(
            " ![[01j9zq3k4m5n6p7q8r9s0t1v2w@v2 | a | b]]\t"
        ));
        assert!(grammar::reserved_line(
            "![[01j9zq3k4m5n6p7q8r9s0t1v2w@v2|x]]"
        ));
        assert!(!grammar::reserved_line(
            "![[01j9zq3k4m5n6p7q8r9s0t1v2w@v2 | a]] b]]"
        ));
        assert!(!grammar::reserved_line(
            "see ![[01j9zq3k4m5n6p7q8r9s0t1v2w@v2]]"
        ));
        assert!(!grammar::reserved_line("![[01j9zq3k4m5n6p7q8r9s0t1v2w@v]]"));
        assert!(!grammar::reserved_line(
            "![[01j9zq3k4m5n6p7q8r9s0t1v2w@v2 | open"
        ));
        assert_eq!(
            host_of("https://Blyg.Example.com:443/x/"),
            "blyg.example.com"
        );
    }

    #[test]
    fn provenance_depth() {
        let html = "<blockquote class=\"blyg-transclusion\" data-blyg-id=\"a\">\n<blockquote class=\"blyg-transclusion\">in</blockquote>\n</blockquote><blockquote>plain</blockquote>";
        let out = inject_provenance(html, &["<p>P</p>".to_string()]);
        assert_eq!(out.matches("<p>P</p>").count(), 1);
        assert!(out.contains("in</blockquote>\n\n<p>P</p></blockquote>"));
    }

    #[test]
    fn provenance_class_token() {
        // A partial quote's class is two tokens; the unresolved marker gets
        // no line and must not advance the index.
        let html = "<blockquote class=\"blyg-transclusion blyg-partial\">a</blockquote>\
                    <blockquote class=\"blyg-transclusion unresolved\">x</blockquote>\
                    <blockquote class=\"blyg-transclusion\">b</blockquote>";
        let out = inject_provenance(html, &["<p>1</p>".into(), "<p>2</p>".into()]);
        assert!(out.contains("a\n<p>1</p></blockquote>"), "{out}");
        assert!(out.contains("x</blockquote>"), "{out}");
        assert!(out.contains("b\n<p>2</p></blockquote>"), "{out}");
    }

    #[test]
    fn attached_quote_run() {
        let lines = ["![[x]]", "> a", ">", "  >b", ">\tc", "", "after"];
        let (q, next) = attached_quote(&lines, 0);
        assert_eq!(q.as_deref(), Some("a\n\nb\nc"));
        assert_eq!(next, 5);
        // Lazy continuation (decision #76): a line that would continue the
        // quote's paragraph is the quote's, kept as written; the `>` line
        // after it too.
        let lines = ["![[x]]", "> a", "  lazy", "> b", "after"];
        assert_eq!(
            attached_quote(&lines, 0),
            (Some("a\n  lazy\nb\nafter".into()), 5)
        );
        // Not after an empty `>` line, and not a line opening a block of
        // its own, or a directive.
        for stop in [
            "# h",
            "- item",
            "---",
            "```",
            "![[01j9zq3k4m5n6p7q8r9s0t1v2w]]",
        ] {
            assert_eq!(attached_quote(&["d", "> a", stop], 0).1, 2, "{stop}");
        }
        assert_eq!(attached_quote(&["d", "> a", ">", "text"], 0).1, 3);
        // Only one space after the marker goes, so "> > nested" stays nested.
        let (q, _) = attached_quote(&["d", ">  > nested"], 0);
        assert_eq!(q.as_deref(), Some(" > nested"));
        assert_eq!(attached_quote(&["d", "", "> x"], 0), (None, 1));
    }

    #[test]
    fn selector_context() {
        let html = "<p>Before it. The middle bit. After it.</p>";
        let s = locate_selection(html, "The middle bit.").unwrap();
        assert_eq!(s.prefix.as_deref(), Some("Before it. "));
        assert_eq!(s.suffix.as_deref(), Some(" After it."));
        let s = locate_selection("<p>Exactly this.</p>", "Exactly this.").unwrap();
        assert_eq!(
            s,
            TextQuoteSelector {
                exact: "Exactly this.".into(),
                prefix: None,
                suffix: None
            }
        );
        // 32 UTF-16 units either side, at most.
        let long = format!("<p>{} mid {}</p>", "x".repeat(50), "y".repeat(50));
        let s = locate_selection(&long, "mid").unwrap();
        assert_eq!(s.prefix.unwrap(), format!("{} ", "x".repeat(31)));
        assert_eq!(s.suffix.unwrap(), format!(" {}", "y".repeat(31)));
        let s = locate_selection("<p>a😀😀 mid</p>", "mid").unwrap();
        assert_eq!(s.prefix.as_deref(), Some("a😀😀 "));
        assert!(locate_selection(html, "not there").is_none());
    }

    // Ported from studio v0.8.3 test/partial-transclusion.test.ts, through
    // the preview (which shares the walker with publish there too).
    mod partial {
        use super::super::*;
        use crate::{Kind, RenderOpts, Rendered, render_markdown, render_preview};
        use std::collections::HashMap;

        const T: &str = "01j9zq3k4m5n6p7q8r9s0t1v2w";
        const U: &str = "01j9zq3k4m5n6p7q8r9s0t1v2x";
        const V: &str = "01j9zq3k4m5n6p7q8r9s0t1v2y";
        const NOPE: &str = "01j9zq3k4m5n6p7q8r9s0t1v2z";

        const TARGET: &str = "Stigmergy is what a protocol looks like from inside, and the reason it\n\
                              looks like nothing at all is the point.\n\
                              \n\
                              A second paragraph that is not quoted.";
        const PARA_ONE: &str = "Stigmergy is what a protocol looks like from inside, and the reason it looks like nothing at all is the point.";

        struct Store(HashMap<&'static str, Found>);

        impl Resolver for Store {
            fn resolve(&self, id: &str) -> Resolution {
                self.0
                    .get(id)
                    .cloned()
                    .map_or(Resolution::NotFound, Resolution::Found)
            }
        }

        fn found(id: &str, version: u32, md: &str) -> Found {
            Found {
                origin: None,
                id: id.into(),
                version,
                kind: ItemKind::Fragment,
                content_html: render_markdown(md),
                author: None,
                page: None,
            }
        }

        fn store(items: &[(&'static str, u32, &str)]) -> Store {
            Store(
                items
                    .iter()
                    .map(|&(id, v, md)| (id, found(id, v, md)))
                    .collect(),
            )
        }

        fn render(s: &Store, md: &str) -> Rendered {
            let opts = RenderOpts {
                data_line: false,
                provenance: false,
                ..RenderOpts::default()
            };
            render_preview(md, Kind::Thread, s, &opts)
        }

        fn exact(r: &Rendered) -> Option<&str> {
            r.stats.transclusions[0]
                .selector
                .as_ref()
                .map(|s| s.exact.as_str())
        }

        #[test]
        fn adjacent_quote_bakes_the_passage() {
            let s = store(&[(T, 1, TARGET)]);
            let r = render(
                &s,
                &format!(
                    "![[{T}]]\n> Stigmergy is what a protocol looks like from inside,\n> and the reason it looks like nothing at all is the point.\n\nCommentary after."
                ),
            );
            assert!(r.html.contains("class=\"blyg-transclusion blyg-partial\""));
            assert!(r.html.contains(&format!("<p>{PARA_ONE}</p>")), "{}", r.html);
            assert!(!r.html.contains("A second paragraph that is not quoted"));
            assert!(r.html.contains("<p>Commentary after.</p>"));
            // The quote is the directive's, not the author's own blockquote.
            assert_eq!(r.html.matches("<blockquote").count(), 1);
            assert_eq!(exact(&r), Some(PARA_ONE));
            assert!(r.stats.unresolved.is_empty());
        }

        #[test]
        fn blank_line_detaches() {
            let s = store(&[(T, 1, TARGET)]);
            let r = render(
                &s,
                &format!(
                    "![[{T}]]\n\n> My own pull-quote, which is not a selection.\n\nAnd commentary."
                ),
            );
            assert!(r.html.contains("class=\"blyg-transclusion\""));
            assert!(!r.html.contains("blyg-partial"));
            assert_eq!(r.stats.transclusions.len(), 1);
            assert_eq!(r.stats.transclusions[0].selector, None);
            assert!(r.html.contains("A second paragraph that is not quoted"));
            assert!(r.html.contains(
                "<blockquote>\n<p>My own pull-quote, which is not a selection.</p>\n</blockquote>"
            ));
        }

        #[test]
        fn empty_quote_line_is_a_paragraph_break() {
            let s = store(&[(
                T,
                1,
                "First paragraph here.\n\nSecond paragraph here.\n\nThird.",
            )]);
            let r = render(
                &s,
                &format!(
                    "![[{T}]]\n> First paragraph here.\n>\n> Second paragraph here.\n\nAfter."
                ),
            );
            assert_eq!(
                exact(&r),
                Some("First paragraph here.\nSecond paragraph here.")
            );
            assert!(
                r.html
                    .contains("<p>First paragraph here.</p>\n<p>Second paragraph here.</p>")
            );
            assert!(!r.html.contains("Third."));
        }

        #[test]
        fn run_takes_lazy_lines_and_ends_at_a_new_block() {
            // Decision #76: an unmarked line continuing the quote's
            // paragraph is the quote's; a list item opens a block of its own.
            let s = store(&[(T, 1, TARGET)]);
            let r = render(
                &s,
                &format!(
                    "![[{T}]]\n> Stigmergy is what a protocol looks like from inside,\nand the reason it\n- Commentary as a list."
                ),
            );
            assert!(r.html.contains("blyg-partial"));
            assert!(r.html.contains("<li>Commentary as a list.</li>"));
            assert_eq!(
                exact(&r),
                Some("Stigmergy is what a protocol looks like from inside, and the reason it")
            );
        }

        #[test]
        fn not_found_names_the_version() {
            let s = store(&[(T, 1, TARGET), (U, 2, "Rewritten entirely.")]);
            let r = render(
                &s,
                &format!("![[{T}]]\n> Words the target never said.\n\n![[{U}]]\n> {PARA_ONE}"),
            );
            let reasons: Vec<String> = r
                .stats
                .unresolved
                .iter()
                .map(|u| u.reason.to_string())
                .collect();
            assert_eq!(
                reasons,
                [
                    "quoted passage not found in the target's version 1",
                    "quoted passage not found in the target's version 2"
                ]
            );
            assert_eq!(
                r.stats.unresolved[1].reason,
                UnresolvedReason::QuoteNotFound { version: 2 }
            );
            assert_eq!(r.stats.unresolved[1].line, 3);
            assert_eq!(r.stats.unresolved[1].directive, format!("![[{U}]]"));
            // The existing marker, and the quote consumed with it.
            assert!(r.html.contains(
                "<blockquote class=\"blyg-transclusion unresolved\"><p>⚠ unresolvable: quoted passage not found in the target&#39;s version 1</p></blockquote>"
            ), "{}", r.html);
            assert!(!r.html.contains("Words the target never said"));
            assert!(r.stats.transclusions.is_empty());
        }

        #[test]
        fn failed_resolve_still_consumes_the_quote() {
            let s = store(&[]);
            let r = render(&s, &format!("![[{NOPE}]]\n> some passage\n\nAfter."));
            assert_eq!(r.stats.unresolved[0].reason, UnresolvedReason::UnknownItem);
            assert!(!r.html.contains("some passage"));
            assert!(r.html.contains("<p>After.</p>"));
        }

        #[test]
        fn whitespace_collapsing_and_block_boundaries() {
            let s = store(&[
                (T, 1, TARGET),
                (U, 1, "Alpha line.\n\nBeta line.\n\nGamma line."),
            ]);
            let r = render(
                &s,
                &format!(
                    "![[{T}]]\n> Stigmergy is what a\n> protocol looks like from inside, and the reason\n> it looks like nothing at all is the point."
                ),
            );
            assert_eq!(exact(&r), Some(PARA_ONE));
            let r = render(&s, &format!("![[{U}]]\n> Alpha line.\n>\n> Beta line."));
            assert!(r.stats.unresolved.is_empty());
            let r = render(&s, &format!("![[{U}]]\n> Alpha line. Beta line."));
            assert_eq!(
                r.stats.unresolved[0].reason,
                UnresolvedReason::QuoteNotFound { version: 1 }
            );
        }

        #[test]
        fn empty_quote_is_refused() {
            let s = store(&[(T, 1, TARGET)]);
            let r = render(&s, &format!("![[{T}]]\n>\n>"));
            assert_eq!(r.stats.unresolved[0].reason, UnresolvedReason::EmptyQuote);
            assert_eq!(
                r.stats.unresolved[0].reason.to_string(),
                "the attached blockquote is empty"
            );
            assert!(
                r.html
                    .contains("unresolvable: the attached blockquote is empty")
            );
        }

        #[test]
        fn wire_selector() {
            let s = store(&[
                (T, 1, "Before it. The middle bit. After it."),
                (U, 1, TARGET),
            ]);
            let r = render(&s, &format!("![[{T}]]\n> The middle bit.\n\n![[{U}]]"));
            let q = &r.stats.transclusions;
            assert_eq!((q[0].id.as_str(), q[0].version), (T, 1));
            let sel = q[0].selector.as_ref().unwrap();
            assert_eq!(sel.exact, "The middle bit.");
            assert_eq!(sel.prefix.as_deref(), Some("Before it. "));
            assert_eq!(sel.suffix.as_deref(), Some(" After it."));
            // A whole quote still has no selector at all.
            assert_eq!(q[1].selector, None);
        }

        #[test]
        fn bake_is_escaped_plain_text() {
            let s = store(&[
                (T, 1, "the *thin* layer where coordination happens"),
                (U, 1, "a < b and c > d"),
            ]);
            let r = render(
                &s,
                &format!(
                    "![[{T}]]\n> the thin layer where coordination happens\n\n![[{U}]]\n> a < b and c > d"
                ),
            );
            assert!(
                r.html
                    .contains("<p>the thin layer where coordination happens</p>")
            );
            assert!(!r.html.contains("<em>thin</em>"));
            assert!(
                r.html.contains("<p>a &lt; b and c &gt; d</p>"),
                "{}",
                r.html
            );
        }

        #[test]
        fn data_attributes() {
            let mut s = store(&[(T, 3, TARGET)]);
            let r = render(&s, &format!("![[{T}]]\n> {PARA_ONE}"));
            assert!(r.html.starts_with(&format!(
                "<blockquote class=\"blyg-transclusion blyg-partial\" data-blyg-id=\"{T}\" data-blyg-version=\"3\">\n<p>"
            )), "{}", r.html);
            assert!(!r.html.contains("data-blyg-origin"));
            s.0.get_mut(T).unwrap().origin = Some("https://blyg.example.com/".into());
            let r = render(&s, &format!("![[{T}]]\n> {PARA_ONE}"));
            assert!(
                r.html
                    .contains("data-blyg-origin=\"https://blyg.example.com/\">")
            );
        }

        #[test]
        fn provenance_pairs_in_a_mixed_thread() {
            let s = store(&[
                (T, 1, "The whole of the first item."),
                (U, 1, TARGET),
                (V, 1, "The whole of the third item."),
            ]);
            let md = format!(
                "![[{T}]]\n\nOne.\n\n![[{NOPE}]]\n> gone\n\n![[{U}]]\n> {PARA_ONE}\n\nTwo.\n\n![[{V}]]\n\nThree."
            );
            let r = render_preview(&md, Kind::Thread, &s, &RenderOpts::default());
            let lines: Vec<&str> = r
                .html
                .match_indices("<p class=\"provenance\">")
                .map(|(i, _)| {
                    let rest = &r.html[i..];
                    &rest[..rest.find("</p>").unwrap()]
                })
                .collect();
            assert_eq!(lines.len(), 3, "{}", r.html);
            assert!(
                lines[0].contains(&format!("/blyg/f/{T}/")) && lines[0].contains("snapshot of v1")
            );
            assert!(
                lines[1].contains(&format!("/blyg/f/{U}/")) && lines[1].contains("excerpt of v1")
            );
            assert!(
                lines[2].contains(&format!("/blyg/f/{V}/")) && lines[2].contains("snapshot of v1")
            );
            // Line map: the partial quote sits on its directive's line and
            // the prose after the consumed run keeps its own.
            let partial = r.html.find("blyg-partial").unwrap();
            assert!(r.html[partial..].contains("data-line=\"7\">"));
            assert!(
                r.html.contains("<p data-line=\"10\">Two.</p>"),
                "{}",
                r.html
            );
        }
    }
}
