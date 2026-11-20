# Burrow's features, for verification

Shared names for what a user can do in Burrow, and how to reach each one with
`scripts/verify/burrow-verify.sh` (see `../SKILL.md`). One file per feature:

| Feature | File | A scene to start from |
|---|---|---|
| Writing and publishing your posts | `writing.md` | `published` |
| Reading what you follow (Stream and Reader) | `reading.md` | `rd-stream` |
| The notes drawer | `notes-drawer.md` | `notes-after-add` |
| Settings | `settings.md` | `settings-full` |
| The blyg switcher | `blyg-switcher.md` | `switcher` |

Every file has the same sections: **Sub-features**, **How to get to it (user
POV)**, **Driving it with burrow-verify**, **Gotchas**. Add a file when a
feature gets its own scenes; update it when a scene is renamed.
