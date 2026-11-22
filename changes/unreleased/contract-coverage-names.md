section: Development

- **Contract coverage: a label no longer counts as a parameter.** A path or
  query parameter in the studio's contract is covered by a test holding its
  name in the contract's own case, or by a query string that sends it
  (`?raw=` or `&raw=`); header names still match in any case. The Raw tab's
  label `"Raw"` had counted as the `raw` query parameter, which Burrow never
  sends, so the parameter's exemption is back in `contract-coverage.toml`
  (`crates/blyg-core/tests/it/contract_coverage.rs`).
- **Contract coverage: the vendored contract is one Burrow accepts.** The
  same file's `burrow_accepts_the_vendored_contract` fails when
  `openapi-current.json` carries a contract version that `api::contract`
  refuses. Release 0.28.0's first commit vendored contract 6.1.0 while the
  guard accepted major 5 only.
- **`skills/burrow-pr-gate`: a test drives the path the app runs.** A
  test-only twin of a production function proves the twin only.
