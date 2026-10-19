#!/usr/bin/env bash
#
# Run the bash installer test suite (tests/installer/).
#
#   scripts/test-installer.sh
#
# These tests are not cargo integration tests. They exercise install.sh
# (install, uninstall, uninstall --purge) against a scratch HOME created
# with mktemp and a shared CARGO_TARGET_DIR under the gitignored
# tmp/installer-target. The real user HOME is never touched.
#
# Requirements: bash, coreutils, rustc + cargo (a release build is
# needed by the install test). util-linux `script` enables the
# interactive y/N purge tests; without it those cases are skipped.
#
# The runner builds the release binary once; each install test then
# re-runs the same `cargo build --release` as a fast freshness check.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"

usage() {
  printf '%s\n' \
    'Usage: scripts/test-installer.sh' \
    '' \
    'Runs every tests/installer/*_test.sh against a scratch HOME.' \
    'A release build is written to tmp/installer-target (override with' \
    'CARGO_TARGET_DIR). Exit status is non-zero when any test fails.'
}

for arg in "$@"; do
  case "$arg" in
    -h | --help)
      usage
      exit 0
      ;;
    *)
      printf 'test-installer.sh: unknown argument: %s\n' "$arg" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if ! command -v cargo >/dev/null 2>&1; then
  printf 'test-installer.sh: cargo was not found on PATH.\n' >&2
  printf 'Install Rust with rustup (https://rustup.rs) and re-run.\n' >&2
  exit 1
fi
if ! command -v rustc >/dev/null 2>&1; then
  printf 'test-installer.sh: rustc was not found on PATH.\n' >&2
  printf 'Install Rust with rustup (https://rustup.rs) and re-run.\n' >&2
  exit 1
fi

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/tmp/installer-target}"
# The tests replace HOME with a scratch directory; hand cargo's real
# dependency cache through so no fresh registry download is needed.
export MNEXIS_TEST_CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"

printf '==> building release binary in %s\n' "$CARGO_TARGET_DIR"
(cd -- "$REPO_ROOT" && cargo build --release)

shopt -s nullglob
tests=("$REPO_ROOT"/tests/installer/*_test.sh)
if ((${#tests[@]} == 0)); then
  printf 'test-installer.sh: no tests found in tests/installer/\n' >&2
  exit 1
fi

overall=0
for test_script in "${tests[@]}"; do
  printf '\n==> %s\n' "${test_script##*/}"
  if ! bash "$test_script"; then
    overall=1
  fi
done

printf '\n'
if ((overall == 0)); then
  printf 'All installer tests passed.\n'
else
  printf 'Installer tests FAILED.\n' >&2
fi
exit "$overall"
