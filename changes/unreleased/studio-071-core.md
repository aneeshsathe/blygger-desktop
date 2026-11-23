section: Fixed

- **Sync: a post whose generated text read a web page keeps that
  disclosure.** Studio 0.70 records the web pages a `[TK]` scope's model
  read as `{url, cited}` sources (decision #117). Burrow read such a scope
  as having no provenance, so an edit that moved the post's scopes sent it
  back as `null` and the blyg lost the disclosure. Web-page sources now read
  into `ScopeProvenance::pages` (posts stay in `sources`, and a remote
  post's `cited` is kept too), follow their scope through edits, and go
  back to the blyg as they came. A followed post's `generated` reads the
  same way (`ReadingItem::generation`). A web page tells no one and is no
  lineage (`ScopeProvenance::notifies`).
