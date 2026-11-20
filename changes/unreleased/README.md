# Unreleased changes

Each change adds one file here, `changes/unreleased/<slug>.md`, instead of
editing `CHANGELOG.md`. Branches then never conflict over the changelog,
and whoever makes the change writes its entry. At release,
`scripts/changelog-stitch.py stitch <version>` moves every fragment into a
new `## [<version>] - <date>` section of `CHANGELOG.md` and deletes them.

## Format

```markdown
section: Added

- **Area: what it does now.** Details for someone using Burrow, with the
  `paths` a reader would look at. Continuation lines are indented.
```

- Line 1 is `section: <name>`, one of: Added, Changed, Deprecated, Removed,
  Fixed, Security, Development, Known issues, Not verified.
- Line 2 is blank.
- The rest is one or more `- ` list items, written as the changelog's
  entries are. No headings.
- A change with entries in two sections writes two fragments
  (`reading-parts.md`, `reading-parts-fix.md`).
- The slug is short and names the change (`tangling.md`, `sync-304.md`).
  Within a section, the release lists fragments in file-name order.

`scripts/changelog-stitch.py check` validates them; the workspace tests run
it too, so a malformed fragment fails `cargo test`.

The release notes in `site/src/releases.md` are written by hand at release,
from the stitched section: they tell a user what the release brings, grouped
by theme, which a list of entries doesn't.
