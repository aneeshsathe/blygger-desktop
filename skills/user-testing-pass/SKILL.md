---
name: user-testing-pass
description: Have agents use Burrow the way people do and report pain points and irritations, so every update ends with a UX pass. Use after the features of an update are merged and before the release, or when asked to user-test, find UX problems or play a persona.
---

# A user-testing pass after every update

Tests prove the code does what we meant. They don't show that it is pleasant,
findable or calm to use. Every update gets a pass where agents act as users,
use the features the update touched plus one everyday journey, and report what
got in their way. The top findings are fixed before the release; the rest go
in the release's follow-ups.

## When

After the update's feature branches are merged into the integration branch and
the gate is green, before `burrow-release`. Re-run it for a journey after its
fixes land.

## Personas

Give each tester agent one persona and two or three journeys, and make the
journeys differ across testers. These are the usual ones:

- **First week:** new to Burrow and Blygger. Is the feature findable without
  the docs? Do the words make sense? Does the tour mention it?
- **Daily reader:** opens Burrow to catch up. Counts, unread state, what is in
  Background and why, and how fast the reading list moves.
- **Writer:** drafts, previews, publishes and revises. Errors, confirmations,
  what happens to the text, and whether publishing is safe and undoable.
- **Keyboard-only:** never touches the mouse. Every step must have a key, with
  the key shown, and focus must land where the next keystroke expects it.
- **Windows wording:** reads every hint as a Windows user would
  (`BLYGGER_TEST_SPELLING=windows`). ⌘ must never be shown.

## How a tester uses the app

The rules in `verify-burrow` apply: never send synthetic OS keystrokes or
clicks, never `pkill` by name, and never use real data.

1. **Find the journey:** in `skills/verify-burrow/features/*.md`, under "How to
   get to it (user POV)". If the journey isn't there, write it there first.
2. **Walk it with scenes:**
   - Run `scripts/verify/burrow-verify.sh run <scene>`, one scene per step of
     the journey.
   - Look at every screenshot as that persona would. Don't just check that the
     window opened.
   - Shots taken while the Mac is locked are refused (the script exits 2), and
     a blank, one-colour shot fails its scene: the 0.28 pass's locked-screen
     shots missed obvious layout bugs. Stop and report it; a pass with no
     real shots is not a pass.
   - Where a step has no scene, add one to the registry
     (`crates/blyg-app/src/scenes.rs`), calling the same methods the keys call.
3. **Type it in-process:** for the keyboard-only and writer journeys, also
   drive the keys inside the app in a GPUI test.
   - Use `VisualTestContext`, the `simulate_keystrokes` family, and
     `crate::test_wait` for waits. `flow_tests.rs` shows the pattern.
   - This shows focus, key conflicts and dead keys, which a scene skips because
     it calls methods directly.
4. **Check the server side:** what needs a real server, such as errors,
   confirmations and slow responses, is exercised with the e2e harness
   (`real-server-testing`). Note what the fake data can't show under the
   feature's Gotchas; never weaken the journey to fit the data.
5. **Time it:** count the steps and keys a journey takes. Note any wait over
   about 100 ms that the UI doesn't acknowledge.

## What to report

Each finding gets:
- a severity:
  - **blocker:** the persona can't finish;
  - **irritant:** they finish, but annoyed or unsure;
  - **polish:** small;
- the persona and journey;
- the steps;
- what they expected and what happened;
- the screenshot path;
- a suggested fix, with the file to change if it's known.

Also list what worked well, so a fix doesn't break it. Report observations,
not code reviews: "I didn't know the post was published" beats "the toast
lacks a role".

## After the pass

- The orchestrator merges duplicate findings and ranks them. It then fixes the
  blockers and the top irritants in the same update, in agent waves (see
  `burrow-agent-cohort`), and re-runs the affected journeys.
- Everything left goes into the release's follow-ups. When a finding is a
  recurring mistake, make it a check (`burrow-code-shape`).
