#!/usr/bin/env bash
# The online installer: the published artifact is downloaded, verified
# against the checksum the release publishes and installed (online-install
# S2, S3, S4, S5, S7, S10).
#
# The network is faked: a `curl` on PATH serves the local files the test
# prepared, so the behaviour under test is the script's, not GitHub's.
set -uo pipefail
TESTS_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=helpers.sh
. "$TESTS_DIR/helpers.sh"

RELEASE_DIR="$SCRATCH/release"
FAKE_BIN_DIR="$SCRATCH/fakebin"
STANDALONE_DIR="$SCRATCH/standalone"
ASSET="mnexis-linux-x86_64.tar.gz"

# --- a fake release --------------------------------------------------------

mkdir -p "$RELEASE_DIR" "$FAKE_BIN_DIR" "$STANDALONE_DIR"

# The artifact: a stand-in binary that reports its version and records how
# it was called, plus the files the packaging script puts in the archive.
STAGE="$SCRATCH/stage"
mkdir -p "$STAGE"
cat >"$STAGE/mnexis" <<'FAKE'
#!/usr/bin/env bash
printf 'mnexis 0.1.0\n'
if [[ -n "${MNEXIS_FAKE_BIN_LOG:-}" ]]; then
  printf '%s\n' "$*" >>"$MNEXIS_FAKE_BIN_LOG"
fi
FAKE
chmod 0755 "$STAGE/mnexis"
printf '0.1.0\n' >"$STAGE/VERSION"
printf 'MIT\n' >"$STAGE/LICENSE"
printf '# mnexis\n' >"$STAGE/README.md"
tar -czf "$RELEASE_DIR/$ASSET" -C "$STAGE" mnexis LICENSE README.md VERSION
(cd "$RELEASE_DIR" && sha256sum -- "$ASSET" >SHA256SUMS)

# The script under test lives outside the repository: it must not find a
# Cargo.toml and try to build.
cp "$REPO_ROOT/install.sh" "$STANDALONE_DIR/install.sh"

# The fake curl: it reads the destination from -o and serves the local
# file the URL names. It also records the URLs it was asked for.
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

run_standalone() {
  local output status
  : >"$MNEXIS_FAKE_CURL_LOG"
  output="$(env PATH="$FAKE_BIN_DIR:$PATH" bash "$STANDALONE_DIR/install.sh" "$@" 2>&1 </dev/null)"
  status=$?
  RUN_OUT="$output"
  RUN_STATUS="$status"
}

# --- a successful install --------------------------------------------------

run_standalone install
assert_status "the standalone install exits 0" 0
assert_file "the binary is installed" "$HOME/.local/bin/mnexis"
assert_executable "the installed binary is executable" "$HOME/.local/bin/mnexis"
assert_contains "the install reports the version" "Installed mnexis 0.1.0"
assert_contains "the install downloads the artifact" "$ASSET"
downloaded="$(tr '\n' ' ' <"$MNEXIS_FAKE_CURL_LOG")"
if [[ "$downloaded" == *"SHA256SUMS"* ]]; then
  ok "the install downloads the checksums"
else
  not_ok "the install did not download the checksums (asked for: $downloaded)"
fi

installed="$("$HOME/.local/bin/mnexis" --version)"
assert_contains "the installed binary runs" "mnexis 0.1.0"

# The temporary file used for the atomic install must not survive.
leftovers="$(find "$HOME/.local/bin" -name '.mnexis.install.*' | wc -l)"
assert_contains "no temporary file is left behind" "0"

# --- the version can be pinned --------------------------------------------

run_standalone install --version 0.1.0
assert_status "the pinned install exits 0" 0
assert_contains "the pinned install asks for the pinned release" "/download/v0.1.0/"

run_standalone install
assert_contains "the default install asks for the latest release" "/releases/latest/download/"

# --- the checksum is enforced ---------------------------------------------

# A corrupted artifact must not be installed.
printf 'corrupted' >>"$RELEASE_DIR/$ASSET"
rm -f -- "$HOME/.local/bin/mnexis"
run_standalone install
assert_status "a corrupted artifact is refused" 1
assert_contains "the refusal names the checksum" "checksum mismatch"
assert_missing "nothing is installed from a corrupted artifact" "$HOME/.local/bin/mnexis"

# Rebuild the good artifact.
tar -czf "$RELEASE_DIR/$ASSET" -C "$STAGE" mnexis LICENSE README.md VERSION
(cd "$RELEASE_DIR" && sha256sum -- "$ASSET" >SHA256SUMS)

