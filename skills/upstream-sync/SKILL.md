---
name: upstream-sync
description: Bring a Blygger client up to date with a new blygger-studio release or spec revision. Use when the studio or spec publishes a new version, when a peer session reports upstream changes, or before a client release.
---

# Syncing with a new studio release or spec revision

## Find out what changed

1. Read the studio release notes and diff from your last synced tag: API
   routes, `/api/capabilities` operation names, settings fields, migrations,
   rendering (`importer/sanitize`, transclusion), contract and SDK numbers.
2. Read the spec's changelog/decisions since your last synced commit.
3. Write down each change as: new (to adopt), changed (to adapt), removed (to
   drop), and doesn't-affect-us (say why).

## Adapt

- New server features: gate on capabilities (`capability-gating`), not on the
  new contract number.
- New settings fields: confirm your client writes settings partially, so it
  preserves fields it doesn't know.
- Rendering changes: regenerate your parity/render fixtures from the new
  studio (in a scratch copy, never someone else's checkout), and drop any
  deliberate emulation of upstream bugs that are now fixed.
- Studio UI features: add them to your client's parity table (works / worse /
  broken / missing) and schedule the gaps.

## Verify

Run your real-server e2e against the new studio build and the conformance
toolkit at the new spec commit. Record the studio version and spec commit you
synced to in your docs.

## Coordinate

If other sessions or people own the studio or spec, tell them what you're
changing upstream before you touch it, and ask before deploying anything that
needs server, route or header changes.

## In Burrow

- Rendering parity: regenerate the fixtures with
  `node crates/blyg-render/tests/fixtures/gen_parity.mjs <studio dir>` (in a
  scratch copy with `npm ci`), drop any bug emulation upstream has fixed, and
  update `docs/RENDER.md`.
- Bump the studio version and spec commit Burrow targets in `docs/SPEC.md`,
  `scripts/reader-conformance.sh` and the parity notes.
