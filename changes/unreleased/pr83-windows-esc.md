section: Fixed

- **Windows: Esc closes sheets, palettes, the Menu and the pane, even after a
  click in a web page.** A WebView2 that has the keyboard (the browser pane's
  page, the preview, the reader's post) keeps every key from the app, so
  after a click in a page Esc did nothing to an open sheet, palette or the
  pane. Each web view now passes Esc on: the keyboard comes back to the app
  and Esc is handled there, as on the Mac. In the browser pane the page
  still sees the key too (a site's own pop-up closes), so there the first
  Esc only takes the keyboard back and closes an open sheet or palette, and
  never closes the pane, which could hold a half-written post; a second Esc
  does. A page in full screen keeps Esc to leave full screen. The Menu
  button's panel closes on Esc before the search field can take the key. A
  click anywhere in the window (outside a page) takes the keyboard back from
  the web views, as clicking the app does on the Mac
  (`crates/blyg-app/src/app/esc.rs`, `webview2_host::on_escape`). Thanks to
  @patwater ([#83](https://github.com/aneeshsathe/blygger-desktop/pull/83)).
