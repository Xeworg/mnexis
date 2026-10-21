#!/usr/bin/env bash
#
# mnexis installer / uninstaller (Linux).
#
#   ./install.sh                 Build with cargo (release) and install
#                                $HOME/.local/bin/mnexis.
#   ./install.sh skills [--env <name>]
#                                Install the shipped agent skills (skills/) to
#                                the selected AI environment (pi first).
#   ./install.sh extensions [--env <name>]
#                                Install the shipped agent extensions
#                                (extensions/) to the selected environment.
#   ./install.sh uninstall       Remove the installed binary; profile data
#                                is kept.
#   ./install.sh uninstall --skills [--env <name>]
#                                Remove the mnexis-installed agent skills
#                                from the selected environment.
#   ./install.sh uninstall --extensions [--env <name>]
#                                Remove the mnexis-installed agent
#                                extensions from the selected environment.
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
    '       ./install.sh skills [--env <name>]' \
    '       ./install.sh extensions [--env <name>]' \
    '       ./install.sh uninstall [--skills [--env <name>]|--extensions [--env <name>]|--purge]' \
    '       ./install.sh --help' \
    '' \
    'Builds the mnexis binary with cargo (release profile) and installs it' \
    'to $HOME/.local/bin/mnexis. Re-running install overwrites the binary.' \
    '' \
    'Commands:' \
    '  install            Build and install (the default when omitted).' \
    '  skills             Install the agent skills shipped in ./skills to an' \
    '                     AI environment. Interactive selection when run from' \
    '                     a terminal; pass --env <name> to run unattended.' \
    '                     Re-running overwrites the installed skills.' \
    '  uninstall          Remove $HOME/.local/bin/mnexis; profile data stays.' \
    '  uninstall --skills Remove the mnexis-installed agent skills from an' \
    '                     environment (interactive selection or --env <name>);' \
    '                     skill directories mnexis does not ship are kept.' \
    '                     Cannot be combined with --extensions or --purge.' \
    '  extensions         Install the agent extensions shipped in ./extensions' \
    '                     into an AI environment (default: pi).' \
    '  uninstall --extensions' \
    '                     Remove the mnexis-installed agent extensions;' \
    '                     extension directories mnexis does not ship are kept.' \
    '                     Cannot be combined with --skills or --purge.' \
    '  uninstall --purge  Also remove the profile data directory' \
    '                     ($MNEXIS_DATA_DIR, or $HOME/.local/share/mnexis)' \
    '                     after an interactive y/N confirmation.' \
    '' \
    'Options:' \
    '  --env <name>       Target AI environment for skills (default: pi).' \
    '                     Supported: pi ($PI_CODING_AGENT_DIR or' \
    '                     $HOME/.pi/agent/skills),' \
    '                     opencode ($HOME/.config/opencode/skills).' \
    '  -h, --help         Show this help and exit.' \
    '' \
    'Environment:' \
    '  CARGO_TARGET_DIR   Cargo target directory (default: <repo>/target).' \
    '  MNEXIS_DATA_DIR    Profile data directory used to resolve --purge.' \
    '  HOME               The binary is installed under $HOME/.local/bin,' \
    '                     pi skills under $HOME/.pi/agent/skills.' \
    '  PI_CODING_AGENT_DIR  pi agent directory (default: $HOME/.pi/agent).'
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

# --- agent skills (skills/) --------------------------------------------
# Supported AI environments and where each one discovers user skills.
# Adding an environment is one entry in each function below.
skills_envs() {
  printf '%s\n' pi opencode
}

skills_env_dir() {
  case "$1" in
    # pi advertises user skills from its agent directory (<agent-dir>/skills/),
    # which defaults to ~/.pi/agent and can be relocated with
    # PI_CODING_AGENT_DIR. ~/.pi/skills is a *project* location for pi, so
    # installing there would leave the skill invisible to the model.
    pi) printf '%s\n' "${PI_CODING_AGENT_DIR:-$HOME/.pi/agent}/skills" ;;
    opencode) printf '%s\n' "$HOME/.config/opencode/skills" ;;
    *) return 1 ;;
  esac
}

