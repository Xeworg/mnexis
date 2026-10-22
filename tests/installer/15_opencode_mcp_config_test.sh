#!/usr/bin/env bash
# opencode's MCP configuration: `mnexis agent install --env opencode`
# wires the MCP server into opencode's own configuration file, and
# `mnexis agent uninstall --env opencode` takes it out again — without
# ever losing what the user wrote there.
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
OC_CONFIG_DIR="$AGENT_HOME/.config/opencode"
OC_CONFIG="$OC_CONFIG_DIR/opencode.json"
OC_JSONC="$OC_CONFIG_DIR/opencode.jsonc"

run_agent() {
  local output status
  output="$(env -u PI_CODING_AGENT_DIR -u MNEXIS_DATA_DIR HOME="$AGENT_HOME" \
    "$BIN" "$@" 2>&1 </dev/null)"
  status=$?
  RUN_OUT="$output"
  RUN_STATUS="$status"
}

# The helpers check the command's output; these check the file it wrote.
assert_file_contains() { # name file needle
  if [[ -f "$2" ]] && grep -qF -- "$3" "$2"; then
    ok "$1"
  else
    not_ok "$1 (file $2 does not contain '$3')"
  fi
}

assert_file_lacks() { # name file needle
  if [[ -f "$2" ]] && ! grep -qF -- "$3" "$2"; then
    ok "$1"
  else
    not_ok "$1 (file $2 still contains '$3')"
  fi
}

# --- install writes the entry --------------------------------------------
run_agent agent install --env opencode
assert_status "install --env opencode exits 0" 0
assert_file "opencode's configuration is created" "$OC_CONFIG"
assert_file_contains "the configuration carries the mnexis server" "$OC_CONFIG" '"mnexis"'
assert_file_contains "the server is a local one" "$OC_CONFIG" '"local"'
assert_file_contains "the server runs the MCP server of the binary" "$OC_CONFIG" '"mcp"'
assert_file_contains "the server is enabled" "$OC_CONFIG" '"enabled": true'
assert_contains "the install reports the host configuration" "host config"

# The written file is valid JSON with the entry where opencode expects it.
if command -v python3 >/dev/null 2>&1; then
  if python3 -c "
import json,sys
value=json.load(open(sys.argv[1]))
server=value['mcp']['mnexis']
assert server['type']=='local'
assert server['enabled'] is True
assert server['command'][-2:]==['mcp','serve']
" "$OC_CONFIG" 2>/dev/null; then
    ok "the configuration is valid JSON with the entry opencode expects"
  else
    not_ok "the configuration is not the JSON opencode expects"
  fi
else
  skip "python3 is not available to parse the configuration"
fi

# --- install is idempotent ----------------------------------------------
before="$(sha256sum <"$OC_CONFIG")"
run_agent agent install --env opencode
assert_status "a second install exits 0" 0
assert_contains "a second install reports the unchanged configuration" "unchanged"
if [[ "$(sha256sum <"$OC_CONFIG")" == "$before" ]]; then
  ok "a second install leaves the configuration byte-identical"
else
  not_ok "a second install rewrote the configuration"
fi

# --- the user's own configuration survives --------------------------------
printf '{\n  "theme": "dark",\n  "model": "anthropic/claude"\n}\n' >"$OC_CONFIG"
run_agent agent install --env opencode
assert_status "install over a user configuration exits 0" 0
assert_file_contains "the user's theme survives" "$OC_CONFIG" '"theme": "dark"'
assert_file_contains "the user's model survives" "$OC_CONFIG" '"model": "anthropic/claude"'
assert_file_contains "the entry is added to the user's configuration" "$OC_CONFIG" '"mnexis"'

# --- uninstall takes the entry out and nothing else -----------------------
run_agent agent uninstall --env opencode
assert_status "uninstall --env opencode exits 0" 0
assert_file_contains "the user's theme survives the uninstall" "$OC_CONFIG" '"theme": "dark"'
assert_file_contains "the user's model survives the uninstall" "$OC_CONFIG" '"model": "anthropic/claude"'
assert_file_lacks "the mnexis entry is gone" "$OC_CONFIG" '"mnexis"'
assert_contains "the uninstall reports the host configuration" "host config removed"

# --- a JSONC configuration is never rewritten ---------------------------
rm -f "$OC_CONFIG"
printf '{\n  // the user writes comments here\n  "theme": "dark"\n}\n' >"$OC_JSONC"
jsonc_hash="$(sha256sum <"$OC_JSONC")"
run_agent agent install --env opencode
assert_status "install over a JSONC configuration exits 0" 0
assert_contains "the refusal is reported" "refused"
assert_contains "the refusal explains what to add" '"mcp"'
if [[ "$(sha256sum <"$OC_JSONC")" == "$jsonc_hash" ]]; then
  ok "the JSONC configuration is byte-identical after the refusal"
else
  not_ok "the JSONC configuration was rewritten"
fi
assert_missing "no JSON configuration is created next to a JSONC one" "$OC_CONFIG"
rm -f "$OC_JSONC"

# --- an invalid configuration is never rewritten -------------------------
printf '{not json' >"$OC_CONFIG"
invalid_hash="$(sha256sum <"$OC_CONFIG")"
run_agent agent install --env opencode
assert_status "install over an invalid configuration exits 0" 0
assert_contains "the refusal is reported" "refused"
if [[ "$(sha256sum <"$OC_CONFIG")" == "$invalid_hash" ]]; then
  ok "the invalid configuration is byte-identical after the refusal"
else
  not_ok "the invalid configuration was rewritten"
fi

# --- the pi environment is not touched ----------------------------------
rm -f "$OC_CONFIG"
run_agent agent install --env pi
assert_status "install --env pi exits 0" 0
assert_missing "the pi install writes no opencode configuration" "$OC_CONFIG"

# --- status reports the configuration ------------------------------------
run_agent agent install --env opencode
run_agent agent status
assert_status "status exits 0" 0
assert_contains "status reports the MCP entry as configured" "mcp:       configured"
run_agent agent uninstall --env opencode
run_agent agent status
assert_contains "status reports the MCP entry as not configured" "mcp:       not configured"

finish
