---
name: burrow-agent-cohort
description: Run several AI coding agents on Burrow at once without them clobbering each other or the machine. Use when splitting work across parallel agents or worktrees.
---

# Running agent cohorts on Burrow

## Isolation

- **One worktree per agent**, never a shared working tree: a `git checkout` in
  a shared tree silently loses another agent's uncommitted work.
- **Own cargo target dir per agent** (e.g. `../burrow-target-<name>`), outside
  the worktree. Never share one across worktrees: cargo hashes path crates the
  same in each, so agents overwrite each other's artifacts and link the wrong
  branch's code. Delete it after the final commit.
- **Non-overlapping scopes.** Give each agent a subtree. When two must touch a
  file, give each a named region and say so in both prompts. One agent per
  cohort owns `Cargo.toml` files.

## Load

- At most **3 Rust-building agents at once** on an 8-core machine; queue the
  rest. Don't run your own cargo builds while a cohort compiles.
- Check free disk first; each cold GPUI build is 6–13 GB.
- Peer sessions doing heavy work can be asked to hold until integration ends.

## Hand-off

- Agents commit on their own branch; they never push, merge or deploy.
- The orchestrator merges into an integration branch, runs the full gate there
  (in its own target dir), then removes merged worktrees and their target dirs.
- Briefs state: the goal, what's already ruled out, the files, the boundaries,
  the test rules, and the exact report wanted.
