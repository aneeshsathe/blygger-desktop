---
name: verify-burrow
description: Prove a Burrow UI change works in the real app, with a screenshot of its own window as evidence. Use before calling any user-visible change done, when asked to run, screenshot or check the app, or when tests pass but you haven't seen the screen.
---

# Verifying Burrow in the real app

Tests passing is not proof a screen works. Run the debug build on sample data,
play a demo scene through the user's path, and capture the app's own window.
`scripts/verify/burrow-verify.sh` does each step; it is macOS only.

The script refuses what would touch real data: it always runs fake mode (sample
data, in-memory tokens, no blyg), puts config and data in a throwaway dir,
runs only a `.../debug/blygger` build (never `/Applications/Burrow.app`), never
takes focus, and refuses to override `BLYGGER_FAKE`, `BLYGGER_DATA_DIR`,
`BLYGGER_CONFIG`, `BLYGGER_TEST_TOKEN` or `HOME`. Never send synthetic
keystrokes or clicks to the OS, and never `pkill` by name.

## Launch

```sh
export CARGO_TARGET_DIR=<your own target dir>
cargo build -p blyg-app
scripts/verify/burrow-verify.sh launch <scene> [BLYGGER_THEME=paper ...]
```

It prints the **run dir**. Ready means usable: the app printed
`blygger: demo <scene> ready` (every scene step has run, `demo.rs`) and owns an
on-screen window; it then holds `BURROW_VERIFY_HOLD` (1.5 s) for the scene's
last animation. It gives up after `BURROW_VERIFY_WAIT` (120 s) and stops the
app. `scripts/verify/burrow-verify.sh scenes` lists the scenes (the build's
`blygger +scenes`, from the registry in `crates/blyg-app/src/scenes.rs`); an
unknown `BLYGGER_DEMO` stops the app and names near matches. Teardown is
`stop` (below).

## Doctor

```sh
scripts/verify/burrow-verify.sh doctor <run dir>
```

Read-only. Checks the PID is still the binary this run started, that it is a
debug build, that the scene played, that `app.log` has no panic, that the app
owns a window, and that its data dir is the run's throwaway one. Exit 1 on any
`FAIL`: fix that before reading anything into a screenshot.

## Drive

A scene is the driver: `BLYGGER_DEMO=<scene>` calls the same methods the keys
call, from inside the app. Every scene is a row in
`crates/blyg-app/src/scenes.rs` (its name and area); `demo.rs` sends each step
to its area's handler (`demo.rs`, `reading/demo.rs`, `notes/drawer.rs`,
`settings_page.rs`, upkeep, …), which matches on the `Scene` variant. To
check a new behaviour, add a scene for it (a registry row, then an arm in its
handler; steps 0, 1, 2 run at about 0.4, 1.3 and 2.5 s) rather than poking
state from outside. Extra `BLYGGER_*` variables tune
a scene (`BLYGGER_DEMO_SCROLL=bottom`, `BLYGGER_THEME=<name>`;
`BLYGGER_TEST_SPELLING=windows` shows the app's text as Windows spells it,
`Ctrl+B` for `⌘B`, in a debug build: the wording only, not the keys). A starting
config goes in `BURROW_VERIFY_CONFIG=<file>` (only `*.example.com` and
localhost URLs). The feature map below names the scenes per feature.

## Evidence

```sh
scripts/verify/burrow-verify.sh shot <run dir> [name]
```

Captures only the windows the run's PID owns (`screencapture -l`, window ids
from `winid.swift`) into `<run dir>/shots/<name>-<window id>.png`. Run dirs
live in `BURROW_VERIFY_OUT` (default `target/verify/`); `run.env` and
`app.log` stay with the shots after cleanup. Shots taken while the Mac is
locked are refused: `launch` and `shot` exit 2 while `ioreg -n Root -d1 -a`
says `IOConsoleLocked` is true (the 0.28 user-testing pass took its shots
that way and missed obvious layout bugs), so unlock it and run again; never
work around it. A shot that comes back one flat colour (checked by
`scripts/verify/shotcheck.swift`) is renamed `*.BLANK.png` and fails the
shot. Open the PNG and look at it:
check the thing the change is about, not just that a window appeared. Report
the PNG paths with your result.

## Cleanup

```sh
scripts/verify/burrow-verify.sh stop <run dir>
```

Signals only the PID it started (after checking it is still that binary), then
deletes the run's data and config; the evidence stays.

`scripts/verify/burrow-verify.sh run <scene>` does launch, doctor, shot and
stop in one go.

## Feature map

`features/README.md` names Burrow's main features and how to reach each one
through a scene. If a scene's sample data can't show a journey, write that
under the feature's Gotchas; never weaken the journey to make it pass.

## Keeping it working

- `crates/blyg-app/tests/verify_script.rs` runs `burrow-verify.sh selftest`
  (the scene list and the refusals, a locked screen and a blank shot among
  them) in `cargo test` on macOS.
- `scenes.rs`'s tests fail on a feature file naming a scene that doesn't
  exist, so renaming a scene means updating the feature files with it.
- Those checks prove the names, not that each feature still renders.
  `user-testing-pass` walks only the features an update touched (plus one
  everyday journey), so `burrow-release` runs every mapped feature's
  starting scene live before each release.
