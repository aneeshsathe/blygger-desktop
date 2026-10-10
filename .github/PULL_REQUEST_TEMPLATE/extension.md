<!--
Listing a community extension. This template is for a pull request that adds
(or updates, or removes) one [[extension]] entry in extensions/community.toml.
Open it with ?template=extension.md on the compare URL.
-->

## Extension

- **Name:** <!-- e.g. wordcount -->
- **Source:** <!-- e.g. https://github.com/example/burrow-ext-wordcount -->

## Checklist

- [ ] This pull request changes only `extensions/community.toml`, and only one entry.
- [ ] The entry is in alphabetical order by `name`.
- [ ] `name` matches the `name` in my `extension.toml`.
- [ ] `capabilities` lists exactly what my manifest asks for.
- [ ] `repo` links to the extension's source code, which is public.
- [ ] `author` is my GitHub handle, and there's no email address anywhere in the entry.
- [ ] `python3 scripts/community-extensions.py check` passes.
- [ ] I understand that the Burrow maintainers check the entry's format only.
      They don't review, verify, audit or endorse the extension, and the listing
      says so. A listing can be removed if it's reported as malicious or misleading.
