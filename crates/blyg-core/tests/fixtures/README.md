# Test fixtures

`openapi.json` is the private owner-API contract 5.0.0 from upstream
[blygger-studio](https://github.com/blygger/blygger-studio) v0.49.12
(MIT License, Copyright (c) 2026 Venkatesh Rao), vendored verbatim. Its paths
are relative to the server URL `{mount}/api`. The mock server in
`tests/common/` validates every request the app sends, and every response the
mock gives, against it (`tests/common/contract.rs`); a route that isn't in it
fails the test.

`lineage-glyph.json` is the two owner reads of blygger-studio's
`lineage-glyph` extension (`GET /api/ext/lineage-glyph/summaries` and
`/lineage`) and their schemas, cut from that studio's `openapi.json` at
commit dc632c5 (branch ext/lineage-glyph, PR #53, studio 0.39.0; same
license). The Rust types are in `src/lineage.rs`; its tests check them
against this file, and the e2e checks a real studio's answers against it.

`openapi-current.json` is the newest studio's contract (6.2.0, studio v0.69.0,
same license). The e2e (`tests/it/e2e`) checks a real studio's answers against it
(`contract::current()`); the mock keeps to `openapi.json`, the oldest contract
Burrow supports, so it isn't taught every new field.

To follow a new studio, copy its `openapi.json` over `openapi-current.json`,
move `[target]` in `contract-coverage.toml`, write the tests the coverage check
lists (below), then build and run the e2e (`scripts/e2e-local.sh`). Replace
`openapi.json` only when Burrow's oldest supported studio (`MIN_STUDIO`) moves.

`contract-coverage.toml` is what `tests/it/contract_coverage.rs` checks: every
operation, property, enum value, parameter and response header in
`openapi-current.json` is named by a test, or listed there as `[[exempt]]`
(with a reason), `[[pending]]` (a work package is building it: a warning until
a test names it, then a failure until the entry goes) or in the generated
`baseline = true` group: everything in the contract at studio v0.61.0, the last
studio synced before the check existed. `[target]` names the studio Burrow
follows; `openapi-current.json` must be its contract, and with
`BLYG_WORKER_DIR` set to a studio checkout, byte for byte its `openapi.json` at
that tag. Regenerate the baseline (only to change which studio it stands for):

```sh
git -C <studio checkout> show v0.61.0:openapi.json > /tmp/baseline.json
BLYG_BASELINE_CONTRACT=/tmp/baseline.json BLYG_BASELINE_STUDIO=0.61.0 \
  cargo test -p blyg-core --test it contract_coverage::regenerate_baseline -- --ignored
```

`feeds/` is blygger-spec's pace-layered order fixtures (`conformance/feeds/`,
RFC-3, decision #97), vendored verbatim from the spec commit pinned in
`scripts/conformance/spec.sh`. blyg-core's `src/pace_tests.rs` runs them
(`the_specs_feed_fixtures_give_their_expected_order`), and
`scripts/feeds-conformance.sh` fails when the copy differs from the spec's.
