# blyg-render: the studio preview renderer

`crates/blyg-render` turns a working copy into the HTML a blyg publishes. It is
a Rust port of the reference Worker's studio preview pipeline. The full editor
(SPEC § "Full editor") shows this HTML in a WebView, so what the author sees is
what readers get.

## Pipeline

```
working copy ─┬─ parse TK scopes (tk.rs, as tk.ts parseScopes)
              ├─ preview strip: scope → output, or "⚠ ungenerated — <instruction>"
              ├─ annotate: block spans → one-line placeholder token (rendered on its own),
              │            inline spans → U+0002…U+0003 sentinels (Markdown parses across them)
              ├─ [[id]] links → U+0004 tokens, except in code (links.rs)
              ├─ fragment: Markdown                     thread: line walker (transclusion.rs)
              │                                           own-line ![[id]] → Resolver → blockquote,
              │                                             except in code or generated text;
              │                                             an attached `>` run → partial quote
              │                                           prose runs → Markdown
              ├─ splice generated blocks, sentinels → span.blyg-tk-gen, strip stray markers
              ├─ splice link anchors (or the unresolved marker) for the tokens
              └─ thread: inject the provenance line into each top-level quote
```

The sentinels are C0 controls (block token U+0001, inline U+0002/U+0003),
as in the Worker. They end a linkify match, an autolink and a link
destination, so they never land in an `href`. A block placeholder that
Markdown put inside code becomes the span's escaped text; sentinels inside
a tag (an image `alt`) are dropped; any leftover is stripped, so no marker
ships. Blocks are spliced in literally (a `$&` in generated text stays
`$&`).

The thread walker leaves a line as prose when Markdown renders it as code
(fenced or indented, at any depth: `markdown::code_lines`) or when it
overlaps a generated TK span. Inside a scope, `![[id]]` is a generation
source, never a quote.

**Partial quotes** (spec §16.4, ported from blygger-studio 0.8.3's
`transclusion.ts`). A `![[id]]` line followed directly (no blank line) by a
run of `>` lines quotes a passage instead of the whole item. The run ends at
the first line that is not a quote line, and it belongs to the directive
even when the target does not resolve. A blank line detaches it: that is a
whole quote followed by the author's own blockquote, as before. The
selection is the run's Markdown (each line stripped of `>` and one
whitespace character), rendered and flattened by `selection_text`: block
boundaries become `\n`, whitespace inside a block collapses, empty blocks
drop. It must be a substring of the `selection_text` of the target's
`content_html`. If it is, the bake is `blockquote.blyg-transclusion.blyg-partial`
(same `data-blyg-*` attributes) holding the selection's escaped plain text,
one `<p>` per line, and the quote's `Quote::selector` carries the wire's
`selector` (`exact`, plus up to 32 UTF-16 units of `prefix`/`suffix`).
Otherwise the unresolved marker shows "the attached blockquote is empty" or
"quoted passage not found in the target's version N". The provenance line
of a partial quote reads "excerpt of vN". Provenance injection matches the
`blyg-transclusion` class token (skipping `unresolved`), so a partial quote
keeps later lines paired with their quotes.

