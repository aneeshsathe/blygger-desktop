#!/usr/bin/env python3
"""Stitch the changelog fragments in changes/unreleased/ into CHANGELOG.md.

Each change adds one fragment, `changes/unreleased/<slug>.md`, instead of
editing CHANGELOG.md, so parallel branches never conflict over it. The
release stitches them into a new section.

    scripts/changelog-stitch.py check
    scripts/changelog-stitch.py stitch VERSION [--date YYYY-MM-DD] [--dry-run]

`check` validates every fragment (none at all is fine), and refuses entries
under `[Unreleased]` as `stitch` does; the workspace tests run it. `stitch` validates,
then writes `## [VERSION] - DATE` (today by default) under `## [Unreleased]`,
with one `### <section>` per section that has entries, in SECTIONS order;
within a section, fragments go in file-name order. Then it deletes the
fragments (README.md stays). `--dry-run` prints the section and changes
nothing. `--root DIR` (before the command) works on another tree.

A fragment is:

    section: Added

    - **Area: what it does now.** Details, `paths`.
      A continuation line is indented.

Line 1 names one of SECTIONS, line 2 is blank, and the rest is one or more
`- ` list items (no headings). Anything else in the folder, other than
README.md, is refused.

`stitch` refuses, changing nothing, when: a fragment is malformed; there
are no fragments; CHANGELOG.md already has a section for VERSION; or the
`[Unreleased]` section holds entries (a `- ` or `### ` line: they belong in
fragments). A note in prose under `[Unreleased]` is kept.

Exit status: 0 on success, 1 when refused, 2 on bad usage. Python 3
standard library only.
"""
import argparse
import datetime
import os
import re
import sys

SECTIONS = (
    "Added",
    "Changed",
    "Deprecated",
    "Removed",
    "Fixed",
    "Security",
    "Development",
    "Known issues",
    "Not verified",
)

DEFAULT_ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
FRAGMENTS = os.path.join("changes", "unreleased")
KEEP = {"README.md"}
VERSION = re.compile(r"^\d+\.\d+\.\d+(-[0-9A-Za-z.]+)?$")
HEADING = re.compile(r"^#{1,6}\s")


class Refused(Exception):
    pass


def parse(name, text):
    """(section, body) of one fragment, or Refused naming the file."""
    lines = text.splitlines()
    m = re.match(r"^section:\s*(.+?)\s*$", lines[0]) if lines else None
    if not m:
        raise Refused(f"{name}: first line must be 'section: <name>'")
    section = m.group(1)
    if section not in SECTIONS:
        raise Refused(f"{name}: unknown section '{section}' (one of: {', '.join(SECTIONS)})")
    if len(lines) > 1 and lines[1].strip():
        raise Refused(f"{name}: second line must be blank")
    body = lines[2:]
    while body and not body[-1].strip():
        body.pop()
    while body and not body[0].strip():
        body.pop(0)
    if not body:
        raise Refused(f"{name}: no entry under 'section: {section}'")
    if not body[0].startswith("- "):
        raise Refused(f"{name}: the entry must start with '- ' (a list item)")
    for line in body:
        if HEADING.match(line):
            raise Refused(f"{name}: a heading inside a fragment ('{line}'); use one fragment per section")
    return section, body


def fragments(root):
    """[(file name, section, body)] in file-name order; refuses on any problem."""
    folder = os.path.join(root, FRAGMENTS)
    if not os.path.isdir(folder):
        raise Refused(f"no {FRAGMENTS}/ folder")
    found, problems = [], []
    for name in sorted(os.listdir(folder)):
        if name in KEEP or name.startswith("."):
            continue
        path = os.path.join(folder, name)
        if not name.endswith(".md") or not os.path.isfile(path):
            problems.append(f"{name}: not a fragment (want <slug>.md)")
            continue
        with open(path, encoding="utf-8") as f:
            text = f.read()
        try:
            found.append((name, *parse(name, text)))
        except Refused as e:
            problems.append(str(e))
    if problems:
        raise Refused("\n".join(problems))
    return found


