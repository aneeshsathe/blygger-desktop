---
name: burrow-code-shape
description: Turn a repeated mistake into a check, and shape Burrow's code so a correct change needs little context. Use when you're corrected, when you fix a bug a second time, when you add a rule to CLAUDE.md or a skill, or before building on a module (a new Backend method, a new sheet, a new list or setting).
---

# Corrections become checks; code carries its own context

Agents copy what the codebase does, so the code is the strongest instruction
Burrow has. Prose is the weakest: every checklist line here is a mistake that
happened at least once and could happen again.

## When a mistake repeats, fix the class at the highest level that works

1. **Make it impossible.** One owner per piece of state, one way to do each
   thing, the old way deleted. Example: `SwitchBackend` used to forward
   ~170 `Backend` methods by hand, and missing forwards shipped twice (the
   shelf methods; `save_typing`). `crates/blyg-app/build.rs` now writes the
   whole impl from the trait, so there's nothing to forget.
2. **Make the build fail**, with a message that names the replacement:
   types, `pub(crate)`, `#[must_use]`, clippy `disallowed-methods` with a
   `reason` (`crates/blyg-app/clippy.toml` bans `std::thread::sleep` and
   points at `crate::test_wait`). If the bad pattern is common, block new
   uses first, then migrate the old ones in the same wave.
3. **A behaviour test** of what a user would see (`tests/cli.rs` runs the
   real `blygger --version` and fails if it starts the app).
4. **Prose**, only for a judgement call nothing can check, with the failure
   that taught it.

Prove the check bites: put the old code back (revert the fixing hunk, or
paste the old helper), run the same command CI runs, and keep the failure
output for the PR. Then add a row to the rule → enforcer table in
`CLAUDE.md` and delete the prose it replaces. If a rule in the table breaks
again, move its enforcer up a level in the same change.

Don't write comments that justify a workaround ("sleep 250 ms so the
animation finishes") or state a constraint ("keep in sync with …", "must
stay in this order"): each is a missing check. Encode it, then delete the
comment. Comments that record facts we can't change (a WebView2 quirk, a
spec clause) stay, with a link or clause id.

## Shape code so the answer is near the question

The test: for any value, someone new should find where it comes from and
what can change it in under 30 seconds.

- **Cut pass-through layers.** A layer that only forwards gets generated or
  deleted (`SwitchBackend`). No trait, factory or wrapper "for later": add
  the second implementation first.
- **Derive, don't sync.** Text that quotes a constant is built from it:
  `MIN_STUDIO` comes from `min_studio!()` and is `concat!`ed into the UI
  strings, and `server_notice.rs`'s test fails on any other spelling of the
  floor in code or docs. Docs generated from the keymap and config keys
  (`site/src/generated/`) are checked by `keymap_docs.rs` and
  `blyg-core/tests/it/config_docs.rs`.
- **Decide once.** One function per rule, called everywhere: list order is
  `blyg-core/src/order.rs`; "unread" is `ReadingItem::is_unread`
  (`model.rs`). A second copy is a bug waiting to happen.
- **Put the domain in one structure.** One `enum` beats several booleans:
  `Sheet` means only one sheet can be open; `ReadMode` is Stream or Reader.
  If a feature needs one more `else if` on the same flags in several files,
  make the states an enum (or a table) instead. Demo scenes were ~170 string
  names routed by `starts_with` prefixes across 18 files; now each is a row
  in `scenes.rs` (a `Scene` variant and its area), and a typo is a compile
  error or, in `BLYGGER_DEMO`, a startup error naming near matches.
- **Let types carry knowledge.** Parse config, env vars, CLI args and wire
  JSON into typed values where they enter (`cli::route`, `Prefs::from_config`,
  `model.rs`), then trust them. Environment variables are parsed once into
  `DevFlags` (`crates/blyg-app/src/dev_flags.rs`); clippy refuses
  `std::env::var` anywhere else in blyg-app. Give ids their own type (`LocalId`), not
  `&str`. Keep business rules in plain functions in `blyg-core` or the
  `*_vm.rs` files, away from GPUI.
- **Narrow state.** A return value over a field, a field over a global.

Prefer the smallest change that does this. Don't force a structure on code
that is local, clear and unlikely to grow.
