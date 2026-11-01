# mnexis-memory (opencode plugin)

The host-side half of the mnexis integration for [opencode](https://opencode.ai).

opencode gets the **memory tools** from the engine's MCP server — seventeen tools, one
shape, shared with every other host. This plugin adds the things MCP and the CLI cannot
do, because they belong to the host:

- **it stores what you type, verbatim**, without the model being asked to;
- **it registers the session** with the engine, where the project's memories live;
- **it points the agent at the door** — `mnexis_memory_context` — once per session,
  with the previous session's summary in one line;
- **it keeps the project state alive through a compaction**;
- **it guards the human's permissions**: a model-issued `mnexis config allow-namespace`
  or an edit of the mnexis profile is refused, with the command the human must run;
- **it curates the definitions the model reads** (opencode 2 only): the seventeen tools
  carry the same vocabulary a pi model meets, and `codemode` is off.

It registers no tools and draws nothing: the tools are the MCP server's, and opencode's
TUI does not let a plugin restyle an MCP tool (RESEARCH.md §1).

## Install

Two commands. The first is the plugin; the second is the rest of the integration, and it
is the engine's own:

```sh
./extensions/mnexis-opencode/install.sh          # → ~/.config/opencode/plugins/mnexis-memory.js
mnexis agent install --env opencode             # → the skill + the mnexis MCP entry
```

Restart opencode. `install.sh --uninstall` removes the plugin file and nothing else;
`--config-dir DIR` installs into another opencode configuration directory
(`$XDG_CONFIG_HOME/opencode` and `$MNEXIS_OPENCODE_CONFIG_DIR` are honoured too).

The MCP entry is not optional: `mnexis agent install --env opencode` writes both the
skill (`~/.config/opencode/skills/mnexis-memory/`) and the `mnexis` server in
`opencode.json`. If your profile is not the default one, add the data directory to the
entry by hand:

```json
{
  "mcp": {
    "mnexis": {
      "type": "local",
      "command": ["mnexis", "mcp", "serve"],
      "enabled": true,
      "codemode": false,
      "environment": { "MNEXIS_DATA_DIR": "/path/to/profile" }
    }
  }
}
```

### opencode 2 and Code Mode

opencode 2 hides the tools of an MCP server behind Code Mode
(`tools.mnexis.memory_context(...)`) unless the server says otherwise — `codemode` defaults
to true there. The door is meant to be one cheap direct call, so both halves write it:

- the entry **the engine writes** (`mnexis agent install --env opencode`) carries
  `"codemode": false`;
- the plugin's catalog transform sets it on **every `mnexis_*` tool it lists**, at each
  refresh, so an entry written by an older mnexis is repaired at runtime too.

```json
{ "mcp": { "mnexis": { "type": "local", "command": ["mnexis", "mcp", "serve"], "enabled": true, "codemode": false } } }
```

`mnexis agent install` never overwrites an entry that already exists, so a configuration
written before this field existed keeps its bytes: the plugin's transform is what keeps those
tools direct. If you edit the entry by hand, keep `"codemode": false` in it —
`mnexis agent uninstall` removes only the exact entry the current mnexis writes.

### The curated definitions (opencode 2 only)

The engine's MCP descriptions say what a tool does. In `ctx.tool.transform` — the one seat
that can rewrite a model-facing definition — the plugin replaces them with the curated ones,
so a model on opencode meets the vocabulary a model on pi meets:

- `mnexis_memory_save` carries the **seven kinds, verbatim**, the same lines pi's settle
  nudge states: one definition of the vocabulary, two consumers;
- the other sixteen carry one curated description each, pi's wording where pi has the same
  tool, adapted to what the MCP surface answers (previews, ids, the fields each tool takes).

Two rules hold the copy honest. An id the plugin does not know (the engine renamed a tool)
is skipped, never guessed at; and a catalog with no `mnexis_*` tools — MCP down, or the
refresh before the first `tools/list` — is a no-op. The whole transform body is wrapped:
an uncaught throw inside this callback corrupts the turn, so a plugin must never let one out.

The copy is drift-pinned by `test/core.test.js`: the seven glosses are compared to pi's
`SETTLE_NUDGE` as it is on disk, the map to the engine's tool inventory in
`src/mcp/server.rs`, and pi's tools block to its 5500 B budget (the glosses must never be
added there).

