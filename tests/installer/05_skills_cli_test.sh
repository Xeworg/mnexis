#!/usr/bin/env bash
# CLI contract for the skills subcommand: help text, argument validation,
# environment names, and the no-toolchain guarantee (no build runs here).
set -uo pipefail
TESTS_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=helpers.sh
. "$TESTS_DIR/helpers.sh"

run_installer skills --help
assert_status "skills --help exits 0" 0
assert_contains "skills --help prints usage" "Usage: ./install.sh"
assert_contains "skills --help documents the subcommand" "skills"
assert_contains "skills --help documents --env" "--env"
assert_contains "skills --help lists the pi environment" "pi"

run_installer skills --bogus
assert_status "unknown skills option exits 2" 2
assert_contains "unknown skills option names the argument" "--bogus"

run_installer skills --env claude
assert_status "unknown environment exits 2" 2
assert_contains "unknown environment names the value" "claude"
assert_contains "unknown environment lists supported environments" "pi"

run_installer skills
assert_status "skills without a tty and without --env fails" 1
assert_contains "skills non-tty failure points at --env" "--env"

run_installer uninstall --skills
assert_status "uninstall --skills without a tty and without --env fails" 1
assert_contains "uninstall --skills non-tty failure points at --env" "--env"

run_installer uninstall --skills --env claude
assert_status "unknown environment for uninstall --skills exits 2" 2
assert_contains "unknown uninstall environment lists supported environments" "pi"

# The skills subcommand is a pure file copy: it must not require the Rust
# toolchain. PATH carries coreutils (cp/mkdir/chmod) but no cargo/rustc.
STUB_PATH="$SCRATCH/path-nocargo"
mkdir -p -- "$STUB_PATH"
for tool in cp mkdir chmod rm; do
  ln -s -- "$(command -v "$tool")" "$STUB_PATH/$tool"
done
run_installer_with_path "$STUB_PATH" skills --env pi
assert_status "skills --env pi succeeds without cargo on PATH" 0
assert_file "skill installed without a toolchain" "$HOME/.pi/skills/mnexis-memory/SKILL.md"
assert_lacks "no toolchain complaint" "cargo was not found"

finish
