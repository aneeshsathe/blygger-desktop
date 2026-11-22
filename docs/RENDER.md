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
              │  (an own-line ![[id]] stays text            own-line ![[id]] → Resolver → blockquote
              │   and is a publish error, #102)               (label baked as data-blyg-label), except in code;
              │                                             an attached `>` run → partial quote;
              │                                             a remote snapshot's relative URLs → its origin's
              │                                           prose runs → Markdown (an inline TK span
              │                                             closes before a quote, reopens after)
              │                                         thread: sanitize (sanitize.rs, lol-html)
              ├─ splice generated blocks, sentinels → span.blyg-tk-gen
              ├─ splice link anchors (or the unresolved marker) for the tokens
              ├─ thread: inject the provenance line into each top-level quote
              └─ relative href/src → absolute against the identity origin (an absolute mount only)
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
is what the decision says publish produces. Studio 0.59 baked the quote
too, but inside the block's wrapper; since 0.67 (#161) it closes the
wrapper before the quote and reopens it after, as here. Since decision #102
(protocol 0.4 §5.7) this shape is the rule: a baked quote is never inside
a `blyg-tk-gen` wrapper. An inline span that a quote interrupts closes
before it and reopens after it (`<span>one</span>` | quote |
`<span>two</span>`), where the studio's preview leaves the span open across
the quote. However many wrappers a scope's output is cut into, it is one
generation and keeps one `generated[]` entry (the app records provenance
per scope). Fragments never quote: there the directive renders as text
and is a publish error (below).

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
the label is the anchor text (below). On a directive it is the author's
name for the quoted item (decision #81): resolution, the provenance line
and `Quote` ignore it, and the bake carries it, HTML-escaped, as
`data-blyg-label` after the other attributes, as studio 0.59 does (an
unlabelled bake has no attribute). A label is plain text in both forms
(decision #102): no Markdown or HTML in it is interpreted. A labelled
own-line directive is a whole or partial quote exactly as an unlabelled
one. Since #102 the reserved form is the own-line `![[id@vN]]` **with or
without** a label (`![[id@vN | x]]` is refused too; the studio's
`RESERVED_LINE` still takes only the unlabelled one); inline, either is
text. A `[TK]` instruction's `![[id | label]]` is a source by its id. The
unresolved marker's `directive` is the trimmed line or the link's literal,
label included, as the studio reports it.

**Decision #102** (protocol 0.4, fifteenth revision, 2026-11-12; v0.4-plan
§21). The nine readings, as this crate applies them:

1. A directive line in a **fragment** (outside code, after TK scopes give
   their output) is a publish error. The preview still renders it as text,
   which is how readers show the ones published before the ruling, and
   reports it in `Stats::unresolved` as `UnresolvedReason::DirectiveInFragment`;
   `publish_grammar_errors(md, kind)` finds it without a render, and
   `blyg_core::grammar::publish_error` turns it into the message the publish
   path refuses with before anything is sent (`LiveBackend::publish_with`).
   Since 0.67 (#161) the studio refuses it at publish too, in the body or
   in a block scope's output (model.ts `publishScoped`, `directiveLines`),
   and the reason (`Display`) is its words: "a fragment can't quote another
   item: make this a thread, or link it with [[id]]". Its preview still
   reports nothing, so the parity harness adds what publish would refuse.
2. The `@vN` reservation is the own-line form, labelled or not (above).
   Reported as `ReservedVersion` in both kinds.
3. Code is CommonMark's block structure (`markdown::code_ranges`, from
   markdown-it's block parser): a tab- or four-space-indented directive at
   the start of a document or after a blank line is code; directly under a
   paragraph line (a lazy continuation) or as a list item's indented
   paragraph it is not, and the line grammar reads it first.
4. Labels are plain text, escaped wherever they are baked (above).
5. A link to an imported item with no `page` names its **item document**,
   `{origin}items/{id}.json` (`item_document_url`), never the
   `{origin}{f|t}/{id}/` convention. The local store does not hold another
   origin's `item` template (§6.1), so the default path is used. The
   provenance line and the Reader's own links are presentation and keep the
   convention, which §5.8 still allows there. Since 0.67 (#161) the studio
   links its item document too, through the origin's `item` template when
   the subscription holds one (`surface.ts` `itemUrl`); Burrow's store holds
   no templates, so it always uses the default `items/{id}.json`.
6. An author's relative URL resolves by RFC 3986 against the item's
   identity origin (`absolutize_html`, run last when `RenderOpts::mount` is
   absolute): `media/x.png` → `{origin}media/x.png`, `/archive/` →
   host-rooted, `../y` per the RFC. A URL with a scheme is untouched. A
   fragment-only `#sec` and a network-path `//host/x` are resolved too,
   because §5.2 wants every URL absolute; the studio's `absolutizeHtml`
   leaves those two alone and joins rather than resolves `../`.
7. A held snapshot's relative URLs resolve against the **source's** origin
   before the bake (the walker, whole quotes; a partial quote is text).
8. A pin-retained bake carries the retained version: `Found::version` is
   the resolver's, and the app's store keeps the highest pinned version it
   holds (`pin_in`); with nothing retained the resolver says
   `SourceWithdrawn`, a publish error.
9. A baked quote is never inside a `blyg-tk-gen` wrapper (above).

**Decision #103, rendering half.** `ShellOpts::lang` (the app's `lang`
setting, default `en`) replaces the hardcoded `lang="en"`; there is no wire
member for an item's language. `studio_css()` isolates every baked quote
and attributed string directionally (`unicode-bidi: isolate` on
`blockquote.blyg-transclusion`, the provenance line's source name, a
citation's `cite`, the Reader's quote footer and anything carrying
`data-blyg-label`), and a citation's author and excerpt are wrapped in
`<bdi>`. This is presentation: nothing in `content_html` changes for it.

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
imported ones to their origin's `page`, or with none to the item document
`{origin}items/{id}.json` (decision #102; the provenance line still uses
importer/util.ts `blygItemUrl`'s `{origin}{f|t}/{id}/`, `util::blyg_item_url`).
A relative `page` is origin-relative, a leading
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

**`@handle` mentions** (`handles.rs`, contacts.ts `consumeHandles`, studio
0.56). On a blyg that rewrites handles, an `@handle` naming a contact
(`Resolver::contact`, any case) becomes `[@Name](url)` before anything
else, and that link the contact's h-card after rendering, through the
`[[id]]` token splice. Handles in code, quoted lines, bracket grammar,
Markdown links and autolinks are left alone. Since studio 0.66 (#158) a
handle reads in natural grammar: a word that names no contact falls back
to its longest prefix ending before a `-` or `_` that does, so `@lin's`,
`@lin-driven` and `@lin_ish` all name Lin, while a longer handle in the
book (`@lin-k-led` → `lin-k`) still wins. Only `@` and the handle become
the link; the rest of the word stays. Studio 0.56–0.65 publish only a
whole-token handle, so the fallback runs only when
`Resolver::natural_handles` says so: the app's resolver asks
`blyg_core::contacts::natural_grammar` of the studio version `blyg.json`
names (0.66 or later). The version is kept in the store (`studio.version`
meta, learnt from the update state and the contacts refresh); one never
known reads as older, so the preview never links what publish won't.

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
pub fn publish_grammar_errors(md: &str, kind: Kind) -> Vec<Unresolved>; // #102: fragment directive lines, reserved @vN lines
pub fn absolutize_html(html: &str, base: &str) -> String; // RFC 3986 against an identity origin (#102)
pub fn item_document_url(origin: &str, id: &str) -> String; // {origin}items/{id}.json
pub fn shell_lang(lang: Option<&str>) -> String;       // the page's lang attribute (#103), `en` by default

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
pub fn page_shell_with(theme_css, body, &ShellOpts { title, base_href, reader, lang }) -> String;
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
`link-label.ts`, `comments.ts`, `contacts.ts` and `read-api.ts`'s
`previewTangles` (exposed to the bundle only), using the Worker's
esbuild, and runs the bundle in workerd through the Worker's own Miniflare
(the sanitizer is built on workerd's HTMLRewriter, which Node lacks). Since
studio 0.59 that is Miniflare 4, which takes each Worker as
`workers: [{ config }]` with its code as a module manifest; the generator
reads the compatibility date from the studio's `wrangler.jsonc`. The
harness is the body of `read-api.ts`'s `POST /api/preview` handler and
`threadPreview()`, step for step, called with a fake D1 that serves `fake_store.json`;
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
# check out the release tag and `npm ci` there (or `cp -R` a built copy;
# never build in a checkout someone else uses), then
node crates/blyg-render/tests/fixtures/gen_parity.mjs /path/to/studio/clone
cargo test -p blyg-render
```

**Reference version: studio 0.64.1** (tag `v0.64.1`, unpatched),
re-generated 2026-11-19 from 0.49.1 (markdown-it 14.3.3). The 0.49.1
fixtures could not be regenerated against 0.59 or later until the
generator moved to Miniflare 4. The harness now follows 0.64.1's preview
handler: comments to self dropped (`previewComments`), `@handle` mentions
resolved (`consumeHandles`; the fake store had no contacts until 0.68), tangle spans
(`previewTangles`, threads only), the generated blocks passed to the walk,
and `injectNestedProvenance` after `injectProvenance`.

Of the 257 existing cases 15 changed, all from studio changes:

- #81's label bake (`data-blyg-label`) is the studio's own now, so the r15
  label rewrite (`SPEC_LABELS`) is gone. The provenance line of a labelled
  quote now anchors on the label: `<a>label ↗</a> · fragment · snapshot of
  vN` (`labelledProvenance`). Six `label_*` cases; adopted (below).
- A quote nested in a quote gets its own `p.provenance.nested` line
  (`tr_found_local_thread`, `tr_provenance_order`); adopted.
- A partial quote's `>` run takes lazy continuation lines
  (`tr_partial_unknown_consumes`); adopted.
- A generated block's HTML in a thread preview is sanitized
  (`tr_sanitize_tk_block` loses `style`); adopted.
- An own-line `![[id]]` in a generated block is baked as a quote, inside
  the block's `div.blyg-tk-gen` (the four decision-#60 cases). This crate
  already baked them, between two wrappers, as #102 (9) says; the parity
  test now rewrites the studio's shape to the spec's (`SPEC_BLOCK_SPLIT`)
  and compares the rest byte for byte, so they are no longer known
  divergences.
- `tk_inline_dollar` holds a formula now (maths, below).

28 cases were added: maths (`math_*`: inline, display, display inside a
paragraph, money `$5 and $10`, `$ x $`-style spacing, `\$`, code, bad TeX,
a refused `\cancel`, emphasis, TK, links, a thread's own formulas, and four
sanitizer cases over new fake-store items `…vm1`–`…vm4`: a `<math>` with an
x-tex annotation, display and bare and case-folded encodings, hostile
annotations and nested `<math>`, bad TeX in an annotation), tangling
(`tangle_*`: a span, a span with a quote or link inside, placeholders,
escaping, code, mid-line and unclosed openers, a span between quotes, a
placeholder in a fragment) and `comment_to_self`. Three `withStubQuote`
vectors put the directive inside a `![[+ …]]` span.

**Adopted from 0.64.1** (the eleven former `TO_ADOPT` cases):

- *Labelled provenance* (`transclusion::inject_provenance`, pages.ts
  `bakedLabel` + `labelledProvenance`): a quote whose tag carries
  `data-blyg-label` gets the label (decoded, whitespace flattened, cut to
  120 UTF-16 units) as its line's anchor, and the old anchor text follows
  as plain text: `<a>label ↗</a> · from <em>Blyg</em> · snapshot of v5`.
  The cut is at a character, where the studio cuts at a grapheme.
- *Nested provenance* (`inject_nested_provenance`, pages.ts
  `injectNestedProvenance`): every baked quote inside a baked quote, at any
  depth, gets `p.provenance.nested`, read from its own `data-blyg-*`
  attributes, the origin carried down from the layer that baked it. Its
  link comes from `Resolver::nested_source` (default: `resolve`; the app's
  `StoreResolver` answers from the store without rendering). Labelled
  nested quotes anchor on the label too. The app now renders a quoted own
  thread with `provenance: false`, as publish bakes it, so a nested quote
  has one line, not two.
- *Lazy continuation* (`attached_quote`, directives.ts `attachedQuote` /
  `quoteExtent`, decision #76): a partial quote's run is the blockquote's
  CommonMark extent, measured by its own markdown-it block parser as the
  studio's is. An unmarked line continuing the quote's paragraph belongs to
  it (kept as written); not after an empty `>` line, not a line opening a
  block, and never a directive or reserved line. The walker, TK block
  splitting, the stub chooser (`with_stub_quote`, and `add_stub_quote`'s
  step past the quote the caret is in) all share it.
- *Generated blocks sanitized* in the thread preview, as the walked HTML is
  (`previewTransclusions` sanitizes `blockReplacements`).
- *Comments to self* (`comments.rs`, `comments.ts` `parseComments` /
  `previewComments`): `/* … */` is dropped from the preview in both kinds,
  after tangle spans and before handles, with the studio's whitespace
  tidying (`closeUp`), its line map kept for `data-line`. What would stop a
  publish (unclosed, a stray `*/`, a `/*` attached to a word) is appended as
  `⚠ …` paragraphs, in the studio's words. `tangle.rs` reads comment ranges
  from it. Code is found once per comment, as in the studio; a text that
  can hold no code (no backtick, tilde, tab or four spaces) skips the parse.
  A long draft with code and hundreds of comments is still quadratic.

**Formulas are compared by shape** (`tests/it/math_shape.rs`). The studio
draws with temml 0.14; no Rust crate matches it byte for byte. Each
top-level `<math>` on both sides becomes a stand-in holding only inline or
display and the entity-decoded `application/x-tex` annotation (the
author's TeX). This crate's formula must also use only `MATH_ELEMENTS` and
`MATH_ATTRIBUTES` and have exactly one annotation; the studio's own are
checked the same way. A refused formula's `code.math-error` keeps its text
and loses its `title` (temml's parse message), which must not be empty.
Everything around a formula is compared byte for byte, and whether a
formula is drawn, refused or prose must agree.

**Expected-to-fail lists** in `tests/it/parity.rs`. A listed case is
expected to fail; once it passes, the test fails with "now passing, remove
from its list", so every list only shrinks:

- `PENDING`: cases waiting for the 0.64.1 cohort's WP1 (tangle preview) and
  WP2 (maths, sanitizer maths); `STUB_SPANS_PENDING` holds the stub-chooser
  span vectors for WP1. Empty both once WP1 and WP2 are merged.
- `TO_ADOPT`: studio changes not followed yet; empty since the 0.64.1
  adoption below.
- `KNOWN_DIVERGENCES`: deliberate differences; empty since 0.64.1.

Bug emulation: none. The patched-Worker bugs (patch 9) and the 0.47.0
label bugs were dropped earlier; the three Worker behaviours under
"Worker behaviour pinned as it is" are still the studio's in 0.64.1, and
the r15 rewrites below that 0.64.1 still needs (#102 (9) inline spans, the
reserved labelled `@vN`, the no-`page` link, the fragment directive error)
remain.

### Earlier syncs

**Studio 0.49.1.** The fixtures came from stock blygger-studio
0.49.1 (tag `v0.49.1`, unpatched), re-generated 2026-10-31
(`parity/_manifest.json` records the studio, markdown-it and linkify-it
versions). Every case came out byte-identical to the 0.48.0 fixtures:
0.49's one rendering change, `injectNestedProvenance` (a provenance line,
`from … ↗ · snapshot of vN`, on each quote nested inside a quote), runs on
the public pages (`pages.ts`'s thread card and block) after
`injectProvenance`, and the generator calls only `injectProvenance`. So the
thread preview here still names the direct layer only, where a 0.49 public
page also names the nested ones; porting it would need the fake store to
answer its subscription and imported-item lookups. Not done yet. Earlier fixtures came from the maintainer's patched Worker; see
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

**Protocol 0.4 r15 rewrites** (2026-11-13; the label rewrite went with the
0.64.1 fixtures, and the no-`page` rule now spots the provenance line by its
`p.provenance` rather than its text). Where decisions #81 and #102
change what the 0.49.1 fixtures show, `tests/it/parity.rs` rewrites the
expected HTML before comparing, so the rest of each case is still checked:
`SPEC_LABELS` adds `data-blyg-label` to the labelled quotes, the no-`page`
rule rewrites anchors to an imported item without a `page` (the fake
store's `…01`, `…03`, `…05`) to its item document (not the provenance
line's), `SPEC_REWRITES` closes and reopens the inline TK span around a
quote and turns the labelled `@vN` line into the reserved marker, and
`SPEC_ERRORS` adds the publish errors the fixtures lack (a fragment's
directive line, the labelled `@vN`). 16 cases are rewritten. The fixtures
were not regenerated against studio 0.59.0: its Miniflare no longer takes
`gen_parity.mjs`'s single-worker options (it wants `workers: [{config}]`),
and 0.59 changed more of the preview than #102 (it bakes a generated
block's quotes inside the block, #105), so a re-sync is its own task. The
rewrites follow the spec where the studio differs; studio 0.59 already
bakes `data-blyg-label` exactly as rewritten.

Results (studio 0.64.1). Tests compare after collapsing whitespace between
tags. The block-scope directive cases of decision #60 (`tr_in_tk_block`,
`tr_block_tk_directive_output`, `tr_block_tk_directive_split`,
`tr_block_tk_directive_attached`) match after the `SPEC_BLOCK_SPLIT`
rewrite; `tests/it/preview.rs` still pins this crate's output for them.

| suite | cases | match | expected to fail |
|---|---|---|---|
| corpus (paragraphs, emphasis, links, linkify edges, headings, lists, code, quotes, images, raw HTML, YouTube, TK, `impyrt`, transclusion, partial quotes, `[[id]]` links, labels, sanitizing, maths, tangling, comments) | 285 | 285 (258 byte-identical; 15 rewritten to r15; 11 with formulas compared by shape) | 0 |
| CommonMark 0.31.2 spec examples | 652 | 652 | 0 |
| linkify-it + markdown-it linkify test vectors | 206 | 206 | 0 |
| attachments (`mediaHtml`) | 4 | 4 | 0 |
| bracket grammar (links, directive, `stripGeneratedGrammar`; `labelText`/`linkSource`; `linkLabel`; `withStubQuote`; `unrequestedOutputDirectives`) | 49 + 19 + 5 + 8 + 9 | exact | 3 `withStubQuote` span vectors (WP1) |

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

**Provenance on the preview is the desktop's own combination.** Since
studio 0.59 the thread preview adds provenance lines (`injectProvenance`,
then `injectNestedProvenance`), but only when every quote resolved. The
desktop preview adds them always, so the generator does too, and hides
unresolved markers from them, as they would be absent on the page.

Every fixture also checks the unresolved directives and reasons, in order,
and the TK error count against the Worker; thread fixtures also check the
resolved quote ids. `tests/it/preview.rs` adds checks that need no Worker:

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

The timing reports live in `blyg-core/tests/perf.rs` (`timing_render_…`),
where the one budget helper, `blyg_core::wait::Budget`, is; blyg-render is a
leaf crate. They're ignored by the everyday run, and CI's `timing` job runs
them without blocking a merge. One renders a 20,400-character mixed thread
with unresolved quotes, TK, links, fuzzy links, lists, tables, code and
videos.

- Release: 4.2 ms median when last measured with the quotes resolved,
  against a 10 ms budget. Keeping code lines from transcluding costs about
  0.3 ms: a thread containing `![[` gets one extra block-only parse
  (`markdown::code_lines`).
- Debug: about 25 ms.

A second runs adversarial inputs aimed at the linkify regexes'
backtracking, and 200,000 lone dollars at the formula finder, each against
a budget of its own.

```sh
cargo test -p blyg-core --release --test perf -- --ignored timing_render --nocapture
```
