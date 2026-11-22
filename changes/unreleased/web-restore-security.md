section: Security

- **Someone else's post can't reach the app's own hooks.** The reader's
  sanitizer (`crates/blyg-app/src/studio/sanitize.rs`, shared by the desktop
  reader and the web app) keeps only the classes and `data-*` names the
  renderer emits, so a post can no longer carry the attributes and classes
  the pages listen for or draw as buttons. In the web app, the links it
  draws from your blyg's answers (a mention's source, a sent Webmention's
  target, a peeked post's page) are http(s) only; a page whose address
  doesn't give a plain mount refuses to run; owner-API paths refuse encoded
  dots and slashes; and the page loads under a Content-Security-Policy.
