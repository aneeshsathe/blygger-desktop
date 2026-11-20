# Burrow development skills

Checklists for anyone developing Burrow, human or AI agent. Each one holds the
common factors and the things not to forget. These are the checks reviewers
used to make in pull requests, moved upstream into the instructions an agent
works from.

## How to use them

Each skill is a folder with one `SKILL.md`: a front-matter `name`, a
`description` that says when it applies, and then the checklist. They're plain
Markdown and tied to no particular tool.

- **Any agent or person:** before a task, read this index, open every skill
  whose "use it when" matches, and follow it. Before you finish, check your
  work against `burrow-pr-gate`.
- **Agents that read `AGENTS.md` / `CLAUDE.md`:** those files point here.
- **Tools that load skills from a folder** (`SKILL.md` with `name` and
  `description`) can point at or link this folder. Claude Code, for example,
  can use `ln -s ../skills .claude/skills` in a local clone. Don't commit that
  link.

The idea is from [@patwater](https://github.com/patwater): skills that let AI
"factories" develop Burrow around the Blygger spec, so every change starts from
what earlier ones learned.

| Skill | Use it when |
|---|---|
| `burrow-pr-gate` | before committing a feature, or opening, merging or reviewing a PR |
| `burrow-code-shape` | you're corrected, fix something twice, add a rule, or build on a module |
| `verify-burrow` | proving a UI change works in the running app, with a screenshot |
| `burrow-release` | cutting a release, or merging anything that bumps the version |
| `burrow-tutorial-update` | a release changes something a user sees |
| `burrow-cross-platform` | touching keys, menus, paths, web views or native APIs |
| `burrow-agent-cohort` | running several agents at once |
| `spec-client-build` | implementing something the spec or studio defines |
| `capability-gating` | calling a server feature not every studio has |
| `real-server-testing` | writing tests that talk to a blyg |
| `list-order-and-refresh` | showing lists, sorting, polling, caching or paging |
| `client-security` | remote content, network calls, tokens, `innerHTML` |
| `conformance-run` | rendering, parsing, order or wire-format changes |
| `upstream-sync` | a new studio release or spec revision |
| `user-testing-pass` | an update's features are merged: agents act as users and report pain points before the release |

## Writing one

- Give each skill one job, and phrase its description in the words a task would use.
- Base every rule on a real failure, and say why in a sentence.
- Before writing a rule, try to make it a check instead (`burrow-code-shape`);
  once a check enforces it, delete the rule.
- Name real files and commands. Update the skill in the same change that renames them.
- This repo is public: no personal data, no real blyg URLs, no tokens.