# --- agent extensions (extensions/) -------------------------------------
# Environments that can load an extension, and where each one looks for
# them. pi loads extensions from <agent-dir>/extensions; the directory is
# created by the installer when it does not exist yet.
extensions_envs() {
  printf '%s\n' pi
}

extensions_env_dir() {
  case "$1" in
    pi) printf '%s\n' "${PI_CODING_AGENT_DIR:-$HOME/.pi/agent}/extensions" ;;
    *) return 1 ;;
  esac
}

require_known_extension_env() {
  local env="$1"
  if ! extensions_env_dir "$env" >/dev/null; then
    error "unknown environment: '$env' (supported: $(extensions_envs | tr '\n' ' '))"
    exit 2
  fi
}

# Every shipped extension name (directories with an extension entry point
# inside extensions/).
shipped_extension_names() {
  local dir name
  for dir in "$SCRIPT_DIR"/extensions/*/; do
    [[ -f "$dir/index.js" || -f "$dir/index.ts" ]] || continue
    name="${dir%/}"
    printf '%s\n' "${name##*/}"
  done
}

require_shipped_extensions() {
  if [[ -z "$(shipped_extension_names)" ]]; then
    die "no extensions found in $SCRIPT_DIR/extensions (each extension is a directory with an index.js)"
  fi
}

# Validate a --env value, dying with the supported list when unknown.
require_known_env() {
  local env="$1"
  if ! skills_env_dir "$env" >/dev/null; then
    error "unknown environment: '$env' (supported: $(skills_envs | tr '\n' ' '))"
    exit 2
  fi
}

# Every shipped skill name (directories with a SKILL.md inside skills/).
shipped_skill_names() {
  local dir name
  for dir in "$SCRIPT_DIR"/skills/*/; do
    [[ -f "$dir/SKILL.md" ]] || continue
    name="${dir%/}"
    printf '%s\n' "${name##*/}"
  done
}

require_shipped_skills() {
  if [[ -z "$(shipped_skill_names)" ]]; then
    die "no skills found in $SCRIPT_DIR/skills (each skill is a directory with a SKILL.md)"
  fi
}

# Interactive environment selection on a tty; refuses without one so
# unattended runs must pass --env (mirrors the purge confirmation policy).
select_env() {
  if [[ ! -t 0 ]]; then
    error "refusing to select an environment interactively: stdin is not a terminal."
    error "Re-run './install.sh skills --env <name>' (supported: $(skills_envs | tr '\n' ' '))"
    return 1
  fi

  local envs choice i
  envs="$(skills_envs)"
  printf 'AI environments:\n' >&2
  i=1
  while IFS= read -r env; do
    printf '  %d) %s -> %s\n' "$i" "$env" "$(skills_env_dir "$env")" >&2
    i=$((i + 1))
  done <<<"$envs"
  printf 'Select environment [1]: ' >&2
  local answer=""
  if ! read -r answer; then
    answer=""
  fi
  [[ -n "$answer" ]] || answer=1
  if [[ ! "$answer" =~ ^[0-9]+$ ]]; then
    error "invalid selection: '$answer'"
    return 1
  fi
  local total
  total="$(skills_envs | wc -l)"
  if ((answer < 1 || answer > total)); then
    error "selection out of range: $answer (1..$total)"
    return 1
  fi
  printf '%s\n' "$(skills_envs | sed -n "${answer}p")"
}

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

# The binary owns the installation: the assets are embedded in it, so the
# shell script only forwards the request. One implementation, one source of
# truth.
#
#   ./install.sh skills --env pi  ->  mnexis agent install --env pi --only skill
#   ./install.sh extensions --env pi -> mnexis agent install --env pi --only extension
ensure_mnexis_binary() {
  if [[ -x "$INSTALL_DIR/mnexis" ]]; then
    return 0
  fi
  printf 'The mnexis binary is required to install the agent assets; building it now.\n'
  cmd_install
}

