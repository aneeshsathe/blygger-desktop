# Reader conformance: Burrow against the blygger-spec reader suite

blygger-spec's reader suite (`conformance/reader/`, blygger-spec#22) is 100
fixtures: an item's `content_md` and the items it refers to, and what the
0.4 text says the published `content_html`, `transclusions[]`, `links[]` and
`generated[]` must contain. Expectations are written from the spec, not
recorded from a client; the reference studio is checked like any other
renderer. This page is how Burrow runs it and what it found.

## What is being tested

Burrow publishes through the studio API, so the **wire document is rendered
by the studio**, not by Burrow. What Burrow computes itself is the
**preview**, which is built to match what publish produces. The adapter,
`crates/blyg-app/examples/blyg-conformance.rs`, drives that preview with the
app's own code:

- `blyg_render::render_preview`;
- the studio module's `StoreResolver` (your posts first, then imported
  reading items) and quoted-HTML sanitizer, compiled in from `src/` as they
  are;
- the TK provenance the app records when it generates a scope
  (`generate::prepare_fill` and `provenance_with`).

Each case becomes a local store: `items` with `local: true` are your posts
(their `content_md` only; Burrow holds no `content_html` for its own posts),
the rest are reading items, and `self` is the post being published (public at
v1 when the case has `prior_content_md`, otherwise a draft; either way its
working copy is the case's `content_md`). A publish the studio would refuse
for something the preview counts (an unresolved quote or link, an
ungenerated or malformed TK scope) is reported as `{error}`.

Output: `content_html` without `data-line` or the provenance line (the public
page adds that, not the wire), `transclusions[]` and `generated[]`.
**`links[]` is not reported**: Burrow never computes it (the studio does at
publish). The suite counts an array that isn't reported as skipped, not
failed, so the 21 `links[]` checks that fail the studio don't count against
Burrow. Keep that in mind when comparing totals.

## Running it

```sh
scripts/reader-conformance.sh                 # build, fetch the suite, run, compare
scripts/reader-conformance.sh --only label    # a subset (any run.mjs option)
```

It builds the example, shallow-fetches blygger-spec at the pinned commit into
a temp dir, runs the suite in batch mode with `--out` in that temp dir (never
the suite's own `out/`), and compares every case with
`scripts/reader-conformance.expected`. Environment: `BLYG_SPEC_DIR` uses an
existing checkout instead of fetching, `BLYG_CONFORMANCE_BIN` an existing
adapter binary instead of building, `BLYG_READER_KEEP=1` keeps the temp dir
(for `report.html`), `NODE` picks Node 20+.

**Exit status** is non-zero only on a regression: a case that passes in the
baseline and now warns or fails, or warns and now fails (or a baseline case
that left the suite). Known fails are listed, not errors, and a case that
got better is reported as fixed so the baseline can be tightened. The suite
itself exits 1 on any fail, which would make the script useless as a check
while known divergences remain, so its exit code is ignored.

**Not in CI.** It needs the network (a fetch from GitHub) and Node 20, and
the suite is still on a fork branch. Run it by hand when `blyg-render`
changes or the pin moves.

By hand, without the script:

```sh
cargo build -p blyg-app --example blyg-conformance
cd <blygger-spec>/conformance/reader
node run.mjs --cmd "<target>/debug/examples/blyg-conformance --batch" --batch \
    --out <scratch dir> --name burrow
```

## Pinned suite

| | |
|---|---|
| Repository | `https://github.com/aneeshsathe/blygger-spec`, branch `conformance-0.4` |
| Commit | `510c76c953f7caab3207ed443e4edf8294ffd559` |
| Snapshot | `spec/0.4/2026-10-18`, plus the living `docs/protocol-v0.4.md` at `8e8bce5` (label bake #81, `links[]` #79, CommonMark blockquote extent and line endings #76) |
| Reference results | blygger-studio 0.49.12, committed in the suite's `out/` |

## Results (Burrow 0.16.0)

**88 pass, 1 warn, 11 fail** of 100. The studio: 72 pass, 0 warn, 28 fail.

| Group | Cases | Burrow pass / warn / fail | Studio 0.49.12 pass / warn / fail |
|---|---:|---|---|
| grammar | 16 | 16 / 0 / 0 | 14 / 0 / 2 |
| code | 12 | 12 / 0 / 0 | 11 / 0 / 1 |
| label | 21 | 16 / 0 / 5 | 7 / 0 / 14 |
| partial | 11 | 9 / 0 / 2 | 10 / 0 / 1 |
| resolution | 13 | 11 / 0 / 2 | 13 / 0 / 0 |
| urls | 9 | 7 / 0 / 2 | 4 / 0 / 5 |
| disclosure | 6 | 6 / 0 / 0 | 3 / 0 / 3 |
| generation | 12 | 11 / 1 / 0 | 10 / 0 / 2 |
| **total** | **100** | **88 / 1 / 11** | **72 / 0 / 28** |

### Where Burrow and the studio differ

**Studio fails, Burrow passes (21).** All but one are the studio emitting no
`links[]` entries, a check Burrow skips (it reports no `links[]`), so these
say nothing about Burrow's links:
`directive-inline-beside-link`, `crlf-line-endings`,
`unknown-ids-in-code-publish`, `link-label`, `link-label-tight`,
`link-label-trimmed`, `link-label-with-pipe`, `link-label-empty`,
`link-label-remote`, `link-label-text-escaped`, `link-label-markup`,
`link-label-ends-at-first-close`, `link-local-page`,
`link-local-thread-page`, `link-remote-page`, `link-remote-absolute-page`,
`link-remote-no-page`, `links-in-fragment`, `links-one-per-link`,
`quote-and-link-same-target`, `tk-link-inside-span`. Their anchors, hrefs and
text match the spec in Burrow's output.

The other is **`tk-output-own-line-directive`** (studio fail, Burrow warn):
an own-line `![[id]]` inside generated output. The studio leaves it literal
inside the `div.blyg-tk-gen` (blygger-studio#105); Burrow quotes it (decision
#60), but closes the generated block before the quote and reopens nothing
after, where the fixture nests the quote inside the block. The placement is
the fixture's declared ambiguity, so Burrow only warns; the quote and its
`transclusions[]` entry are right.

**Burrow fails, studio passes (5).** `partial-of-thread-reaches-its-quote`,
`local-nested-thread`, `closure-contains-self`, `markdown-relative-links`,
`markdown-relative-image`. See the findings below.

**Both fail (6).** The five `directive-label*` cases (no `data-blyg-label`)
and `partial-lazy-continuation`. `blyg-render` reproduces the studio here on
purpose (byte parity with the reference renderer, see `docs/RENDER.md`), so
these move when the studio moves (studio queue item q, blygger-studio#104).

### The nine ambiguous fixtures

| Fixture | Burrow's reading | Matches the fixture's reading |
|---|---|---|
| `directive-reserved-version-inline` | inline `![[id@vN]]` is inert text, publishes | yes |
| `directive-in-fragment` | inert text in a fragment, no bake | yes (either reading is accepted) |
| `directive-tab-indented` | tab-indented line is indented code, inert | yes |
| `link-label-markup` | label is literal, escaped text (`*not emphasis*`, `&lt;b&gt;`) | yes |
| `remote-withdrawn-pin-retained` | bakes the retained pin, version 1 | yes |
| `link-remote-no-page` | falls back to the convention page `https://remote.example/f/{id}/` | yes (one of the accepted hrefs) |
| `markdown-relative-links` | leaves the author's relative hrefs relative | the ambiguity (what they resolve against) doesn't arise; they are not absolute, which fails the non-ambiguous check |
| `baked-remote-relative-url` | resolves the source's relative URL against its origin on bake | yes |
| `tk-output-own-line-directive` | quote placed after the generated block, not inside it | no (the only warn) |

## Findings

Each Burrow fail or warn, checked against the code and the output, and
whether it is a real renderer divergence or an artefact of how the adapter
models a case.

### Real divergences

1. **`data-blyg-label` is never baked** (5 `directive-label*` cases).
   `blyg-render` accepts `![[id | label]]` but drops the label; no code
   writes the attribute. Same as the studio (#81); follow the studio.

2. **A partial selection ignores lazy continuation** (`partial-lazy-continuation`).
   The selection ends at the first line without `>`; the unprefixed next line
   becomes a separate paragraph. Same as the studio (blygger-studio#104,
   §10.1 #76). `blyg-render`'s ordinary blockquote rendering does follow
   CommonMark here; only the partial-selection extent doesn't.

3. **Relative URLs the author writes stay relative in the preview**
   (`markdown-relative-links`, `markdown-relative-image`). `[x](/archive/)`
   and `![x](media/x.png)` come out as written. §5.2 wants every URL in
   `content_html` absolute; the studio resolves them at publish, so the wire
   document is fine and only the preview differs. Low impact.

4. **A local closure that contains the publishing thread is not refused**
   (`closure-contains-self`, §10.2). The resolver's cycle guard starts with an
   empty stack, so the post being edited is not on it: quoting a thread that
   quotes this one renders, and the cycle it then meets one level down is an
   inline "circular" marker, not something the publish check counts (only
   top-level unresolved directives are). The studio refuses the publish, so
   Burrow's pre-publish check passes a post the server will reject.

5. **Your own posts are baked from their working copy, with provenance lines**
   (the cause of `local-nested-thread` and `partial-of-thread-reaches-its-quote`,
   see the artefact note below). `StoreResolver::own` re-renders the post's
   `content_md` (the comment there says so) rather than baking the published
   `content_html`, so a post with unpublished edits previews differently from
   what publish bakes, and a nested quote inside it carries a
   `p.provenance` line the wire HTML doesn't have. With the nested item
   held, `local-nested-thread`'s text gains "fragment ↗ · snapshot of v1",
   and the partial selector's `suffix` context picks up the start of it.

6. **The AI crate's TK parser doesn't skip code, the others do** (not hit by
   any fixture; shown with an extra case). `blyg_ai::prompts::parse_scopes`
   is a plain token scan; `blyg_core::tk::parse` and
   `blyg_render::parse_scopes` skip tokens inside code (#68). The app picks a
   scope by its `tk::parse` index and then builds the job with
   `prompts::tk_job` / `apply_fill`, which index `parse_scopes`. With
   `` `[TK]x[=]y[/TK]` `` in code before a real scope, index 0 means
   different scopes to the two parsers: the job is built for the code
   example, and the real scope's recorded sources come out empty (a probe
   case that should give `sources: [{id, version: 1}]` gives `[]`).

7. **The AI crate takes TK source ids from the output too.**
   `prompts::parse_scopes` collects `![[id]]` from the whole scope;
   `tk::parse` from the instruction only (studio 0.32.2: a ref only the
   output names is not a source). Recorded provenance is filtered to the
   instruction's ids, so disclosure is right, but regenerating sends
   output-only quotes to the model as sources, and refuses ("unresolvable
   source") when the output names an id the store can't resolve. No fixture
   reaches it.

8. **Own-line directive in generated output: quote outside the block**
   (`tk-output-own-line-directive`, warn). Decision #60's block split puts the
   quote after the `div.blyg-tk-gen`. The fixture's chosen reading nests it.
   Ambiguous in the text; worth settling upstream before changing.

### Adapter-model artefacts

- **`local-nested-thread` and `partial-of-thread-reaches-its-quote`** fail with
  "unknown item" / "quoted passage not found" only because the fixture lists
  the local thread but not the local fragment it quotes, and Burrow, which
  holds no `content_html` for its own posts, re-renders the thread from
  `content_md` and can't resolve the inner quote. A real Burrow store always
  holds all your posts. Re-run with the fragment added, the partial case
  passes and the nested case differs only by the provenance line of finding 5.
- **`links[]` passes are skips**, not passes (above).
- **TK generation is simulated**: every scope with output is treated as
  generated in the app, with `prepare_fill` listing its sources. That is the
  app's real code path, but it means `generated[]` is what Burrow would
  record, not what the studio stores.

## Updating

When the suite pin moves or `blyg-render` changes: run the script, update
`SPEC_COMMIT` in `scripts/reader-conformance.sh` and the table above, and
take fixed cases out of `scripts/reader-conformance.expected`.
