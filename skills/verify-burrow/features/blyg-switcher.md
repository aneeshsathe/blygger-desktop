# The blyg switcher

## Sub-features

- Several signed-in blygs, one active: the window title is the switcher.
- The switcher sheet: every blyg (1–9, ↑/↓, ⏎), Add a blyg, Manage (sign out
  of one, keep or delete its local copy).
- Switching saves the editor, swaps the backend and reloads posts, reading
  and mentions; a toast says where you are and what's still waiting.

## How to get to it (user POV)

Click the blyg's name in the title bar, or Blyg › Switch Blyg (⌃⌘0), or
Switch To ⌃⌘1…9.

## Driving it with burrow-verify

The switcher lists the config's `blyg` lines, so give the run a config:

```sh
printf 'blyg-url = https://a.blyg.example.com\nblyg = https://a.blyg.example.com\nblyg = https://b.blyg.example.com\n' > /tmp/two-blygs.conf
BURROW_VERIFY_CONFIG=/tmp/two-blygs.conf scripts/verify/burrow-verify.sh run switcher
```

| Scene | Shows |
|---|---|
| `switcher` | the switcher sheet |
| `switcher-manage` | Manage… |
| `switched` | switching to the second blyg |

## Gotchas

- In fake mode the second blyg is its own small sample (three drafts); the
  title shows `Burrow — b.blyg.example.com`. Outbox and sync behaviour on a
  switch need the switcher e2e against two local studios
  (`real-server-testing`), not this script.
- Without a config naming two blygs the switcher has only one to list.
- The "Switched to …" toast may have faded by the time of the shot; check
  the title instead.
