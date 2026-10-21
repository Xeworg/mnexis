#!/usr/bin/env bash
# The release packaging: build one platform artifact and its checksum.
#
# The GitHub Actions workflow calls this script once per platform and then
# merges the SHA256SUMS files into the one the release publishes. The
# asset names are stable across versions (`mnexis-linux-<arch>.tar.gz`)
# because the release tag already namespaces them:
#
#   https://github.com/Xeworg/mnexis/releases/latest/download/mnexis-linux-x86_64.tar.gz
#   https://github.com/Xeworg/mnexis/releases/download/v0.1.0/mnexis-linux-x86_64.tar.gz
#
# The archive carries the binary, the licence, the readme and a VERSION
# file: the installer reads VERSION to refuse an artifact that does not
# match the version it was asked for.
set -uo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"

VERSION=""
ARCH=""
OUTDIR="$REPO_ROOT/dist"
BIN=""

usage() {
  printf '%s\n' \
    'Usage: scripts/package-release.sh --version <semver> [options]' \
    '' \
    'Options:' \
    '  --version <semver>   version to package (e.g. 0.1.0 or v0.1.0) [required]' \
    '  --arch <arch>        x86_64 or aarch64 [default: the host architecture]' \
    '  --outdir <dir>       where the artifact is written [default: ./dist]' \
    '  --bin <path>         the binary to package [default: target/release/mnexis]' \
    '  -h, --help           show this help'
}

die() {
  printf 'package-release.sh: %s\n' "$*" >&2
  exit 2
}

detect_arch() {
  case "$(uname -m)" in
    x86_64 | amd64) printf 'x86_64' ;;
    aarch64 | arm64) printf 'aarch64' ;;
    *) printf 'unknown' ;;
  esac
}

while (($# > 0)); do
  case "$1" in
    --version)
      (($# >= 2)) || die "--version needs a value"
      VERSION="$2"
      shift 2
      ;;
    --arch)
      (($# >= 2)) || die "--arch needs a value"
      ARCH="$2"
      shift 2
      ;;
    --outdir)
      (($# >= 2)) || die "--outdir needs a value"
      OUTDIR="$2"
      shift 2
      ;;
    --bin)
      (($# >= 2)) || die "--bin needs a value"
      BIN="$2"
      shift 2
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      printf 'package-release.sh: unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

[[ -n "$VERSION" ]] || {
  usage >&2
  die "--version is required"
}
# The tag may carry a leading "v"; the version itself never does.
VERSION="${VERSION#v}"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] ||
  die "'$VERSION' is not a semantic version"

[[ -n "$ARCH" ]] || ARCH="$(detect_arch)"
case "$ARCH" in
  x86_64 | aarch64) ;;
  *) die "unsupported architecture '$ARCH' (expected x86_64 or aarch64)" ;;
esac

[[ -n "$BIN" ]] || BIN="$REPO_ROOT/target/release/mnexis"
[[ -f "$BIN" ]] || die "the binary was not found at $BIN (build the release profile first)"
[[ -x "$BIN" ]] || die "$BIN is not executable"

# The packaged binary must be the version being released: packaging 0.2.0
# with a 0.1.0 binary is the kind of mistake nobody notices until a user
# reports the wrong version.
binary_version="$("$BIN" --version 2>/dev/null | awk '{print $NF}' || true)"
if [[ -n "$binary_version" && "$binary_version" != "$VERSION" ]]; then
  die "the binary reports version '$binary_version' but '$VERSION' was requested"
fi

mkdir -p -- "$OUTDIR"
OUTDIR="$(cd -- "$OUTDIR" && pwd)"

stage="$(mktemp -d)"
trap 'rm -rf -- "$stage"' EXIT

install -m 0755 -- "$BIN" "$stage/mnexis"
install -m 0644 -- "$REPO_ROOT/LICENSE" "$stage/LICENSE"
install -m 0644 -- "$REPO_ROOT/README.md" "$stage/README.md"
printf '%s\n' "$VERSION" >"$stage/VERSION"

asset="mnexis-linux-$ARCH.tar.gz"
tar -czf "$OUTDIR/$asset" -C "$stage" mnexis LICENSE README.md VERSION

# The checksum file lists every artifact produced for this release. Running
# the script twice for the same architecture refreshes its line instead of
# duplicating it.
checksum="$(sha256sum -- "$OUTDIR/$asset" | awk '{print $1}')"
checksums="$OUTDIR/SHA256SUMS"
if [[ -f "$checksums" ]]; then
  grep -v -- " $asset\$" "$checksums" >"$checksums.tmp" || true
  mv -f -- "$checksums.tmp" "$checksums"
fi
printf '%s  %s\n' "$checksum" "$asset" >>"$checksums"

printf 'Packaged %s (%s) in %s\n' "$asset" "$VERSION" "$OUTDIR"
printf '  %s\n' "$checksum  $asset"
