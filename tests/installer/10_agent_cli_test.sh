#!/usr/bin/env bash
# `mnexis agent` against the installed binary: the flag contract, the exit
# codes, the per-environment destinations, idempotency, non-destructiveness
# and the promise never to touch the profile (S5-S9).
set -uo pipefail
TESTS_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=helpers.sh
. "$TESTS_DIR/helpers.sh"

if [[ ! -x "$MNEXIS_TEST_BINARY" ]]; then
  skip "the mnexis binary is not available (build the release profile first)"
  finish
  exit $?
fi

BIN="$MNEXIS_TEST_BINARY"
AGENT_HOME="$SCRATCH/agent-home"
mkdir -p "$AGENT_HOME"
PI_SKILLS="$AGENT_HOME/.pi/agent/skills/mnexis-memory"
PI_EXTENSIONS="$AGENT_HOME/.pi/agent/extensions/mnexis-memory"
OC_SKILLS="$AGENT_HOME/.config/opencode/skills/mnexis-memory"

# The binary with a HOME of its own, never the caller's.
run_agent() {
  local output status
  output="$(env -u PI_CODING_AGENT_DIR -u MNEXIS_DATA_DIR HOME="$AGENT_HOME" \
    "$BIN" "$@" 2>&1 </dev/null)"
  status=$?
  RUN_OUT="$output"
  RUN_STATUS="$status"
}

# sha256 of every file under the isolated HOME, sorted and folded once.
agent_tree_hash() {
  (cd "$AGENT_HOME" && find . -type f -exec sha256sum {} + | sort | sha256sum)
}

# --- status before anything is installed ---------------------------------
run_agent agent status
assert_status "status exits 0 on a clean home" 0
assert_contains "status names the pi environment" "pi"
assert_contains "status names the opencode environment" "opencode"
assert_contains "status reports the skill as not installed" "skill:     not installed"
assert_contains "status reports the extension as not installed" "extension: not installed"
assert_contains "status lists the pi skill directory" ".pi/agent/skills/mnexis-memory"
assert_contains "status lists the opencode skill directory" ".config/opencode/skills/mnexis-memory"

status_hash="$(agent_tree_hash)"
run_agent agent status --json
assert_status "status --json exits 0" 0
assert_contains "status --json reports the pi assets" '"environment": "pi"'
assert_contains "status --json reports the installed flag" '"installed": false'
if [[ "$(agent_tree_hash)" == "$status_hash" ]]; then
  ok "status creates nothing in the home"
else
  not_ok "status wrote into the home"
fi

# --- install for pi ------------------------------------------------------
run_agent agent install --env pi
assert_status "install --env pi exits 0" 0
assert_contains "install names the skill" "Installed skill 'mnexis-memory'"
assert_contains "install names the extension" "Installed extension 'mnexis-memory'"
assert_contains "install names the written file" "SKILL.md"
assert_file "the pi skill is installed" "$PI_SKILLS/SKILL.md"
assert_file "the pi extension entry point is installed" "$PI_EXTENSIONS/index.js"
assert_file "the pi extension lib is installed" "$PI_EXTENSIONS/lib/cli.js"
assert_file "the pi extension package.json is installed" "$PI_EXTENSIONS/package.json"
assert_dir_missing "the extension tests are not installed" "$PI_EXTENSIONS/test"

# The installed bytes are the repository bytes (S7).
while IFS='|' read -r installed shipped; do
  [[ -n "$installed" ]] || continue
  if [[ -f "$installed" && -f "$REPO_ROOT/$shipped" ]] &&
    [[ "$(sha256sum <"$installed")" == "$(sha256sum <"$REPO_ROOT/$shipped")" ]]; then
    ok "installed ${installed##*/} is the repository file"
  else
    not_ok "installed ${installed##*/} differs from $shipped"
  fi
done <<EOF
$PI_SKILLS/SKILL.md|skills/mnexis-memory/SKILL.md
$PI_EXTENSIONS/index.js|extensions/mnexis-memory/index.js
$PI_EXTENSIONS/lib/cli.js|extensions/mnexis-memory/lib/cli.js
$PI_EXTENSIONS/lib/chrome.js|extensions/mnexis-memory/lib/chrome.js
$PI_EXTENSIONS/lib/door.js|extensions/mnexis-memory/lib/door.js
$PI_EXTENSIONS/lib/gate.js|extensions/mnexis-memory/lib/gate.js
$PI_EXTENSIONS/lib/workspace.js|extensions/mnexis-memory/lib/workspace.js
$PI_EXTENSIONS/package.json|extensions/mnexis-memory/package.json
$PI_EXTENSIONS/README.md|extensions/mnexis-memory/README.md
EOF

# --- status after the install -------------------------------------------
run_agent agent status
assert_contains "status reports the skill as installed" "skill:     installed"
assert_contains "status reports the extension as installed" "extension: installed"

# --- idempotency ---------------------------------------------------------
first_tree="$(agent_tree_hash)"
run_agent agent install --env pi
assert_status "a second install exits 0" 0
assert_contains "a second install reports the update" "Updated skill"
if [[ "$(agent_tree_hash)" == "$first_tree" ]]; then
  ok "a second install leaves the bytes untouched"
else
  not_ok "a second install changed the installed bytes"
fi

# --- a foreign file inside the asset survives install and uninstall -----
printf 'not ours\n' >"$PI_SKILLS/foreign.md"
printf 'not ours\n' >"$PI_EXTENSIONS/NOTES.md"
foreign_hash="$(sha256sum <"$PI_SKILLS/foreign.md")"
run_agent agent install --env pi
if [[ "$(sha256sum <"$PI_SKILLS/foreign.md")" == "$foreign_hash" ]]; then
  ok "install does not overwrite a foreign file"