def section_text(version, date, found):
    out = [f"## [{version}] - {date}"]
    for section in SECTIONS:
        entries = [line for (_, s, body) in found if s == section for line in body]
        if entries:
            out += ["", f"### {section}", *entries]
    return "\n".join(out) + "\n"


def read_changelog(root):
    with open(os.path.join(root, "CHANGELOG.md"), encoding="utf-8") as f:
        return f.read().splitlines()


def unreleased_end(lines):
    """Index of the line after [Unreleased]'s section; refuses when it holds
    entries (a `- ` or `### ` line): they belong in fragments."""
    try:
        start = lines.index("## [Unreleased]")
    except ValueError:
        raise Refused("CHANGELOG.md has no '## [Unreleased]' heading")
    end = next((i for i in range(start + 1, len(lines)) if lines[i].startswith("## ")), len(lines))
    held = [l for l in lines[start + 1 : end] if l.startswith("- ") or l.startswith("### ")]
    if held:
        raise Refused(
            "CHANGELOG.md's [Unreleased] has entries; move them into fragments first:\n  "
            + "\n  ".join(held)
        )
    return end


def check(root):
    """Fragments well-formed, and no entries under [Unreleased]; refuses
    listing every problem."""
    problems = []
    try:
        n = len(fragments(root))
    except Refused as e:
        problems.append(str(e))
    try:
        unreleased_end(read_changelog(root))
    except Refused as e:
        problems.append(str(e))
    if problems:
        raise Refused("\n".join(problems))
    return n


def stitch(root, version, date, dry_run):
    if not VERSION.match(version):
        raise Refused(f"'{version}' isn't a version (x.y.z)")
    found = fragments(root)
    if not found:
        raise Refused(f"no fragments in {FRAGMENTS}/: nothing to release")
    path = os.path.join(root, "CHANGELOG.md")
    lines = read_changelog(root)
    end = unreleased_end(lines)
    for line in lines:
        if line == f"## [{version}]" or line.startswith(f"## [{version}] "):
            raise Refused(f"CHANGELOG.md already has a section for [{version}]")
    text = section_text(version, date, found)
    if dry_run:
        sys.stdout.write(text)
        return
    before = lines[:end]
    while before and not before[-1].strip():
        before.pop()
    after = lines[end:]
    new = "\n".join(before) + "\n\n" + text + ("\n" + "\n".join(after) + "\n" if after else "")
    with open(path, "w", encoding="utf-8") as f:
        f.write(new)
    for name, _, _ in found:
        os.remove(os.path.join(root, FRAGMENTS, name))
    print(f"CHANGELOG.md: [{version}] - {date} from {len(found)} fragment(s), which are deleted", file=sys.stderr)


def main():
    p = argparse.ArgumentParser(description="Stitch changes/unreleased/ into CHANGELOG.md.")
    p.add_argument("--root", default=DEFAULT_ROOT, help="the tree to work on (default: this repo)")
    sub = p.add_subparsers(dest="cmd", required=True)
    sub.add_parser("check", help="validate the fragments")
    s = sub.add_parser("stitch", help="write the release section and delete the fragments")
    s.add_argument("version")
    s.add_argument("--date", default=datetime.date.today().isoformat())
    s.add_argument("--dry-run", action="store_true")
    a = p.parse_args()
    try:
        if a.cmd == "check":
            n = check(a.root)
            print(f"{n} fragment(s) in {FRAGMENTS}/ are well-formed; [Unreleased] holds no entries", file=sys.stderr)
        else:
            if not re.match(r"^\d{4}-\d{2}-\d{2}$", a.date):
                raise Refused(f"'{a.date}' isn't a date (YYYY-MM-DD)")
            stitch(a.root, a.version, a.date, a.dry_run)
    except Refused as e:
        print(f"changelog-stitch: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
