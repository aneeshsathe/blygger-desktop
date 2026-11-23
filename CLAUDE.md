# Burrow

Burrow is a blygger client: a native (Rust + GPUI), Notational-Velocity-fast, local-first studio for Blygger blogs
("blygs", https://blygger.org). Read `docs/SPEC.md` first. The agreed UX lives in the
clickable mocks under `docs/prototype/`.

- `crates/blyg-core`: API client, SQLite store, sync, and the `Backend` trait (the contract).
- `crates/blyg-ai`: AI providers, sign-in, TK prompts.
- `crates/blyg-app`: the GPUI UI. It talks only to `Arc<dyn Backend>`. `BLYGGER_FAKE=1 cargo run -p blyg-app` runs it on in-memory sample data.
- Server requirements (owner-API extensions to the reference Worker): `docs/SERVER.md`.
- Development checklists live in `skills/`. Read `skills/README.md` and follow every skill that matches the task: `burrow-pr-gate` before a commit, `burrow-release` for a release.

Rules
- **This is a public repo.** Never commit personal data: names, emails, real blyg URLs,
  home-directory paths, account ids. Use `blyg.example.com` and invented sample content.
  User configuration lives in the Ghostty-style config file (`~/.config/blygger/config`),
  and secrets live in the macOS Keychain. Neither is ever in the repo.
- Never publish to, or write to, a real blyg during development. Use mocks or `wrangler dev`.
- Never print or log tokens.
- Never send synthetic keystrokes to the OS, or `pkill` by name, when testing the UI. Kill only PIDs you started, and capture only the app's own windows
  (`scripts/verify/burrow-verify.sh` does it this way; see `skills/verify-burrow`).
- The app was renamed from Blygger to Burrow in display only (see "Naming" in `docs/SPEC.md`).
  Never rename the bundle ID, Keychain service/accounts, data dir or db names, config path,
  `BLYGGER_*` env vars, the `blygger` binary, key contexts or action namespaces, or crates.
- `scripts/check.sh --fix` must pass before any commit: it formats, then runs what CI runs (fmt, the extension-list and CI-skip checks, clippy `-D warnings`, tests, and the Windows-wording tests; cargo `--locked`). Add `docs` after changing `site/`.
- Personal and orchestration notes go in `CLAUDE.local.md` and `docs/private/` (both gitignored).
- A mistake that happens twice gets a check, not another sentence here; code is shaped so a
  correct change needs little context. How: `skills/burrow-code-shape`.

Rules with an enforcer (a new row needs a check shown failing on a real past instance; a row
goes once its mistake is a compile error):

| Rule | Enforced by |
|---|---|
| fmt, clippy (`-D warnings`) and tests pass | `scripts/check.sh`, which every CI step in `.github/workflows/ci.yml` calls (macOS and Windows) |
| `BLYGGER_*` and other env vars are read only through `DevFlags` | `crates/blyg-app/clippy.toml` bans `std::env::var`/`var_os`; read `crate::dev_flags::get()` |
| a main-window feature lives in its own file (`app/<feature>.rs`, an `impl MainView` block), not in `app.rs` | `crates/blyg-app/src/app/size_cap.rs`: `app.rs` may not grow past `APP_RS_MAX_LINES` (lowered, never raised) |
| tests wait for a condition, never a fixed sleep | `clippy.toml` bans `std::thread::sleep` in blyg-app (use `crate::test_wait`) and in blyg-core, blyg-ai, `blyg-ext*` (use `blyg_core::wait`) |
| `blygger --version`/`--help`/`--option` never start the app | `cli::route` + `crates/blyg-app/tests/cli.rs` |
| text naming the studio floor says `MIN_STUDIO` | `server_notice.rs` tests (scan of code and `site/src`) |
| `site/src/generated/*` match the keymap and config keys | `keymap_docs.rs`, `blyg-core/tests/it/config_docs.rs` |
| the running version has a `releases.md` heading | `update/tests.rs` |
| UI verification never touches real data | `scripts/verify/burrow-verify.sh` refusals, `tests/verify_script.rs` |
| UI text with ⌘ ⌥ ⇧ ⌃ is respelled on Windows (`keymap::hint` or `platform_text::Text`) | `platform_text.rs` scan of blyg-app's string literals; `steps.rs` Windows-wording test |
| UI text tests hold on both platforms' spelling | `scripts/check.sh wording` (CI's macOS step "Test (Windows wording)"); `platform_text::spelled_as` for one test |
| tests move the caret and edit with keys both platforms read alike (no ⌘+arrow, ⌥+←/→, no literal redo: `platform_text::redo_keys()`) | `platform_text.rs` `tests_dont_send_macos_only_caret_keys` (scan of blyg-app's sources) |
| Windows keys: no AltGr characters, no clashes, overrides name real keys | `keymap.rs` tests `no_windows_key_is_an_altgr_character`, `windows_overrides_are_real_and_clash_with_nothing` (run on every platform) |
| tour steps quote real keys (bound to what the step waits for), menu items and “labels” | `onboarding/caption_tests.rs` (both platforms' wording) |
| the reading list converges, prunes only after a whole walk, and stays cheap | `blyg-core/src/sync/walk_model_tests.rs` (proptest model driving `walk::Read`) |
| crates depend only on what their runtime allows (GPUI in `blyg-app` only) | `blyg-core/tests/it/crate_graph.rs` (`cargo metadata`) |
| each new thing in the target studio's contract gets a test before it's built (or an exemption with a reason); a sync starts at `[target]` | `blyg-core/tests/it/contract_coverage.rs` over `openapi-current.json` and `tests/fixtures/contract-coverage.toml` |
| tests are committed before the code they test, and fail without it | `scripts/test-first.sh` (`--run` before each merge); CI's `test-first` job fails a PR changing product code with no test, unless a commit has `No-test: <reason>` |
| every repo path or file name a dev doc names in backticks exists (another repo's path followed by `(other repo: <name>)`) | `blyg-core/tests/it/doc_paths.rs` |
| a PR skips the Rust jobs only for paths no code reads | `scripts/ci-skip.py check` (CI `changes` job) |
| new dependencies pass licences and sources (and RustSec advisories when dependencies change, weekly and on main) | `deny.toml` + CI `deny` job |
| unsafe code only in blyg-app, item by item | every other crate root has `#![forbid(unsafe_code)]` (`crate_graph.rs`); `[workspace.lints] unsafe_code = "deny"` + `#[expect(unsafe_code, reason)]` in blyg-app |
| process spawning only where a module says why | clippy `disallowed-types` `std::process::Command` in each crate's `clippy.toml` |
| the files lifting the unsafe or `Command` ban are one reviewed list | `blyg-core/tests/it/lint_exceptions.rs` + `tests/fixtures/lint-exceptions.txt` |
| a silenced lint says why, and goes once stale | clippy `allow_attributes` + `allow_attributes_without_reason` (workspace lints) |
| every crate has a `clippy.toml` and `[lints] workspace = true`; tracked crates are members; only the NETWORK crates have direct HTTP-client dependencies | `blyg-core/tests/it/crate_graph.rs` |
| no personal data that a pattern can find (home paths, emails, blyg-named hosts, GitHub owners) in tracked or new files; names and account ids still need the rule above | `blyg-core/tests/it/personal_data.rs` + `tests/fixtures/personal-data-allow.txt` |
| checks that read git don't skip on CI | `blyg-core/tests/it/no_git.rs` |
| names existing installs depend on never change (bundle ID, Keychain, data dir, db, config path, binary, key contexts, action namespaces, `BLYGGER_*` names DevFlags reads) | `crates/blyg-app/src/never_rename_tests.rs` |
| e2e signs in only through `studio_login` and waits out a 429 only through `retry_429` | `crates/blyg-core/tests/it/e2e/sign_in_path.rs` (`one_sign_in_path`, runs in the normal `cargo test`) |
| no entries are written into `CHANGELOG.md` between releases (a change adds a fragment in `changes/unreleased/`) | `scripts/changelog-stitch.py check`, run by `blyg-core/tests/it/changelog_stitch.rs` (`the_repos_fragments_pass_check`) |
| tests assert time only through `wait::Budget`, in ignored `timing_…` reports run by CI's non-blocking `timing` job (macOS only; Windows timing isn't measured); tests guarding an algorithm count work | `blyg-core/tests/it/timing_scan.rs` |
| blyg-core's test-only hooks (`test-support`) are enabled only in `[dev-dependencies]` | `blyg-core/tests/it/crate_graph.rs` (`test_support_is_only_a_dev_dependency`) |
| the contract Burrow targets is one its own guard accepts (widen `api::contract` in the change that vendors a new major) | `blyg-core/tests/it/contract_coverage.rs` (`burrow_accepts_the_vendored_contract`) |
| no feature is gated on a parsed version (`contractVersion` or a studio version); the functions that may read one are one reviewed list | `blyg-core/tests/it/version_gates.rs` (scan of product code; `ALLOWED`) |
| every capability gate is listed as guarding a read or a write and asks for a named `…OPERATION…` constant; a read or display gate names only operations the contract grants on the read scope | `blyg-core/tests/it/capability_gates.rs` (`GATES`, over `openapi-current.json`'s `security` scopes) |
| a store function that deletes rows of a list table (subscriptions, hoppers, reading, items, versions, shelf) names the prune decision (`api::Prune`), is private, or is on the reviewed list | `blyg-core/tests/it/list_prune_scan.rs` + `tests/fixtures/list-prune-allow.txt` |
| every e2e subscription is bound to a drop guard that names it (`Unsub`, `ApiUnsub`) | `blyg-core/tests/it/e2e/subscribe_guard.rs` (runs in the normal `cargo test`) |
| a `BLYGGER_*` variable this build ignores is said at startup (a debug build refuses to start); every variable has a class | `crates/blyg-app/src/dev_flags.rs` (`ignored`, and the test `every_blygger_name_has_a_class`) |
| a key or click modifier handled outside the keymap is on a reviewed list with its chord (checked against the Windows keymap), and none reads the platform key alone | `crates/blyg-app/src/key_handlers_tests.rs` + `crates/blyg-app/fixtures/key-handlers-allow.txt` |
| tests hold cmd for a click through `Modifiers::secondary_key()` or a per-platform `match`, never the platform key bare (a deliberate use says `the platform key itself` in a comment) | `crates/blyg-app/src/key_handlers_tests.rs` (`tests_dont_hold_the_platform_key_bare`) |
