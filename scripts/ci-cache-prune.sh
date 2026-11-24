#!/usr/bin/env bash
# Keeps the repo's Actions caches under GitHub's 10 GB limit, so the release
# workflow's caches (saved once per release, read once per release) aren't
# evicted by CI's between releases.
#
# - On main, keeps only the newest cache of each name. A rust-cache key ends
#   in a hash of Cargo.lock and the toolchain; a lockfile change starts a new
#   generation and leaves the old one behind until eviction.
# - Deletes the caches of closed pull requests (none are saved any more: CI
#   saves on main only, but older ones can linger).
#
# Usage: scripts/ci-cache-prune.sh [--dry-run]
# Needs GH_TOKEN with actions:write (CI's cache-prune job) and GH_REPO, or a
# checkout gh can resolve.
set -euo pipefail

dry=false
[ "${1:-}" = "--dry-run" ] && dry=true

caches="$(gh cache list --limit 200 --json id,key,ref,createdAt,sizeInBytes)"

# Superseded on main: every cache but the newest per key less its last hash.
superseded="$(jq -r '
  [.[] | select(.ref == "refs/heads/main")]
  | group_by(.key | sub("-[0-9a-f]+$"; ""))
  | map(sort_by(.createdAt) | .[:-1][])
  | .[] | "\(.id)\t\(.sizeInBytes)\t\(.key)"' <<<"$caches")"

# Closed pull requests' caches.
closed=""
for pr in $(jq -r '.[].ref | capture("^refs/pull/(?<n>[0-9]+)/merge$").n' <<<"$caches" | sort -u); do
  state="$(gh pr view "$pr" --json state -q .state 2>/dev/null || echo UNKNOWN)"
  [ "$state" = OPEN ] && continue
  closed+="$(jq -r --arg ref "refs/pull/$pr/merge" '
    .[] | select(.ref == $ref) | "\(.id)\t\(.sizeInBytes)\t\(.key) (PR, \($ref))"' <<<"$caches")"$'\n'
done

freed=0
while IFS=$'\t' read -r id size what; do
  [ -n "$id" ] || continue
  freed=$((freed + size))
  if $dry; then
    echo "would delete $what ($((size / 1000000)) MB)"
  else
    gh cache delete "$id" >/dev/null && echo "deleted $what ($((size / 1000000)) MB)"
  fi
done <<<"$superseded"$'\n'"$closed"

total="$(jq '[.[].sizeInBytes] | add // 0' <<<"$caches")"
echo "caches: $((total / 1000000)) MB before, $(((total - freed) / 1000000)) MB after (limit 10000 MB)"
