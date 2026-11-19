---
name: burrow-pr-gate
description: The checks a Burrow change must pass before it becomes a PR or is merged, applied at authoring time instead of in review. Use before committing a feature, before opening or merging a PR, and when reviewing an outside contribution.
---

# The pre-PR gate

These are the checks reviewers used to catch in PRs. Run them while writing.

## Mechanical (must pass)

```sh
cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Server-facing changes also run the real-studio e2e
(`BLYG_WORKER_DIR=<scratch studio build> scripts/e2e-local.sh`); browser pane
changes run `scripts/browser-*-check.sh`.

## Judgement (answer each in the PR body)

1. **Public repo hygiene:** no names, emails, real blyg URLs, home paths or
   account ids; samples on `blyg.example.com`; no tokens in output.
2. **Naming:** display name is Burrow; never renamed: bundle ID, Keychain
   service/accounts, data dir and db names, config path, `BLYGGER_*` env vars,
   the `blygger` binary, key contexts, action namespaces, crates.
3. **Contract:** the UI talks only to `Arc<dyn Backend>`.
4. **Typing budget:** nothing new on the UI thread per keystroke; SQLite and
   network stay off it.
5. **Studio parity:** does the studio have this feature? Match its behaviour;
   gate on capabilities (`capability-gating`).
6. **Tests:** real SQLite and a real local studio; no new mock-server or
   fake-backend tests.
7. **Cross-platform:** `burrow-cross-platform`.
8. **Security:** `client-security` for remote content,
   network or web view changes.
9. **Docs:** SPEC paragraph, site docs, CHANGELOG `[Unreleased]` entry; tour
   at release time (`burrow-tutorial-update`).
10. **Merging conflicting union edits:** after a "keep both sides" merge, run
    fmt and clippy and grep for duplicated or dropped lines.

## Outside contributions

Read the whole diff before merging: new network calls, `unsafe`, process
spawning, CI workflow or build-script changes, new dependencies. Merging a
version bump releases (`burrow-release`).
