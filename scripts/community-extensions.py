#!/usr/bin/env python3
"""Check the community extension list, and render it as a documentation page.

    scripts/community-extensions.py check  [extensions/community.toml]
    scripts/community-extensions.py render [extensions/community.toml] [OUT.md]

`check` exits 1 and prints every problem when an entry is malformed. `render`
checks first (and fails the same way), then writes the "Community extensions"
page (default: site/src/generated/community-extensions.md).
scripts/build-site.sh runs `render`; CI runs `check` on every pull request.

The checks are about format only: they never fetch, run or review an
extension. Python 3.11+ standard library only (tomllib).
"""
import datetime
import os
import re
import sys
import tomllib
from urllib.parse import urlsplit

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
DATA = os.path.join(ROOT, "extensions", "community.toml")
OUT = os.path.join(ROOT, "site", "src", "generated", "community-extensions.md")

NAME_MAX = 40  # crates/blyg-ext/src/manifest.rs NAME_MAX
TITLE_MAX = 60
DESCRIPTION_MAX = 160
PLATFORMS = ("macos", "windows")
PLATFORM_WORDS = {"macos": "macOS", "windows": "Windows"}
REQUIRED = ("name", "title", "description", "repo", "author", "capabilities", "platforms", "added")
OPTIONAL = ("homepage",)

NAME_RE = re.compile(r"^[a-z0-9]+(-[a-z0-9]+)*$")
# GitHub's rule: 1-39 alphanumerics or single hyphens, not starting or ending with one.
HANDLE_RE = re.compile(r"^[A-Za-z0-9](?:[A-Za-z0-9]|-(?=[A-Za-z0-9])){0,38}$")
EMAIL_RE = re.compile(r"[^\s@<>()]+@[^\s@<>()]+\.[A-Za-z]{2,}")

# What each capability lets an extension do, in the words of site/src/extensions.md.
CAPABILITY_WORDS = {
    "items.read": "read your posts, drafts and scratch notes",
    "items.write": "create drafts and scratch notes, and edit their text",
    "reading.read": "read the posts held from your subscriptions",
    "blyg.identity": "know your blyg's address",
    "ui": "show messages, and select a post in the list",
    "hooks:itemPublished": "be told when you publish a post, with its text",
    "hooks:itemSaved": "be told when you edit a post, with its text",
    "hooks:itemCreated": "be told when you create a post, with its text",
    "browser.capture": "read the page open in the browser pane",
    "net": "use the network (declared, not enforced)",
}
FS_PREFIX = "fs:"
AUTOMATE_PREFIX = "browser.automate:"


def read_rust_strings(path, pattern):
    try:
        with open(path, encoding="utf-8") as fh:
            return re.findall(pattern, fh.read())
    except OSError:
        return []


def known_capabilities():
    """EXTENSION_CAPABILITIES from the config key table, so the two can't drift."""
    keys = os.path.join(ROOT, "crates", "blyg-core", "src", "config", "keys.rs")
    try:
        with open(keys, encoding="utf-8") as fh:
            m = re.search(r"EXTENSION_CAPABILITIES: &\[&str\] = &\[(.*?)\];", fh.read(), re.S)
    except OSError:
        m = None
    caps = re.findall(r'"([^"]+)"', m.group(1)) if m else []
    return caps or list(CAPABILITY_WORDS)


def bundled_names():
    """The `NAME` of every bundled extension crate (crates/blyg-ext-*)."""
    names = {"markdown-notes", "cross-post"}
    crates = os.path.join(ROOT, "crates")
    if os.path.isdir(crates):
        for d in sorted(os.listdir(crates)):
            if d.startswith("blyg-ext-"):
                lib = os.path.join(crates, d, "src", "lib.rs")
                names.update(read_rust_strings(lib, r'pub const NAME: &str = "([^"]+)";'))
    return names


def check_url(value):
    if not isinstance(value, str):
        return "must be a string"
    if re.search(r"[\s<>\"'`]", value):
        return "must not contain spaces, quotes or angle brackets"
    parts = urlsplit(value)
    if parts.scheme != "https":
        return "must be an https:// URL"
    if not parts.hostname or "." not in parts.hostname:
        return "must name a host"
    if parts.username or parts.password:
        return "must not contain a user name or password"
    return None


def check_plain(value, limit):
    if not isinstance(value, str):
        return "must be a string"
    if not value.strip():
        return "must not be empty"
    if value != value.strip():
        return "must not start or end with spaces"
    if any(c in value for c in "\r\n\t"):
        return "must be one line"
    if re.search(r"[<>`]", value):
        return "must be plain text (no HTML or Markdown code)"
    if len(value) > limit:
        return f"is {len(value)} characters; at most {limit}"
    return None


