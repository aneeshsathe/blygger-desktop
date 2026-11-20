#!/usr/bin/env python3
"""Which pull requests may skip CI's Rust jobs, and the check that keeps
that list honest.

A pull request whose every changed file is under a SKIP directory runs no
Rust job (fmt, clippy, tests, the Windows release build); the docs-site job
and test-first still run, and pushes to main always run everything. A path
belongs in SKIP only if no test, build script or script a test runs reads
it, which `check` enforces.

  scripts/ci-skip.py decide BASE HEAD   prints rust=true or rust=false (CI)
  scripts/ci-skip.py check              fails if code may read a SKIP path

`check` scans every tracked file under crates/ and scripts/ (outside SKIP)
for a quoted string naming a SKIP path, or a directory above one (a walk of
`docs` reads `docs/screenshots`), with any leading ../ or ./ dropped. It
also fails on a `git ls-files` call it hasn't been told about: a test that
lists every tracked file reads the SKIP paths too. It can't see a path
built from pieces (`join("docs").join(dir)`); keep SKIP to directories
nothing has a reason to read.
"""
import os
import re
import subprocess
import sys

# Directories (with a trailing /) whose changes alone don't need the Rust
# jobs. Screenshots and the HTML mocks are for people; no code reads them.
SKIP = [
    "docs/screenshots/",
    "docs/prototype/",
    "site/",
]

# Files allowed to call `git ls-files`, and why that doesn't read SKIP.
LS_FILES_OK = {
    "crates/blyg-app/build.rs": "asks only whether build.rs itself is tracked",
}

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def git(*args):
    return subprocess.run(
        ["git", *args], cwd=ROOT, check=True, capture_output=True, text=True
    ).stdout


def skipped(path):
    return any(path.startswith(s) for s in SKIP)


def decide(base, head):
    changed = [p for p in git("diff", "--name-only", f"{base}...{head}").splitlines() if p]
    rust = not changed or not all(skipped(p) for p in changed)
    line = f"rust={'true' if rust else 'false'}"
    print(line)
    out = os.environ.get("GITHUB_OUTPUT")
    if out:
        with open(out, "a") as f:
            f.write(line + "\n")
    if not rust:
        print("only skip-listed paths changed:", *changed, sep="\n  ", file=sys.stderr)


def names_skip_path(literal):
    s = literal.strip()
    while s.startswith(("../", "./")):
        s = s[3:] if s.startswith("../") else s[2:]
    s = s.rstrip("/")
    if not s or s in (".", ".."):
        return None
    for d in SKIP:
        d = d.rstrip("/")
        # The SKIP directory, a file in it, or a directory above it.
        if s == d or s.startswith(d + "/") or d.startswith(s + "/"):
            return d + "/"
    return None


QUOTED = re.compile(r'"([^"\n]*)"')


def check():
    problems = []
    for path in git("ls-files", "crates", "scripts").splitlines():
        if skipped(path) or path == "scripts/ci-skip.py":
            continue
        try:
            with open(os.path.join(ROOT, path), encoding="utf-8") as f:
                lines = f.readlines()
        except (UnicodeDecodeError, IsADirectoryError):
            continue
        for n, line in enumerate(lines, 1):
            for lit in QUOTED.findall(line):
                hit = names_skip_path(lit)
                if hit:
                    problems.append(f"{path}:{n}: reads {hit} (\"{lit}\")")
            if "ls-files" in line and path not in LS_FILES_OK:
                problems.append(f"{path}:{n}: `git ls-files` reads every tracked file, SKIP included")
    if problems:
        print("ci-skip: a skip-listed path is read by code, so a change there", file=sys.stderr)
        print("must run the Rust jobs. Drop it from SKIP in scripts/ci-skip.py,", file=sys.stderr)
        print("or make the reader leave it out (and list it in LS_FILES_OK):", file=sys.stderr)
        for p in problems:
            print("  " + p, file=sys.stderr)
        sys.exit(1)
    print(f"ci-skip: nothing reads {', '.join(SKIP)}")


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == "decide":
        decide(sys.argv[2], sys.argv[3])
    elif len(sys.argv) == 2 and sys.argv[1] == "check":
        check()
    else:
        print(__doc__, file=sys.stderr)
        sys.exit(2)
