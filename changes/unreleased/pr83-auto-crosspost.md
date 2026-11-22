section: Added

- **Cross-post automatically, per post.** The Publish sheet has **Then
  cross-post to** with a chip per granted cross-post site (Substack Notes,
  Bluesky, Farcaster). Tick one, and every time that post is published
  Burrow starts that cross-post by itself, one site after another. ⇧⌘P with
  the post open has **Cross-post automatically to <site>: on/off** rows to
  turn each one on or off. The choice is remembered per post (in
  `state.json`, never on your blyg), and other posts are unaffected. It only
  starts the run: each one still shows the text and posts only when you
  click Post. Cancelling one skips to the next. A run that leaves the page
  to you stops the rest, and its toast names the sites still to do. Each
  site posts once per post: a later publish skips a site the post's copies
  elsewhere already list, and a run that posted without recording its copy
  turns that site off for the post
  (`crates/blyg-app/src/extensions/auto_crosspost.rs`). Thanks to @patwater
  ([#83](https://github.com/aneeshsathe/blygger-desktop/pull/83)).
