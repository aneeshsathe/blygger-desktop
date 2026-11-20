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

`openapi-current.json` is the newest studio's contract (5.14.0, studio v0.65.0,
same license). The e2e (`tests/it/e2e`) checks a real studio's answers against it
(`contract::current()`); the mock keeps to `openapi.json`, the oldest contract
Burrow supports, so it isn't taught every new field.

To follow a new studio, copy its `openapi.json` over `openapi-current.json` and
run the e2e (`scripts/e2e-local.sh`). Replace `openapi.json` only when
Burrow's oldest supported studio (`MIN_STUDIO`) moves.
