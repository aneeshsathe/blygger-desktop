# blyg-render: the studio preview renderer

`crates/blyg-render` turns a working copy into the HTML a blyg publishes. It is
a Rust port of the reference Worker's studio preview pipeline. The full editor
(SPEC § "Full editor") shows this HTML in a WebView, so what the author sees is
what readers get.

## Pipeline

```
working copy ─┬─ parse TK scopes (tk.rs, as tk.ts parseScopes), except inside code
              ├─ preview strip: scope → output, or "⚠ ungenerated — <instruction>"
              ├─ annotate: block spans → one-line placeholder token (rendered on its own),
              │            except, in a thread, an own-line ![[id]] in the output stays a
              │            directive between tokens (decision #60);
              │            inline spans → U+E001…U+E002 sentinels (Markdown parses across them)
              ├─ [[id]] / [[id | label]] links → U+E003 tokens, except in code (links.rs)
              ├─ fragment: Markdown                     thread: line walker (transclusion.rs)
              │                                           own-line ![[id]] (label ignored) → Resolver → blockquote,
              │                                             except in code;
              │                                             an attached `>` run → partial quote
              │                                           prose runs → Markdown
              │                                         thread: sanitize (sanitize.rs, lol-html)
              ├─ splice generated blocks, sentinels → span.blyg-tk-gen
              ├─ splice link anchors (or the unresolved marker) for the tokens
              └─ thread: inject the provenance line into each top-level quote
```

The sentinels are Private Use Area characters, as in the Worker since
studio 0.32: block token U+E000, inline U+E001/U+E002, link token U+E003.
Markdown treats them as ordinary text, so linkify, an autolink or a link
destination can take one into a URL. There the percent-encoded marker is
dropped from the `href` (a TK span inside a linkified URL still wraps the
link text, so the Worker emits `<span><a>…</span></a>`; this crate does
too). Sentinels inside a tag (an image `alt`) are dropped. Blocks are
spliced in literally (a `$&` in generated text stays `$&`).

"Code" is the Worker's `codeRanges` (`markdown::code_ranges`): code blocks
(fenced or indented, at any depth) as markdown-it's block parser maps them,
plus code spans (a run of N backticks to the next run of exactly N, never
across a blank line). Inside code, `[TK]`, `[=]`, `[/TK]`, `![[id]]` and
`[[id]]` are inert text (studio#3, #4; since studio 0.41.1, #68, a `[/TK]`
or `[TK]` quoted in code *inside* a scope no longer closes or nests it:
`tk_code_*`). The thread walker leaves a line as prose when its start lies
in code.

