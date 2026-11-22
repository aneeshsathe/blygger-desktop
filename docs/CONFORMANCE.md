# Conformance: Burrow against the blygger-spec suites

Burrow builds on the Blygger spec. It may add things the spec doesn't cover,
but it must not contradict the spec. The spec's conformance toolkit
(`conformance/` in blygger-spec) is how that is checked. This page lists
which suites apply to a client, how Burrow runs each one, and what the
latest run found.

## Pin

| | |
|---|---|
| Spec commit | `fbc3f6243f3efe686629e2713f9859715a183973`, branch `toolkit-0.69.0` of `https://github.com/aneeshsathe/blygger-spec` (open upstream as blygger/blygger-spec#38) |
| Its base | upstream `main` at `64b272e` (2026-11-21), protocol 0.4, twenty-first revision |
| Studio | blygger-studio v0.69.0 (`27b5924`, contract 6.2.0), the toolkit's reference run |
| Burrow | 0.28.0 |

The pin is `SPEC_COMMIT` in `scripts/conformance/spec.sh`, which every
script below reads. Upstream `main` now carries the older pins (`510c76c`,
`d248bc4`), but its toolkit is still at studio 0.65.0 and revision 18.
Move the pin to upstream once #38 merges.

## Results (Burrow 0.28.0)

| Suite | What it checks | Burrow | Before this run |
|---|---|---|---|
| `reader/` | `content_md` → `content_html`, `transclusions[]`, `generated[]` (110 fixtures) | **110 / 0 / 0** pass / warn / fail. 75 pass outright; 35 pass what Burrow reports and are marked "unreported" (Burrow sends no `links[]`) | 96 / 1 / 3 of 100 at `510c76c`. At the new pin, before the fixes: 108 / 1 / 1 |
| `schemas/` grammar | the §10.1 bracket grammar (28 cases), through Burrow's publish preview | **28 / 0 / 0** (studio 0.69.0: 28 / 0 / 0) | not run before |
| `feeds/` | the pace-layered order (RFC-3, 9 histories) | **9 / 0 / 0** (`pace_layered_order` and `reader_positions`) | checked by hand |

Not run, because they don't apply to a client:

- `schemas/` `samples()`, `roundtrip()` and `staticFields()`. Burrow
  publishes through the studio API, so the documents it publishes are the
  studio's, and the studio's column covers them. Burrow imports and stores
  no item documents itself: the studio imports them, and Burrow reads the
  owner API. The owner API is checked separately, against the studio's
  contract (`crates/blyg-core/tests/fixtures/README.md`).
- `clauses/`, `live/`, `alloy/`, `model/`: the clause register and checks
  that span the whole network, for spec work.

## Deviations

There are none open: every case passes at the pin. Fixed in this run:

| Case | Decision | What changed |
|---|---|---|
| `closure-contains-self` (reader, §10.2, C-10.2-04) | Burrow bug, fixed | The preview didn't refuse a quote of one of your own threads that quotes back to the post being published. The studio refuses that publish. `StoreResolver::publishing` now names the post, and such a quote is unresolved (`Circular`), so the pre-publish check refuses it too (`crates/blyg-app/src/studio/resolver_closure_tests.rs`). |
| `whole-list-start-kept` (reader, §10.2, #114, SHOULD) | Burrow bug, fixed | Burrow's thread sanitizer dropped `start` on `ol`, but studio 0.69 keeps an integer `start`. `blyg-render` now keeps it. The 0.64.1 parity fixture that expected the drop is rewritten in `SPEC_REWRITES` (`crates/blyg-render/tests/it/list_start.rs`). |
| `partial-lazy-continuation`, `local-nested-thread`, `tk-output-own-line-directive`, `partial-of-thread-reaches-its-quote` | fixed earlier, or the fixture changed | These were in the old baseline or regressed at `510c76c`, and they pass at the new pin. The 0.64.1 re-sync adopted lazy continuation. The suite's fixtures now list the fragment that a local thread quotes, and they settled the generated-block placement. |

Known differences that the suite accepts:

- **No `links[]`** (35 reader cases "unreported"): the studio computes
  `links[]` at publish. Burrow's anchors, hrefs and text are checked and
  pass.
- **Partial quotes of lists are flattened** (reader cases 104–109, #114):
  Burrow's preview bakes the selected items as paragraphs, as studio 0.64.1
  did. Studio 0.69 keeps their list structure. The suite accepts both, and
  the wire document is the studio's. Adopt this with the next render
  re-sync to 0.69 (`docs/RENDER.md`).

Not reached by any fixture (`docs/CONFORMANCE-READER.md`, findings): the
AI crate's TK parser doesn't skip code, and it takes source ids from a
scope's output as well as its instruction.

## Re-running

```sh
scripts/reader-conformance.sh    # builds the adapter, fetches the pin, compares with scripts/reader-conformance.expected
scripts/schemas-conformance.sh   # the grammar corpus; needs a studio (STUDIO_DIR, or it clones and builds v0.69.0)
scripts/feeds-conformance.sh     # checks the vendored copy against the pin, then runs the test
```

All three take `BLYG_SPEC_DIR` (an existing spec checkout) and fetch
nothing into this repo. The reader and schemas scripts also take
`BLYG_CONFORMANCE_BIN` (a built adapter). Each exits non-zero only on a
regression against its `.expected` baseline. The feeds fixtures are
vendored in `crates/blyg-core/tests/fixtures/feeds/`, so their test runs in
every `cargo test`. The other suites need the network and Node 20, so CI
doesn't run them. Run them when the renderer, the order or the pin
changes, and before a release.

To move the pin: change `SPEC_COMMIT` in `scripts/conformance/spec.sh`, run
all three scripts, then follow what each one reports. Copy changed feeds
fixtures over the vendored ones. Remove fixed cases from the baselines.
Decide each new fail or warn: fix it (test first), keep it in the baseline
with the reason, or write it up for the spec. Then update this page.

The adapters: `crates/blyg-app/examples/blyg-conformance.rs` (the reader
suite's renderer, the app's own preview code; `docs/CONFORMANCE-READER.md`)
and `scripts/conformance/schemas-adapter.mjs` (the schemas suite's
`grammar()`, which drives the same binary).
