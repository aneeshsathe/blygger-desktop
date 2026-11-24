section: Development

- **Verification: no shots of a locked screen, and no blank ones.**
  `scripts/verify/burrow-verify.sh` refuses to launch or capture while the
  Mac is locked (`IOConsoleLocked`), and a shot that is one flat colour
  (`scripts/verify/shotcheck.swift`) is renamed `*.BLANK.png` and fails the
  shot. The 0.28 user-testing pass took its shots while locked and missed
  obvious layout bugs. `crates/blyg-app/tests/verify_script.rs` runs both
  checks through the script's selftest.
