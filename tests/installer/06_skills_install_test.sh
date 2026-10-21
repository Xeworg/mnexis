#!/usr/bin/env bash
# Skills install flow against a scratch HOME: --env installs the shipped
# skills, re-running overwrites cleanly, foreign skills are never touched,
# and the interactive selector reaches the same result through a pty.
set -uo pipefail
TESTS_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=helpers.sh
. "$TESTS_DIR/helpers.sh"

PI_SKILLS="$HOME/.pi/agent/skills"
SHIPPED="$REPO_ROOT/skills/mnexis-memory/SKILL.md"

# A relocated pi agent directory is honoured: pi advertises user skills
# from <agent-dir>/skills, and <agent-dir> follows PI_CODING_AGENT_DIR.
# Runs first, while the default agent dir is still empty.
CUSTOM_AGENT="$HOME/custom-agent"
run_installer_with_env "PI_CODING_AGENT_DIR=$CUSTOM_AGENT" skills --env pi
assert_status "skills --env pi honours PI_CODING_AGENT_DIR" 0
assert_file "skill installed in the relocated agent dir" \
  "$CUSTOM_AGENT/skills/mnexis-memory/SKILL.md"
assert_missing "relocated install does not touch the default agent dir" \
  "$PI_SKILLS/mnexis-memory/SKILL.md"

run_installer skills --env pi
assert_status "skills --env pi exits 0" 0
assert_file "mnexis-memory SKILL.md installed" "$PI_SKILLS/mnexis-memory/SKILL.md"
assert_contains "install names the skill" "Installed skill mnexis-memory"
assert_contains "install names the target directory" "$PI_SKILLS"

if cmp -s -- "$SHIPPED" "$PI_SKILLS/mnexis-memory/SKILL.md"; then
  ok "installed SKILL.md matches the shipped file"
else
  not_ok "installed SKILL.md differs from the shipped file"
fi

# Re-running overwrites: mutate the installed copy, reinstall, and the
# shipped content must be back.
printf 'drift\n' >>"$PI_SKILLS/mnexis-memory/SKILL.md"
run_installer skills --env pi
assert_status "re-running skills --env pi exits 0" 0
if cmp -s -- "$SHIPPED" "$PI_SKILLS/mnexis-memory/SKILL.md"; then
  ok "re-running restores the shipped SKILL.md"
else
  not_ok "re-running left drifted SKILL.md in place"
fi

# A foreign skill in the same directory must survive an install.
mkdir -p -- "$PI_SKILLS/foreign-skill"
printf -- '---\nname: foreign-skill\ndescription: not ours\n---\n' \
  >"$PI_SKILLS/foreign-skill/SKILL.md"
run_installer skills --env pi
assert_status "install next to a foreign skill exits 0" 0
assert_file "foreign skill survives the install" "$PI_SKILLS/foreign-skill/SKILL.md"

# The binary is not a prerequisite: skills install with nothing in
# ~/.local/bin.
assert_missing "skills install does not install the binary" "$HOME/.local/bin/mnexis"

# Interactive selection: one environment (pi), chosen through the pty.
if ((HAVE_PTY == 1)); then
  run_installer_pty "1" skills
  assert_status "interactive skills selection exits 0" 0
  assert_file "interactive selection installs the skill" \
    "$PI_SKILLS/mnexis-memory/SKILL.md"
else
  skip "interactive skills selection (util-linux script unavailable)"
fi

finish
