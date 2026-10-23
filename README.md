# Mnexis

Local-first memory engine for AI agents. SQLite + FTS5, single Rust
binary, no daemon, no cloud. Linux-first MVP.

## Install

Linux. The quick way — a published binary, verified against the checksum
the release publishes:

```sh
curl -fsSL https://github.com/Xeworg/mnexis/releases/latest/download/install.sh | bash
```

It downloads the release artifact for your platform, checks it against
`SHA256SUMS`, installs it to `~/.local/bin/mnexis` and opens the TUI. Pin
a version with `MNEXIS_VERSION`:

```sh
MNEXIS_VERSION=0.2.0 \
  curl -fsSL https://github.com/Xeworg/mnexis/releases/latest/download/install.sh | bash
```

The releases are **not signed yet**: the checksum protects against a
corrupted or truncated download, not against a compromised release. See
[docs/release.md](docs/release.md) for what that means and for how a
release is cut.

From the sources — Rust 1.95+ (`rust-toolchain.toml`) and a C toolchain
(bundled SQLite):

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

An installed mnexis can update itself:

```sh
mnexis upgrade --check   # is there a newer release?
mnexis upgrade           # install it
```

To install the shipped agent skills (`skills/`, starting with
`mnexis-memory`) into an AI environment — `pi`
(`$PI_CODING_AGENT_DIR` or `~/.pi/agent/skills`) or `opencode`
(`~/.config/opencode/skills`):

```sh
./install.sh skills            # pick the environment interactively
./install.sh skills --env pi   # unattended
./install.sh uninstall --skills --env pi   # remove them again
```

The `mnexis-memory` skill tells the agent to load the project's memory at the
start of a session (`mnexis context "<task>"`) and before it changes code
that already exists, so the decisions behind the code are never lost. One
call tells it where it may write:

```sh
mnexis context "extend the snake game with a wrap mode"   # budgeted context
mnexis search "wrap mode borde"                            # ranked search
mnexis status                                             # namespace, allowlists, recent
```

The engine also keeps what a repository cannot: the session the agent runs
in, and what the user said.

```sh
mnexis session --id 01JX                     # register the session
mnexis prompt "el puerto es 8081"             # what the user said
mnexis prompts --search "puerto 8081"         # find it again
mnexis end --id 01JX --summary "qué hicimos"  # end it with a summary
```

A search query is plain language: every word counts, a hyphen is part of a
word, and `--match all` narrows it to the memories that hold every word.

The engine also knows which topic it already holds: **a memory has a title**
— what the fact *is*, next to the content, which is what it says *right now* —
and the topic key derives from the title. The same title always lands on the
same record, so a fact that changed updates its record instead of adding a
second one next to it. `mnexis suggest-topic-key "<title>"` answers which key
a title yields and whether the project already records it.

Two **different** titles for the same fact (`puerto del servicio` and
`puerto de pagos`) are two records, and the engine cannot tell they are the
same: it ranks the records whose title looks like yours and proposes them,
it does not decide. `mnexis duplicates --title "<title>"` asks on demand,
and a save that creates a record answers with the same candidates — the
agent links the one that is the same fact, and leaves the others apart.

The repository also ships a pi extension that exposes the memory engine as
native tools, so an agent can store and recall project memory without being
asked to, and without choosing a namespace itself:

```sh
./install.sh extensions --env pi            # → <agent-dir>/extensions/
./install.sh uninstall --extensions --env pi
```

An installed `mnexis` can do it too — the assets travel inside the binary —
and the TUI offers the same operation (`mnexis tui` → *Install for agents*):

```sh
mnexis agent status
mnexis agent install --env pi
mnexis agent install --env opencode
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

## Permissions

Two permissions sit next to the namespace allowlists, and both belong to
the human:

| Permission | Default in a new profile | What it opens |
| --- | --- | --- |
| `allow_read_all` | `true` | Reading memories from any project. |
| `allow_global` | `false` | The reserved `global` namespace, for reads and writes. |

```sh
mnexis config allow-read-all --revoke   # scope the reads to the allowlist
mnexis config allow-read-all             # open them again
mnexis config allow-global               # open the reserved 'global' namespace
mnexis config allow-global --revoke
```

A profile that does not mention these keys keeps both permissions closed:
nothing opens itself silently, and `mnexis init` is what writes them
open (reads) or closed (the reserved namespace).

Reads are open across projects because projects share components, and
writes are not: the CLI refuses to write outside the namespace of the
current workspace, so an agent working in one project cannot modify
another project's memories. See
[docs/projects.md](docs/projects.md#the-agent-protocol).

## CLI

The full surface (25 command groups; see `mnexis --help`):

```
version           print the version and exit
init              create a new profile (mnexis.db + mnexis.toml)
mcp serve         run the MCP stdio server
save              save a new memory (--title names the fact)
update            update a memory (optimistic revision)
supersede         mark an older memory superseded by a newer one
search            ranked search (every word of the query counts)
suggest-topic-key the topic key a title yields, and who holds it
duplicates         the records whose title looks like a given one
list              list memories
session           register the session this process runs in
end               end a session, with an optional summary
sessions          the sessions this profile can read
prompt            store what the user said
prompts           list or search what the user said
status            where this workspace may write, and what is already there
show              show a memory (current or specific revision)
history           show revision history
delete            soft-delete a memory
restore           restore a soft-deleted memory
purge             hard-delete soft-deleted memories (dry-run by default)
reindex           rebuild the FTS5 index from canonical data
doctor            integrity checks; --repair rebuilds FTS5
upgrade           check for a newer release and install it
config show       print effective configuration and scope
config allow-namespace
                  authorize a namespace for read and write in the profile
config allow-global
                  open (or --revoke) the reserved 'global' namespace
config allow-read-all
                  open (or --revoke) reads across namespaces
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

The server exposes 16 tools (`memory_save`, `memory_get`,
`memory_list`, `memory_update`, `memory_history`, `memory_supersede`,
`memory_delete`, `memory_restore`, `memory_search`, `memory_workspace`,
`memory_session_start`, `memory_session_end`, `memory_save_prompt`,
`memory_search_prompts`, `memory_suggest_topic_key`,
`memory_scan_duplicates`).
`memory_workspace` answers "where may I write, and what is already there?"
with the same payload as `mnexis status`: the bound namespace, the read and
write allowlists, the memory count and the most recent memories. All errors
map to JSON-RPC code `-32050` with `data.error_code` carrying the stable
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

v0.2.0 (E0..E5 MVP, plus the agent-memory-ergonomics feature: the recall
happens on its own, `mnexis status` answers "where may I write?" and the
installer wires opencode's MCP server). Linux only. The 500-memory/150-query
evaluation
fixture is not yet committed; the spike corpora (`docs/t14-fts5-benchmark.md`)
are the only measured quality evidence at the moment. See `odd/tasks/`
for the implementation record.
