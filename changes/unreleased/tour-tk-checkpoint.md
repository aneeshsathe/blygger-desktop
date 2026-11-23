section: Added

- **Tour: "When your blyg can't read a link"** in the AI and TK tour (since
  0.28.0, the first step What's new shows). ⌘G on the tour's sample blyg
  stops at the checkpoint sheet, one of the gap's two links unreadable;
  “Details” (d), ⏎ to generate anyway and esc are live, and once it fills
  the gap what it read is listed under the preview. The step's ⌘G asks the
  sample blyg; every other step keeps the canned AI
  (`crates/blyg-app/src/onboarding/tutorial.rs`, `Setup::TkOnBlyg`).
