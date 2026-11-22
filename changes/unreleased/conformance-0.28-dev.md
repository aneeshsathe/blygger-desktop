section: Development

- **Conformance: three suites run at blygger-spec fbc3f62 (protocol 0.4
  r21, studio 0.69.0), and every case passes.** The reader suite passes
  110 of 110 (35 of them without `links[]`, which the studio computes).
  The schemas suite's §10.1 grammar corpus passes 28 of 28, through the new
  adapter `scripts/conformance/schemas-adapter.mjs` and
  `scripts/schemas-conformance.sh`. The pace-layered order's nine RFC-3
  fixtures pass too. They are vendored in
  `crates/blyg-core/tests/fixtures/feeds/` and run in every `cargo test`;
  `scripts/feeds-conformance.sh` checks the copy against the spec. The pin
  lives in one place, `scripts/conformance/spec.sh`. `docs/CONFORMANCE.md`
  has the results, the deviations and how to re-run.
