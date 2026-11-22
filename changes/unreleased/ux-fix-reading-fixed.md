section: Fixed

- **Settings: locked Background rules don't act like buttons.** For a
  sign-in that can read Background but not change it, the greyed rules no
  longer take a keyboard stop (where space did nothing), a pointer cursor or
  a hover (`crates/blyg-app/src/reading/background.rs`).