**Directives in TK output** (decision #60, ruled 2026-10-06). A scope's
sources are the `![[id]]`s in its instruction only. Its output is content:
once publish consumes the scope, an own-line `![[id]]` left there is an
ordinary transclusion directive, which bakes a quote and notifies the
quoted blyg. A multi-line inline scope holding one splits its `span` around
the quote, as the Worker's does (`tr_inline_tk_multiline_directive`). A
block scope is rendered on its own, so here the preview splits its output
at each such line (outside code, with any attached `>` run): the prose on
either side becomes its own `div.blyg-tk-gen`, the directive stays in the
document for the thread walk, and whitespace-only prose is dropped. That
is what the decision says publish produces. The studio's preview does not
do it (it shows the directive as text inside the block, and so does its
publish for a scope with recorded provenance, an upstream inconsistency),
so four fixtures are known divergences (see "Parity"). Fragments never
quote, so there the directive stays text.

`unrequested_output_directives(md)` (tk.ts `unrequestedOutputDirectives`)
lists the ids such a line names when the scope's instruction does not,
skipping `impyrt` and ungenerated scopes. The app warns in the publish
sheet before publishing a thread that has any; the studio's publish
returns the same warning afterwards. Unlike the studio's, it skips a line
inside code, which never bakes.

A scope is a **block** (its output, or the ungenerated marker, renders as a
`div` of its own) when a blank line, or the document's edge, sits on both
sides of it; otherwise it is an inline `span`. Since studio 0.36 the edge
counts even with whitespace or a single newline between it and the scope,
since editors commonly end a post with `\n` (tk.ts `BLANK_BEFORE` /
`BLANK_AFTER`; the `tk_doc_edge_*` cases).

**`impyrt` scopes** (decision #37): `[TK]impyrt=<pasted text>[/TK]` or
`[TK]impyrt <model>=…[/TK]` (also with `[=]`; the keyword is
case-insensitive) mark text generated elsewhere. The instruction reads
`impyrt`, the output is the text after the first `=`, `TkScope::imported`
carries the model, and no sources are declared.

**Sanitizing** (studio 0.32). The Worker's thread preview runs its
allowlist sanitizer (`importer/sanitize.ts`) over the walked HTML before the
TK and link splices; its public pages run it over a thread's baked HTML, so
this is what readers see. `sanitize.rs` ports it on lol-html, the library
HTMLRewriter is built on, so untouched markup keeps its bytes. Listed tags
keep listed attributes (`alt class title width height colspan rowspan
scope datetime open dir lang`), `data-blyg-*`, an `<a href>` that is
http(s), mailto or tel, and an `<img src>` that is http(s). Scripts,
styles, iframes, SVG, forms and the like go with their content; other
unknown tags are unwrapped. So a thread preview loses table alignment,
`<ol start>` and `loading` on images. Fragments are not sanitized, in the
Worker or here.

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

**The bracket grammar** (`grammar.rs`, the studio's `grammar.ts`, protocol
0.4 §10.1, studio 0.47). Every site that reads `![[id]]` or `[[id]]` goes
through one scanner, so a label is honoured everywhere or nowhere. Since
studio 0.47 (decision #70) either form may carry a **label** after the id:
`[[id | text]]`, `![[id | text]]`. The id is positional (the 26 characters
right after the brackets), so the first `|` after it separates, with
optional spaces or tabs before it; everything from there to the **first**
`]]` on the same line is the raw label, pipes and single `]` included. The
label is trimmed (JavaScript `trim`), and an empty one is no label. There is
no escape: `[[id | cut]] here]]` is the label `cut` followed by text, and a
label never spans lines. A label-first `[[text | id]]`, a short or long id,
an uppercase id, `[[id ]]` (space, no pipe) and a label with no closing
`]]` on its line are not grammar at all and stay literal text. On a link
the label is the anchor text (below). On a directive it is a note for the
author: resolution, the bake, the provenance line and `Quote` ignore it,
and a labelled own-line directive is a whole or partial quote exactly as an
unlabelled one. `![[id@vN | x]]` is not the reserved form (that has no
label) and is prose. A `[TK]` instruction's `![[id | label]]` is a source
by its id. The unresolved marker's `directive` is the trimmed line or the
link's literal, label included, as the studio reports it.

**`[[id]]` links** (protocol 0.3 §16.2) work in both kinds (`links.rs`, the
Worker's `resolveInternalLinks` / `previewInternalLinks` /
`applyInternalLinks`). A link is inline and bakes nothing: no
`transclusions[]` entry, no mention, no self or cycle check. It resolves
through the same `Resolver` as a quote (`Resolver::resolve_link`, which
defaults to `resolve`) and becomes `<a href="…">label</a>`
(transclusion.ts `anchorText`, `anchor_label`). An author's label
(`[[id | text]]`, studio 0.47) is the anchor text, HTML-escaped, in place of
the target's; an empty label falls back to it. Otherwise (link-label.ts
`linkLabel`, `link_label`), since studio 0.36 the text
is the target's heading, as plain text without quotes (80 UTF-16 units),
when its `content_html` opens with an `<h1>`–`<h6>`; otherwise it is
`“excerpt”`, the first 60 UTF-16 units of the target's text, else "a
thread" or "a fragment". Like everything rendered, it is frozen at publish.
The Reader labels `[[id]]` links in a held post the same way
(`stream_vm::link_label`). Your own items link to `{mount}/{f|t}/{id}/`,
imported ones to their origin's `page` or `{origin}{f|t}/{id}/`
(importer/util.ts `blygItemUrl`, `util::blyg_item_url`, which the
provenance line uses too). A relative `page` is origin-relative, a leading
`/` included; since studio 0.36 an absolute http(s) `page` is kept as it is
(templated blygs, §16.6e). An unresolved link shows
`span.blyg-link-unresolved` and joins `Stats::unresolved` after the quotes,
with `directive` `[[id]]`; publish refuses it ("one or more references do not
resolve"). A link in code (`codeRanges`) is literal text and never
resolved. A label is text, not Markdown: once a live link's label holds a
backtick, the label is blanked and code found again, left to right, so a
backtick in a label never opens a code span that makes a later link inert,
while a code span opened before a link still wins (studio 0.48). A generated
block's links are resolved over the block's own Markdown, as the body's
are, and the block is rendered again from the result (`resolveBlockLinks`,
studio 0.48), so a label is escaped once wherever it sits. Every
other link is resolved and reported, then spliced only where an anchor can
go (`applyInternalLinks`, studio#13): inside a tag (an image's alt) or a
URL it is the author's literal `[[id]]` (percent-encoded in an `href`);
inside another link's text it is the anchor's text, or the literal when
that link's URL carries it (an autolink). The literal is the whole
`[[id | label]]`; in an `href` it is `encodeURI`'d (`%20`, `%7C`, UTF-8)
and then HTML-escaped, so a `&` in it is `&amp;` (studio 0.48). `native_blocks` turns links into `Span { item: Some(id), .. }` for
the stream to label; a labelled link also sets `Span::label` (and `text`
to it), which the caller keeps instead of its own label. Its ids are read
looser (either case), like its directive grammar; labels are read as the
grammar reads them.

A line map follows every rewrite (`linemap.rs`), so each top-level block can
carry `data-line="N"` (0-based source line) for source ↔ preview jumping.

## Markdown engine: markdown-it.rs, not comrak or pulldown-cmark

The Worker runs markdown-it 14 with `{ html: false, linkify: true,
typographer: false }` (and, until the stock studio replaced it in 0.40, an
embeds plugin). I rendered the first corpus
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
- **Rendering details**: image `alt` text (the Worker's `altText`, studio#6:
  text, escapes, entities and inline code), line
  breaks around code blocks, list items and empty blockquotes, and no final
  `\n` in a fence left open at the end of a document without one.
- **Embeds**: none in the HTML. Stock studio (0.40, embeds.ts) keeps
  embeds out of `content_html` and upgrades a bare YouTube link to the
  click-to-play facade with a page script; the preview's own script
  (`preview_script`) does the same, so the preview looks as before. A
  bare video link still counts in `Stats::videos`. (The facade and
  `referrerpolicy` used to be baked, emulating the maintainer's patched
  Worker; both are gone.)

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

// The bracket grammar (grammar.ts, link-label.ts; studio 0.48)
pub struct LinkRef { range: Range<usize> /* bytes, brackets included */, id: String, label: Option<String> }
pub struct DirectiveRef { id: String, label: Option<String> }
pub fn link_refs(text: &str) -> Vec<LinkRef>;               // matchAll(linkInline()) + label(); code not skipped
pub fn directive_ref(line: &str) -> Option<DirectiveRef>;   // DIRECTIVE_LINE + label()
pub fn label_text(text: &str) -> String;                    // labelText: one line, trimmed, a `]` before `]` or the end → "] "
pub fn link_source(id: &str, label: Option<&str>) -> String; // linkSource: [[id]] or [[id | label]]
pub fn link_label(content_html: &str, kind: ItemKind) -> String; // linkLabel: an unlabelled link's text
pub fn strip_generated_grammar(text: &str) -> String;       // stripGeneratedGrammar (row 67)
pub fn unrequested_output_directives(md: &str) -> Vec<String>; // tk.ts unrequestedOutputDirectives (#60)
pub fn studio_css() -> String;                         // tint, unresolved marker, facade styles
pub fn article_html(kind, content_html, stub, fork) -> String;  // pages.ts <article> + citations
pub struct Attachment { key: String /* media r2_key */, alt: Option<String>, inline: bool }
pub fn media_html(&[Attachment], mount, content_html) -> String;     // pages.ts mediaHtml: what the content doesn't show
pub fn preview_media(&[Attachment], mount, content_html) -> String;  // media_html in div#preview-media
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
- **Attachments.** Public pages append, after `.item-content`, each attached
  image the content does not already show (studio#24: not `inline`, and its
  key not in `content_html`). They are live rows, not part of the Markdown,
  so the host renders them with `preview_media` next to `Rendered.html`.

## Parity

`tests/fixtures/gen_parity.mjs` bundles `tests/fixtures/gen_parity_worker.js`
with the Worker's own `markdown.ts`, `embeds.ts`, `tk.ts`, `authoring.ts`,
`transclusion.ts`, `importer/sanitize.ts`, `pages.ts`, `grammar.ts` and
`link-label.ts`, using the Worker's
esbuild, and runs the bundle in workerd through the Worker's own Miniflare
(the sanitizer is built on workerd's HTMLRewriter, which Node lacks). The
harness is the body of `read-api.ts`'s `POST /api/preview` handler and
`threadPreview()`, called with a fake D1 that serves `fake_store.json`;
`tests/common` implements the same store as a Rust `Resolver`. It writes the
expected HTML to `tests/fixtures/parity/`, and runs the vectors in
`grammar_input.json` through `grammar.ts` and `link-label.ts` into
`parity/_grammar.json`. The fixtures are committed, so CI
never needs the Worker. Nothing in the Worker directory is modified, and
nothing is written unless every case rendered.

```sh
# after editing corpus.json, fake_store.json, media_input.json or
# grammar_input.json, or when
# the studio changes: clone stock blygger-studio into a scratch directory,
# check out the release tag and `npm ci` there, then
node crates/blyg-render/tests/fixtures/gen_parity.mjs /path/to/studio/clone
cargo test -p blyg-render
```

**Reference version.** The fixtures come from stock blygger-studio
0.48.0 (tag `v0.48.0`, commit 320fe2e, unpatched), re-generated 2026-10-19
(`parity/_manifest.json` records the studio, markdown-it and linkify-it
versions). Earlier fixtures came from the maintainer's patched Worker; see
"Studio 0.46.1" below for what changed then, "Studio 0.47.0" for the
labels, and "Studio 0.48.0" for the label fixes. The Worker's preview helpers moved again in 0.32
(`previewLinkDocs`/`spliceLinkDocs` became `previewInternalLinks`,
`resolveBlockLinks` and `applyInternalLinks`), so the generator now calls the
handler's own sequence instead of a copy of it.

Against the previous fixtures (studio 0.11 with local patches), 18 of 191
corpus cases changed and this crate followed every one:

- TK sentinels are U+E000–E002 and the link token U+E003, not C0 controls,
  so a marker can sit inside a link destination, an autolink or a linkified
  URL (`tk_sentinel_*`).
- `[TK]` inside code is not a scope (`tk_in_code_span`, `tk_indented_block*`,
  `tk_block_in_fence*`, `tk_inline_in_indented_code`).
- A directive on its own line in TK output is a quote
  (`tr_inline_tk_multiline_directive`, `tr_ungenerated_tk_with_directive`).
- `[[id]]` is resolved everywhere outside code and spliced as text, literal
  or anchor by where it landed (`link_in_attrs`, `link_autolink_literal`,
  `link_in_link_text_unresolved`).
- Image alt text keeps inline code (`image_alt_markup`,
  `images_alt_escapes_link`).
- The thread preview is sanitized (`mixed_document`).

The patch 9 and patch 12 behaviours this crate used to reproduce are gone
where upstream fixed the same bugs differently; nothing emulates a Worker
bug that the Worker no longer has. 11 corpus cases were added for `impyrt`
scopes, the sanitizer (a hostile quoted target, Markdown that loses
attributes, a sanitized TK block) and TK or links in code, plus a media
case for placed and `inline` attachments.

Results. Tests compare after collapsing whitespace between tags, but every
matching case is also byte-identical. The four known divergences are the
block-scope directive cases of decision #60 (`tr_in_tk_block`,
`tr_block_tk_directive_output`, `tr_block_tk_directive_split`,
`tr_block_tk_directive_attached`); `tests/preview.rs` pins this crate's
output for them. The split shape (generated | quote | generated, and how
`generated[]` pairs with the pieces) is not ruled yet, so these four are
provisional: revisit them when upstream fixes its preview and publish.

| suite | cases | normalised | byte-identical |
|---|---|---|---|
| corpus (paragraphs, emphasis, links, linkify edges, headings, lists, code, quotes, images, raw HTML, YouTube, TK, `impyrt`, transclusion, partial quotes, `[[id]]` links, labels, sanitizing) | 257 | 253 + 4 known | 253 |
| CommonMark 0.31.2 spec examples | 652 | 652 | 652 |
| linkify-it + markdown-it linkify test vectors | 206 | 206 | 206 |
| attachments (`mediaHtml`) | 4 | 4 | 4 |
| bracket grammar (links, directive, `stripGeneratedGrammar`; `labelText`/`linkSource`; `linkLabel`; `withStubQuote`; `unrequestedOutputDirectives`) | 49 + 19 + 5 + 5 + 9 | exact | exact |

**Studio 0.48.0** (re-synced 2026-10-19 from 0.47.0). The release fixes
the six label-grammar bugs that 0.47.0 had and this crate pinned
(upstream #78: fix d482b02, regressions in
`test/label-grammar-edge-cases.test.ts`). This crate now does what the
studio does, and no longer emulates any of them:

- A block TK scope's links are resolved over the block's Markdown and the
  block rendered again (`resolveBlockLinks` takes `blockSources`), never
  over its HTML, so a label is escaped once, as in the body and inline
  scopes. `htmlCodeRanges` is gone upstream, and `html_code_ranges` here.
- A backtick in a live link's label no longer opens a code span that makes
  a later link inert: labels are blanked and code found again, left to
  right, so a code span opened before a link still wins.
- `labelText` puts a space after every `]` followed by `]` or by the end,
  so `a]]]b` is `a] ] ]b` and `x]` is `x] `: `linkSource` round-trips
  any run of `]`.
- In an autolinked URL the restored literal is `escapeHtml(encodeURI(…))`,
  so the `href` carries `&amp;`, never a raw `&`.
- `withStubQuote` keeps the directive line as written, label included.
- `directiveOwnLine` takes `[^\S\n]` at both ends, as `DIRECTIVE_LINE`
  does, so an NBSP-, FF- or VT-led own-line directive in TK output is
  warned about. This crate's `unrequested_output_directives` already used
  the directive-line rule, so nothing changed here; the new vectors check it.

Four existing fixtures changed, all from those fixes, and this crate
follows each: `label_link_tk_block` (the block labels are escaped once:
`a &lt;b&gt; &amp; label`, `*em*` literal), `label_link_backtick_in_label`
(the later link resolves), `label_link_unicode` (`&amp;` in the autolink's
`href`) and `link_tk_block_nested` (a linkified URL in a block scope now
sees the link token in the block's Markdown, not the closing `]]` already
in its HTML, so linkify takes the whole `[[id]]` into the URL, as in the
body). Seven corpus cases were added (`label_block_tk_escape_once`, its
thread form, `label_block_tk_unresolved`, `label_backtick_later_link`,
`label_backtick_code_before_wins`, `label_backtick_two_labels`,
`label_autolink_amp`), with eight `labelText` vectors (runs of `]`, a
trailing `]`), and two new grammar suites run through the studio's own
functions: `withStubQuote` (5) and `unrequestedOutputDirectives` (9: NBSP,
FF, VT, CRLF, em and ideographic spaces, a requested source and an inline
directive). All match.

Nothing else in the render path changed. Between the tags `markdown.ts`,
`embeds.ts`, `importer/sanitize.ts` and `link-label.ts` are unchanged;
`authoring.ts`, `tk.ts`, `transclusion.ts`, `grammar.ts` and
`code-ranges.ts` changed only for the fixes; `pages.ts` adds a public
page's "Local mentions" list (page chrome, never in a document) and reads a
version's own attachments (`listVersionMedia`), neither of which the
preview or `media_html` sees.

**Studio 0.32.2** (upstream #60) takes a scope's sources from its
instruction only: a `![[id]]` that only the output names is not a source.
Both `blyg-render`'s `parse_scopes` and `blyg-core`'s `tk::parse` follow it,
and the regenerated fixtures (two `tr_inline_tk_*` cases changed) match.

**Studio 0.47.0** (re-synced 2026-10-18 from 0.46.1). The release adds
labels (`[[id | text]]`, `![[id | text]]`, upstream #70) and has generated
text drop the bracket grammar (ROADMAP row 67). Every `![[id]]`/`[[id]]`
pattern in the studio now comes from `grammar.ts`, and `anchorText` moved
to `link-label.ts`; `markdown.ts` and `pages.ts` changed only in
comments, and `authoring.ts`, `embeds.ts` and the sanitizer not at all. Against 0.47.0 the 219 existing cases
were unchanged and still matched. This crate's link, directive, source-ref,
stub-quote and native-model patterns now share one scanner (`grammar.rs`),
and 31 corpus cases were added (`label_*`): every label case from the
studio's `labels.test.ts` (forms, the first-`]]` cut, several per line,
not-ids, never a directive, a labelled directive line only whole, escaping,
the empty-label fallback, inert in code, a labelled directive quoting whole
and partial), plus a label in an image alt, link text, a linkified or
autolinked URL (`encodeURI`), non-ASCII, Markdown inside a label (literal),
a backtick inside a label, TK inline and block output, a labelled source
ref, a labelled own-line directive in TK output, unresolved and remote
targets, a self-quote and malformed directives. All 31 match byte for byte.
The new grammar vectors (`parity/_grammar.json`: the `labels.test.ts` and
`tk-generate.test.ts` strings plus edges) check `link_refs`,
`directive_ref`, `strip_generated_grammar`, `label_text`, `link_source` and
`link_label` against the studio's functions; all match.

`strip_generated_grammar` is not part of rendering: the studio applies it
to a model's output in `runGenerateScope` before saving the scope, so the
preview of an existing working copy is unchanged. Burrow's generation path
should apply it to the provider's text before writing `[=]…[/TK]`.

**Studio 0.46.1** (re-synced 2026-10-13 from 0.36.1). Between the two,
`markdown.ts` is unchanged; `tk.ts`, `authoring.ts`, `transclusion.ts`
and `pages.ts` changed, and `embeds.ts` became presentation only. Against
stock 0.46.1 the old fixtures matched 197 of 212: all 15 misses were the
baked YouTube facade and image `referrerpolicy`, which came from the
patched Worker, not the studio. This crate no longer bakes either, and
matches all 212. Seven cases were added:

- `[TK]`, `[=]` and `[/TK]` quoted in code inside a scope are text
  (0.41.1, #68: `tk_code_close_in_scope`, `tk_code_open_in_scope`,
  `tk_code_eq_in_scope`, `tk_code_fence_in_scope`). `parse_scopes` now
  looks for every token outside code, not only the opening one.
- Own-line directives in block TK output (decision #60:
  `tr_block_tk_directive_split`, `_attached`, `_in_code`); the first two
  are known divergences, as above.

Remote generation sources (0.41, `GenerationSource {id, version, origin?,
cited?}`) and the preview's per-scope `sources` live in the studio's
authoring API, not in the rendered HTML, so nothing here changed for them;
`blyg-core` owns the wire side.

**Studio 0.36.1** changed three things the preview shows, and this crate
follows each. None of the 202 existing cases changed; 10 were added, with
five local items and one imported item in `fake_store.json`:

- An `[[id]]` anchor is labelled with the target's opening heading
  (`link_heading_label`: a heading with markup and an entity, an empty
  heading, a heading after a paragraph, one over 80 units, and `<header>`,
  which is not a heading; `link_heading_label_thread`).
- A scope at the document's edge is a block across whitespace or one
  newline (`tk_doc_edge_*`: a trailing or leading newline, stray
  whitespace, an ungenerated scope, a scope one newline from text, which
  stays inline, and a thread).
- An imported item's absolute `page` is its link and its provenance `href`
  as it is (`link_imported_absolute_page`, `link_heading_label_thread`).

**Partial quotes** match the Worker byte for byte in all 11 `tr_partial_*`
cases: found, emphasis in the quote, paragraph breaks, text inside a nested
quote, not found, empty, detached by a blank line, a remote target, an
unknown target (the run is still consumed), partial then whole (provenance
pairing), and loose `>` markers. Unit tests ported from the studio's
`selection.test.ts` and `partial-transclusion.test.ts` cover the normalizer
and selector context as well.

**Provenance on the preview is the desktop's own combination.** The Worker's
previews (`POST /api/preview`, the thread editor) show no provenance.
`injectProvenance` only ever runs on published HTML, which can't hold an
unresolved marker. The generator composes the preview with the page's
`injectProvenance`, as this crate does, and hides unresolved markers from it,
as they would be absent on the page.

Every fixture also checks the unresolved directives and reasons, in order,
and the TK error count against the Worker; thread fixtures also check the
resolved quote ids. `tests/preview.rs` adds checks that need no Worker:

- Turning `data-line` on changes nothing but the attributes, for every
  fixture and for 1,500 fuzzed documents built from the grammar's sharp
  edges. The fuzz run also checks that nothing panics and that no TK marker
  reaches the output.
- The stats, the self-quote check, CRLF handling, the page shell, and that
  content can never inject markup.

### Worker behaviour pinned as it is

These look like upstream bugs, and the fixtures pin them because readers get
them too:

- A TK span around a URL that linkify takes closes inside the link:
  `<span class="blyg-tk-gen"><a …>…</span></a>` (`tk_sentinel_in_url`).
- A marker glued to an email or `www.` host stops linkify from taking it
  (`tk_sentinel_after_email`).
- A multi-line inline scope holding an own-line `![[id]]` puts the quote
  inside the scope's `span`, across paragraphs
  (`tr_inline_tk_multiline_directive`, `label_tk_output_directive`).

The six label-grammar bugs pinned here for 0.47.0 are fixed upstream in
0.48.0 and no longer emulated (see "Studio 0.48.0").

### Deliberate differences (outside the fixtures' reach, or safer)

- **Leftover markers.** A block token Markdown did not leave in a paragraph
  of its own becomes the span's escaped text, and any other U+E000–E002 is
  stripped, so no marker ships. The Worker leaves them in; this also strips
  those characters when an author types them.
- **`data-line`.** The sanitizer keeps it; it is the desktop preview's own
  attribute, and the Worker's preview has none.
- **Attachment `src`.** It is HTML-escaped; the Worker interpolates it raw.
  They differ only if the mount or a media key contains `& < > " '`. (The
  provenance `href` goes through `escapeHref` in both: http(s) or mailto,
  else `#`.)
- **Line endings.** CRLF and CR are normalised to LF before rendering. The
  Worker receives LF from browsers.
- **Remote provenance label.** It uses the origin's host, lower-cased with the
  default port dropped, where the Worker uses `new URL(origin).host`. That
  also punycodes the host, which this crate does not.
- **Citation dates.** They use the ISO date part, where the Worker uses
  `toLocaleDateString` in the server's time zone.
- **Unrequested output directives.** `unrequested_output_directives`
  skips a directive line inside code; the studio's warns about it too.
- **Preview script.** It gives the iframe a `referrerpolicy`, since YouTube
  embeds need a referrer. It resolves image URLs against `document.baseURI`,
  so the fallback works on a non-http preview origin. It also re-checks
  images after DOM patches.
- **Sanitizer URL check.** Entities are decoded with `html-escape` and URLs
  resolved with the `url` crate, where the Worker uses `entities`'
  `decodeHTMLAttribute` and WHATWG `URL`. They can differ on legacy entities
  without a semicolon and on malformed URLs.
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