**`[[id]]` links** (protocol 0.3 §16.2) work in both kinds (`links.rs`, the
Worker's `resolveInternalLinks` / `previewInternalLinks` /
`applyInternalLinks`). A link is inline and bakes nothing: no
`transclusions[]` entry, no mention, no self or cycle check. It resolves
through the same `Resolver` as a quote (`Resolver::resolve_link`, which
defaults to `resolve`) and becomes `<a href="…">“excerpt”</a>`, where the
excerpt is the first 60 UTF-16 units of the target's text (else "a thread"
or "a fragment"). Your own items link to `{mount}/{f|t}/{id}/`, imported ones
to their origin's `page` or `{origin}{f|t}/{id}/`. An unresolved link shows
`span.blyg-link-unresolved` and joins `Stats::unresolved` after the quotes,
with `directive` `[[id]]`; publish refuses it ("one or more references do not
resolve"). A link Markdown renders inside `<code>` (span, fence or indented)
stays literal text; that is found by rendering a probe copy, as the Worker
does. So does one inside an autolink's URL. `native_blocks` turns links into `Span { item: Some(id), .. }` for
the stream to label; its grammar is looser (either case), like its
directive grammar.

A line map follows every rewrite (`linemap.rs`), so each top-level block can
carry `data-line="N"` (0-based source line) for source ↔ preview jumping.

## Markdown engine: markdown-it.rs, not comrak or pulldown-cmark

The Worker runs markdown-it 14 with `{ html: false, linkify: true,
typographer: false }` plus its embeds plugin. I rendered the first corpus
(56 plain-Markdown cases) through each candidate, stock:

| crate | matches the Worker | why it misses |
|---|---|---|
| `markdown-it` 0.6 (a port of markdown-it.js) | 42/56 | only linkify, the embeds plugin and alt text |
| `comrak` 0.55 | 34/56 | XHTML `<br />`, table `align=`, escaped HTML not wrapped in `<p>`, unsafe links become `href=""`, GFM autolink rules |
| `pulldown-cmark` 0.13 | 22/56 | no linkify at all, plus the same structural differences |

markdown-it.rs shares markdown-it's token model and renderer conventions, so
the remaining gaps were ones this crate has to fill anyway. These pieces are
replaced to match the JavaScript library exactly:

- **Link normalisation** (`normalizeLink`, `normalizeLinkText`): mdurl
  parse/format, punycode hosts (`punycode.rs`, same behaviour as punycode.js),
  mdurl encode and decode.
- **Linkify** (`linkify.rs`): a port of linkify-it 5, using its own regular
  expressions on `fancy-regex`, with both of markdown-it 14's rules (the
  inline `scheme://` rule and the core fuzzy-link and email rule).
- **Emphasis** (`emph.rs`): markdown-it.rs's `emph_pair` with CommonMark 0.31
  delimiter classification, where Unicode symbols count as punctuation.
