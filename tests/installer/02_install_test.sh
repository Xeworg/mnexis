#!/usr/bin/env bash
# Install flow against a scratch HOME and the suite's CARGO_TARGET_DIR:
# build via cargo, overwrite an existing binary cleanly, PATH guidance.
set -uo pipefail
TESTS_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=helpers.sh
. "$TESTS_DIR/helpers.sh"

# Seed a stale placeholder so the install must overwrite it.
mkdir -p -- "$HOME/.local/bin"
printf 'stale-placeholder\n' >"$HOME/.local/bin/mnexis"

run_installer
assert_status "default action installs and exits 0" 0
assert_executable "binary installed executable" "$HOME/.local/bin/mnexis"
assert_contains "reports the install path" "Installed mnexis to $HOME/.local/bin/mnexis"
assert_contains "prints PATH guidance when the install dir is not on PATH" \
  "$HOME/.local/bin is not on your PATH"

# The placeholder must be gone: the installed file is the cargo build output.
built="$CARGO_TARGET_DIR/release/mnexis"
assert_file "release build exists in CARGO_TARGET_DIR" "$built"
if cmp -s -- "$built" "$HOME/.local/bin/mnexis"; then
  ok "installed copy matches the CARGO_TARGET_DIR build"
else
  not_ok "installed copy does not match the CARGO_TARGET_DIR build"
fi

version_output="$("$HOME/.local/bin/mnexis" --version 2>&1)"
if [[ "$version_output" == mnexis* ]]; then
  ok "installed binary runs (--version: $version_output)"
else
  not_ok "installed binary does not run: $version_output"
fi

# Re-running with explicit `install` overwrites cleanly and stays quiet
# about PATH when the install dir is already on it.
RUN_OUT="$(PATH="$HOME/.local/bin:$PATH" bash "$INSTALL_SH" install 2>&1 </dev/null)"
RUN_STATUS=$?
assert_status "re-running install exits 0" 0
assert_lacks "no PATH guidance when the install dir is already on PATH" "is not on your PATH"
assert_contains "re-install keeps reporting success" "Installed mnexis to"

finish
