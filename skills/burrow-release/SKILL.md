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
7. After merge: watch the Release run; confirm the release has the macOS,
   Windows assets, plus signed sums.
