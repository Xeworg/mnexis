#!/usr/bin/env bash
# Skills uninstall flow against a scratch HOME: --env removes exactly the
# mnexis-shipped skill directories, foreign skills and the plain uninstall
# scope stay untouched, and a second run is a quiet no-op.
set -uo pipefail
TESTS_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=helpers.sh
. "$TESTS_DIR/helpers.sh"

PI_SKILLS="$HOME/.pi/skills"

run_installer skills --env pi
assert_status "seeding: skills --env pi exits 0" 0
mkdir -p -- "$PI_SKILLS/foreign-skill"
printf -- '---\nname: foreign-skill\ndescription: not ours\n---\n' \
  >"$PI_SKILLS/foreign-skill/SKILL.md"

# Plain uninstall (binary scope) must not touch installed skills.
touch "$HOME/.local/bin/mnexis" 2>/dev/null || mkdir -p -- "$HOME/.local/bin"
run_installer uninstall
assert_status "plain uninstall exits 0" 0
assert_file "plain uninstall keeps the installed skill" \
  "$PI_SKILLS/mnexis-memory/SKILL.md"

run_installer uninstall --skills --env pi
assert_status "uninstall --skills --env pi exits 0" 0
assert_dir_missing "mnexis-memory removed from pi skills" "$PI_SKILLS/mnexis-memory"
assert_file "foreign skill survives the uninstall" "$PI_SKILLS/foreign-skill/SKILL.md"
assert_contains "uninstall names the removed skill" "Removed skill mnexis-memory"

run_installer uninstall --skills --env pi
assert_status "second uninstall --skills is a quiet success" 0
assert_contains "second uninstall reports nothing to do" "No mnexis skills"

# Interactive selection through the pty after reseeding.
run_installer skills --env pi
assert_status "reseeding: skills --env pi exits 0" 0
if ((HAVE_PTY == 1)); then
  run_installer_pty "1" uninstall --skills
  assert_status "interactive skills uninstall exits 0" 0
  assert_dir_missing "interactive uninstall removes the skill" \
    "$PI_SKILLS/mnexis-memory"
  assert_file "interactive uninstall keeps foreign skills" \
    "$PI_SKILLS/foreign-skill/SKILL.md"
else
  skip "interactive skills uninstall (util-linux script unavailable)"
fi

finish
