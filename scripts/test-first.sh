#!/usr/bin/env bash
# Tests come before the feature: checks a branch's commits for that order.
#
#   scripts/test-first.sh <base> <branch>         every commit that changes product
#                                                 code has a test commit at or before it
#   scripts/test-first.sh --run <base> <branch>   also runs the new tests of the
#                                                 first test commit adding any, without the feature: one must fail
#   scripts/test-first.sh --pr <base> <head>      CI's whole-PR check: product code
#                                                 changed means test files changed too
#
# Product code is a `.rs` file under `crates/*/src/` that isn't a test file.
# Test files: anything under `crates/*/tests/`, `*_tests.rs`, `tests.rs`, and
# files under a `tests/` directory in `src/`; a product file whose change only
# declares a test module (`#[cfg(test)] mod x_tests;`) counts as a test
# change. A commit also adds tests when its diff adds a `#[test]` or
# `#[gpui_kit::test]` line anywhere (an inline `mod tests`). A commit with the
# trailer `No-test: <reason>` (a refactor the existing tests cover, a comment,
# a version bump) is allowed through. Merge commits are skipped.
#
# --run takes the first test commit that adds a test function (earlier test
# commits that only touch fixtures or vendored files are named and skipped; none
# adding one, or product code before it, exits 1). It wants that commit to hold
# tests only (no product code), checks it out in a temporary worktree, runs the tests it added with
# `cargo test -p <crate> -- <names>`, and wants at least one FAILURE (or a
# build failure: the code they call doesn't exist yet). New e2e tests
# (`#[ignore]`d, in blyg-core) run through `scripts/e2e-local.sh` when
# BLYG_WORKER_DIR is set, and are listed as not run otherwise. It never
# touches your working tree and removes the worktree when done.
#
# Those old sources never build into the target directory your working tree
# uses (a test binary an older commit leaves there is one the next run can
# take for its own): every cargo run in the worktree, the e2e script's too,
# gets `${CARGO_TARGET_DIR:-<repo>/target}/test-first`, which is kept between
# runs. So set CARGO_TARGET_DIR (and CARGO_BUILD_JOBS) as for any build: the
# script builds under it, apart from your build, and leaves your environment
# alone.
#
# Exit status: 0 when the order holds (and, with --run, a new test failed
# before the feature), 1 when not (with --run also: no test to run, or none that
# can be red), 2 on bad usage.
set -euo pipefail

die() { printf 'test-first: %s\n' "$*" >&2; exit 2; }

mode=order
case "${1:-}" in
  --run) mode="run"; shift ;;
  --pr) mode="pr"; shift ;;
esac
[ $# -eq 2 ] || die "usage: scripts/test-first.sh [--run|--pr] <base> <branch>"
base=$1
branch=$2
git rev-parse --verify -q "$base^{commit}" >/dev/null || die "no such commit: $base"
git rev-parse --verify -q "$branch^{commit}" >/dev/null || die "no such commit: $branch"

is_test_file() {
  case "$1" in
    # (`*` matches `/` here, so this takes `src/**/tests/` too.)
    crates/*/tests/*) return 0 ;;
    crates/*/src/*_tests.rs | crates/*/src/*/tests.rs | crates/*/src/tests.rs) return 0 ;;
  esac
  return 1
}

