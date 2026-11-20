# Writing and publishing your posts

## Sub-features

- The Posts list with the omnibar: search filters as you type; ⏎ with no match
  starts a draft seeded with the query.
- The editor: fragments (up to 1000 characters) and threads, autosave, the
  length counter in the status bar.
- Preview (⌘P), publish with a version note (⌘⏎), versions, Discard.
- Sync state in the status bar: synced, offline with pending changes,
  conflicts.

## How to get to it (user POV)

Burrow opens on Posts. Type in "Search or start writing…", pick a post with
↑/↓ and ⏎, write, then ⌘⏎ and a note to publish. The status bar says
"synced" or what's waiting.

**Tangling (studio 0.63).** In a thread, write `![[+ passage]]` on lines of
its own, or `[[+ idea]]` inline. The preview shows a dashed "NEW ITEM" quote
and a dashed link. ⌘⏎ and ⏎ (the note) bring the confirm step, "This will
publish N new items and this thread"; ⏎ publishes all, esc cancels. The
editor then shows `![[id | first words…]]` and the toast adds "· and N new
items". Revise as usual afterwards; there's no confirm step the second time.

**Maths (studio 0.62).** Write `$x^2$` inline or `$$…$$` on its own lines,
or use Edit › Format › Formula (⌃⌘M; Bold ⌃⌘B, Italic ⌃⌘I). ⌘P shows a
formula that doesn't parse in red. Replying to a post with formulas, a
passage selected through a formula gets "A passage can’t include a
formula…" instead of "quote only this".

## Driving it with burrow-verify

| Scene | Shows |
|---|---|
| `search` | live filter with highlighted matches, ↓ previewing the next row |
| `create` | ⏎ on no match: a new draft seeded with the query, typed into |
| `edit` | the list filtered to one post |
| `publish` | a published post edited (unpublished changes) |
| `published` | publish with a note: the "Published v1" toast, `v1` in the list |
| `preview` | the preview pane beside the editor |
| `long` | a fragment over the limit (the ⌘T hint) |
| `offline` | an edit while offline: pending count in the status bar |
| `conflict` | the conflict sheet |
| `rd-tangle-preview` | a thread with a passage and a placeholder, its preview beside it |
| `rd-tangle` | the same, published up to the confirm step |
| `rd-maths-preview` | a draft with formulas in the preview, a broken one in red |
| `rd-maths-quote` | a reply to Ada's maths post, a passage selected through a formula |

The keyed journeys (confirm, esc, publish all, revise; the format keys) are
typed in `crates/blyg-app/src/writer_flow_tests.rs`.

```sh
scripts/verify/burrow-verify.sh run published
```

## Gotchas

- Sample posts are fixed (`fake.rs`); ids like `01J9QK3` are the lighthouse
  fragment. A scene that types appends to it.
- `live-*` scenes publish to a local studio and need `BLYGGER_FAKE` unset;
  this script always runs fake mode, so they aren't for it. Use
  `real-server-testing` for those.
- The sample backend refuses an unconfirmed tangle as a studio does, but
  not a tangle in a fragment, more than 10 new items, or a formula that
  doesn't parse: it publishes those. Check the studio's refusals with
  `real-server-testing` (`e2e/tangle.rs`, `e2e/maths.rs`).
- The toast fades after a few seconds: capture soon after `launch` (the
  default hold is 1.5 s).
