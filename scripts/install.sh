#!/usr/bin/env bash
# Install the latest Blygger Desktop release.
#
#   curl -fsSL https://raw.githubusercontent.com/aneeshsathe/blygger-desktop/main/scripts/install.sh | bash
#
# What it does: downloads Blygger-macos-universal.zip from the latest GitHub
# release, checks it against the release's SHA256SUMS, and unzips Blygger.app
# into /Applications (or ~/Applications if /Applications isn't writable).
#
# Files downloaded with curl don't get macOS's quarantine attribute, so
# Gatekeeper doesn't block the unsigned app. The script also strips the
# attribute explicitly, in case a future macOS starts adding it.
#
# Environment overrides:
#   BLYGGER_VERSION=0.1.0   install that version instead of the latest
#   BLYGGER_DEST=<dir>      install into <dir>
#   BLYGGER_NO_VERIFY=1     skip the checksum check (not recommended)
#   BLYGGER_BASE_URL=<url>  download from <url>/<asset> instead of GitHub
set -euo pipefail

REPO="aneeshsathe/blygger-desktop" # set with scripts/set-repo.sh
ASSET="Blygger-macos-universal.zip"
APP="Blygger.app"

say() { printf '%s\n' "$*"; }
die() { printf 'blygger install: %s\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = Darwin ] || die "Blygger Desktop is macOS-only."
major="$(sw_vers -productVersion | cut -d. -f1)"
[ "$major" -ge 11 ] || die "Blygger needs macOS 11 or later (this is $(sw_vers -productVersion))."

if [ -n "${BLYGGER_BASE_URL:-}" ]; then # a mirror, or a local server for testing
  base="${BLYGGER_BASE_URL%/}"
elif [ -n "${BLYGGER_VERSION:-}" ]; then
  base="https://github.com/$REPO/releases/download/v${BLYGGER_VERSION#v}"
else
  base="https://github.com/$REPO/releases/latest/download"
fi

if [ -n "${BLYGGER_DEST:-}" ]; then
  dest="$BLYGGER_DEST"
elif [ -w /Applications ]; then
  dest=/Applications
else
  dest="$HOME/Applications"
fi
mkdir -p "$dest"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

say "Downloading $ASSET…"
curl -fL --progress-bar -o "$tmp/$ASSET" "$base/$ASSET" || die "download failed: $base/$ASSET"

if [ -z "${BLYGGER_NO_VERIFY:-}" ]; then
  curl -fsSL -o "$tmp/SHA256SUMS" "$base/SHA256SUMS" || die "couldn't download SHA256SUMS"
  (cd "$tmp" && grep " $ASSET\$" SHA256SUMS | shasum -a 256 -c -) >/dev/null \
    || die "checksum mismatch for $ASSET; not installing."
  say "Checksum OK."
fi

ditto -x -k "$tmp/$ASSET" "$tmp/unpacked"
[ -d "$tmp/unpacked/$APP" ] || die "$APP not found in the archive"

if [ -d "$dest/$APP" ]; then
  if pgrep -qf "$dest/$APP/Contents/MacOS/"; then
    die "Blygger is running from $dest/$APP. Quit it (⌘Q) and run this again."
  fi
  rm -rf "${dest:?}/${APP:?}"
fi
ditto "$tmp/unpacked/$APP" "$dest/$APP"
xattr -dr com.apple.quarantine "$dest/$APP" 2>/dev/null || true

version="$(defaults read "$dest/$APP/Contents/Info" CFBundleShortVersionString 2>/dev/null || echo '?')"
say ""
say "Installed Blygger $version to $dest/$APP"
say "Open it from Launchpad or Spotlight, or run:  open \"$dest/$APP\""
