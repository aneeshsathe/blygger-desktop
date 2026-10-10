# Building and testing

## Building from source

You need macOS or Windows, and Rust via [rustup](https://rustup.rs). The
toolchain version is pinned in `rust-toolchain.toml` and installs
automatically. (Windows: see [Building on Windows](#building-on-windows).)

```sh
git clone https://github.com/aneeshsathe/blygger-desktop && cd blygger-desktop
BLYGGER_FAKE=1 cargo run -p blyg-app --release   # the full UI on sample data, no server
cargo test --workspace
```

`BLYGGER_FAKE=1` runs the app on in-memory sample data, so you can try every
screen without a blyg. It never touches the real Keychain or checks for
updates.

To build the app bundle, zip and dmg in `dist/`:

```sh
rustup target add x86_64-apple-darwin   # only for universal builds
scripts/bundle.sh                        # universal (the default)
scripts/bundle.sh --arch arm64           # this Mac's architecture only
```

`scripts/bundle.sh` signs ad-hoc unless Developer ID credentials are set in
the environment. With them, it signs, notarizes and staples the app and the
dmg. `scripts/sign.sh` documents the variables.

## The checks

Every commit must pass:

```sh
cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

CI (`.github/workflows/ci.yml`) runs the same on every pull request, on
macOS and on Windows, and builds this documentation site. The Windows job
also builds the release zip and keeps it on the run's page for a week, so a
pull request can be tried on a real Windows machine.

## Building on Windows

Install Rust with `rustup` using the MSVC toolchain, which needs the Visual
Studio Build Tools with the "Desktop development with C++" workload (it
includes the Windows SDK). Then, in PowerShell:

```powershell
$env:BLYGGER_FAKE = "1"; cargo run -p blyg-app   # on sample data, no server
cargo build --release -p blyg-app                # target\release\blygger.exe
pwsh scripts/package-windows.ps1                 # dist\Burrow-<version>-windows-x64.zip
```

Debug builds keep a console window for their logs; release builds are GUI
programs (`blygger +action` still prints to the terminal that ran it).

## Keeping Windows building

Windows is a supported platform, so every change has to keep it building,
linting and passing tests (the CI job above), and must not change macOS
behaviour on the way. The conventions:

- **Platform code goes behind `cfg`.** Use `#[cfg(target_os = "macos")]` /
  `#[cfg(target_os = "windows")]` (or `cfg!(…)` in an expression when both
  arms compile everywhere). Prefer putting the switch in one function in
  `crates/blyg-app/src/platform.rs` (the Windows menu, `key_button`, the dock
  icon) over scattering `cfg` through views.
- **macOS-only dependencies** (`objc2`, `objc2-web-kit`, `block2`, …) go
  under `[target.'cfg(target_os = "macos")'.dependencies]`.
- **Keys and wording.** Write keys in the table in macOS terms
  (`cmd-…`); `keymap::platform_keys` respells them for Windows. A key or a
  Mac word in the app's text (`⌘G`, `Keychain`, `this Mac`) goes through
  `keymap::hint` (or `hint_owned` for a `format!`), which is the identity on
  macOS and respells it on Windows. Tests type keys through `keymap::keys`.
- **Tests that need macOS** (they run `ditto`, `codesign`, a WKWebView or
  shell scripts) are `#[cfg(target_os = "macos")]` or `#[cfg(unix)]`, not
  deleted.
- **Paths.** Build them with `Path::join` and `std::env::join_paths`, never
  with `/` or `:`; `blyg_core::config::paths::home_var` is `$HOME`, or
  `%USERPROFILE%` on Windows.

You can type-check the Windows build from a Mac with
`rustup target add x86_64-pc-windows-msvc` and
`cargo check --workspace --all-targets --target x86_64-pc-windows-msvc`;
crates with C code (the bundled SQLite) need a Windows C toolchain such as
[cargo-xwin](https://github.com/rust-cross/cargo-xwin) for that, or leave it to CI.

### How the port works

| Area | Windows |
|---|---|
| Preview and editor | A WebView2 surface through `wry` (`studio/webview.rs`), with the macOS page script, IPC and navigation guard. GPUI's topmost DirectComposition layer is turned off at startup (`GPUI_DISABLE_DIRECT_COMPOSITION`), because it would cover the child webview. Creating a WebView2 runs a nested message loop, which crashed the app when GPUI asked for one mid-frame, so `DeferredSurface` builds it from a thread timer in GPUI's top-level loop and replays what it was asked meanwhile. |
| Keys | `keymap::platform_keys` respells `cmd` as `ctrl` (and ⌘Y as Ctrl+Shift+Y); `glyphs` and `hotkey_glyphs` write `Ctrl+Shift+X`; the clash tests use a Windows list of system shortcuts. |
| Window and menu | The native title bar, and `windows_menu.rs`: a Menu button that lists `cx.get_menus()`. The toolbar strip isn't a window drag area on Windows, because GPUI answers `HTCAPTION` for it and clicks on the buttons drawn over it would move the window. |
| Secrets | `KeychainTokenStore` is Windows Credential Manager (`keyring`'s `windows-native`), and splits secrets longer than one entry holds (1,280 UTF-16 units, less than a ChatGPT sign-in) across `account#1`, `account#2`… (`config/tokens.rs`). |
| Paths | `%USERPROFILE%` for `~`, `%APPDATA%\Blygger` for the second config file and `%LOCALAPPDATA%\Blygger` for app state (`blyg-core/src/config/paths.rs`). |
| Fonts | `prefs::system` names Windows fonts in place of the macOS system fonts, and `prefs::find` maps a macOS font named by a theme or the config to its substitute. |
| External programs | Notepad for the config file, Explorer for Reveal, the shell's URL handler for the browser, `.exe`/`.cmd` CLI shims. The CLI bridges start with `CREATE_NO_WINDOW`, and npm's `claude.cmd` shim gets the system prompt through `--system-prompt-file`, because cmd.exe can't pass a multi-line argument. |
| Updates | Off on Windows (`update::disabled_reason`); the updater's tests are macOS-only. |
| Executable | `build.rs` embeds `packaging/Blygger.ico`; GPUI's `windows-manifest` feature supplies the per-monitor-DPI manifest. A panic writes `%LOCALAPPDATA%\Blygger\crash.log` and shows a message box. |

`cargo test` covers the core against a mock HTTP server (sync, the outbox,
conflicts, scratch notes that must never touch the network), the config
parser, the renderer's parity fixtures, the AI prompts' parity with the
server, the updater (offline, with a throwaway signing key), and the app's
view models, keymap, menus and toolbar.

## End-to-end tests

End-to-end tests run blyg-core against a real Worker under `wrangler dev
--local` (never a deployed blyg). Point them at a Worker checkout that carries
the extensions in [Server requirements](../server.md) (blygger-studio 0.9 or
later; build it first):

```sh
(cd /path/to/worker && npm install && npm run build)
BLYG_WORKER_DIR=/path/to/worker scripts/e2e-local.sh
```

Never test against a real blyg: use the mocks, the sample data, or
`wrangler dev`.

## Parity fixtures

The preview renderer and the AI prompts are checked byte for byte against the
reference Worker's own TypeScript. When the Worker changes, regenerate the
fixtures:

```sh
node crates/blyg-render/tests/fixtures/gen_parity.mjs /path/to/worker/package
node crates/blyg-ai/tests/fixtures/gen_parity.mjs /path/to/worker/src
```

See [blyg-render](render.md#parity) and [blyg-ai](ai.md) for the details.

## This documentation site

The site is an [mdBook](https://rust-lang.github.io/mdBook/) in `site/`. Its
pages include the files in `docs/` where they exist, so there's one source for
each. Two pages are generated from the code:

| Page | Generated from | By |
|---|---|---|
| `site/src/generated/config-keys.md` | `crates/blyg-core/src/config/keys.rs` | `crates/blyg-core/tests/config_docs.rs` |
| `site/src/generated/keybindings.md` | `crates/blyg-app/src/keymap.rs` | `crates/blyg-app/src/keymap_docs.rs` |

`cargo test` fails while either is out of date. After changing a config key
or a shortcut, regenerate them and commit the result:

```sh
scripts/gen-docs.sh
```

To build the site locally:

```sh
scripts/build-site.sh  # into site/book/, with the pinned mdBook; checks links
```

It downloads the pinned mdBook release into `target/` the first time
(checksum-verified), and fails on an mdBook error (such as a missing
`\{{#include}}` file) or a broken internal link. CI runs it on every pull request. With mdBook installed
yourself, `mdbook serve site` serves the site at `http://localhost:3000` and
rebuilds it as you edit. Run `python3 scripts/community-extensions.py render`
once first: the Community extensions page is generated, not committed.

`.github/workflows/pages.yml` publishes the site to GitHub Pages on every push
to `main` that touches the site, `docs/`, the config keys, the keymap or the
changelog.
