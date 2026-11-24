# Settings

## Sub-features

- A full page (not a sheet) of the config file's options: theme, fonts,
  writing, reading, AI, extensions, the blyg's own settings.
- Changes write the Ghostty-style config file; the app reloads it.

## How to get to it (user POV)

Burrow › Settings… (⌘,), or the toolbar's settings button.

## Driving it with burrow-verify

| Scene | Shows |
|---|---|
| `settings` | Settings opened as a user does |
| `settings-full` | the page at a fixed size with the blyg's changelog loaded |
| `settings-full` + `BLYGGER_DEMO_SCROLL=bottom` | the end of the page |
| `settings-count-read` | READ MARKS' own row, "Count an entry as read" (when I see it / when I open it / only when I mark it) and how time on screen is measured |
| `settings-background-keys` | the keyboard on BACKGROUND's second rule (⇥ until the cursor is on BACKGROUND, below READ MARKS, then →): focus ring, ✓ / ○ off states, Withdrawn shown as fixed |
| `tb-settings` | the toolbar's way in |

```sh
scripts/verify/burrow-verify.sh run settings-full BLYGGER_DEMO_SCROLL=bottom
```

## Gotchas

- The run's config file starts empty (or from `BURROW_VERIFY_CONFIG`), so the
  page shows defaults. A setting changed in the scene is written to the run's
  throwaway config, never `~/.config/blygger/config`.
- The blyg's own settings come from the fake backend; there's no server to
  disagree with.
