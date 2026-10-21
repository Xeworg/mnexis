#!/usr/bin/env bash
# The release workflow: the step that merges the checksums has to work,
# and the gate it sets must not be red before it starts
# (online-install S6).
set -uo pipefail
TESTS_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=helpers.sh
. "$TESTS_DIR/helpers.sh"

WORKFLOW="$REPO_ROOT/.github/workflows/release.yml"

# --- the workflow says what it means ---------------------------------------

if grep -q 'install.sh | bash' "$WORKFLOW"; then
  ok "the release notes show the bash one-liner"
else
  not_ok "the release notes must show the bash one-liner (the script is bash)"
fi
if grep -q 'install.sh | sh' "$WORKFLOW"; then
  not_ok "the release notes must not show the sh one-liner"
else
  ok "the release notes do not show the sh one-liner"
fi
if grep -qE 'run: .*-D warnings' "$WORKFLOW"; then
  not_ok "the workflow must not turn the warning baseline into errors"
else
  ok "the workflow does not turn the warning baseline into errors"
fi

# --- the merge step runs ---------------------------------------------------

# Extract the body of one workflow step, with its YAML indentation removed.
extract_step() {
  awk -v name="$1" '
    $0 ~ ("- name: " name "$") { found = 1 }
    found && /^        run: \|/ { inrun = 1; next }
    inrun {
      if ($0 ~ /^          /) { sub(/^          /, ""); print; next }
      exit
    }
  ' "$WORKFLOW"
}

body="$SCRATCH/merge.sh"
extract_step "Merge the checksums" >"$body"
if [[ -s "$body" ]]; then
  ok "the merge step can be extracted from the workflow"
else
  not_ok "the merge step could not be extracted from the workflow"
fi

# A fake artifacts tree, as the build job would leave it.
WORK="$SCRATCH/workflow"
mkdir -p "$WORK/artifacts/mnexis-x86_64" "$WORK/artifacts/mnexis-aarch64"
STAGE="$SCRATCH/stage"
mkdir -p "$STAGE"
printf '#!/usr/bin/env bash\nprintf "mnexis 0.1.0\\n"\n' >"$STAGE/mnexis"
chmod 0755 "$STAGE/mnexis"
printf '0.1.0\n' >"$STAGE/VERSION"
printf 'MIT\n' >"$STAGE/LICENSE"
printf '# mnexis\n' >"$STAGE/README.md"
for arch in x86_64 aarch64; do
  tar -czf "$WORK/artifacts/mnexis-$arch/mnexis-linux-$arch.tar.gz" -C "$STAGE" mnexis LICENSE README.md VERSION
  (cd "$WORK/artifacts/mnexis-$arch" && sha256sum -- "mnexis-linux-$arch.tar.gz" >SHA256SUMS)
done
cp "$REPO_ROOT/install.sh" "$WORK/install.sh"

output="$(cd -- "$WORK" && bash "$body" 2>&1)"
status=$?
RUN_OUT="$output"
RUN_STATUS="$status"
assert_status "the merge step exits 0" 0

if [[ -f "$WORK/release/SHA256SUMS" ]]; then
  ok "the merge step writes the checksum file"
else
  not_ok "the merge step did not write the checksum file: $RUN_OUT"
fi

# The published checksums must cover every artifact, and verify.
if (cd -- "$WORK/release" && sha256sum -c SHA256SUMS >/dev/null 2>&1); then
  ok "the merged checksums verify"
else
  not_ok "the merged checksums do not verify: $(cat -- "$WORK/release/SHA256SUMS" 2>/dev/null)"
fi

lines="$(grep -c . "$WORK/release/SHA256SUMS" 2>/dev/null || echo 0)"
if [[ "$lines" == "3" ]]; then
  ok "the checksum file covers both artifacts and the installer"
else
  not_ok "the checksum file has $lines lines, expected 3"
fi

finish
