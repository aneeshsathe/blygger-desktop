#!/usr/bin/env bash
# Watch a PR's CI to the end, and optionally merge it and wait for the Pages
# deploy of the merge.
#
#   scripts/pr-watch.sh <pr> [--merge] [--pages <url>] [--timeout <minutes>]
#
# Exit codes: 0 all green (and merged/deployed if asked), 1 a job failed,
# 2 timed out, 3 GitHub could not be read (gh errors), 4 bad usage or state.
#
# Why it is written this way (each one went wrong once):
# - It reads job conclusions from the Actions API, not `gh pr checks`, which
#   kept showing a failed job as "pending".
# - It finds runs by the PR's full head SHA, not `gh run list --commit <short>`,
#   which returned nothing, so the watcher waited on a run that didn't exist.
# - Every gh/jq failure is counted and ends the watch (exit 3) instead of being
#   swallowed in a loop that never ends.
# - It refuses to merge if the PR's head moved since CI started, and merges with
#   --match-head-commit so GitHub refuses too.
set -euo pipefail

usage() { sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'; exit 4; }

pr="" merge=0 pages="" timeout_min=60 poll=30
while [ $# -gt 0 ]; do
  case "$1" in
    --merge) merge=1 ;;
    --pages) pages="${2:?--pages needs a URL}"; shift ;;
    --timeout) timeout_min="${2:?--timeout needs minutes}"; shift ;;
    -h|--help) usage ;;
    [0-9]*) pr="$1" ;;
    *) echo "unknown argument: $1" >&2; usage ;;
  esac
  shift
done
[ -n "$pr" ] || usage

deadline=$(( $(date +%s) + timeout_min * 60 ))
# ghq runs inside $(…), a subshell, so the error count lives in a file.
errfile=$(mktemp); trap 'rm -f "$errfile" "$errfile.err"' EXIT; echo 0 >"$errfile"
log() { printf '%s %s\n' "$(date -u +%H:%M:%S)" "$*"; }

# gh with error counting: 5 failures in a row end the watch.
ghq() {
  local out
  local errors err
  # stderr kept apart: gh prints hints there that would corrupt the JSON.
  if out=$(gh "$@" 2>"$errfile.err"); then echo 0 >"$errfile"; printf '%s' "$out"; return 0; fi
  err=$(cat "$errfile.err")
  errors=$(( $(cat "$errfile") + 1 )); echo "$errors" >"$errfile"
  log "gh error ($errors/5): ${err:-$out}" >&2
  return 1
}
# Called from the main shell after a failed ghq, where exit really ends the script.
too_many_errors() {
  if [ "$(cat "$errfile")" -ge 5 ]; then log "giving up: GitHub could not be read" >&2; exit 3; fi
}
check_deadline() {
  if [ "$(date +%s)" -ge "$deadline" ]; then log "timed out after ${timeout_min} min: $1" >&2; exit 2; fi
}

# Wait for every job of every workflow run on <sha> (optionally one workflow file).
# Prints a line whenever a job's state changes. Returns 0 green, exits 1 on failure.
wait_runs() {
  local sha="$1" workflow="${2:-}" prev="" cur runs jobs n
  while true; do
    check_deadline "CI on ${sha:0:7}"; too_many_errors
    if ! runs=$(ghq api "repos/{owner}/{repo}/actions/runs?head_sha=$sha&per_page=50" \
        --jq "[.workflow_runs[] | select(\"$workflow\" == \"\" or (.path | endswith(\"/$workflow\"))) | .id]"); then
      sleep "$poll"; continue
    fi
    n=$(jq length <<<"$runs")
    if [ "$n" = 0 ]; then log "no runs yet for ${sha:0:7}${workflow:+ ($workflow)}"; sleep "$poll"; continue; fi
    jobs="[]" ; local ok=1
    for id in $(jq -r '.[]' <<<"$runs"); do
      local j
      if ! j=$(ghq api "repos/{owner}/{repo}/actions/runs/$id/jobs?per_page=100" --jq '.jobs'); then ok=0; break; fi
      jobs=$(jq -s 'add' <(echo "$jobs") <(echo "$j"))
    done
    [ "$ok" = 1 ] || { sleep "$poll"; continue; }
    cur=$(jq -r '.[] | "\(.name): \(.conclusion // .status)"' <<<"$jobs" | sort)
    if [ "$cur" != "$prev" ]; then
      log "jobs on ${sha:0:7}:"; sed 's/^/    /' <<<"$cur"; prev="$cur"
    fi
    if jq -e 'any(.[]; .conclusion != null and (.conclusion | IN("success","skipped","neutral") | not))' <<<"$jobs" >/dev/null; then
      log "FAILED:"; jq -r '.[] | select(.conclusion != null and (.conclusion | IN("success","skipped","neutral") | not)) | "    \(.name): \(.conclusion)  \(.html_url)"' <<<"$jobs"
      exit 1
    fi
    if [ "$(jq length <<<"$jobs")" -gt 0 ] && jq -e 'all(.[]; .conclusion != null)' <<<"$jobs" >/dev/null; then
      return 0
    fi
    sleep "$poll"
  done
}

info=$(ghq pr view "$pr" --json headRefOid,state,url) || { log "can't read PR $pr" >&2; exit 3; }
jq -e .headRefOid <<<"$info" >/dev/null 2>&1 || { log "PR $pr: unexpected reply: $info" >&2; exit 3; }
sha=$(jq -r .headRefOid <<<"$info"); state=$(jq -r .state <<<"$info")
log "PR #$pr ($state) head ${sha:0:7}  $(jq -r .url <<<"$info")"
[ "$state" = OPEN ] || [ "$merge" = 0 ] || { log "PR is $state, not open" >&2; exit 4; }

wait_runs "$sha"
log "CI green on ${sha:0:7}"

[ "$merge" = 1 ] || exit 0
now=$(ghq pr view "$pr" --json headRefOid --jq .headRefOid)
[ "$now" = "$sha" ] || { log "head moved to ${now:0:7} since CI started; not merging" >&2; exit 4; }
gh pr merge "$pr" --merge --match-head-commit "$sha"
merge_sha=""
until [ -n "$merge_sha" ] && [ "$merge_sha" != null ]; do
  check_deadline "merge commit"; too_many_errors; sleep 5
  merge_sha=$(ghq pr view "$pr" --json mergeCommit --jq '.mergeCommit.oid' || true)
done
log "merged as ${merge_sha:0:7}"

[ -n "$pages" ] || exit 0
wait_runs "$merge_sha" pages.yml
log "Pages deploy green; checking $pages"
while true; do
  check_deadline "$pages"
  code=$(curl -s -o /dev/null -w '%{http_code}' "$pages" || echo 000)
  [ "$code" = 200 ] && { log "live: $pages (HTTP 200)"; exit 0; }
  log "HTTP $code, retrying"; sleep 15
done
