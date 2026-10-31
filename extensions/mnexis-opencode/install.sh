#!/usr/bin/env bash
#
# Install the mnexis plugin into opencode's plugin directory.
#
#   ./install.sh                     # install for the current user
#   ./install.sh --config-dir DIR    # install into DIR (an opencode config dir)
#   ./install.sh --uninstall         # remove the file this script installed
#   ./install.sh --uninstall --force # remove it even if it was edited since
#
# The plugin is one file: opencode 1.x discovers `plugins/*.js`, and opencode
# 2.x discovers both `plugins/*.js` and `plugins/*/` packages, so the same file
# serves both plugin models (see RESEARCH.md). The script never touches
# opencode's configuration: the MCP server and the skill are the engine's to
# install (`mnexis agent install --env opencode`), and the plugin only adds the
# host-side behaviour MCP cannot have.

set -euo pipefail

readonly PLUGIN_NAME="mnexis-memory.js"

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly here
readonly source_file="$here/index.js"

mode="install"
force="no"
config_dir="${MNEXIS_OPENCODE_CONFIG_DIR:-${XDG_CONFIG_HOME:-$HOME/.config}/opencode}"

usage() {
	cat <<'EOF'
Usage: install.sh [--config-dir DIR] [--uninstall [--force]]

Installs the mnexis plugin for opencode (one file) into DIR/plugins,
where DIR is opencode's configuration directory.

  --config-dir DIR  The opencode configuration directory
                    (default: $MNEXIS_OPENCODE_CONFIG_DIR, else
                    ${XDG_CONFIG_HOME:-$HOME/.config}/opencode)
  --uninstall       Remove the plugin file this script installed
  --force           With --uninstall: remove the file even when it was edited
  -h, --help        This message
EOF
}

while [ "$#" -gt 0 ]; do
	case "$1" in
	--config-dir)
		[ "$#" -ge 2 ] || {
			echo "install.sh: --config-dir needs a directory" >&2
			exit 2
		}
		config_dir="$2"
		shift 2
		;;
	--config-dir=*)
		config_dir="${1#--config-dir=}"
		shift
		;;
	--uninstall)
		mode="uninstall"
		shift
		;;
	--force)
		force="yes"
		shift
		;;
	-h | --help)
		usage
		exit 0
		;;
	*)
		echo "install.sh: unknown argument: $1" >&2
		usage >&2
		exit 2
		;;
	esac
done

plugin_dir="$config_dir/plugins"
target="$plugin_dir/$PLUGIN_NAME"

if [ "$mode" = "uninstall" ]; then
	if [ ! -e "$target" ]; then
		echo "nothing to remove: $target does not exist"
		exit 0
	fi
	if [ "$force" != "yes" ] && ! cmp -s "$source_file" "$target" 2>/dev/null; then
		echo "refusing to remove $target: it differs from the plugin this script installs" >&2
		echo "re-run with --force if it is yours to remove" >&2
		exit 1
	fi
	rm -f "$target"
	echo "removed $target"
	exit 0
fi

if [ ! -f "$source_file" ]; then
	echo "install.sh: $source_file is missing; run this from the extension directory" >&2
	exit 1
fi

mkdir -p "$plugin_dir"
if [ -f "$target" ] && cmp -s "$source_file" "$target"; then
	echo "already installed: $target"
else
	cp "$source_file" "$target"
	echo "installed $target"
fi

cat <<EOF

The plugin is loaded the next time opencode starts. Its other half is the
engine's, and it is one command:

  mnexis agent install --env opencode

That writes the skill (opencode's skills directory) and the mnexis MCP server
entry (opencode.json). Nothing works without the MCP entry: the plugin captures,
points and guards, but the memory tools are the MCP server's.
EOF

if command -v opencode >/dev/null 2>&1; then
	version="$(opencode --version 2>/dev/null | head -n 1 || true)"
	case "$version" in
	2.*)
		cat <<'EOF'

This host is opencode 2, which hides MCP tools behind Code Mode by default.
To keep the mnexis tools as direct tools, add "codemode": false to the mnexis
server entry in opencode.json (see README.md).
EOF
		;;
	esac
fi
