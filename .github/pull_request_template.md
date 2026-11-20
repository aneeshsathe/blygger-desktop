<!--
Title: "Area: what it does now", as the commit titles are written
("Writer tests: move the caret the same way on Windows"). Fill each
heading; write "None" rather than deleting one. skills/burrow-pr-gate has
the rest of the checklist.
-->

## Why

<!-- The problem or request, and who it's for. Link the issue. -->

## What changed

<!-- The change, in a few bullets. Name the files a reviewer should read first. -->

## Scope

<!-- What this deliberately doesn't do, and any follow-ups. -->

## Tradeoffs

<!-- The alternatives you weighed and why this one. -->

## Blast Radius

<!--
What could break and for whom. Say yes or no to each:
- public-repo hygiene the scan can't see: names, account ids, tokens, a blyg on its own domain
- crate names stay (the other never-renamed identifiers are pinned by a test)
- the UI talks only to Arc<dyn Backend>; nothing new on the UI thread per keystroke
- remote content, network or web view (skills/client-security)
- Windows and macOS (skills/burrow-cross-platform)
- studio parity and capability gating (skills/capability-gating)
- new network calls; CI workflow, build-script or CI-script changes
-->

## Verification

<!--
- scripts/check.sh --fix passed (and `docs` if site/ changed)
- tests committed before the code (scripts/test-first.sh), or a No-test: trailer saying why
- tests use real SQLite and a real local studio, not a mock server or the fake backend
- real-studio e2e for server-facing changes (scripts/e2e-local.sh)
- for UI changes: a screenshot from skills/verify-burrow
- docs: SPEC, site, CHANGELOG [Unreleased]; tour at release time
-->
