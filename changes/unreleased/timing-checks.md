section: Development

- **Tests count work instead of timing it.** Tests that guard an algorithm now assert on SQLite steps (a test-only counter behind blyg-core's `test-support` feature) instead of wall-clock budgets, so a slow or loaded machine no longer fails them. The remaining timings go through one `wait::Budget` helper and run in CI's non-blocking `timing` job (`scripts/check.sh timing`, release build, macOS only). A scan refuses new hand-rolled time asserts.