# A release without a checksum file must not be installed either.
mv "$RELEASE_DIR/SHA256SUMS" "$RELEASE_DIR/SHA256SUMS.away"
run_standalone install
assert_status "a release without a checksum file is refused" 1
assert_contains "the refusal names the missing checksum file" "checksum file could not be downloaded"
assert_missing "nothing is installed without a checksum file" "$HOME/.local/bin/mnexis"

# And a checksum file without a line for the artifact is refused too.
printf '%s  mnexis-linux-something-else.tar.gz\n' "$(printf '0' | sha256sum | awk '{print $1}')" \
  >"$RELEASE_DIR/SHA256SUMS"
run_standalone install
assert_status "a release without a checksum for the artifact is refused" 1
assert_contains "the refusal explains the missing checksum" "no checksum"
assert_missing "nothing is installed without a checksum" "$HOME/.local/bin/mnexis"
mv "$RELEASE_DIR/SHA256SUMS.away" "$RELEASE_DIR/SHA256SUMS"

# --- the artifact must match the pinned version ---------------------------

printf '9.9.9\n' >"$STAGE/VERSION"
tar -czf "$RELEASE_DIR/$ASSET" -C "$STAGE" mnexis LICENSE README.md VERSION
(cd "$RELEASE_DIR" && sha256sum -- "$ASSET" >SHA256SUMS)
run_standalone install --version 0.1.0
assert_status "an artifact of another version is refused" 1
assert_contains "the refusal names both versions" "9.9.9"
assert_missing "nothing is installed from a mismatched artifact" "$HOME/.local/bin/mnexis"
printf '0.1.0\n' >"$STAGE/VERSION"

# --- an unsupported platform ----------------------------------------------

cat >"$FAKE_BIN_DIR/uname" <<'FAKE'
#!/usr/bin/env bash
case "$1" in
  -s) printf 'Darwin\n' ;;
  -m) printf 'x86_64\n' ;;
  *) printf 'Darwin\n' ;;
esac
FAKE
chmod 0755 "$FAKE_BIN_DIR/uname"
run_standalone install
assert_status "an unsupported platform is refused" 1
assert_contains "the refusal names the platforms mnexis publishes" "linux-x86_64"
rm -f -- "$FAKE_BIN_DIR/uname"

# --- installing twice is idempotent ---------------------------------------

run_standalone install
assert_status "the second install exits 0" 0
assert_file "the binary is still installed" "$HOME/.local/bin/mnexis"

# --- the one-liner closes with the TUI ------------------------------------

export MNEXIS_FAKE_BIN_LOG="$SCRATCH/bin.log"

# Without a terminal on stdout nobody is watching: the TUI must not start
# (and the script must not hang).
: >"$MNEXIS_FAKE_BIN_LOG"
run_standalone install
if grep -q '^tui$' "$MNEXIS_FAKE_BIN_LOG" 2>/dev/null; then
  not_ok "the TUI must not start when the output is not a terminal"
else
  ok "the TUI does not start when the output is not a terminal"
fi

# With a terminal, the install opens the TUI (and creates the profile it
# needs).
if ((HAVE_PTY == 1)); then
  : >"$MNEXIS_FAKE_BIN_LOG"
  quoted=""
  printf -v quoted '%q ' "$BASH_BIN" "$STANDALONE_DIR/install.sh" install
  printf '%s\n' "$quoted" | script -qec "env PATH='$FAKE_BIN_DIR:$PATH' $quoted" /dev/null >"$SCRATCH/tui.out" 2>&1
  RUN_STATUS=$?
  RUN_OUT="$(cat -- "$SCRATCH/tui.out")"
  assert_status "the install with a terminal exits 0" 0
  if grep -q '^init$' "$MNEXIS_FAKE_BIN_LOG" 2>/dev/null; then
    ok "the install creates the profile before the TUI"
  else
    not_ok "the install did not create the profile (calls: $(cat -- "$MNEXIS_FAKE_BIN_LOG"))"
  fi
  if grep -q '^tui$' "$MNEXIS_FAKE_BIN_LOG" 2>/dev/null; then
    ok "the install opens the TUI"
  else
    not_ok "the install did not open the TUI (calls: $(cat -- "$MNEXIS_FAKE_BIN_LOG"))"
  fi
else
  skip "the TUI launch needs util-linux script for a pty"
fi

finish
