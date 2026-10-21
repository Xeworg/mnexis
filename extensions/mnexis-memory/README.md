# mnexis-memory (pi extension)

Exposes the mnexis memory engine as native pi tools, so an agent can store
and recall project memory without being told to, and without deciding which
namespace to write into.

## Install

```sh
./install.sh extensions --env pi   # → <agent-dir>/extensions/mnexis-memory/
```

`<agent-dir>` is `$PI_CODING_AGENT_DIR` when set, otherwise `~/.pi/agent`.
Remove it again with `./install.sh uninstall --extensions --env pi`.

## Tools

| Tool | Purpose |
| --- | --- |
| `mnexis_save` | Store a durable, self-contained memory about the project. |
| `mnexis_search` | Full-text search over the stored memories (ranked). |
| `mnexis_list` | List what is already stored. |
| `mnexis_show` | Read one memory by UUID. |
| `mnexis_supersede` | Replace an outdated memory with a newer one, keeping history. |

Every tool is declared to the model in every session: availability does not
depend on the model discovering a skill.

## Read across projects, write inside one

Reads are open: `mnexis_search` and `mnexis_list` reach every project
(projects share components) unless the model narrows them with a
`namespace` argument, and `mnexis_show` reads any memory the profile
allows.

Writes are not: `mnexis_save` always targets the workspace namespace, and
`mnexis_supersede` refuses a memory that lives in another project. The
mnexis CLI enforces the same boundary for writes run from a bound
workspace, so the rule holds even when the agent uses the shell.

## How the namespace is resolved

The extension resolves the workspace → namespace binding itself, so the model
never picks a namespace:

1. From the session's working directory it takes the git repository root and
   the `origin` remote.
2. The workspace key is `git:<normalized remote>`: protocol and `.git` suffix
   removed, the whole URL lowercased, `/` and `:` replaced by `-`, the host's
   dots kept — `https://github.com/Xeworg/AegisPI.git` becomes
   `git:github.com-xeworg-aegispi`. Without a remote it is
   `path:<repo root>`, and outside a repository `path:<cwd>`.
3. `mnexis project list` is consulted: an existing binding is never renamed.
   For a new workspace the name is derived from the directory and bound with
   `mnexis project bind`.

## The permission gate

Permissions belong to the human, so the extension never grants itself one:

- **Already authorized** → the write happens silently. The agent does not
  ask anything.
- **Not authorized, interactive session** → pi shows one confirmation naming
  the namespace and exactly what it grants. Confirming runs
  `mnexis config allow-namespace <ns>` once; the grant persists in the
  profile, so later writes never ask again.
- **Not authorized, headless session** (`pi -p`, JSON, RPC without UI) →
  nothing is authorized. The tool fails with the exact command a human must
  run: `mnexis config allow-namespace <ns>`.

The extension only drives the mnexis CLI; it never edits `mnexis.toml` or
`projects.toml` by hand.

## Configuration

| Variable | Meaning |
| --- | --- |
| `MNEXIS_BIN` | Path to the `mnexis` binary (default: `mnexis` on `PATH`). |
| `MNEXIS_DATA_DIR` | Profile directory, forwarded to the CLI unchanged. |

## Tests

```sh
node --test "extensions/mnexis-memory/test/*.test.js"
```

The unit tests inject a fake exec, so they need neither the binary nor a
profile.
