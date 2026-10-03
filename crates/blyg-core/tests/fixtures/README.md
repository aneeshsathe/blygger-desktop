# Test fixtures

`openapi.json` is the owner-API contract from upstream
[blygger-studio](https://github.com/blygger/blygger-studio) v0.10.0
(commit 8a65934; MIT License, Copyright (c) 2026 Venkatesh Rao). The mock
server in `tests/common/` validates every request the app sends, and every
response the mock gives, against it (`tests/common/contract.rs`).

To update it, copy `openapi.json` from a newer upstream checkout, then run
`cargo test -p blyg-core`. Any contract violation fails the test that caused it.