else
  not_ok "install overwrote a foreign file"
fi
run_agent agent uninstall --env pi
assert_status "uninstall exits 0" 0
assert_file "the foreign skill file survives" "$PI_SKILLS/foreign.md"
assert_file "the foreign extension file survives" "$PI_EXTENSIONS/NOTES.md"
assert_dir_missing "the shipped skill file is removed" "$PI_SKILLS/SKILL.md"
assert_dir_missing "the shipped extension file is removed" "$PI_EXTENSIONS/index.js"
rm -f "$PI_SKILLS/foreign.md" "$PI_EXTENSIONS/NOTES.md"
run_agent agent uninstall --env pi
assert_dir_missing "an emptied skill directory is removed" "$PI_SKILLS"
assert_dir_missing "an emptied extension directory is removed" "$PI_EXTENSIONS"
assert_dir_missing "an emptied lib directory is removed" "$PI_EXTENSIONS/lib"

# --- a symlinked asset file is replaced, its target is not destroyed -----
TARGET="$SCRATCH/dotfiles-SKILL.md"
printf 'my own dotfile\n' >"$TARGET"
run_agent agent install --env pi
ln -sfn "$TARGET" "$PI_SKILLS/SKILL.md"
run_agent agent install --env pi
if [[ "$(cat "$TARGET")" == "my own dotfile" ]]; then
  ok "install does not write through a symlink"
else
  not_ok "install wrote through the symlink and destroyed the target"
fi
run_agent agent uninstall --env pi
if [[ -f "$TARGET" ]]; then
  ok "uninstall leaves the symlink target alone"
else
  not_ok "uninstall destroyed the symlink target"
fi

# --- opencode gets the skill only ---------------------------------------
pi_hash="$(cd "$AGENT_HOME/.pi" && find . -type f -exec sha256sum {} + | sort | sha256sum)"
run_agent agent install --env opencode
assert_status "install --env opencode exits 0" 0
assert_file "the opencode skill is installed" "$OC_SKILLS/SKILL.md"
assert_dir_missing "nothing is installed in opencode extensions" \
  "$AGENT_HOME/.config/opencode/extensions"
if [[ "$(cd "$AGENT_HOME/.pi" && find . -type f -exec sha256sum {} + | sort | sha256sum)" == "$pi_hash" ]]; then
  ok "the opencode install leaves the pi installation untouched"
else
  not_ok "the opencode install changed the pi installation"
fi
run_agent agent uninstall --env opencode
assert_status "uninstall --env opencode exits 0" 0
assert_dir_missing "the opencode skill directory is removed" "$OC_SKILLS"

# opencode has no extension target: asking for one is an error.
run_agent agent install --env opencode --only extension
assert_status "an extension in opencode exits 2" 2
assert_contains "the opencode extension error names the skill" "--only skill"

# --- --only installs exactly what was asked for -------------------------
run_agent agent install --env pi --only skill
assert_status "install --env pi --only skill exits 0" 0
assert_file "--only skill writes the skill" "$PI_SKILLS/SKILL.md"
assert_dir_missing "--only skill does not create the extension" "$PI_EXTENSIONS"
run_agent agent install --env pi --only extension
assert_status "install --env pi --only extension exits 0" 0
assert_file "--only extension writes the extension" "$PI_EXTENSIONS/index.js"
run_agent agent uninstall --env pi --only skill
assert_status "uninstall --env pi --only skill exits 0" 0
assert_dir_missing "--only skill removes only the skill" "$PI_SKILLS"
assert_file "--only skill leaves the extension alone" "$PI_EXTENSIONS/index.js"
run_agent agent uninstall --env pi
assert_dir_missing "the tree is clean after the filters" "$PI_EXTENSIONS"

# uninstall for opencode cannot take an extension either
run_agent agent uninstall --env opencode --only extension
assert_status "removing an extension in opencode exits 2" 2
assert_contains "the uninstall error names the skill" "--only skill"

# --- unknown values ------------------------------------------------------
run_agent agent install --env claude
assert_status "an unknown --env exits 2" 2
assert_contains "an unknown --env names the supported environments" "supported: pi, opencode"
run_agent agent uninstall --env claude
assert_status "an unknown --env on uninstall exits 2" 2
run_agent agent install --only bogus
assert_status "an unknown --only exits 2" 2
assert_contains "an unknown --only names the supported selections" \
  "supported: all, skill, extension"

# --- the profile is never touched ---------------------------------------
mkdir -p "$AGENT_HOME/.local/share/mnexis"
printf 'namespace = "x"\n' >"$AGENT_HOME/.local/share/mnexis/mnexis.toml"
printf 'projects = []\n' >"$AGENT_HOME/.local/share/mnexis/projects.toml"
printf 'fake db\n' >"$AGENT_HOME/.local/share/mnexis/mnexis.db"
profile_hash="$(cd "$AGENT_HOME/.local/share/mnexis" && sha256sum * | sort | sha256sum)"
run_agent agent install --env pi
run_agent agent uninstall --env pi
run_agent agent status
if [[ "$(cd "$AGENT_HOME/.local/share/mnexis" && sha256sum * | sort | sha256sum)" == "$profile_hash" ]]; then
  ok "install, uninstall and status leave the profile byte-identical"
else
  not_ok "the profile changed across the agent commands"
fi

finish
