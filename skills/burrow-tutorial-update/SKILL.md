---
name: burrow-tutorial-update
description: Update Burrow's in-app tutorial (tour), what's-new card and docs for a release. Use for every release that adds or changes something a user sees, and whenever a feature's keys, menus or layout change.
---

# Tutorial and docs, every release

Users have finished a tour that never showed them the newest feature. So the
tour is a release checklist item, not an afterthought.

## For each new or changed user-visible feature

1. **Docs:** a section in the relevant `site/src/*.md` page and a paragraph in
   `docs/SPEC.md` in its dense style (file names, keys, rules).
2. **Tour:** a step in `crates/blyg-app/src/onboarding/steps.rs` that lands on
   the feature (the quick tour, or the matching feature tour), with
   platform-correct key captions (`keymap::hint`).
3. **Existing steps:** audit every step whose UI, keys or behaviour changed
   and edit it; don't only add new ones.
4. **What's new:** if the tour teaches something new, set the newest entry's
   `version` in `onboarding/whats_new.rs` to the release's. The card stays short
   (version and first step); the details live in `site/src/releases.md`.
5. **Verify by running it:** step through the tour in the app with
   `verify-burrow` (the `ob-tour` and `tut-tours` scenes), screenshotting only
   the app's own window.
