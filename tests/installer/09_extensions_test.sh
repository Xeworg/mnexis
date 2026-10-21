#!/usr/bin/env bash
# The extensions subcommand: extensions land in pi's extensions directory
# (<agent-dir>/extensions), the test directory is not shipped, uninstall
# removes only the shipped extension directories, and the scopes stay
# separate from --skills and --purge.
set -uo pipefail
TESTS_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=helpers.sh
. "$TESTS_DIR/helpers.sh"

PI_EXTENSIONS="$HOME/.pi/agent/extensions"
PI_SKILLS="$HOME/.pi/agent/skills"

run_installer extensions --env pi
assert_status "extensions --env pi exits 0" 0
assert_file "extension entry point installed" "$PI_EXTENSIONS/mnexis-memory/index.js"
assert_file "extension module installed" "$PI_EXTENSIONS/mnexis-memory/lib/cli.js"
assert_contains "install names the extension" "Installed extension mnexis-memory"
assert_contains "install names the target directory" "$PI_EXTENSIONS"

# The test directory is development-only and must not be shipped.
assert_missing "extension tests are not installed" "$PI_EXTENSIONS/mnexis-memory/test"

# A relocated pi agent directory is honoured.
CUSTOM_AGENT="$HOME/custom-agent"
run_installer_with_env "PI_CODING_AGENT_DIR=$CUSTOM_AGENT" extensions --env pi
assert_status "extensions honours PI_CODING_AGENT_DIR" 0
assert_file "extension installed in the relocated agent dir" \
  "$CUSTOM_AGENT/extensions/mnexis-memory/index.js"

# Unknown environments are refused.
run_installer extensions --env claude
assert_status "unknown extension environment exits 2" 2
assert_contains "unknown extension environment lists supported environments" "pi"

# The scope is separate from the skills one.
run_installer uninstall --skills --extensions --env pi
assert_status "--skills with --extensions exits 2" 2
assert_contains "separate scopes are explained" "separate scopes"

# Uninstall removes only what mnexis ships.
mkdir -p -- "$PI_EXTENSIONS/foreign-extension"
printf -- 'export default function () {}\n' >"$PI_EXTENSIONS/foreign-extension/index.js"
run_installer uninstall --extensions --env pi
assert_status "uninstall --extensions --env pi exits 0" 0
assert_missing "shipped extension removed" "$PI_EXTENSIONS/mnexis-memory"
assert_file "foreign extension kept" "$PI_EXTENSIONS/foreign-extension/index.js"

finish
