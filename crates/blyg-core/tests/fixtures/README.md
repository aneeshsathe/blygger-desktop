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

To update the contract, copy `openapi.json` from a newer upstream checkout,
then run `cargo test -p blyg-core`. Any contract violation fails the test that
caused it.
