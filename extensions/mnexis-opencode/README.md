# mnexis-memory (opencode plugin)

The host-side half of the mnexis integration for [opencode](https://opencode.ai).

opencode gets the **memory tools** from the engine's MCP server — seventeen tools, one
shape, shared with every other host. This plugin adds the four things MCP and the CLI
cannot do, because they belong to the host:

- **it stores what you type, verbatim**, without the model being asked to;
- **it registers the session** with the engine, where the project's memories live;
- **it points the agent at the door** — `mnexis_memory_context` — once per session,
  with the previous session's summary in one line;
- **it keeps the project state alive through a compaction**;
- **it guards the human's permissions**: a model-issued `mnexis config allow-namespace`
  or an edit of the mnexis profile is refused, with the command the human must run.

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
      "environment": { "MNEXIS_DATA_DIR": "/path/to/profile" }
    }
  }
}
```

### opencode 2 and Code Mode

opencode 2 hides MCP tools behind Code Mode unless the server says otherwise, and the
engine writes the opencode 1 entry. To keep the memory tools as direct tools (the door as
one cheap call), add `"codemode": false` to the `mnexis` entry:

```json
{ "mcp": { "mnexis": { "type": "local", "command": ["mnexis", "mcp", "serve"], "enabled": true, "codemode": false } } }
```

The engine writing that field itself is engine work (RESEARCH.md §6).

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
