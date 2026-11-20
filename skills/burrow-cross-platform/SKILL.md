---
name: burrow-cross-platform
description: Check a Burrow change for Windows + macOS compatibility. Use when a change touches keys, menus, file paths, web views, the browser pane, native APIs, or adds a cfg!(target_os) gate.
---

# Windows + macOS compatibility pass

Windows CI is the only Windows machine most changes ever see, and one release
took four CI rounds to go green there. Check these before pushing.

## Keys and menus

- App text is written in macOS terms and respelled for Windows by
  `keymap::hint` / `platform_text::Text` (a test fails on any other literal
  glyph). Text Windows says differently, not just with other keys, is
  `ui_text_per_platform(mac, windows)`, never a `cfg!(target_os)` branch.
- Windows keys are the macOS ones respelled; `keymap.rs` tests refuse a
  Windows key that AltGr typing would trigger (Ctrl+Alt is AltGr) or that
  clashes, and point at `WINDOWS_OVERRIDES`. Pick the override's key yourself.
- Windows has no global menu bar; the Windows Menu button's panel must offer
  every menu item. Panels over web views must hide the view.

## Testing wording

Six tests once compared UI text with Mac glyphs ("On your shelf · ⌘B"); they
passed on macOS and failed only on Windows CI ("On your shelf · Ctrl+B").

- A test of text that can hold a key writes the expected string through
  `keymap::hint("… ⌘B")` (or `hint_owned`), so it holds on either platform.
- A test that means one platform's wording wraps the code that builds the
  text: `platform_text::spelled_as(Platform::Windows, || toggled_toast(true))`
  is "On your shelf · Ctrl+B" on any machine. Assert a literal Windows string
  at least once, so the test proves the spelling and not just `hint == hint`.
  Tests about macOS glyphs themselves pin `spelled_as(Platform::Mac, …)`.
- Only wording changes: `spelled_as` and the whole-suite mode leave the key
  bindings (`keymap::keys`, `keyless_here`, the table) as this machine's.
- Whole suite: `BLYGGER_TEST_SPELLING=windows cargo test -p blyg-app` starts
  every test thread in Windows' wording. Run it before pushing a wording
  change; CI's macOS job runs it.
- Wording read through `Platform::spelling()` (`hint`, `hint_owned`,
  `Text::get`) follows the override; a bare `cfg!(target_os = "windows")`
  branch doesn't, which is one more reason to write `ui_text_per_platform`.

## Platform features

- Gate on a capability function (e.g. `browser::pane_available()`), not on
  scattered `cfg!(target_os = "macos")` checks.
- Web views: WKWebView on macOS, WebView2 on Windows. WebView2 double-encodes
  `eval` results, reports no key modifiers on clicks, and completes
  navigations the policy cancelled; test those paths.
- Paths: build them with `Path`/`PathBuf`, never `/` string joins; data dir,
  config path and Keychain/Credential Manager names don't change per platform
  without a migration.

## Tests and evidence

- Tests that only run on one platform say so; shared view-model tests must
  pass on both.
- If you couldn't run it on a real machine, say so in the CHANGELOG's "Not
  verified" note and the PR. Cross-compiling (e.g. the MinGW target) proves it
  builds, not that it works.
