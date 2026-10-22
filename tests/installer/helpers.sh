# shellcheck shell=bash
# Shared helpers for the mnexis installer tests (tests/installer/).
#
# Every test sources this file. HOME is pointed at a fresh `mktemp -d`
# system temp directory (never inside the repository), so the installer
# tests never read or modify the real user profile. CARGO_HOME is
# preserved so cargo can reuse the dependency cache; CARGO_TARGET_DIR
# defaults to the gitignored tmp/installer-target that
# scripts/test-installer.sh shares across the suite.

set -uo pipefail

TESTS_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$TESTS_DIR/../.." && pwd)"
INSTALL_SH="$REPO_ROOT/install.sh"
BASH_BIN="$(command -v bash)"

CARGO_HOME_REAL="${MNEXIS_TEST_CARGO_HOME:-${CARGO_HOME:-$HOME/.cargo}}"
CARGO_TARGET_DIR_REAL="${CARGO_TARGET_DIR:-$REPO_ROOT/tmp/installer-target}"

SCRATCH="$(mktemp -d)"
trap 'rm -rf -- "$SCRATCH"' EXIT

export HOME="$SCRATCH/home"
# The installer resolves pi's agent directory from PI_CODING_AGENT_DIR when
# it is set. The suite must not inherit the caller's value (a relocated pi
# agent dir would move the install target out of the scratch HOME), so it is
# cleared here; the dedicated test sets it explicitly for one call.
unset PI_CODING_AGENT_DIR
# The opencode integration honours XDG_CONFIG_HOME. The suite must not
# inherit the caller's value: a relocated config directory would move the
# install target out of the scratch HOME.
unset XDG_CONFIG_HOME
export CARGO_HOME="$CARGO_HOME_REAL"
export CARGO_TARGET_DIR="$CARGO_TARGET_DIR_REAL"
mkdir -p -- "$HOME"

# The installer delegates the agent assets to the mnexis binary (the assets
# are embedded in it). The suite installs the prebuilt release binary into the
# scratch HOME so the delegation runs the real code without a cargo build.
MNEXIS_TEST_BINARY="$HOME/.local/bin/mnexis"
PREBUILT_BINARY="$CARGO_TARGET_DIR_REAL/release/mnexis"
if [[ -x "$PREBUILT_BINARY" ]]; then
  mkdir -p -- "$HOME/.local/bin"
  cp -f -- "$PREBUILT_BINARY" "$MNEXIS_TEST_BINARY"
  chmod 0755 -- "$MNEXIS_TEST_BINARY"
fi

PASSES=0
FAILS=0
SKIPS=0

ok() { PASSES=$((PASSES + 1)); printf 'ok - %s\n' "$1"; }
skip() { SKIPS=$((SKIPS + 1)); printf 'skip - %s\n' "$1"; }
not_ok() { FAILS=$((FAILS + 1)); printf 'not ok - %s\n' "$1" >&2; }

# Run install.sh with the scratch HOME, captured stdout+stderr and no tty.
run_installer() {
  local output status
  output="$(bash "$INSTALL_SH" "$@" 2>&1 </dev/null)"
  status=$?
  RUN_OUT="$output"
  RUN_STATUS="$status"
}

# Run install.sh with an explicit PATH (used for toolchain detection).
run_installer_with_path() {
  local path_value="$1"
  shift
  local output status
  output="$(env PATH="$path_value" "$BASH_BIN" "$INSTALL_SH" "$@" 2>&1 </dev/null)"
  status=$?
  RUN_OUT="$output"
  RUN_STATUS="$status"
}

# Run install.sh with an extra environment variable set (e.g.
# PI_CODING_AGENT_DIR=/some/dir), keeping the scratch HOME.
run_installer_with_env() {
  local env_assignment="$1"
  shift
  local output status
  output="$(env "$env_assignment" "$BASH_BIN" "$INSTALL_SH" "$@" 2>&1 </dev/null)"
  status=$?
  RUN_OUT="$output"
  RUN_STATUS="$status"
}

# util-linux `script` gives the child a real tty on stdin, which is the
# only way to exercise the interactive y/N purge confirmation.
HAVE_PTY=0
if command -v script >/dev/null 2>&1 && script --help 2>&1 | grep -q -- '--return'; then
  HAVE_PTY=1
fi

# run_installer_pty <answer> [args...]
run_installer_pty() {
  local answer="$1"
  shift
  local quoted
  printf -v quoted '%q ' "$BASH_BIN" "$INSTALL_SH" "$@"
  printf '%s\n' "$answer" | script -qec "$quoted" /dev/null >"$SCRATCH/pty.out" 2>&1
  RUN_STATUS=$?
  RUN_OUT="$(cat -- "$SCRATCH/pty.out")"
}

assert_status() { # name expected
  if [[ "$RUN_STATUS" -eq "$2" ]]; then
    ok "$1"
  else
    not_ok "$1 (exit $RUN_STATUS, expected $2; output: $RUN_OUT)"
  fi
}

assert_contains() { # name needle
  if [[ "$RUN_OUT" == *"$2"* ]]; then
    ok "$1"
  else
    not_ok "$1 (output does not contain '$2': $RUN_OUT)"
  fi
}

assert_lacks() { # name needle
  if [[ "$RUN_OUT" != *"$2"* ]]; then
    ok "$1"
  else
    not_ok "$1 (output unexpectedly contains '$2': $RUN_OUT)"
  fi
}

assert_file() {
  if [[ -f "$2" ]]; then ok "$1"; else not_ok "$1 (missing file: $2)"; fi
}

assert_missing() {
  if [[ -e "$2" ]]; then not_ok "$1 (unexpected path: $2)"; else ok "$1"; fi
}

assert_executable() {
  if [[ -x "$2" ]]; then ok "$1"; else not_ok "$1 (not executable: $2)"; fi
}

assert_dir() {
  if [[ -d "$2" ]]; then ok "$1"; else not_ok "$1 (missing directory: $2)"; fi
}

assert_dir_missing() {
  if [[ -e "$2" ]]; then not_ok "$1 (unexpected path: $2)"; else ok "$1"; fi
}

finish() {
  printf '# %s: %d passed, %d failed, %d skipped\n' \
    "${BASH_SOURCE[1]##*/}" "$PASSES" "$FAILS" "$SKIPS"
  [[ "$FAILS" -eq 0 ]]
}
