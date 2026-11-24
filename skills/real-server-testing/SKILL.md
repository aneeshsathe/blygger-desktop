---
name: real-server-testing
description: Test a Blygger client against a real local blygger-studio under wrangler dev instead of mocks. Use when adding tests for anything that talks to a blyg, when a mock test passes but the real thing fails, or when reproducing a server-behaviour bug.
---

# Test against a real studio, not a mock

Hand-written mock servers drift from the studio. In one upgrade, mock tests all
passed while 16 of 22 real-server tests failed (sign-in budgets, private draft
media, 429s, scopes). The studio is cheap to run locally; use it.

## The pattern

1. **Never build in someone else's checkout.** Copy `blygger-studio` to a
   scratch directory, then `npm ci && npm run build` there.
2. **Run two blygs**, `a` (under test) and `b` (a blyg `a` subscribes to), each
   with `wrangler dev --local` on a free port and its own `--persist-to` state.
   Derive a local-only wrangler config and random dev secrets in the scratch
   dir; write nothing into the studio directory.
3. **Mint tokens, raise budgets.** Create manual owner tokens for the test and
   raise the local API/sign-in budgets so tests don't trip 429s. Keep tokens in
   files; never print them.
4. **Seed with invented data** on `*.blyg.example.com`-style names. Include a
   large seed (a few thousand reading posts) for order and performance tests.
5. **Assert behaviour the user sees**: order, what appears after a change and
   how fast, what a withdrawn/edited post looks like. Compare against the
   studio's own API answer and, where one exists, the client's native path.
6. **Prove the test bites.** Break the code under test once and watch it fail
   before you commit it.
7. **Clean up**: stop only the PIDs you started, delete the scratch studio and
   its state.

For a browser client, drive it headless (Playwright) through a local
same-origin proxy against the same two blygs. Install node deps in scratch,
never commit `node_modules`.

## Never

- Point a test at a real blyg, or publish to one during development.
- `pkill` by name or send synthetic keystrokes to the OS.
- Extend a mock server or fake backend to make a test pass.

## In Burrow

```sh
BLYG_WORKER_DIR=<scratch studio build> scripts/e2e-local.sh
```

- It derives the config, starts blygs `a` and `b`, mints tokens and runs
  `crates/blyg-core/tests/it/e2e/mod.rs`, the markdown-notes e2e, the web
  read-path e2e (`crates/blyg-web/tests/e2e.rs`) and the switcher e2e
  (`BLYG_E2E_SKIP_*` skips each).
- Browser pane checks: `scripts/browser-*-check.sh` (content blocking,
  macros) run the app against local fixture pages, no network. The web app's
  (`browser-web-live-check.sh`, `browser-web-perf-check.sh`) serve `blyg-web`
  through `crates/blyg-web/dev-proxy.py` against the local blygs.
- Store logic is tested on real SQLite. The UI is checked by running the app
  on sample data with `verify-burrow` (`scripts/verify/burrow-verify.sh`).
- Don't add tests to the mock server (`crates/blyg-core/tests/common`) or the
  fake backend; existing ones stay green but aren't extended.
