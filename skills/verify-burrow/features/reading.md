# Reading what you follow

## Sub-features

- **Stream** (the default): one column of posts from blygs you follow, with
  inline quotes, Read more, Quote into a thread, Reply, Catch up, thumbs, and
  a compose box at the bottom.
- **Reader**: sources, list and post in three panes; smart feeds, folders,
  search, mark all read.
- Responses (mentions), Subscriptions (subscribe, rename, pause, folders),
  Discover, Background, a blyg's own site view, hoppers.

## How to get to it (user POV)

The Reading tab in the title bar (or the key shown in the tour). Stream and
Reader are the toggle at the top right. j/k move, ⏎ reads more, / searches.

## Driving it with burrow-verify

| Scene | Shows |
|---|---|
| `rd-stream` | the Stream on sample data |
| `rd-stream-quote` | the Stream moved to a reply with a quoted post |
| `rd-stream-compose-hold` | the compose box with text in it |
| `rd-stream-search` | search in the Stream |
| `rd-reading` | Reader with a post open |
| `rd-three-pane` | Reader's three panes |
| `rd-mentions` | Responses |
| `rd-subscribe` | Subscriptions: subscribing to a blyg |
| `rd-discover`, `rd-background` | Discover and Background |

`scripts/verify/burrow-verify.sh scenes | grep '^rd-'` lists the rest.

## Gotchas

- Sample reading is `fake_reading.rs` / `fake::reading_seed` (Ada, Lin, Omar
  on `*.blyg.example.com`). Unread counts in the title bar come from it.
- A scene that opens a post's web view (the original page) shows a blank
  pane in fake mode: there's no network.