def check_capability(cap, known):
    if not isinstance(cap, str):
        return "must be a string"
    if cap.startswith(FS_PREFIX):
        path = cap[len(FS_PREFIX):]
        if not path.strip() or path != path.strip() or re.search(r"[<>`\n]", path):
            return "an fs: capability needs a folder, e.g. fs:~/Notes"
        return None
    if cap.startswith(AUTOMATE_PREFIX):
        origin = cap[len(AUTOMATE_PREFIX):]
        parts = urlsplit(origin)
        if parts.scheme not in ("https", "http") or not parts.hostname or parts.path not in ("", "/") \
                or parts.query or parts.fragment or parts.username:
            return "browser.automate: needs an origin, e.g. browser.automate:https://example.com"
        return None
    if cap not in known:
        return f"isn't a capability Burrow knows (one of {', '.join(known)}, fs:<folder> or browser.automate:<origin>)"
    return None


def validate(path):
    """Return (entries, problems)."""
    try:
        with open(path, "rb") as fh:
            data = tomllib.load(fh)
    except OSError as e:
        return [], [f"can't read it: {e}"]
    except tomllib.TOMLDecodeError as e:
        return [], [f"isn't valid TOML: {e}"]

    problems = []
    extra = sorted(set(data) - {"extension"})
    if extra:
        problems.append(f"unknown top-level keys {extra}: entries are [[extension]] tables")
    entries = data.get("extension", [])
    if not isinstance(entries, list):
        return [], problems + ["`extension` must be an array of tables ([[extension]])"]

    known = known_capabilities()
    bundled = bundled_names()
    seen_names, seen_repos = {}, {}
    today = datetime.date.today()

    for i, e in enumerate(entries, 1):
        label = f"entry {i}"
        if isinstance(e, dict) and isinstance(e.get("name"), str):
            label += f" ({e['name']})"

        def bad(field, msg):
            problems.append(f"{label}: `{field}` {msg}")

        if not isinstance(e, dict):
            problems.append(f"{label}: must be a table")
            continue
        for f in REQUIRED:
            if f not in e:
                bad(f, "is required")
        for f in sorted(set(e) - set(REQUIRED) - set(OPTIONAL)):
            bad(f, "isn't a known field")

        name = e.get("name")
        if "name" in e:
            if not isinstance(name, str) or not NAME_RE.match(name):
                bad("name", "must be lowercase kebab-case, like word-count")
            elif len(name) > NAME_MAX:
                bad("name", f"is longer than {NAME_MAX} characters")
            elif name in bundled:
                bad("name", "is the name of an extension bundled with Burrow")
            elif name in seen_names:
                bad("name", f"is already listed (entry {seen_names[name]})")
            else:
                seen_names[name] = i

        for f, limit in (("title", TITLE_MAX), ("description", DESCRIPTION_MAX)):
            if f in e and (msg := check_plain(e[f], limit)):
                bad(f, msg)

        for f in ("repo", "homepage"):
            if f in e and (msg := check_url(e[f])):
                bad(f, msg)
        repo = e.get("repo")
        if isinstance(repo, str):
            key = repo.rstrip("/").lower()
            if key in seen_repos:
                bad("repo", f"is already listed (entry {seen_repos[key]})")
            seen_repos[key] = i

        if "author" in e:
            a = e["author"]
            if not isinstance(a, str) or not HANDLE_RE.match(a):
                bad("author", "must be a GitHub handle, like octocat (no @, no email)")

        if "capabilities" in e:
            caps = e["capabilities"]
            if not isinstance(caps, list):
                bad("capabilities", "must be a list, e.g. [\"items.read\", \"ui\"] (or [])")
            else:
                for c in caps:
                    if msg := check_capability(c, known):
                        bad("capabilities", f"{c!r} {msg}")
                if len(set(map(str, caps))) != len(caps):
                    bad("capabilities", "lists a capability twice")

        if "platforms" in e:
            ps = e["platforms"]
            if not isinstance(ps, list) or not ps:
                bad("platforms", f"must be a non-empty list of {', '.join(PLATFORMS)}")
            else:
                for p in ps:
                    if p not in PLATFORMS:
                        bad("platforms", f"{p!r} isn't one of {', '.join(PLATFORMS)}")
                if len(set(map(str, ps))) != len(ps):
                    bad("platforms", "lists a platform twice")

        if "added" in e:
            d = e["added"]
            if not isinstance(d, datetime.date) or isinstance(d, datetime.datetime):
                bad("added", "must be a TOML date without quotes, like 2026-10-09")
            elif d > today + datetime.timedelta(days=1):
                bad("added", "is in the future")

        for f, v in e.items():
            if isinstance(v, str) and EMAIL_RE.search(v):
                bad(f, "looks like it contains an email address; don't list one")

    names = [e.get("name") for e in entries if isinstance(e, dict) and isinstance(e.get("name"), str)]
    if names != sorted(names):
        problems.append(f"entries must be sorted by name: {', '.join(sorted(names))}")

    return entries, problems


