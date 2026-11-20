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
| `rd-burst` | a thread's "+N parts": on its row, unfolded under its card, listed in the open thread |
| `rd-stream-burst` | the stream at a burst thread, its "+2 parts" folded |
| `rd-background-rule-off` | "Parts published with a thread" turned off: a part back in All, open |
| `rd-linked-from` | Responses with a post's link notices under "Linked from" |
| `rd-maths` | a post with formulas in the reader |
| `rd-responses-keys` | Responses with the row cursor on the link notice (j ×3); the header lists j/k, h, r, o |

Journeys (0.27.0):
- Why is this in Background? Reader › Background: each row carries a badge
  ("published with ‘<thread title>’", "no words of its own", "republished unchanged").
- Background rules: Settings (⌘,), the BACKGROUND row under CHANGELOG; turn a
  chip off (a click, or ⇥ past THEME then space), a toast says where its
  posts went, and they're back in the stream.
- A thread's parts by keys: `p` folds or unfolds them (open thread, else the
  selected stream card), 1–9 open one.
- Responses by keys: j/k move, h hide, r reply, o open ↗.
- "+N parts": click it under a stream card to unfold; the open thread lists
  them ("−N parts" folds); click a part to open it. There is no key.
- Link notices: Responses (⇧⌘M), under a post's responses, "Linked from".

`scripts/verify/burrow-verify.sh scenes | grep '^rd-'` lists the rest.

## Gotchas

- Sample reading is `fake_reading.rs` / `fake::reading_seed` (Ada, Lin, Omar
  on `*.blyg.example.com`). Unread counts in the title bar come from it.
- A scene that opens a post's web view (the original page) shows a blank
  pane in fake mode: there's no network.
