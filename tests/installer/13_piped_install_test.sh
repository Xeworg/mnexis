#!/usr/bin/env bash
# The documented one-liner: the script piped from the network
# (online-install S3, and the fixes the verification found).
#
# The script is piped, so it has no file of its own, and it is piped from
# the repository root, which *does* have a Cargo.toml: the mode must
# still be the online one.
set -uo pipefail
TESTS_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=helpers.sh
. "$TESTS_DIR/helpers.sh"

RELEASE_DIR="$SCRATCH/release"
FAKE_BIN_DIR="$SCRATCH/fakebin"
ASSET="mnexis-linux-x86_64.tar.gz"

mkdir -p "$RELEASE_DIR" "$FAKE_BIN_DIR"

# --- a fake release --------------------------------------------------------

STAGE="$SCRATCH/stage"
mkdir -p "$STAGE"
cat >"$STAGE/mnexis" <<'FAKE'
#!/usr/bin/env bash
printf 'mnexis 0.1.0\n'
FAKE
chmod 0755 "$STAGE/mnexis"
printf '0.1.0\n' >"$STAGE/VERSION"
printf 'MIT\n' >"$STAGE/LICENSE"
printf '# mnexis\n' >"$STAGE/README.md"
tar -czf "$RELEASE_DIR/$ASSET" -C "$STAGE" mnexis LICENSE README.md VERSION
(cd "$RELEASE_DIR" && sha256sum -- "$ASSET" >SHA256SUMS)

# The fake curl: it serves the local files and records the URLs.
cat >"$FAKE_BIN_DIR/curl" <<'FAKE'
#!/usr/bin/env bash
out=""
url=""
while (($# > 0)); do
  case "$1" in
    -o)
      out="$2"
      shift 2
      ;;
    -*)
      shift
      ;;
    *)
      url="$1"
      shift
      ;;
  esac
done
printf '%s\n' "$url" >>"$MNEXIS_FAKE_CURL_LOG"
name="${url##*/}"
src="$MNEXIS_FAKE_RELEASE_DIR/$name"
if [[ ! -f "$src" ]]; then
  printf 'curl: (22) The requested URL returned error: 404\n' >&2
  exit 22
fi
if [[ -n "$out" ]]; then
  cp -- "$src" "$out"
else
  cat -- "$src"
fi
FAKE
chmod 0755 "$FAKE_BIN_DIR/curl"

export MNEXIS_FAKE_RELEASE_DIR="$RELEASE_DIR"
export MNEXIS_FAKE_CURL_LOG="$SCRATCH/curl.log"

# Pipe the real script into bash, from the repository root.
run_piped() {
  local output status
  : >"$MNEXIS_FAKE_CURL_LOG"
  # The script IS the stdin of bash: redirecting it would feed bash an
  # empty program.
  output="$(cd -- "$REPO_ROOT" && cat "$REPO_ROOT/install.sh" | env \
    PATH="$FAKE_BIN_DIR:$PATH" HOME="$HOME" MNEXIS_FAKE_RELEASE_DIR="$RELEASE_DIR" \
    MNEXIS_FAKE_CURL_LOG="$MNEXIS_FAKE_CURL_LOG" bash 2>&1)"
  status=$?
  RUN_OUT="$output"
  RUN_STATUS="$status"
}

# --- the piped install -----------------------------------------------------

run_piped
assert_status "the piped install exits 0" 0
assert_file "the piped install installs the binary" "$HOME/.local/bin/mnexis"
assert_executable "the installed binary is executable" "$HOME/.local/bin/mnexis"
if grep -q "BASH_SOURCE" <<<"$RUN_OUT"; then
  not_ok "the piped install must not read BASH_SOURCE unbound: $RUN_OUT"
else
  ok "the piped install does not read BASH_SOURCE unbound"
fi

# The repository root has a Cargo.toml, and the script is still the online
# one: the mode follows the script, not the working directory.
if grep -q "latest/download" "$MNEXIS_FAKE_CURL_LOG"; then
  ok "the piped script takes the online path even inside the repository"
else
  not_ok "the piped script did not take the online path (asked for: $(cat -- "$MNEXIS_FAKE_CURL_LOG"))"
fi

# Nothing is left behind in $HOME.
leftovers="$(find "$HOME" -maxdepth 1 -name '.mnexis-install.*' | wc -l)"
if [[ "$leftovers" == "0" ]]; then
  ok "no temporary directory is left in HOME"
else
  not_ok "the install left $leftovers temporary directories in HOME"
fi

# --- the version can be pinned without a subcommand -----------------------

run_piped
: >"$MNEXIS_FAKE_CURL_LOG"
output="$(cd -- "$REPO_ROOT" && cat "$REPO_ROOT/install.sh" | env \
  PATH="$FAKE_BIN_DIR:$PATH" HOME="$HOME" MNEXIS_VERSION=0.1.0 \
  MNEXIS_FAKE_RELEASE_DIR="$RELEASE_DIR" MNEXIS_FAKE_CURL_LOG="$MNEXIS_FAKE_CURL_LOG" \
  bash 2>&1)"
status=$?
RUN_OUT="$output"
RUN_STATUS="$status"
assert_status "the pinned piped install exits 0" 0
asked="$(tr '\n' ' ' <"$MNEXIS_FAKE_CURL_LOG")"
if [[ "$asked" == *"/download/v0.1.0/"* ]]; then
  ok "MNEXIS_VERSION pins the release without a subcommand"
else
  not_ok "MNEXIS_VERSION was ignored (asked for: $asked)"
fi
if [[ "$asked" == *"latest/download"* ]]; then
  not_ok "the pinned install must not ask for the latest release"
else
  ok "the pinned install does not ask for the latest release"
fi

finish
