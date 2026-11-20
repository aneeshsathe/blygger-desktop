---
name: burrow-pr-gate
description: The checks a Burrow change must pass before it becomes a PR or is merged, applied at authoring time instead of in review. Use before committing a feature, before opening or merging a PR, and when reviewing an outside contribution.
---

# The pre-PR gate

These are the checks reviewers used to catch in PRs. Run them while writing.

## Mechanical (must pass)

```sh
scripts/check.sh --fix          # Windows: bash scripts/check.sh --fix (Git Bash)
scripts/check.sh docs           # also, after changing site/
```

`scripts/check.sh` is the gate and CI calls it, step by step, so what passes
here passes there. It formats (`--fix`), checks the community extension list
and the CI skip list (`scripts/ci-skip.py`), then runs clippy with `-D warnings`,
the workspace tests, and blyg-app's tests again with every UI string spelled
as Windows does (`Ctrl+B`, not `⌘B`), so a test that expects Mac glyphs fails
on your Mac instead of on Windows CI. Cargo runs `--locked`. Add a new
mechanical check as a step there, not as a line in this skill.

Commit tests before the feature: the first commit adds tests that fail,
later ones make them pass. `scripts/test-first.sh <base> <branch>` checks the
order (`--run` also runs those tests without the feature and wants a
failure). CI's `test-first` job fails a PR that changes product code but no
test, unless a commit has a `No-test: <reason>` trailer.

Server-facing changes also run the real-studio e2e
(`BLYG_WORKER_DIR=<scratch studio build> scripts/e2e-local.sh`); browser pane
changes run `scripts/browser-*-check.sh`.

## Judgement: the PR body

Fill in `.github/pull_request_template.md` (GitHub puts it in every new PR):
Why, What changed, Scope, Tradeoffs, Blast Radius, Verification. Its Blast
Radius and Verification prompts hold the questions a reviewer would ask:
public-repo hygiene, never-renamed identifiers, the `Backend` contract and
typing budget, security, both platforms, studio parity, tests against a real
studio, docs. Answer them there, not from memory.

Titles, of commits and of the PR, read `Area: what it does now` ("Writer
tests: move the caret the same way on Windows"). A new check's first commit
is `Check: <rule> (red: <what it caught>)`.

After a "keep both sides" merge of conflicting edits, run the gate and grep
for duplicated or dropped lines.

## Outside contributions

Read the whole diff before merging. CI checks new dependencies: the `deny`
job (policy in `deny.toml`) fails on a licence outside the allowed list or
a source other than crates.io on every PR, and on a RustSec advisory when
the PR changes `Cargo.lock` or `deny.toml` (and weekly, and on main). Run
`scripts/check.sh deny` after changing dependencies. A change to
`deny.toml` is a policy change; it needs its reason.

No check can judge a change to `.github/workflows/`, a `build.rs`, or a
script CI runs (`scripts/`). Reviewing those stays with the maintainer:
an outside contribution that touches them waits for the maintainer's own
read. Also read for new network calls, `unsafe` and process spawning
until a check covers them. Merging a version bump releases
(`burrow-release`).
