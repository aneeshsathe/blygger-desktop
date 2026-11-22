section: Development

- **`scripts/test-first.sh --run`: the old commit builds in a target
  directory of its own.** The first test commit's tests, and the e2e step
  through `scripts/e2e-local.sh`, build under
  `${CARGO_TARGET_DIR:-<repo>/target}/test-first`, kept between runs. They
  built into the caller's target directory before, and the stale test binary
  left there made the next e2e run from the working tree report 20 failures
  that weren't real (`crates/blyg-core/tests/it/test_first_script.rs`).
