# Reader conformance: Burrow against the blygger-spec reader suite

blygger-spec's reader suite (`conformance/reader/`, blygger-spec#22) is 110
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
publish). The suite counts a case that passes everything else but leaves out
an expected array as "unreported" (status `info`), not as a pass or a fail.
The script counts those with the passes.

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
the pinned toolkit is still on a fork branch. Run it by hand when `blyg-render`
changes or the pin moves.

By hand, without the script:

```sh
cargo build -p blyg-app --example blyg-conformance
cd <blygger-spec>/conformance/reader
node run.mjs --cmd "<target>/debug/examples/blyg-conformance --batch" --batch \
    --out <scratch dir> --name burrow
```

## Pin and results

The pin, the per-suite results and the deviations are in
`docs/CONFORMANCE.md`. At blygger-spec `fbc3f62` (110 fixtures, Burrow
0.28.0), every case passes. 35 cases are "unreported": they pass on
everything Burrow reports, but Burrow sends no `links[]`. The studio's
reference run (v0.69.0) is in the suite's committed `out/`.

The suite marks some fixtures ambiguous, where the spec text allows more
than one reading. Burrow's readings: an inline `![[id@vN]]` is inert text;
a directive in a fragment is a publish error (#102); a tab-indented line is
code; a link label is literal, escaped text; a withdrawn remote with a
retained pin bakes the pin; a remote with no `page` links to its item
document; a relative URL in a baked remote is resolved against its origin.
Since the suite's fixture changed, an own-line directive in generated
output bakes as the fixture reads it.


## Findings

Findings from earlier runs, checked against the code.

### Fixed

- **`data-blyg-label` is baked** (the `directive-label*` cases), since the
  0.64.1 re-sync. The same re-sync made a partial selection follow lazy
  continuation (`partial-lazy-continuation`).
- **A local closure that contains the publishing thread is refused**
  (`closure-contains-self`, §10.2). The resolver didn't know which post was
  being published, so it found the cycle only one level down and showed it
  as a nested marker. Now `StoreResolver::publishing` names the post, and
  the quote is unresolved at the top, where the pre-publish check counts it.
- **An ordered list keeps its integer `start`** (`whole-list-start-kept`),
  as studio 0.69's sanitizer does (#114).

### Open, not reached by any fixture

1. **Relative URLs the author writes stay relative in the preview.** The
   studio makes them absolute at publish, so the wire document is fine.
   The suite's `markdown-relative-*` cases pass, because the adapter makes
   links absolute under `self.origin` as publish would.
2. **Your own posts are baked from their working copy.** `StoreResolver::own`
   re-renders the post's `content_md` rather than baking the published
   `content_html`. So a post with unpublished edits previews differently
   from what publish bakes.
3. **The AI crate's TK parser doesn't skip code, but the others do.**
   `blyg_ai::prompts::parse_scopes` is a plain token scan, while
   `blyg_core::tk::parse` and `blyg_render::parse_scopes` skip tokens inside
   code (#68). With `` `[TK]x[=]y[/TK]` `` in code before a real scope,
   scope index 0 means different scopes to the two parsers.
4. **The AI crate takes TK source ids from the output too.** The recorded
   provenance is filtered to the instruction's ids, so what Burrow
   discloses is right. But regenerating sends quotes that appear only in
   the output to the model as sources.


### Adapter-model artefacts

- **Your own posts hold no `content_html`**: the adapter re-renders a local
  item from its `content_md`, as the app does. So a fixture must list every
  local item that a local thread quotes. The current fixtures do; the
  `local-nested-thread` and `partial-of-thread-reaches-its-quote` failures
  at `510c76c` came from fixtures that didn't.
- **`links[]` is not reported**, so those cases count as unreported (above).
- **TK generation is simulated**: every scope with output is treated as
  generated in the app, with `prepare_fill` listing its sources. That is the
  app's real code path, but it means `generated[]` is what Burrow would
  record, not what the studio stores.

## Updating

When the pin moves or `blyg-render` changes: run the script, update
`docs/CONFORMANCE.md`, and take fixed cases out of
`scripts/reader-conformance.expected`. The pin is `SPEC_COMMIT` in
`scripts/conformance/spec.sh`.
