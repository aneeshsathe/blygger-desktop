section: Development

- **Scenes: the Settings BACKGROUND scenes reach their row by name.**
  `settings-background-keys` tabs until the cursor is on BACKGROUND (READ
  MARKS sits above it now); `settings-background-locked` presses no key (a
  locked rule is no tab stop) and scrolls to the greyed row.
- **Scenes: one Agentic empty scene, and `+` tour scenes only where they act.**
  `rd-agentic-empty` and `-narrow` are gone (`rd-agentic-unwritten`, with
  `BLYGGER_SNAPSHOT_WIDTH`, shows the same); `tut-<id>+` exists only for a
  step whose key the scene does (`DEMO_ACTS` in `onboarding/tutorial.rs`).
- **Checks: Windows wording, contrast and accent text.** The
  `platform_text.rs` scan also catches ⏎ ⇥ ⌫ in prose; a test computes the
  toolbar icons' contrast in every theme; another refuses accent text not
  drawn through `accent_text()`.
