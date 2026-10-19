#!/usr/bin/env bash
#
# mnexis installer / uninstaller (Linux).
#
#   ./install.sh                 Build with cargo (release) and install
#                                $HOME/.local/bin/mnexis.
#   ./install.sh uninstall       Remove the installed binary; profile data
#                                is kept.
#   ./install.sh uninstall --purge
#                                Also remove the resolved profile data
#                                directory after an interactive y/N prompt.
#   ./install.sh --help          Show usage.
#
# The purge path mirrors src/cli/profile.rs::resolve_data_dir:
# $MNEXIS_DATA_DIR when set, otherwise $HOME/.local/share/mnexis. Both the
# profile database (mnexis.db) and the profile settings (mnexis.toml) live
# inside that directory (src/cli/profile.rs:59-65), so removing it removes
# the data and the config together. No XDG_* variable is consulted because
# the CLI does not consult one either.

set -euo pipefail

usage() {
  printf '%s\n' \
    'Usage: ./install.sh [install]' \
    '       ./install.sh uninstall [--purge]' \
    '       ./install.sh --help' \
    '' \
    'Builds the mnexis binary with cargo (release profile) and installs it' \
    'to $HOME/.local/bin/mnexis. Re-running install overwrites the binary.' \
    '' \
    'Commands:' \
    '  install            Build and install (the default when omitted).' \
    '  uninstall          Remove $HOME/.local/bin/mnexis; profile data stays.' \
    '  uninstall --purge  Also remove the profile data directory' \
    '                     ($MNEXIS_DATA_DIR, or $HOME/.local/share/mnexis)' \
    '                     after an interactive y/N confirmation.' \
    '' \
    'Options:' \
    '  -h, --help         Show this help and exit.' \
    '' \
    'Environment:' \
    '  CARGO_TARGET_DIR   Cargo target directory (default: <repo>/target).' \
    '  MNEXIS_DATA_DIR    Profile data directory used to resolve --purge.' \
    '  HOME               The binary is installed under $HOME/.local/bin.'
}

error() { printf 'install.sh: %s\n' "$*" >&2; }
die() {
  error "$*"
  exit 1
}

if [[ -z "${HOME:-}" ]]; then
  die "HOME is not set; cannot resolve the install or profile directories"
fi
INSTALL_DIR="$HOME/.local/bin"

# Resolve the repository root without external tools so the missing
# toolchain check works even with an empty PATH.
SCRIPT_DIR="${BASH_SOURCE[0]%/*}"
if [[ "$SCRIPT_DIR" == "${BASH_SOURCE[0]}" ]]; then
  SCRIPT_DIR="."
fi
SCRIPT_DIR="$(cd -- "$SCRIPT_DIR" && pwd -P)"

require_rust_toolchain() {
  local missing=0
  if ! command -v cargo >/dev/null 2>&1; then
    error "cargo was not found on PATH."
    missing=1
  fi
  if ! command -v rustc >/dev/null 2>&1; then
    error "rustc was not found on PATH."
    missing=1
  fi
  if ((missing != 0)); then
    error "Install Rust with rustup (https://rustup.rs) and re-run this script."
    exit 1
  fi
}

