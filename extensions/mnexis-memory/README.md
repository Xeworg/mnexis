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
| `mnexis_workspace` | First call of a session: the namespace this workspace writes into, the allowlists, and the most recent memories. |
| `mnexis_save` | Store a durable, self-contained memory about the project. |
| `mnexis_search` | Full-text search over the stored memories (ranked). |
| `mnexis_list` | List what is already stored. |
| `mnexis_show` | Read one memory by UUID. |
| `mnexis_history` | Every revision of one memory, and why each happened. |
| `mnexis_recall` | What a session was, in one answer: the session, its prompts, and the memories of its window (previews by default, `full` for the records). |
| `mnexis_supersede` | Replace an outdated memory with a newer one, keeping history. |

Every tool is declared to the model in every session: availability does not
depend on the model discovering a skill.

`mnexis_save` also takes `keywords`: the comma-separated words the fact is
called by in another language or another team's jargon (`puerto, port,
listen port`). They are the topic's vocabulary, never a value, and they
accumulate on the record across sessions, so the tool's answer shows the
words the memory carries now — read it and prune a word that is wrong.

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

The model never picks a namespace, and the engine owns the identity of the
workspace:

1. The engine publishes the canonical key of the workspace the session runs
   in (`mnexis project list --json` → `current_key`): `git:<normalized
   remote>`, with the protocol and the `.git` suffix removed, the whole URL
   lowercased, `/` and `:` replaced by `-` and the host's dots kept —
   `https://github.com/Xeworg/AegisPI.git` becomes
   `git:github.com-xeworg-aegispi`. Without a remote it is `path:<repo
   root>`, and outside a repository `path:<cwd>`.
2. `mnexis project list` is consulted with that key: an existing binding is
   never renamed.
3. A workspace with no binding and no `.mnexis-namespace` marker has **no**
   namespace. The extension does not invent one — the name of a directory is
   not a namespace — so the write is refused and the message names the two
   commands the human has to run: the bind command with the canonical key,
   and the allow-namespace command for the namespace. A marker names one
   (the human wrote it down), and only then is the binding recorded.

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
