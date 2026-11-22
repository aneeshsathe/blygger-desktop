section: Fixed

- **Saved no longer says "until Burrow is updated" before it has checked.**
  Opened offline with a Saved change waiting, Saved said the change was held
  for a newer Burrow. Until the blyg's contract is checked, the change now
  waits as "syncing" and nothing is removed (`Store::newer_contract`).
- **A studio upgraded mid-session removes nothing between safety nets.**
  On a blyg without change counters, every pull lays your items, thumbs,
  hoppers and read marks over what's held here. A removal on a check from an
  earlier pull now reads the contract again first (`Engine::lay_list`).
- **A read-only sign-in follows Saved.** It now merges the blyg's Saved list
  three ways, as a writer does, without sending anything: a post another
  device took off comes off here too, and what you took off here stays off.
- **A post taken off Saved stays off with no sync behind it.** On a fresh
  install on a newer studio, or a sign-in that has only ever read Saved, what
  you take off is recorded as you take it off (`shelf::TAKEN_OFF_META`).
- **Background on studios 0.49.12 to 0.60 is asked once.** A studio that
  ignores the Background filter is judged on the lens's first page, and
  remembered for that studio's contract version, not just the session.
- **Background rules are greyed for a sign-in that can't change them**, with
  the reason shown, instead of failing when turned.
- **On a newer studio, a Saved list, a hopper the blyg can't find, and the
  contact book remove nothing here.** Each now takes the remove-or-keep
  decision itself (`Store::apply_shelf`, `Store::hopper_not_found`,
  `Store::take_served_contacts`) rather than relying on its caller. A hopper
  your blyg answers "not found" for while saving an edit stays, with its
  waiting changes, until Burrow knows the contract.
- Changelog display is read once a pull rather than on each call.
