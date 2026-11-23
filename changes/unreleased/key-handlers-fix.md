section: Fixed

- **Ctrl+click works on Windows as ⌘-click does on a Mac.** In the
  Reader, Ctrl+click picks a post instead of opening it. In the stream,
  Ctrl+click on a link to a blyg post keeps it a link and opens it in the
  browser pane over the whole reading area. Clicks used to read the
  Windows key, which Windows keeps for itself. A Mac turns Ctrl+click into
  a right-click (the post's menu) before Burrow sees it, so nothing changes
  there (`keymap::secondary_held`).
- **⌘⏎ (Ctrl+Enter on Windows) saves Blyg › Site….** Publish's ⌘⏎ took
  the key first, so the sheet's own handler never ran, on either
  platform. It's now a keymap row for the sheet (`SiteSheet`) and its
  fields, listed in the shortcut reference.