DISCLAIMER = """\
> **Use at your own risk.** These are third-party extensions, written and
> published by other people. The Burrow maintainers do not review, verify,
> audit or endorse them: a listing only means its entry is in the right
> format. An extension is a program that runs with your user's rights.
> Burrow enforces most of the permissions you grant it, but `fs:` and `net`
> are only the extension's own declarations, so it can read and change your
> files and use the network whatever it declares. Read the source before
> you install one, and install only what you trust. If a listing is
> malicious or misleading,
> [open an issue](https://github.com/aneeshsathe/blygger-desktop/issues/new)
> and it will be removed."""


def md(text):
    """Escape plain text for Markdown (it was already checked for HTML)."""
    return re.sub(r"([\\`*_\[\]{}#|!~&])", r"\\\1", text)


def capability_words(cap):
    if cap.startswith(FS_PREFIX):
        return f"read and write files in `{cap[len(FS_PREFIX):]}` (declared, not enforced)"
    if cap.startswith(AUTOMATE_PREFIX):
        return f"fill in and, when you confirm, post on {md(cap[len(AUTOMATE_PREFIX):])}, signed in as you"
    return CAPABILITY_WORDS.get(cap, md(cap))


def render(entries):
    out = [
        "<!-- Generated from extensions/community.toml by scripts/build-site.sh",
        "     (scripts/community-extensions.py render). Don't edit it by hand. -->",
        "",
        "# Community extensions",
        "",
        DISCLAIMER,
        "",
        "Extensions other people have written for Burrow. To install one, follow",
        "[Installing someone else's extension](../extensions.md#installing-someone-elses-extension);",
        "to list yours here, see [Submit your extension](../extensions.md#submit-your-extension).",
        "",
    ]
    if not entries:
        out += ["*No community extensions yet — be the first.*", ""]
        return "\n".join(out)
    for e in entries:
        caps = e["capabilities"]
        out += [f"## {md(e['title'])}", "", md(e["description"]), ""]
        out.append(f"- **Name:** `{e['name']}`")
        out.append(f"- **Source:** [{md(e['repo'])}]({e['repo']})")
        if e.get("homepage"):
            out.append(f"- **Homepage:** [{md(e['homepage'])}]({e['homepage']})")
        out.append(f"- **Author:** [{md(e['author'])}](https://github.com/{e['author']})")
        if caps:
            out.append("- **Asks to:**")
            out += [f"  - `{c}`: {capability_words(c)}" for c in caps]
        else:
            out.append("- **Asks to:** nothing (no capabilities)")
        out.append("- **Platforms:** " + ", ".join(PLATFORM_WORDS[p] for p in e["platforms"]))
        out.append(f"- **Listed:** {e['added'].isoformat()}")
        out.append("")
    return "\n".join(out)


def main(argv):
    if len(argv) < 2 or argv[1] not in ("check", "render"):
        print(__doc__.strip(), file=sys.stderr)
        return 2
    data = argv[2] if len(argv) > 2 else DATA
    inside = os.path.abspath(data).startswith(ROOT + os.sep)
    shown = os.path.relpath(data, ROOT) if inside else data
    entries, problems = validate(data)
    for p in problems:
        print(f"{shown}: {p}", file=sys.stderr)
    if problems:
        print(f"{len(problems)} problem(s); see \"Submit your extension\" in site/src/extensions.md",
              file=sys.stderr)
        return 1
    if argv[1] == "check":
        print(f"{shown}: {len(entries)} extension(s), all well-formed")
        return 0
    out = argv[3] if len(argv) > 3 else OUT
    os.makedirs(os.path.dirname(out), exist_ok=True)
    with open(out, "w", encoding="utf-8") as fh:
        fh.write(render(entries))
    print(f"wrote {os.path.relpath(out, ROOT)} ({len(entries)} extension(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
