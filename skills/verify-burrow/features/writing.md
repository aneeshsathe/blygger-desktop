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

```sh
scripts/verify/burrow-verify.sh run published
```

## Gotchas

- Sample posts are fixed (`fake.rs`); ids like `01J9QK3` are the lighthouse
  fragment. A scene that types appends to it.
- `live-*` scenes publish to a local studio and need `BLYGGER_FAKE` unset;
  this script always runs fake mode, so they aren't for it. Use
  `real-server-testing` for those.
- The toast fades after a few seconds: capture soon after `launch` (the
  default hold is 1.5 s).