# Cargo resolves a relative CARGO_TARGET_DIR against its working directory,
# which is the repository root here.
cargo_target_dir() {
  local dir="${CARGO_TARGET_DIR:-$SCRIPT_DIR/target}"
  case "$dir" in
    /*) printf '%s\n' "$dir" ;;
    *) printf '%s\n' "$SCRIPT_DIR/$dir" ;;
  esac
}

cmd_install() {
  require_rust_toolchain
  if [[ ! -f "$SCRIPT_DIR/Cargo.toml" ]]; then
    die "Cargo.toml not found in $SCRIPT_DIR; install.sh must stay at the repository root"
  fi

  printf 'Building mnexis with cargo (release profile)...\n'
  (cd -- "$SCRIPT_DIR" && cargo build --release)

  local target_dir built
  target_dir="$(cargo_target_dir)"
  built="$target_dir/release/mnexis"
  if [[ ! -x "$built" ]]; then
    die "the build succeeded but no executable was found at $built"
  fi

  mkdir -p -- "$INSTALL_DIR"
  cp -f -- "$built" "$INSTALL_DIR/mnexis"
  chmod 0755 -- "$INSTALL_DIR/mnexis"
  printf 'Installed mnexis to %s\n' "$INSTALL_DIR/mnexis"

  case ":$PATH:" in
    *":$INSTALL_DIR:"*)
      printf 'Run `mnexis --version` to verify the installation.\n'
      ;;
    *)
      printf '%s is not on your PATH.\n' "$INSTALL_DIR"
      printf 'Add it to your shell profile and restart the shell, for example:\n'
      printf '  export PATH="%s:$PATH"\n' "$INSTALL_DIR"
      ;;
  esac
}

# Exact resolution order of src/cli/profile.rs:74-90 (the installer has no
# --data-dir flag, so the first step does not apply).
resolve_data_dir() {
  if [[ -n "${MNEXIS_DATA_DIR:-}" ]]; then
    printf '%s\n' "$MNEXIS_DATA_DIR"
  else
    printf '%s\n' "$HOME/.local/share/mnexis"
  fi
}

# Deletion is destructive: refuse obviously unsafe targets.
guard_purge_path() {
  local raw="$1"
  if [[ "$raw" == "/" ]]; then
    die "refusing to purge '/'"
  fi
  local dir="${raw%/}"
  if [[ -z "$dir" || "$dir" != /* ]]; then
    die "refusing to purge a non-absolute data directory: '$raw' (set MNEXIS_DATA_DIR to an absolute path)"
  fi
  if [[ "$dir" == "/" ]]; then
    die "refusing to purge '/'"
  fi
  if [[ "$dir" == "$HOME" ]]; then
    die "refusing to purge the home directory: '$dir'"
  fi
}

confirm_purge() {
  local dir="$1"
  if [[ ! -t 0 ]]; then
    error "refusing to purge without confirmation: stdin is not a terminal."
    error "Re-run './install.sh uninstall --purge' from an interactive terminal,"
    error "or remove '$dir' yourself once you are sure:"
    error "  rm -rf -- '$dir'"
    return 1
  fi

  printf 'This will permanently delete the mnexis profile data at:\n  %s\n' "$dir"
  printf 'Memories, the database (mnexis.db), and the settings (mnexis.toml) will be lost.\n'
  printf 'Continue? [y/N] '
  local answer=""
  if ! read -r answer; then
    answer=""
  fi
  case "$answer" in
    y | Y | yes | YES | Yes) return 0 ;;
    *) return 1 ;;
  esac
}

cmd_uninstall() {
  local binary="$INSTALL_DIR/mnexis"
  if [[ -e "$binary" ]]; then
    rm -f -- "$binary"
    printf 'Removed %s\n' "$binary"
  else
    printf 'mnexis is not installed at %s; nothing to do.\n' "$binary"
  fi
}

cmd_uninstall_purge() {
  local binary="$INSTALL_DIR/mnexis"
  local data_dir
  data_dir="$(resolve_data_dir)"
  guard_purge_path "$data_dir"
  data_dir="${data_dir%/}"

  # Ask before touching anything so an abort (non-tty or "no") leaves the
  # installation and the profile exactly as they were.
  if ! confirm_purge "$data_dir"; then
    printf 'Purge aborted; the installed binary and profile data were left untouched.\n'
    exit 1
  fi

  if [[ -e "$binary" ]]; then
    rm -f -- "$binary"
    printf 'Removed %s\n' "$binary"
  else
    printf 'mnexis is not installed at %s; nothing to remove there.\n' "$binary"
  fi

  if [[ -d "$data_dir" ]]; then
    rm -rf -- "$data_dir"
    printf 'Removed profile data at %s\n' "$data_dir"
  else
    printf 'No profile data found at %s.\n' "$data_dir"
  fi
}

main() {
  if (($# == 0)); then
    cmd_install
    return 0
  fi

  case "$1" in
    -h | --help)
      usage
      return 0
      ;;
    install)
      shift
      case "${1:-}" in
        "") cmd_install ;;
        -h | --help) usage ;;
        *)
          error "unknown option for 'install': $1"
          usage >&2
          return 2
          ;;
      esac
      ;;
    uninstall)
      shift
      local purge=0
      for arg in "$@"; do
        case "$arg" in
          --purge) purge=1 ;;
          -h | --help)
            usage
            return 0
            ;;
          *)
            error "unknown option for 'uninstall': $arg"
            usage >&2
            return 2
            ;;
        esac
      done
      if ((purge == 1)); then
        cmd_uninstall_purge
      else
        cmd_uninstall
      fi
      ;;
    *)
      error "unknown command: $1"
      usage >&2
      return 2
      ;;
  esac
}

main "$@"