- **Rendering details**: image `alt` text (`renderInlineAsText`, keeping
  escapes and entities as the Worker's `blyg_image_alt_text` rule does), line
  breaks around code blocks, list items and empty blockquotes, and no final
  `\n` in a fence left open at the end of a document without one.
- **Embeds**: the YouTube facade (`figure.blyg-yt`, byte-identical to
  embeds.ts) and `referrerpolicy="no-referrer"` on off-origin images.

Raw HTML is never passed through: `html: false` escapes it.

## API

```rust
pub fn render_preview(md: &str, kind: Kind, resolver: &dyn Resolver, opts: &RenderOpts) -> Rendered;
pub struct Rendered { pub html: String, pub line_map: Vec<(usize, usize)>, pub stats: Stats }
pub struct Stats { quotes, unresolved: Vec<Unresolved>, ai_spans, ungenerated, tk_errors, videos, images, transclusions }
pub struct RenderOpts { data_line: bool /* true */, provenance: bool /* true */, mount: String /* "/blyg" */, self_id: Option<String> }

pub trait Resolver { fn resolve(&self, id: &str) -> Resolution; }
pub enum Resolution { Found(Found), NotFound, Ambiguous, RssNotQuotable, ReservedVersion, Unavailable(UnresolvedReason) }
pub struct Found { origin: Option<String>, id, version: u32, kind: ItemKind, content_html, author: Option<String>, page: Option<String> }

pub struct Quote { id, version: u32, origin: Option<String>, line: usize, selector: Option<TextQuoteSelector> }
pub struct TextQuoteSelector { exact: String, prefix: Option<String>, suffix: Option<String> }
pub fn selection_text(html: &str) -> String;           // markdown.ts selectionText (§16.4 normalizer)
pub fn normalize_selection(text: &str) -> String;      // markdown.ts normalizeSelection
pub fn selection_from_quote(quote_md: &str) -> String; // transclusion.ts selectionFromQuote
pub fn locate_selection(target_html: &str, selection: &str) -> Option<TextQuoteSelector>;

pub fn render_markdown(md: &str) -> String;           // the Worker's renderMarkdown
pub fn studio_css() -> String;                         // tint, unresolved marker, facade styles
pub fn article_html(kind, content_html, stub, fork) -> String;  // pages.ts <article> + citations
pub struct Attachment { key: String /* media r2_key */, alt: Option<String> }
pub fn media_html(&[Attachment], mount) -> String;     // pages.ts mediaHtml (public pages, after the content)
pub fn preview_media(&[Attachment], mount) -> String;  // attachments.ts previewMedia: div#preview-media
pub fn page_shell(theme_css: &str, body: &str) -> String;       // complete document with CSP
pub fn page_shell_with(theme_css, body, &ShellOpts { title, base_href }) -> String;
pub fn preview_script() -> String;  pub fn embed_css() -> &'static str;  pub fn csp(nonce) -> String;
```

- **Resolver.** It resolves from the local store only: your published items
  first, then imported blyg items (the Worker's `resolveTarget` order).
  `Found.origin` is `None` for your own items. `author` is the source blyg's
  display name, which the provenance line prints as "from *name*". The
  renderer itself flags a thread quoting itself (`self_id`) and `@vN`
  directives. Circular quotes need the store's closure, so the resolver
  reports them as `Unavailable(Circular)`.
- **Unresolved quotes.** The marker shows the Worker's exact reason string
  (`Display`). `UnresolvedReason::human()` gives a plain sentence for the
  status bar.
- **Line map.** `line_map` lists `(source_line, block_index)` for each
  `[data-line]` element, in document order. Nested blocks carry no line.
- **Page shell.** Its CSP is `default-src 'none'`, images from anywhere,
  `frame-src` limited to youtube-nocookie.com, and `script-src` limited to one
  nonce, used only by our own script (click-to-play and the failed-image
  link). Content has no nonce, so no script from content runs: inline
  handlers and `javascript:` URLs are blocked too. Theme CSS cannot close its
  `<style>` early. Links opening in the default browser is the host WebView's
  job.
- **Attachments.** Public pages append every attached image after
  `.item-content`, and the Worker's studio preview shows them in
  `div#preview-media` right after the preview body. They are live rows, not
  part of the Markdown, so the host renders them with `preview_media` next to
  `Rendered.html`.

## Parity

`tests/fixtures/gen_parity.mjs` bundles the Worker's own `markdown.ts`,
`embeds.ts`, `tk.ts`, `transclusion.ts`, `pages.ts` and `attachments.ts` with
the Worker's esbuild. It runs them over neutral inputs and writes the expected
HTML to `tests/fixtures/parity/`. For threads it serves `fake_store.json` as a
fake D1, and `tests/common` implements the same store as a Rust `Resolver`.
The fixtures are committed, so CI never needs the Worker.

```sh
# after editing corpus.json or media_input.json
# (the Worker package needs `npm install` first)
node crates/blyg-render/tests/fixtures/gen_parity.mjs /path/to/worker/package
cargo test -p blyg-render
```

**Reference version.** The fixtures come from the reference Worker rebased
onto upstream blygger-studio v0.8.3, with local patches 1–12 re-applied
(2026-09-29). Regenerating against v0.8.3 changed one earlier fixture,
`tr_provenance_order`, which is now a known divergence (below), and 11
`tr_partial_*` cases were added for partial quotes. Before that, v0.7.0
(protocol 0.3, Worker version `3a25a1bd`, 2026-09-28) changed none of the 161
earlier fixtures and added 15 `link_*` cases for `[[id]]`.
Before that, the fixtures came from the reference Worker with its
local patch 9 applied (commit `fcc0c4425`, "`$`-safe splicing, no marker
leaks, code/generated text never transcludes, alt keeps escapes"), on
markdown-it 14.3.0 and linkify-it 5.0.2 (`parity/_manifest.json`). Patch 9
fixed the five Worker bugs this crate used to reproduce. Their fixtures are
kept and now pin the fixed behaviour: `tk_block_dollar`, `tk_sentinel_in_url`,
`tk_indented_block`, `tr_in_code_fence`, `tr_inline_tk_multiline_directive`
and `images_edge`, plus 19 new cases around them (`tk_*dollar*`,
`tk_sentinel_*`, `tk_block_in_fence*`, `tr_in_*_fence`, `tr_*tk_directive*`,
`images_alt_escapes*`). Four more (`code_*fence_eof*`) pin a fence left open
at the end of the document, an engine gap one of those cases exposed. Patch 8 (studio attachments) shows attached images in
the studio preview through the public pages' `mediaHtml`; the `_media.json`
suite pins `media_html` and `preview_media` against it.

Results. Tests compare after collapsing whitespace between tags, but every
case is also byte-identical:

| suite | cases | normalised | byte-identical |
|---|---|---|---|
| corpus (paragraphs, emphasis, links, linkify edges, headings, lists, code, quotes, images, raw HTML, YouTube, TK, transclusion, partial quotes, `[[id]]` links) | 191 | 190 + 1 known divergence | 190 |
| CommonMark 0.31.2 spec examples | 652 | 652 | 652 |
| linkify-it + markdown-it linkify test vectors | 206 | 206 | 206 |
| attachments (`mediaHtml`, `previewMedia`) | 3 | 3 | 3 |

**Partial quotes** match the v0.8.3 Worker byte for byte in all 11
`tr_partial_*` cases: found, emphasis in the quote, paragraph breaks, text
inside a nested quote, not found, empty, detached by a blank line, a remote
target, an unknown target (the run is still consumed), partial then whole
(provenance pairing), and loose `>` markers. Unit tests ported from the
studio's `selection.test.ts` and `partial-transclusion.test.ts` cover the
normalizer and selector context as well.

**Known divergence: `tr_provenance_order`.** v0.8.3's provenance pairing
matches the `blyg-transclusion` class token and so counts the studio
preview's unresolved markers too. When an unresolved quote comes before
resolved ones, every provenance line lands one quote early: the marker gets
the next quote's line, and the last quote gets none. v0.7 matched the literal
`class="blyg-transclusion"` and skipped markers. This crate skips `unresolved`
tags, which is the v0.7 behaviour and the correct one. The bug is preview-only,
since publishing refuses unresolved quotes. Remove the entry when the Worker
fixes it; the test fails once the fixture passes.

Every fixture also checks the unresolved directives and reasons, in order,
and the TK error count against the Worker; thread fixtures also check the
resolved quote ids. `tests/preview.rs` adds checks that
need no Worker:

- Turning `data-line` on changes nothing but the attributes, for every
  fixture and for 1,500 fuzzed documents built from the grammar's sharp
  edges. The fuzz run also checks that nothing panics and that no TK marker
  reaches the output.
- The stats, the self-quote check, CRLF handling, the page shell, and that
  content can never inject markup.

### Worker patch 12 (and what is still pinned as-is)

Two Worker bugs these fixtures exposed were fixed in local patch 12 (filed
upstream as blygger-studio #13 and #14), and the fixtures follow it:

- A `[[id]]` inside link text (`[see [[id]]](url)`) splices only the anchor's
  text, never a nested `<a>`. Inside a CommonMark autolink
  (`<https://x.test/[[id]]>`) it stays literal, part of the URL, and is no
  error (`link_in_attrs`, `link_autolink_literal`,
  `link_in_link_text_unresolved`).
- The preview resolves `[[id]]` in a block TK scope's output as publish does,
  over the block's rendered HTML; an unresolved one is marked and reported
  with the block's source line (`link_in_tk*`, `link_tk_block_*`).

Still as the Worker has it: a `[[id]]` glued to a bare URL
(`https://x.test/[[id]]`) ends the linkified URL and follows it as its own
anchor; inside block TK output, linkify splits it between the two `]` and the
id stays literal (`link_tk_block_nested`).

### Deliberate differences (outside the fixtures' reach, or safer)

- **Provenance `href` and attachment `src`.** They are HTML-escaped; the
  Worker still interpolates them raw (patch 9 did not change this). They
  differ only if an origin, `page`, mount or media key contains `& < > " '`.
- **Line endings.** CRLF and CR are normalised to LF before rendering. The
  Worker receives LF from browsers.
- **Remote provenance label.** It uses the origin's host, lower-cased with the
  default port dropped, where the Worker uses `new URL(origin).host`. That
  also punycodes the host, which this crate does not.
- **Citation dates.** They use the ISO date part, where the Worker uses
  `toLocaleDateString` in the server's time zone.
- **Preview script.** It gives the iframe a `referrerpolicy`, since YouTube
  embeds need a referrer. It resolves image URLs against `document.baseURI`,
  so the fallback works on a non-http preview origin. It also re-checks
  images after DOM patches.
- **Unicode data.** `\p{P}`, `\p{S}`, `\p{Z}` come from Rust's Unicode tables,
  not uc.micro's. linkify-it's length caps count UTF-16 units in JavaScript
  and code points here. Both matter only at the edges: newly assigned code
  points, or astral-plane labels near 63 characters.

## Performance

`tests/bench.rs` renders a 20,400-character mixed thread. It has quotes,
unresolved quotes, TK, links, fuzzy links, lists, tables, code and videos.

- Release: 4.2 ms median, 4.6 ms worst, against a 10 ms budget. Keeping
  code lines from transcluding costs about 0.3 ms: a thread containing
  `![[` gets one extra block-only parse (`markdown::code_lines`).
- Debug: about 20 ms.

A second test runs 11 adversarial inputs aimed at the linkify regexes'
backtracking. Each takes 1–10 ms in release.

```sh
cargo test -p blyg-render --release --test bench -- --nocapture
```
