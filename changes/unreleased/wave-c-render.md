section: Changed

- **Mentions read in natural grammar** (blygger-studio 0.66). On a blyg that
  turns `@handle`s into contact cards, `@lin's notes` and an `@lin-driven
  agenda` both name Lin in the preview: a handle that names no contact falls
  back to the longest part before a `-` or `_` that does, and an exact or
  longer handle still wins. Picking a contact from the `@` popup now writes
  `@handle ` with a space, as Studio does; a possessive, hyphen or closing
  punctuation typed straight after the pick takes the space's place.
- **The `[[` / `![[` picker stays shut in a finished reference** (blygger-studio
  0.66). Retyping a bracket of a `[[id | label]]` or `![[id]]` that a `]]`
  already closes no longer opens the picker, and the picker closes once you
  type a label's `|`. Shift+Enter stays a plain new line, as Studio now has
  it.
- **Rendering follows blygger-studio 0.68.** Studio now refuses what Burrow
  already refused or wrote: a fragment's own-line `![[id]]` (in its text or
  in generated output), a guessed `f/…/` address for another blyg's item
  with no `page`, and a baked quote inside a generated block's wrapper. The
  preview's reason for the first is Studio's own wording now, and its parity
  fixtures come from Studio 0.68 (`docs/RENDER.md`).
