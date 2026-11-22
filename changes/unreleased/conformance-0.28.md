section: Fixed

- **Preview: a quote that leads back to the post is refused before
  publish.** When a thread quotes one of your own threads that quotes it
  back, the preview now shows that quote as unresolved ("circular
  transclusion") and blocks publishing, as the studio does (spec §10.2).
  Before this fix, the cycle showed only as a marker one level down, and
  publishing failed on the server (`crates/blyg-app/src/studio/resolver.rs`).
- **Preview: numbered lists keep their first number.** A thread's list that
  starts at 3, or a quoted list that starts at 7, now keeps that number,
  as studio 0.69 does (spec §16.15, decision #114).
  Before this fix, it restarted at 1 (`crates/blyg-render/src/sanitize.rs`).
