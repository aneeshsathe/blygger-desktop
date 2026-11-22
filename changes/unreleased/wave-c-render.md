section: Changed

- **Mentions read in natural grammar** (blygger-studio 0.66). On a blyg that
  turns `@handle`s into contact cards and runs Studio 0.66 or later,
  `@lin's notes` and an `@lin-driven agenda` both name Lin in the preview: a
  handle that names no contact falls back to the longest part before a `-`
  or `_` that does, and an exact or longer handle still wins. There, picking
  a contact from the `@` popup writes `@handle ` with a space, as Studio
  does; a possessive, hyphen or closing punctuation typed straight after the
  pick takes the space's place, and one undo takes it back to the pick. On
  Studio 0.56 to 0.65, which publish `@lin-driven` as plain text, the preview
  and the pick work as before, as they do until Burrow has seen which Studio
  the blyg runs (it remembers that across launches).
- **The `[[` / `![[` picker stays shut in a finished reference** (blygger-studio
  0.66). Retyping a bracket of a `[[id | label]]` or `![[id]]` that a `]]`
  already closes no longer opens the picker, and the picker closes once you
  type a label's `|`. Shift+Enter stays a plain new line, as Studio now has
  it.
- **A fragment's quote line says what to do instead** (blygger-studio 0.67).
  The publish refusal, the status bar and the preview warning for an
  own-line `![[id]]` in a fragment now say "make this a thread, or link it
  with [[id]]", as Studio does, which now refuses the line too. The preview's
  parity fixtures come from Studio 0.68 (`docs/RENDER.md`).
