---
name: burrow-web-handoff
description: Hand a Burrow web build (blyg-web) to whoever deploys it. Use when a release's web zip is going onto a blyg, or when a deploy session asks for a build.
---

# Handing off a Burrow web build

The deployer verifies and audits; give them everything they need in one
message, and only for a signed release build.

## Send

1. The `Burrow-web-X.Y.Z.zip` path, with `SHA256SUMS` and `SHA256SUMS.sig`.
2. The version and the commit it was built from.
3. **Audit notes** since the last deployed version:
   - new network calls (all must be same-origin `/api/`);
   - new `innerHTML` sinks (each with a reason);
   - service-worker changes (scope, cached shell, still never `/api/`);
   - new files, headers or content types the host must serve (e.g.
     pre-compressed `.br`/`.gz` with `Content-Encoding`).
4. Anything the server must have (capabilities, studio version).

## Don't

- Deploy it yourself, or ask the deployer to bypass a step.
- Send an unsigned or locally built zip as if it were the release.
- Claim something about the build you haven't verified; mark guesses as such.
