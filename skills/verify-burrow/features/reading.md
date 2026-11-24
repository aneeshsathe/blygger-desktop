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
| `rd-feed`, `rd-raw` | the Reader on the pace-layered Feed (order in the header); Raw |
| `rd-agentic` | the Agentic feed tab with the sample edition (masthead, sections, cards) |
| `rd-agentic-empty` | the Agentic feed tab on a blyg with no edition yet: its explanation, wrapped across the reading pane |
| `rd-agentic-empty-narrow` | the same in an 800 px window: the explanation still wraps |
| `rd-agentic-unwritten` | the Agentic feed with no edition: how one gets made (⌥⌘E, Settings › Agentic feed, an assistant on your blyg) and the "Write an edition now" line |
| `rd-agentic-writing` | ⌥⌘E with a slow stand-in provider: "Writing an edition with Codex (local)…" above the empty tab |
| `rd-agentic-written` | the edition Burrow wrote (stand-in provider): "by Burrow · Codex (local) (demo-model)" |
| `rd-saved` | Saved (the shelf, renamed), its sync line, the first post open |
| `rd-stream-copies` | Kit's tides opened from the stream: "the author says it is also at …" under its source line |
| `rd-stream-catchup-summary` | Catch up on Rue's thread, then `s`: the blyg's summary first |
| `newer` | the toast sync shows for a blyg on a newer studio ("Your blyg runs a newer studio") |
| `rd-stream-date` | the first card's date clicked: its post open in the side pane (studio 0.71) |
| `rd-gen-read` | Lin's post in the Reader, scrolled to its end: what its generated sentence read (Kit's post, told; a web page) |
| `settings-background-locked` | Settings › BACKGROUND for a sign-in that can't change the rules: greyed, with the reason; ⇥ ×5, →, space |

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

Journeys (0.28.0):
- Studio 0.68's tabs: Reader sources, under Feed: Feed, Agentic feed, Saved,
  Background, Raw. The Feed's order (pace-layered or chronological) is the
  header's control and Settings › READER ORDER.
- Agentic feed: pick the tab; with an edition on the blyg it shows a
  masthead, then sections of cards with the assistant's headline, summary
  and reason. Its name is Blyg › AI on Your Blyg… › Agentic feed tab name.
- Burrow writes the edition: in the empty Agentic feed tab, ⌥⌘E (or the
  line's “Write an edition now”) says “Writing an edition with <provider>…”,
  then shows the edition signed “Burrow · <provider>”. Settings (⌘,) ›
  AGENTIC FEED › “Write the edition after a sync” does it after a sync,
  with the provider and the gap chosen there.
- Saved: “+ Save” under a post, ⌥⌘B (Ctrl+Shift+A on Windows), Post ›
  Saved… (⌘B). Its line says whether it's synced.
- Copies elsewhere: a stream card whose author lists a copy says so under its
  source line; a click opens the copy.
- A blyg on a newer studio: a toast once a session; syncing goes on.

`scripts/verify/burrow-verify.sh scenes | grep '^rd-'` lists the rest.

## Gotchas

- Sample reading is `fake_reading.rs` / `fake::reading_seed` (Ada, Lin, Omar
  on `*.blyg.example.com`). Unread counts in the title bar come from it.
- No sample post has its current version pinned (forking it, 0.28.0): the
  test `a_pinned_current_version_forks_from_the_current_view` pins one with
  `FakeBackend::pin_current`; no scene can.
- No followed sample post lists a copy elsewhere: only Kit's tides (fetched
  through Lin's fork) does, so a stream card with the line isn't shown.
- `settings-background-locked` turns the fake's placing off itself; a real
  read-only sign-in is `real-server-testing` with an owner:read token.
- `newer` shows the toast sync raises; that sync keeps going and removes
  nothing on a newer contract is the e2e's (`real-server-testing`).
- A scene that opens a post's web view (the original page) shows a blank
  pane in fake mode: there's no network.
