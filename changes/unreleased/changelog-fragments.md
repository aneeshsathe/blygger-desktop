section: Development

- **Changelog fragments.** A change adds `changes/unreleased/<slug>.md` instead of editing `CHANGELOG.md`, so parallel branches no longer conflict over it; `scripts/changelog-stitch.py stitch <version>` writes them into the release's section and deletes them. The workspace tests check the fragments' format.
