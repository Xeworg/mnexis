#!/usr/bin/env bash
# The opencode environment: skills land in opencode's global skills
# directory (~/.config/opencode/skills), uninstall removes only the shipped
# skill directories, and the environments stay independent from each other.
set -uo pipefail
TESTS_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=helpers.sh
. "$TESTS_DIR/helpers.sh"

OPENCODE_SKILLS="$HOME/.config/opencode/skills"
PI_SKILLS="$HOME/.pi/agent/skills"
SHIPPED="$REPO_ROOT/skills/mnexis-memory/SKILL.md"

run_installer skills --env opencode
assert_status "skills --env opencode exits 0" 0
assert_file "mnexis-memory SKILL.md installed for opencode" \
  "$OPENCODE_SKILLS/mnexis-memory/SKILL.md"
assert_contains "install names the target directory" "$OPENCODE_SKILLS"

if cmp -s -- "$SHIPPED" "$OPENCODE_SKILLS/mnexis-memory/SKILL.md"; then
  ok "installed opencode SKILL.md matches the shipped file"
else
  not_ok "installed opencode SKILL.md differs from the shipped file"
fi

# Environments are independent: installing for opencode must not create a
# pi installation, and vice versa.
assert_missing "opencode install does not touch the pi skills directory" \
  "$PI_SKILLS/mnexis-memory/SKILL.md"

# A foreign skill in the opencode directory must survive an install.
mkdir -p -- "$OPENCODE_SKILLS/foreign-skill"
printf -- '---\nname: foreign-skill\ndescription: not ours\n---\n' \
  >"$OPENCODE_SKILLS/foreign-skill/SKILL.md"
run_installer skills --env opencode
assert_status "opencode install next to a foreign skill exits 0" 0
assert_file "foreign opencode skill survives the install" \
  "$OPENCODE_SKILLS/foreign-skill/SKILL.md"

# Uninstall removes only the shipped skills, keeps foreign ones, and does
# not touch the other environment.
run_installer uninstall --skills --env opencode
assert_status "uninstall --skills --env opencode exits 0" 0
assert_missing "shipped opencode skill removed" \
  "$OPENCODE_SKILLS/mnexis-memory/SKILL.md"
assert_file "foreign opencode skill kept" \
  "$OPENCODE_SKILLS/foreign-skill/SKILL.md"
assert_missing "uninstall for opencode leaves the pi directory alone" \
  "$PI_SKILLS/mnexis-memory/SKILL.md"

finish
