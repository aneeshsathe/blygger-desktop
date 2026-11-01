# Test fixtures

`openapi.json` is the owner-API contract from upstream
[blygger-studio](https://github.com/blygger/blygger-studio) v0.32.1
(commit a3d2cd6; MIT License, Copyright (c) 2026 Venkatesh Rao).

`extensions.json` describes, in the same form, the routes and replies the
app uses that upstream lacks (the Worker fork's extensions, docs/SERVER.md).
An upstream operation is checked against `openapi.json`; anything else, and a
reply status upstream doesn't declare (`POST /api/media` → 200 `duplicate`),
against `extensions.json`. A route in neither fails the test. The mock
server in `tests/common/` validates every request the app sends, and every
response the mock gives, against them (`tests/common/contract.rs`).

`studio-0.49.json` holds the operations the app uses whose contract changed
after the vendored `openapi.json`, cut verbatim from blygger-studio v0.49.1's
`openapi.json` (same license): `GET /api/reading` (its `background`
parameter) and the 0.49 routes `GET /api/peek`, `POST /api/peek/links` and
`GET/POST/DELETE /api/browse`, with the schemas they reference. A request is
checked against it before `openapi.json`; a mock reply against the contract
it was written for. The e2e checks a real studio's browse and peek answers
against it. Vendoring the whole 0.49.1 contract instead would mean bringing
the mock replies up to it (new required fields since 0.32.1).

`lineage-glyph.json` is the two owner reads of blygger-studio's
`lineage-glyph` extension (`GET /api/ext/lineage-glyph/summaries` and
`/lineage`) and their schemas, cut from that studio's `openapi.json` at
commit dc632c5 (branch ext/lineage-glyph, PR #53, studio 0.39.0; same
license). The Rust types are in `src/lineage.rs`; its tests check them
against this file, and the e2e checks a real studio's answers against it.

To update it, copy `openapi.json` from a newer upstream checkout, then run
`cargo test -p blyg-core`. Any contract violation fails the test that caused it.