**opencode 1 has no equivalent seat.** It keeps the engine's own descriptions, as it always
did — the transform is registered only where the host offers it.

### One file, both opencode generations

The plugin is a single file with a default export both plugin models read:

- **opencode 1.x** (`opencode-ai@1.19.5`) discovers `plugins/*.js`, accepts a default
  object with an `id` and calls its `server()`;
- **opencode 2.x** discovers the same file and runs its `setup(ctx)`.

`install.sh` writes that one file; there is no second install path and no `node_modules`
(the plugin imports nothing from opencode). RESEARCH.md §4 explains how, and
`test/host.smoke.test.js` asks the installed opencode to load it.

## What you get

| Rule | What it does | opencode 1 | opencode 2 |
| --- | --- | --- | --- |
| capture | every prompt you type, byte for byte, in the workspace's namespace | `chat.message` | `session.hook("prompt")` |
| session | `mnexis session --id … --directory …`, and `mnexis end` when the plugin unloads | `session.created`, `dispose` | `session.created`, setup cleanup |
| pointer | one system-prompt line per session: the namespace, the memory count, the door, the previous session's summary | `experimental.chat.system.transform` | `session.hook("context")` |
| compaction | the live `project_state` and `decision` records, from `mnexis list --type … --full`, bounded and counted | `experimental.session.compacting` | `session.hook("compaction")` |
| gate | refuses a model-issued namespace grant and any edit of `mnexis.toml`/`projects.toml` | `tool.execute.before` | `tool.hook("execute.before")` |
| definitions | the curated description of every `mnexis_*` tool, and `codemode: false` | — (no seat) | `tool.transform` |

Everything is best effort: a memory engine that cannot answer never fails a turn. A
capture the store refuses keeps its size and exit code in the host's log (opencode 1) and
never keeps the message text.

The pointer is a **pointer**, not a second answer: it names the door and the rules that
make the door cheap, and it does not re-render the memories. The measured reason is in
`odd/next.md` §11 — a state block injected into the context did not remove the
verification round, so shipping one would buy tokens and nothing else.

## The permission gate

Permissions belong to the human, and a plugin cannot open a consent dialog on this host
(RESEARCH.md §1). So the gate does what the pi extension does in a headless session: it
refuses, and names the command you must run.

- `mnexis config allow-namespace …` or `mnexis project bind …` issued by the model → refused.
- An edit of `mnexis.toml` or `projects.toml` → refused.
- Everything else is the model's to do, including `mnexis context`, `mnexis search` and
  `mnexis list`.

## Configuration

| Variable | Meaning |
| --- | --- |
| `MNEXIS_BIN` | Path to the `mnexis` binary (default: `mnexis` on `PATH`). |
| `MNEXIS_DATA_DIR` | Profile directory, forwarded to the CLI unchanged. |
| `MNEXIS_OPENCODE_POINTER` | Set to `0` to stop the plugin from pointing at the door. |

## Tests

```sh
node --test "extensions/mnexis-opencode/test/*.test.js"
```

The unit tests fake both plugin models and the CLI (`exec` is injected), so they need
neither the binary nor a profile nor a host. `test/install.test.js` runs `install.sh`
against a throw-away `HOME`; `test/host.smoke.test.js` is opt-in and asks the real host to
load the file:

```sh
OPENCODE_BIN="$(command -v opencode)" node --test "extensions/mnexis-opencode/test/host.smoke.test.js"
```

## Limits, honestly

- The compaction summary is **not** stored as an episode. opencode 1 does not hand a
  plugin the summary it produced, and opencode 2's `session.compaction.ended` carries the
  text but its meaning has not been verified at runtime, so nothing writes it
  (RESEARCH.md §1, §6).
- The pointer costs about 127 tokens (508 characters, by the chars/4 estimate), once per
  session. It is off with `MNEXIS_OPENCODE_POINTER=0`.
- `mnexis agent install --env opencode` does not install this plugin yet: the engine only
  embeds the pi extension (`AgentInstaller::extensions_dir` answers `None` for opencode).
  That is the one distribution blocker, and it is engine work (RESEARCH.md §6).
