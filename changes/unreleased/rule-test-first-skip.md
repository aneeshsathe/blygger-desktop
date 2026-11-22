section: Development

- **test-first: `--run` skips test commits that add no test function.** A
  commit that only touches a fixture or a vendored file under `crates/*/tests/`
  counts as a test change for the order check, but `scripts/test-first.sh --run`
  now names it (`test-first: <short> changes test files but adds no #[test];
  skipped`) and runs the first test commit that does add a `#[test]`, instead of
  dying on the fixture commit. It exits 1 when no test commit on the branch adds
  a test function, and when the first one that does comes after product code
  (so its tests can't be red before the feature).
