# Mnexis

Local-first memory engine for AI agents. SQLite + FTS5, single Rust
binary, no daemon, no cloud. Linux-first MVP.

## Install

Linux, Rust 1.95+ (`rust-toolchain.toml`), and a C toolchain (bundled SQLite).

```sh
git clone <this-repo> mnexis
cd mnexis
./install.sh
```

The script builds the release binary with cargo and installs it to
`~/.local/bin/mnexis`, printing PATH guidance when that directory is not on
your PATH. Re-running it overwrites the installed binary. See
[docs/install.md](docs/install.md) for requirements, uninstall, and
`uninstall --purge` semantics.

To install the shipped agent skills (`skills/`, starting with
`mnexis-memory`) into an AI environment — `pi`
(`$PI_CODING_AGENT_DIR` or `~/.pi/agent/skills`) or `opencode`
(`~/.config/opencode/skills`):

```sh
./install.sh skills            # pick the environment interactively
./install.sh skills --env pi   # unattended
./install.sh uninstall --skills --env pi   # remove them again
```

On a fresh profile, `mnexis init` creates the default data directory
(`~/.local/share/mnexis`) with `mnexis.db` and `mnexis.toml`; copy those two
files to migrate the profile to another machine.

```sh
mnexis init
mnexis save --namespace default "first memory"
mnexis list
mnexis search "first memory"
```

## Projects

A "project" is the user's current focus of work. mnexis scopes memories
by **namespace**, and a binding ties a workspace to the namespace an
agent writes into, so a project's memories stay together across
sessions:

```sh
mnexis project bind mnexis --key git:github.com-Xeworg-mnexis
mnexis config allow-namespace mnexis
mnexis save --namespace mnexis "a fact worth keeping"
```

See [docs/projects.md](docs/projects.md) for the identity rules, the
agent protocol, and the registry format.

## CLI

The full surface (16 command groups; see `mnexis --help`):

```
version           print the version and exit
init              create a new profile (mnexis.db + mnexis.toml)
mcp serve         run the MCP stdio server
save              save a new memory
update            update a memory (optimistic revision)
supersede         mark an older memory superseded by a newer one
search            FTS5 ranked search
list              list memories
show              show a memory (current or specific revision)
history           show revision history
delete            soft-delete a memory
restore           restore a soft-deleted memory
purge             hard-delete soft-deleted memories (dry-run by default)
reindex           rebuild the FTS5 index from canonical data
doctor            integrity checks; --repair rebuilds FTS5
config show       print effective configuration and scope
config allow-namespace
                  authorize a namespace for read and write in the profile
project bind/list/unbind
                  manage workspace → namespace bindings (see docs/projects.md)
import / export   JSONL round-trip with conflict policies
context           budgeted memory_context pipeline (PRD §9)
tui               browse and edit memories in a terminal UI
```

Exit codes follow the stable error set (`INVALID_ARGUMENT=2`,
`NOT_FOUND=3`, `FORBIDDEN=4`, `REVISION_CONFLICT|TOPIC_CONFLICT=5`,
`INDEX_UNAVAILABLE=6`, `BUDGET_TOO_SMALL=7`, `CURSOR_EXPIRED=8`,
`UNSUPPORTED_VERSION=9`, `BUSY=10`); 1 is reserved for infrastructure
errors, 0 for success.

## TUI

`mnexis tui` opens a full-CRUD terminal UI over the same service as the
CLI: three panes (namespaces / memories / detail), FTS5 search with `/`,
and the write flows — save (`n`), update (`e`), supersede (`s`),
soft-delete (`d`), restore (`r`) — each with validation, revision-conflict
recovery (`Ctrl-R`), and y/N confirmations. It requires an initialised
profile and enforces the profile scope. Every key is documented in
[docs/tui.md](docs/tui.md); the footer inside the TUI always shows the
keys of the current mode.

## MCP

The binary ships the MCP stdio server (`mnexis mcp serve`). Point your
MCP-capable host at the binary, with the `--data-dir` flag in the
command line. Example `mcp.json` entry:

```json
{
  "mcpServers": {
    "mnexis": {
      "command": "mnexis",
      "args": ["--data-dir", "~/.mnexis", "mcp", "serve"]
    }
  }
}
```

The server exposes 9 tools (`memory_save`, `memory_get`,
`memory_list`, `memory_update`, `memory_history`, `memory_supersede`,
`memory_delete`, `memory_restore`, `memory_search`). All errors map to
JSON-RPC code `-32050` with `data.error_code` carrying the stable
label.

## Testing

`cargo test` runs 244 tests across 15 suites (lib + binary + integration).
CI is currently the dev host's `cargo fmt --check && git diff --check
&& cargo test --all-targets` chain — there is no hosted CI yet.

The bash installer harness is separate: `bash scripts/test-installer.sh`
(see [docs/install.md](docs/install.md)).

## License

MIT. See [LICENSE](LICENSE).

## Status

v0.1.0 (E0..E5 MVP). Linux only. The 500-memory/150-query evaluation
fixture is not yet committed; the spike corpora (`docs/t14-fts5-benchmark.md`)
are the only measured quality evidence at the moment. See `odd/tasks/`
for the implementation record.
