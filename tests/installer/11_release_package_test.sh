#!/usr/bin/env bash
# The release packaging: the artifact, its contents and its checksum
# (online-install S2, S5, S6).
set -uo pipefail
TESTS_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=helpers.sh
. "$TESTS_DIR/helpers.sh"

PACKAGE_SH="$REPO_ROOT/scripts/package-release.sh"
DIST="$SCRATCH/dist"

# A stand-in for the built binary: it only has to report its version, the
# way the real one does.
FAKE_BIN="$SCRATCH/fake-mnexis"
cat >"$FAKE_BIN" <<'FAKE'
#!/usr/bin/env bash
printf 'mnexis 0.1.0\n'
FAKE
chmod 0755 "$FAKE_BIN"

run_package() {
  local output status
  output="$(bash "$PACKAGE_SH" "$@" 2>&1 </dev/null)"
  status=$?
  RUN_OUT="$output"
  RUN_STATUS="$status"
}

# --- packaging one platform -----------------------------------------------

run_package --version 0.1.0 --arch x86_64 --outdir "$DIST" --bin "$FAKE_BIN"
assert_status "packaging exits 0" 0
assert_file "the artifact is written" "$DIST/mnexis-linux-x86_64.tar.gz"
assert_file "the checksum file is written" "$DIST/SHA256SUMS"

# The archive carries the binary, the licence, the readme and the version.
listing="$(tar -tzf "$DIST/mnexis-linux-x86_64.tar.gz" | sort | tr '\n' ' ')"
for member in mnexis LICENSE README.md VERSION; do
  if [[ " $listing " == *" $member "* ]]; then
    ok "the archive carries $member"
  else
    not_ok "the archive does not carry $member (contents: $listing)"
  fi
done

version_file="$(tar -xzf "$DIST/mnexis-linux-x86_64.tar.gz" -O VERSION)"
assert_contains "the version file names the version" "0.1.0"

# The checksum line is the one sha256sum itself verifies.
(cd "$DIST" && sha256sum -c SHA256SUMS >/dev/null 2>&1)
assert_status "the checksum verifies with sha256sum -c" 0
assert_contains "the checksum file names the artifact" "mnexis-linux-x86_64.tar.gz"

# --- the same platform again does not duplicate its line -------------------

run_package --version 0.1.0 --arch x86_64 --outdir "$DIST" --bin "$FAKE_BIN"
assert_status "packaging twice exits 0" 0
lines="$(grep -c 'mnexis-linux-x86_64.tar.gz' "$DIST/SHA256SUMS" || true)"
assert_contains "the checksum file keeps one line per artifact" "1"
[[ "$lines" == "1" ]] || not_ok "the checksum file has $lines lines for the same artifact"

# --- a second platform adds its own line ----------------------------------

run_package --version 0.1.0 --arch aarch64 --outdir "$DIST" --bin "$FAKE_BIN"
assert_status "packaging another platform exits 0" 0
assert_file "the second artifact is written" "$DIST/mnexis-linux-aarch64.tar.gz"
(cd "$DIST" && sha256sum -c SHA256SUMS >/dev/null 2>&1)
assert_status "both checksums verify together" 0

# --- refusals --------------------------------------------------------------

run_package --version 9.9.9 --arch x86_64 --outdir "$DIST" --bin "$FAKE_BIN"
assert_status "a version the binary does not report is refused" 2
assert_contains "the refusal names both versions" "9.9.9"

run_package --version 0.1.0 --arch ppc64 --outdir "$DIST" --bin "$FAKE_BIN"
assert_status "an unsupported architecture is refused" 2
assert_contains "the refusal names the architecture" "ppc64"

run_package --version 0.1.0 --arch x86_64 --outdir "$DIST" --bin "$SCRATCH/missing"
assert_status "a missing binary is refused" 2
assert_contains "the refusal names the missing path" "missing"

run_package --arch x86_64 --outdir "$DIST" --bin "$FAKE_BIN"
assert_status "a missing version is refused" 2

run_package --version "not-a-version" --arch x86_64 --outdir "$DIST" --bin "$FAKE_BIN"
assert_status "a malformed version is refused" 2

finish
