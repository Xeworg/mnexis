#!/usr/bin/env bash
# The install-order trap (#51): skills and extensions delegate to the
# installed binary, and the assets are embedded in it when it is compiled.
# From a repository, a binary older than the working tree is rebuilt
# before the delegation, so the installed assets always match the tree —
# the order of ./install.sh and ./install.sh skills no longer matters.
# A piped (standalone) script never rebuilds: it has no tree to compare
# with, and the binary it delegates to is the one it just installed.
set -uo pipefail
TESTS_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=helpers.sh
. "$TESTS_DIR/helpers.sh"

PI_SKILLS="$HOME/.pi/agent/skills"
PI_EXTENSIONS="$HOME/.pi/agent/extensions"
BINARY="$HOME/.local/bin/mnexis"

# A binary dated before everything in the repository stands for the trap:
# whatever the tree carries, the binary's embedded assets are older.
make_binary_stale() {
  touch -d '2000-01-01 00:00:00 UTC' "$BINARY"
}

# --- repository mode: skills rebuilds a stale binary first ------------------

make_binary_stale
run_installer skills --env pi
assert_status "stale binary: skills --env pi still exits 0" 0
assert_contains "stale binary: skills explains the rebuild" \
  "older than the working tree"
assert_contains "stale binary: skills rebuilds with cargo" \
  "Building mnexis with cargo"
assert_file "stale binary: the skill still lands" \
  "$PI_SKILLS/mnexis-memory/SKILL.md"

# The rebuild refreshed the binary: nothing it was built from is newer
# than it any more.
if [[ -z "$(find "$REPO_ROOT/src" "$REPO_ROOT/skills" \
  "$REPO_ROOT/extensions" "$REPO_ROOT/Cargo.toml" "$REPO_ROOT/Cargo.lock" \
  -newer "$BINARY" -print -quit 2>/dev/null)" ]]; then
  ok "the rebuild left the binary fresh against the tree"
else
  not_ok "the rebuild left the binary older than the tree"
fi

# --- repository mode: extensions rebuild a stale binary first ---------------

make_binary_stale
run_installer extensions --env pi
assert_status "stale binary: extensions --env pi still exits 0" 0
assert_contains "stale binary: extensions explains the rebuild" \
  "older than the working tree"
assert_contains "stale binary: extensions rebuilds with cargo" \
  "Building mnexis with cargo"
assert_file "stale binary: the extension still lands" \
  "$PI_EXTENSIONS/mnexis-memory/index.js"

# --- standalone (piped) mode never rebuilds ----------------------------------

# The piped script has no file of its own: even with a stale installed
# binary it must delegate as-is, because there is no tree to rebuild from.
make_binary_stale
piped_output="$(cd -- "$REPO_ROOT" && cat -- "$REPO_ROOT/install.sh" |
  env HOME="$HOME" "$BASH_BIN" -s -- skills --env pi 2>&1)"
piped_status=$?
RUN_OUT="$piped_output"
RUN_STATUS="$piped_status"
assert_status "piped skills --env pi with a stale binary exits 0" 0
assert_lacks "piped mode never rebuilds" "Building mnexis with cargo"
assert_file "piped mode still installs the skill" \
  "$PI_SKILLS/mnexis-memory/SKILL.md"

finish
