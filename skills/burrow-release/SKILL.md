---
name: burrow-release
description: Cut a Burrow release (0.N.0 or 0.N.1). Use when preparing a version bump, merging a release PR, or when a PR changes the version in the root Cargo.toml.
---

# Cutting a Burrow release

Versions stay below 1.0: after 0.9.0 came 0.10.0. Never propose 1.0. Patch
releases (0.N.1) are for fixes only.

## Merging to main releases

`release.yml` tags and publishes any untagged version that lands on `main`.
So merging a PR that bumps the root `Cargo.toml` `version` **is** the release.
Never tag by hand. Never merge on red CI (macOS, Windows and the docs site).

## Checklist

1. `CHANGELOG.md`: rename `[Unreleased]` to `[0.N.0] - YYYY-MM-DD`, add a fresh
   empty `[Unreleased]`. Untested things go under a "Not verified" note.
2. Root `Cargo.toml` `version` bumped; `Cargo.lock` updated.
3. `site/src/releases.md` gets a `## Burrow 0.N.0` section. The app's "What's
   new" opens it, and a test fails if the running version has no heading.
4. The tutorial and docs: run `burrow-tutorial-update`.
5. Generated docs (`site/src/generated/*`: keybindings, config keys)
   regenerated if keys or config changed.
6. The gate passes (`burrow-pr-gate`).
7. A user-testing pass ran on the integration branch (`user-testing-pass`):
   its blockers and top irritants are fixed, and the rest are listed as
   follow-ups.
8. Every mapped feature still runs live: for each row of
   `skills/verify-burrow/features/README.md`, run
   `scripts/verify/burrow-verify.sh run <scene>` with its starting scene and
   look at the screenshot. The user-testing pass covers only what the update
   touched, so this catches a feature the update broke from the side. List
   each feature's run directory and screenshot path in the release PR's
   Verification section (add the heading if the body has none), so the check
   leaves a record.
9. Before merging (merging tags the release): the latest weekly CI run (the
   scheduled deny advisories job) is green. The weekly run blocks nothing,
   so this is where it gets looked at.
10. Before merging: the release PR's `timing` job (the wall-clock timing
    reports, `scripts/check.sh timing`, macOS only) is green, or the PR
    explains each overrun it logs (which step, by how much, and why it's
    the runner and not the code). The job never fails the run, so this is
    where its numbers get read.
11. After merge: watch the Release run; confirm the release has the macOS,
    Windows assets, plus signed sums.
