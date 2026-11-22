section: Not verified

- **Windows Esc:** the WebView2 accelerator-key hook and the Menu panel's Esc
  have not been run on Windows; the tests drive them with stub web views on
  macOS, and the Windows code is checked by Windows CI only.
- **Automatic cross-posting** was tested with the bundled cross-post
  extension running for real and a stub browser pane, up to its Preview
  sheet; no run went past it, and none reached a live site.
