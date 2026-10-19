#!/usr/bin/env bash
# `uninstall --purge`: tty requirement, y/N confirmation, exact purge
# paths (MNEXIS_DATA_DIR override and $HOME/.local/share/mnexis default).
set -uo pipefail
TESTS_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=helpers.sh
. "$TESTS_DIR/helpers.sh"

seed_profile() {
  rm -rf -- "$HOME/.local/bin" "$HOME/.local/share/mnexis" "$SCRATCH/custom-data"
  mkdir -p -- "$HOME/.local/bin" "$HOME/.local/share/mnexis" "$SCRATCH/custom-data"
  mkdir -p -- "$HOME/.local/share/other"
  printf 'binary-placeholder\n' >"$HOME/.local/bin/mnexis"
  chmod 0755 -- "$HOME/.local/bin/mnexis"
  printf 'db\n' >"$HOME/.local/share/mnexis/mnexis.db"
  printf 'config\n' >"$HOME/.local/share/mnexis/mnexis.toml"
  printf 'unrelated\n' >"$HOME/.local/share/other/keep.txt"
  printf 'db\n' >"$SCRATCH/custom-data/mnexis.db"
}

# 1. No tty on stdin: abort before removing anything.
seed_profile
run_installer uninstall --purge
assert_status "non-tty purge aborts with exit 1" 1
assert_contains "non-tty purge explains the terminal requirement" "not a terminal"
assert_contains "non-tty purge says nothing was removed" "left untouched"
assert_file "non-tty purge keeps the binary" "$HOME/.local/bin/mnexis"
assert_file "non-tty purge keeps the database" "$HOME/.local/share/mnexis/mnexis.db"
assert_file "non-tty purge keeps the config" "$HOME/.local/share/mnexis/mnexis.toml"

# 2. Interactive declines and accepts through a pty.
if ((HAVE_PTY)); then
  seed_profile
  run_installer_pty 'n' uninstall --purge
  assert_status "declined purge exits 1" 1
  assert_contains "declined purge reports the cancellation" "Purge aborted"
  assert_file "declined purge keeps the binary" "$HOME/.local/bin/mnexis"
  assert_file "declined purge keeps the database" "$HOME/.local/share/mnexis/mnexis.db"
  assert_file "declined purge keeps the config" "$HOME/.local/share/mnexis/mnexis.toml"

  seed_profile
  run_installer_pty 'y' uninstall --purge
  assert_status "confirmed purge exits 0" 0
  assert_contains "confirmed purge removes the binary" "Removed $HOME/.local/bin/mnexis"
  assert_contains "confirmed purge removes the data directory" \
    "Removed profile data at $HOME/.local/share/mnexis"
  assert_missing "confirmed purge removes the binary" "$HOME/.local/bin/mnexis"
  assert_dir_missing "confirmed purge removes the profile directory" "$HOME/.local/share/mnexis"
  assert_file "confirmed purge keeps unrelated data" "$HOME/.local/share/other/keep.txt"
else
  skip "interactive y/N prompt tests (util-linux script --return unavailable)"
fi

# 3. MNEXIS_DATA_DIR overrides the default profile location (the same
#    resolution as src/cli/profile.rs::resolve_data_dir).
if ((HAVE_PTY)); then
  seed_profile
  export MNEXIS_DATA_DIR="$SCRATCH/custom-data"
  run_installer_pty 'y' uninstall --purge
  unset MNEXIS_DATA_DIR
  assert_status "MNEXIS_DATA_DIR purge exits 0" 0
  assert_dir_missing "MNEXIS_DATA_DIR directory removed" "$SCRATCH/custom-data"
  assert_dir "default profile directory untouched when MNEXIS_DATA_DIR is set" \
    "$HOME/.local/share/mnexis"
  assert_file "default config untouched when MNEXIS_DATA_DIR is set" \
    "$HOME/.local/share/mnexis/mnexis.toml"
else
  skip "MNEXIS_DATA_DIR purge test (util-linux script --return unavailable)"
fi

# 4. Unsafe data directories are refused outright.
RUN_OUT="$(MNEXIS_DATA_DIR=relative/path bash "$INSTALL_SH" uninstall --purge 2>&1 </dev/null)"
RUN_STATUS=$?
assert_status "relative MNEXIS_DATA_DIR is refused" 1
assert_contains "relative path refusal explains the guard" "non-absolute"

RUN_OUT="$(MNEXIS_DATA_DIR=/ bash "$INSTALL_SH" uninstall --purge 2>&1 </dev/null)"
RUN_STATUS=$?
assert_status "'/' MNEXIS_DATA_DIR is refused" 1
assert_contains "'/' refusal explains the guard" "refusing to purge"

# 4b. MNEXIS_DATA_DIR equal to $HOME is refused (third documented unsafe target).
RUN_OUT="$(MNEXIS_DATA_DIR="$HOME" bash "$INSTALL_SH" uninstall --purge 2>&1 </dev/null)"
RUN_STATUS=$?
assert_status "HOME MNEXIS_DATA_DIR is refused" 1
assert_contains "HOME refusal names the home directory" "home directory"

# 4c. Trailing-slash variant: $HOME/ collapses to $HOME after the strip.
RUN_OUT="$(MNEXIS_DATA_DIR="${HOME}/" bash "$INSTALL_SH" uninstall --purge 2>&1 </dev/null)"
RUN_STATUS=$?
assert_status "HOME/ MNEXIS_DATA_DIR is refused" 1
assert_contains "trailing-slash HOME refusal names the home directory" "home directory"

finish
