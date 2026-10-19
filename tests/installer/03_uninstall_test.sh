#!/usr/bin/env bash
# Plain `uninstall`: removes only the installed binary, keeps profile data
# and sibling files, and is a clean no-op when the binary is missing.
set -uo pipefail
TESTS_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=helpers.sh
. "$TESTS_DIR/helpers.sh"

seed_install() {
  mkdir -p -- "$HOME/.local/bin" "$HOME/.local/share/mnexis"
  printf 'binary-placeholder\n' >"$HOME/.local/bin/mnexis"
  chmod 0755 -- "$HOME/.local/bin/mnexis"
  printf 'keep-me\n' >"$HOME/.local/bin/other-tool"
  printf 'db\n' >"$HOME/.local/share/mnexis/mnexis.db"
  printf 'config\n' >"$HOME/.local/share/mnexis/mnexis.toml"
}

seed_install
run_installer uninstall
assert_status "uninstall exits 0" 0
assert_contains "uninstall reports the removed path" "Removed $HOME/.local/bin/mnexis"
assert_missing "installed binary removed" "$HOME/.local/bin/mnexis"
assert_file "sibling file in the install dir untouched" "$HOME/.local/bin/other-tool"
assert_file "profile database kept by plain uninstall" "$HOME/.local/share/mnexis/mnexis.db"
assert_file "profile config kept by plain uninstall" "$HOME/.local/share/mnexis/mnexis.toml"

# Idempotent: a second uninstall of a missing binary is a clean success.
run_installer uninstall
assert_status "second uninstall (nothing installed) exits 0" 0
assert_contains "no-op uninstall says nothing to do" "nothing to do"
assert_file "no-op uninstall leaves profile data alone" "$HOME/.local/share/mnexis/mnexis.db"

finish
