section: Development

- **Web usability pass on a path-mounted blyg.** `scripts/browser-web-ux-pass.sh`
  also starts a studio with `MOUNT=/notes` and walks the app at
  `/notes/burrow/` (desktop and phone), and seeds posts a second apart so the
  Feed's orders can be judged. `crates/blyg-web/dev-proxy.py` takes `--mount`.
- **Capability gates: `Guards::Retired`** for an operation the current
  contract no longer has (the web's `getChanges`), checked to be absent from
  it (`crates/blyg-core/tests/it/capability_gates.rs`).
