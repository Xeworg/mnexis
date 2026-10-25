#!/usr/bin/env bash
# The content level of #51 (A1): from a repository, a binary older than the
# tree must install the tree's assets, not the snapshot the binary was
# compiled with — and a binary already fresh must not be rebuilt again.
# The tree is copied into a scratch directory and marked: the probe must
# not dirty the repository it runs from.
set -uo pipefail
TESTS_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=helpers.sh
. "$TESTS_DIR/helpers.sh"

PI_SKILLS="$HOME/.pi/agent/skills"
PI_EXTENSIONS="$HOME/.pi/agent/extensions"
BINARY="$HOME/.local/bin/mnexis"
MARKER="PROBE_MARKER_$$"

TREE="$SCRATCH/tree"
mkdir -p "$TREE"
tar -cf - -C "$REPO_ROOT" --exclude=./.git --exclude=./target --exclude=./tmp . |
  tar -xf - -C "$TREE"
printf '\n<!-- %s -->\n' "$MARKER" >>"$TREE/skills/mnexis-memory/SKILL.md"
printf '\n// %s\n' "$MARKER" >>"$TREE/extensions/mnexis-memory/index.js"

run_tree() { # args...
  # The tree builds into its own target directory: building into the suite's
  # shared one would overwrite the prebuilt binary the other tests install
  # with one that embeds the marked (scratch) assets.
  RUN_OUT="$(cd -- "$TREE" && CARGO_TARGET_DIR="$SCRATCH/tree-target" \
    bash ./install.sh "$@" 2>&1 </dev/null)"
  RUN_STATUS=$?
}

# A binary dated before the tree stands for the trap.
make_binary_stale() { touch -d '2000-01-01 00:00:00 UTC' "$BINARY"; }

make_binary_stale
run_tree skills --env pi
assert_status "a stale binary: skills --env pi still exits 0" 0
assert_contains "the rebuild is announced" "older than the working tree"
if grep -q "$MARKER" "$PI_SKILLS/mnexis-memory/SKILL.md"; then
  ok "the installed skill carries the working tree's content"
else
  not_ok "the installed skill is the stale snapshot (marker missing)"
fi

make_binary_stale
run_tree extensions --env pi
assert_status "a stale binary: extensions --env pi still exits 0" 0
assert_contains "the extension path announces its rebuild too" \
  "older than the working tree"
if grep -q "$MARKER" "$PI_EXTENSIONS/mnexis-memory/index.js"; then
  ok "the installed extension carries the working tree's content"
else
  not_ok "the installed extension is the stale snapshot (marker missing)"
fi

# The binary the last run installed is fresh: no rebuild may fire again.
run_tree skills --env pi
assert_status "the second run still exits 0" 0
assert_lacks "a fresh binary is not rebuilt again" "Building mnexis with cargo"

finish
