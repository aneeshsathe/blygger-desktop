# The notes drawer

## Sub-features

- A drawer on the right edge (the "Notes" tab) holding a private note per
  post, with the post's quote seeded at the top.
- "→ Notes" on a post adds it to the open note; the drawer stays out.
- Docking: the drawer can stay open while you write, and its edge drags to
  resize (double-click resets).
- It makes room for the browser pane instead of covering it.

## How to get to it (user POV)

Click "Notes" on the right edge, View › Notes (⇧⌘N), or "→ Notes" from a
post's actions. Esc inside the drawer, or ⇧⌘N again, closes it.

## Driving it with burrow-verify

| Scene | Shows |
|---|---|
| `notes-open` | the drawer out over the Stream, with a note typed |
| `notes-after-add` | two posts added to the note with "→ Notes" |
| `notes-posts` | the drawer over Posts |
| `notes-over-browser` | the drawer beside the browser pane |

```sh
scripts/verify/burrow-verify.sh run notes-after-add
```

## Gotchas

- The drawer slides on the wall clock; `launch` holds 1.5 s after the ready
  line. If a shot shows it mid-slide, raise `BURROW_VERIFY_HOLD`.
- `notes-over-browser` shows a blank page in the pane (fake mode has no
  network); the layout is what to check.
