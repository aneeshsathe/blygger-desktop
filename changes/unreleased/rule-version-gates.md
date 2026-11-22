section: Development

- **Checks: no feature gated on a version number, and read gates on read-scope
  operations.** `blyg-core/tests/it/version_gates.rs` scans product code for
  functions that read the studio's `contractVersion` or parse a version they
  are handed, and fails on any that isn't in its reviewed list (the major
  check, the `wholeBlyg` floor rule, the natural-grammar studio version).
  `blyg-core/tests/it/capability_gates.rs` now lists every capability gate
  with whether it guards a read or a write, fails on an operation constant
  that isn't listed or a gate that asks for a bare string, and checks each
  read gate's operations against the scopes in `openapi-current.json`: one
  that needs more than `owner:read` isn't listed to a read-only token, so
  the feature would vanish for it. The skills' sentences these checks replace
  are gone (`skills/capability-gating`, `skills/spec-client-build`,
  `skills/upstream-sync`).
