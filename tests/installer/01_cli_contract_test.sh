#!/usr/bin/env bash
# CLI contract for install.sh: help text, argument validation, and the
# missing cargo/rustc detection (no build runs in this test).
set -uo pipefail
TESTS_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=helpers.sh
. "$TESTS_DIR/helpers.sh"

run_installer --help
assert_status "--help exits 0" 0
assert_contains "--help prints usage" "Usage: ./install.sh"
assert_contains "--help documents uninstall" "uninstall"
assert_contains "--help documents --purge" "--purge"

run_installer -h
assert_status "-h exits 0" 0

run_installer uninstall --help
assert_status "uninstall --help exits 0" 0
assert_contains "uninstall --help prints usage" "Usage: ./install.sh"

run_installer frobnicate
assert_status "unknown command exits 2" 2
assert_contains "unknown command names the argument" "frobnicate"

run_installer --bogus
assert_status "unknown top-level option exits 2" 2

run_installer uninstall --bogus
assert_status "unknown uninstall option exits 2" 2
assert_contains "unknown uninstall option names the argument" "--bogus"

# Missing cargo and rustc: PATH has no toolchain at all.
mkdir -p -- "$SCRATCH/empty-path"
run_installer_with_path "$SCRATCH/empty-path"
assert_status "install fails when cargo/rustc are missing" 1
assert_contains "missing toolchain points to rustup" "rustup"
assert_missing "no install dir is created when the toolchain is missing" "$HOME/.local/bin"

# cargo present but rustc missing.
mkdir -p -- "$SCRATCH/cargo-only"
printf '#!/bin/sh\nexit 0\n' >"$SCRATCH/cargo-only/cargo"
chmod 0755 -- "$SCRATCH/cargo-only/cargo"
run_installer_with_path "$SCRATCH/cargo-only"
assert_status "install fails when rustc is missing" 1
assert_contains "missing rustc points to rustup" "rustup"
assert_missing "no install dir is created when rustc is missing" "$HOME/.local/bin"

finish
