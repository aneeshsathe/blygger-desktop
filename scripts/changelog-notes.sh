#!/usr/bin/env bash
# Print the CHANGELOG.md section for a version (the release notes).
#   scripts/changelog-notes.sh 0.1.0      # the "## [0.1.0] …" section
#   scripts/changelog-notes.sh Unreleased
# Exits 1 if the section is missing or empty.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${1:?usage: changelog-notes.sh <version>}"
VERSION="${VERSION#v}"

notes="$(awk -v want="$VERSION" '
  /^## \[/ {
    if (found) exit
    hdr = $0
    sub(/^## \[/, "", hdr); sub(/\].*/, "", hdr)
    if (hdr == want) { found = 1; next }
  }
  found { print }
' "$ROOT/CHANGELOG.md")"

# Trim leading/trailing blank lines.
notes="$(printf '%s\n' "$notes" | sed -e '/./,$!d' | sed -e ':a' -e '/^\n*$/{$d;N;ba' -e '}')"
# grep, not ${notes//[[:space:]]/}: under macOS's bash 3.2 (the release
# runner's) that substitution took over an hour on 0.28.0's 60 KB of notes.
# A here-string, not `printf | grep -q`: grep -q exits at the first match,
# and under pipefail printf's SIGPIPE (notes past the ~64 KB pipe buffer)
# would read as "no section".
if ! grep -q '[^[:space:]]' <<<"$notes"; then
  echo "CHANGELOG.md has no section for [$VERSION]" >&2
  exit 1
fi
printf '%s\n' "$notes"