mnexis_binary() {
  if [[ -x "$INSTALL_DIR/mnexis" ]]; then
    printf '%s\n' "$INSTALL_DIR/mnexis"
    return 0
  fi
  return 1
}

cmd_skills() {
  local env="$1" only="$2"
  require_known_env "$env"
  ensure_mnexis_binary
  local binary
  binary="$(mnexis_binary)" || die "the mnexis binary is not available at $INSTALL_DIR"
  "$binary" agent install --env "$env" --only "$only"
}

cmd_uninstall_skills() {
  local env="$1" only="$2"
  require_known_env "$env"
  local binary
  binary="$(mnexis_binary)" || {
    error "the mnexis binary is not installed, so the agent assets cannot be removed."
    error "Install mnexis again (./install.sh) and re-run this command."
    return 1
  }
  "$binary" agent uninstall --env "$env" --only "$only"
}

cmd_extensions() {
  local env="$1" only="$2"
  require_known_extension_env "$env"
  ensure_mnexis_binary
  local binary
  binary="$(mnexis_binary)" || die "the mnexis binary is not available at $INSTALL_DIR"
  "$binary" agent install --env "$env" --only "$only"
}

cmd_uninstall_extensions() {
  local env="$1" only="$2"
  require_known_extension_env "$env"
  local binary
  binary="$(mnexis_binary)" || {
    error "the mnexis binary is not installed, so the agent assets cannot be removed."
    error "Install mnexis again (./install.sh) and re-run this command."
    return 1
  }
  "$binary" agent uninstall --env "$env" --only "$only"
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
    skills)
      shift
      local env=""
      while (($# > 0)); do
        case "$1" in
          --env)
            (($# >= 2)) || die "--env requires a value"
            env="$2"
            shift 2
            ;;
          -h | --help)
            usage
            return 0
            ;;
          *)
            error "unknown option for 'skills': $1"
            usage >&2
            return 2
            ;;
        esac
      done
      if [[ -z "$env" ]]; then
        env="$(select_env)" || exit 1
      fi
      require_known_env "$env"
      cmd_skills "$env" skill
      ;;
    extensions)
      shift
      local env=""
      while (($# > 0)); do
        case "$1" in
          --env)
            (($# >= 2)) || die "--env requires a value"
            env="$2"
            shift 2
            ;;
          -h | --help)
            usage
            return 0
            ;;
          *)
            error "unknown option for 'extensions': $1"
            usage >&2
            return 2
            ;;
        esac
      done
      env="${env:-pi}"
      cmd_extensions "$env" extension
      ;;
    uninstall)
      shift
      local purge=0 skills=0 extensions=0 env=""
      while (($# > 0)); do
        case "$1" in
          --purge) purge=1 ;;
          --skills) skills=1 ;;
          --extensions) extensions=1 ;;
          --env)
            (($# >= 2)) || die "--env requires a value"
            env="$2"
            shift
            ;;
          -h | --help)
            usage
            return 0
            ;;
          *)
            error "unknown option for 'uninstall': $1"
            usage >&2
            return 2
            ;;
        esac
        shift
      done
      if ((skills == 1)); then
        if ((purge == 1 || extensions == 1)); then
          error "--skills, --extensions and --purge are separate scopes; run them separately."
          return 2
        fi
        if [[ -z "$env" ]]; then
          env="$(select_env)" || exit 1
        fi
        require_known_env "$env"
        cmd_uninstall_skills "$env" skill
      elif ((extensions == 1)); then
        if ((purge == 1)); then
          error "--extensions and --purge are separate scopes; run them separately."
          return 2
        fi
        cmd_uninstall_extensions "${env:-pi}" extension
      elif ((purge == 1)); then
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