is_product_file() {
  case "$1" in
    crates/*/src/*.rs) is_test_file "$1" && return 1; return 0 ;;
  esac
  return 1
}

# Lines a diff adds that mark a test function.
TEST_ATTR='^\+[[:space:]]*#\[(test|gpui_kit::test|gpui::test|tokio::test)'

# A changed line that only declares a test module: `#[cfg(test)]`,
# `#[path = …]`, `mod x;` or blank.
DECL='^[-+][[:space:]]*(#\[cfg\(test\)\]|#\[path *=.*\]|(pub(\(crate\))? )?mod [A-Za-z0-9_]+;)?[[:space:]]*$'

# The diff of one file in a commit (`commit <id>`) or a range (`range a...b`).
file_diff() {
  if [ "$1" = commit ]; then git show --format= "$2" -- "$3"; else git diff "$2" -- "$3"; fi
}

# The product and test files a commit or range changes, one line each as
# "P path" / "T path". A product file whose change only declares a test
# module (`#[cfg(test)] mod foo_tests;`) counts as a test change.
classify() {
  local f changed
  while IFS= read -r f; do
    if is_test_file "$f"; then
      echo "T $f"
    elif is_product_file "$f"; then
      changed=$(file_diff "$1" "$2" "$f" | grep -E '^[-+]' | grep -Ev '^(\+\+\+|---) ' || true)
      if grep -Evq "$DECL" <<<"$changed"; then
        echo "P $f"
      else
        echo "T $f"
      fi
    fi
  done
}

commit_files() { git diff-tree --no-commit-id --root -r --name-only "$1" | classify commit "$1"; }

# (Here-strings, not pipes into `grep -q`: with pipefail, the writer's
# SIGPIPE would read as "no match".)
adds_test_attr() { grep -Eq "$TEST_ATTR" <<<"$(git show --format= "$1" -- '*.rs')"; }

no_test_reason() { git log -1 --format='%(trailers:key=No-test,valueonly)' "$1" | awk 'NF { print; exit }' || true; }

short() { git log -1 --format='%h %s' "$1"; }

if [ "$mode" = pr ]; then
  files=$(git diff --name-only "$base...$branch" | classify range "$base...$branch")
  product=$(printf '%s\n' "$files" | sed -n 's/^P //p')
  tests=$(printf '%s\n' "$files" | sed -n 's/^T //p')
  if [ -z "$product" ]; then
    echo "test-first: no product code changed"
    exit 0
  fi
  if [ -n "$tests" ] || grep -Eq "$TEST_ATTR" <<<"$(git diff "$base...$branch" -- '*.rs')"; then
    echo "test-first: product code changed, and so did tests"
    exit 0
  fi
  for c in $(git rev-list "$base..$branch"); do
    reason=$(no_test_reason "$c")
    if [ -n "$reason" ]; then
      echo "test-first: no test changed; allowed by $(git log -1 --format=%h "$c")'s trailer No-test: $reason"
      exit 0
    fi
  done
  echo "test-first: this change edits product code but adds or changes no test:"
  printf '%s\n' "$product" | sed 's/^/  /'
  echo "Write the test first (it should fail without the change), or add a trailer"
  echo "'No-test: <reason>' to a commit when the existing tests already cover it."
  exit 1
fi

seen_tests=""
first_test=""
test_cands=""   # one "commit|product commits since the first test commit" line per test commit
prod_since=""
bad=0
for c in $(git rev-list --reverse --no-merges "$base..$branch"); do
  files=$(commit_files "$c")
  has_product=$(printf '%s\n' "$files" | grep -c '^P ' || true)
  has_tests=$(printf '%s\n' "$files" | grep -c '^T ' || true)
  if [ "$has_tests" = 0 ] && [ "$has_product" != 0 ] && adds_test_attr "$c"; then
    has_tests=1
  fi
  if [ "$has_tests" != 0 ] && [ -z "$seen_tests" ]; then
    seen_tests=$c
    first_test=$c
  fi
  [ "$has_tests" = 0 ] || test_cands="$test_cands$c|$prod_since"$'\n'
  if [ "$has_product" != 0 ] && [ -n "$seen_tests" ]; then prod_since="$prod_since $c"; fi
  tag="     "
  code="code      "
  if [ "$has_tests" != 0 ]; then tag="tests"; code="tests+code"; fi
  if [ "$has_product" != 0 ]; then
    if [ -n "$seen_tests" ]; then
      echo "ok    $code $(short "$c")"
    elif reason=$(no_test_reason "$c") && [ -n "$reason" ]; then
      echo "ok    no-test    $(short "$c")  (No-test: $reason)"
    else
      echo "FAIL  code       $(short "$c")"
      printf '%s\n' "$files" | sed -n 's/^P /        /p'
      bad=1
    fi
  else
    echo "ok    $tag      $(short "$c")"
  fi
done

if [ "$bad" = 1 ]; then
  echo "test-first: product code came before any test on $branch. Commit the failing"
  echo "tests first (red), then the code that passes them (green); or add a trailer"
  echo "'No-test: <reason>' to a commit the existing tests already cover."
  exit 1
fi
[ -n "$first_test" ] || { echo "test-first: no commit on $branch adds tests (nor product code)"; exit 0; }
echo "test-first: order holds; first test commit $(git log -1 --format=%h "$first_test")"
[ "$mode" = run ] || exit 0


# ---- --run: the first test commit that adds a test function, before the feature ----

# "crate-dir test-name ignored(0/1)" for each test function a commit adds.
new_tests_of() {
  git show --format= -U0 "$1" -- '*.rs' | awk '
    /^\+\+\+ b\// { split(substr($0, 7), p, "/"); dir = p[2]; armed = 0; next }
    /^\+/ {
      line = substr($0, 2)
      if (line ~ /^[[:space:]]*#\[(test|gpui_kit::test|gpui::test|tokio::test)/) { armed = 1; ign = 0; next }
      if (armed && line ~ /^[[:space:]]*#\[ignore/) { ign = 1; next }
      if (armed && match(line, /fn [A-Za-z0-9_]+/)) {
        print dir, substr(line, RSTART + 3, RLENGTH - 3), ign; armed = 0
      }
      next
    }
    { armed = 0 }'
}

# A fixture or a vendored file under tests/ makes a test commit with no test
# function: skip those and take the first test commit that adds one.
first_test=""
new_tests=""
chosen_prods=""
while IFS='|' read -r c prods; do
  [ -n "$c" ] || continue
  nt=$(new_tests_of "$c")
  if [ -n "$nt" ]; then
    first_test=$c
    new_tests=$nt
    chosen_prods=$prods
    break
  fi
  echo "test-first: $(git log -1 --format=%h "$c") changes test files but adds no #[test]; skipped"
done <<<"$test_cands"
if [ -z "$first_test" ]; then
  echo "test-first: no test commit on $branch adds a #[test] function, so there is nothing to run and red before green isn't shown"
  exit 1
fi

at=$(git log -1 --format=%h "$first_test")
if [ -n "$chosen_prods" ]; then
  first_prod=${chosen_prods# }
  first_prod=${first_prod%% *}
  echo "test-first: the first commit that adds tests ($at) comes after product code at $(short "$first_prod"),"
  echo "so they can't be red before the feature. Add the tests before that code."
  exit 1
fi
product=$(commit_files "$first_test" | sed -n 's/^P //p')
if [ -n "$product" ]; then
  echo "test-first: $at adds tests together with product code, so they can't be"
  echo "red before the feature. Commit the tests on their own first:"
  printf '%s\n' "$product" | sed 's/^/  /'
  exit 1
fi

tmp=$(mktemp -d "${TMPDIR:-/tmp}/test-first.XXXXXX")
# shellcheck disable=SC2329 # run by the trap
cleanup() {
  git worktree remove --force "$tmp/wt" >/dev/null 2>&1 || true
  rm -rf "$tmp"
  git worktree prune
}
trap cleanup EXIT
git worktree add --detach -q "$tmp/wt" "$first_test"

# The worktree's own target directory (see the header): under the caller's,
# never the caller's. Absolute, since cargo runs from inside the worktree.
wt_target=${CARGO_TARGET_DIR:-$(git rev-parse --show-toplevel)/target}/test-first
case "$wt_target" in
  /*) ;;
  *) wt_target=$PWD/$wt_target ;;
esac

red=0
ran=0
# Reads cargo's output: counts what ran, prints failures, sets red.
judge() {
  local log=$1 n
  n=$(grep -cE '^test .* \.\.\. (ok|FAILED)$' "$log" || true)
  ran=$((ran + n))
  if grep -qE '^test .* FAILED$' "$log"; then
    grep -E '^test .* FAILED$' "$log" | sed 's/^/  red: /'
    red=1
  elif grep -q '^error: could not compile' "$log"; then
    echo "  red: doesn't build before the feature:"
    grep -m 5 -E '^error(\[E[0-9]+\])?:' "$log" | sed 's/^/    /'
    red=1
    ran=$((ran + 1))
  elif [ "$n" -gt 0 ]; then
    echo "  all $n passed"
  fi
}

for dir in $(printf '%s\n' "$new_tests" | awk '$3 == 0 { print $1 }' | sort -u); do
  pkg=$(sed -n 's/^name *= *"\(.*\)"/\1/p' "$tmp/wt/crates/$dir/Cargo.toml" | head -1)
  names=$(printf '%s\n' "$new_tests" | awk -v d="$dir" '$1 == d && $3 == 0 { print $2 }' | sort -u | tr '\n' ' ')
  echo "test-first: $pkg at $at: $names"
  # shellcheck disable=SC2086
  (cd "$tmp/wt" && CARGO_TARGET_DIR="$wt_target" cargo test -p "$pkg" -- $names) >"$tmp/$dir.log" 2>&1 || true
  judge "$tmp/$dir.log"
done

e2e=$(printf '%s\n' "$new_tests" | awk '$3 == 1 && $1 == "blyg-core" { print $2 }' | sort -u | tr '\n' ' ')
if [ -n "$e2e" ]; then
  if [ -n "${BLYG_WORKER_DIR:-}" ]; then
    echo "test-first: e2e at $at against $BLYG_WORKER_DIR: $e2e"
    # shellcheck disable=SC2086
    (cd "$tmp/wt" && CARGO_TARGET_DIR="$wt_target" BLYG_E2E_SKIP_NOTES=1 BLYG_E2E_SKIP_SWITCHER=1 scripts/e2e-local.sh $e2e) >"$tmp/e2e.log" 2>&1 || true
    judge "$tmp/e2e.log"
  else
    echo "test-first: not run (set BLYG_WORKER_DIR to a studio build to run them): e2e $e2e"
  fi
fi
other=$(printf '%s\n' "$new_tests" | awk '$3 == 1 && $1 != "blyg-core" { print $1 "::" $2 }' | tr '\n' ' ')
[ -z "$other" ] || echo "test-first: not run (ignored): $other"

if [ "$red" = 1 ]; then
  echo "test-first: red before green at $(short "$first_test")"
  exit 0
fi
if [ "$ran" = 0 ]; then
  echo "test-first: none of $at's new tests ran, so red before green isn't shown"
  exit 1
fi
echo "test-first: $at's new tests pass before the feature, so they don't test it"
exit 1
