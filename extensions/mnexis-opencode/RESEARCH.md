# opencode, as it is today — and what a quality mnexis integration is

Research for T1/S1+S2 (`odd/tasks/opencode-extension.md`). Everything below was read or
measured on **2026-10-31**; every claim names its source, and the claims that needed a
running host were checked against the opencode installed on this machine
(`opencode --version` → **2.2.2**, V2).

The question this answers: what is opencode's plugin/integration model today, what can a
plugin actually observe and change, what does the engine already give opencode, and which
of those pieces does a *quality* mnexis integration need — built so the host's own model
carries it, not fought against.

Sources are collected in `tmp/reference/` in the worktree (`docs-*.html`, `v2-*.html`,
`pkgs/`), fetched as noted in [Sources](#sources).

---

## 0. The finding that changes the shape: there are two plugin models, and one of them is live

opencode shipped **V2** while the V1 docs stayed online, and the two do not share plugin
code:

> "V1 plugin implementations do not run in V2. Moving a file or renaming its config entry
> is not enough."
> — https://opencode.ai/v2/docs/build/plugins/migrate-v1/

| | V1 | V2 |
| --- | --- | --- |
| npm dist-tag `latest` | `opencode-ai@1.19.5` | `@opencode/plugin@2.2.5` (plugin SDK; the CLI ships through the installer) |
| plugin entry | a module exporting a function that returns a hooks object | a module default-exporting `{ id, setup(ctx) }` |
| discovery | `{plugin,plugins}/*.{ts,js}` files | `.opencode/plugin/` **and** `.opencode/plugins/`, files **and** package directories |
| docs | https://opencode.ai/docs/plugins/ | https://opencode.ai/v2/docs/plugins/, https://opencode.ai/v2/docs/build/plugins/ |
| hook registration | keys in one returned object | `ctx.<domain>.hook(...)` / `ctx.<domain>.transform(...)` |
| tool registration | `tool(...)` with a Zod schema | `ctx.tool.transform(editor => editor.add({…JSON Schema…}))` |
| event access | `event({ event })` hook | `ctx.event.subscribe({ types, signal })` AsyncIterable |

Both are shipped and supported. `opencode.ai/docs` (the default docs tree) still documents
V1; `opencode.ai/v2/docs` documents V2; the V2 migration guide states V2 reads existing
V1 *configuration* (including `mcp`) and normalizes it, while plugin *implementations*
must be ported.

**Measured on this host (V2.2.2), not assumed:**

- `opencode --version` → `2.2.2` (V2), so the host a user runs today is very often V2.
- The engine's existing MCP entry — the V1 shape `{"mcp":{"mnexis":{"type":"local","command":[…],"enabled":true}}}`
  that `src/application/opencode_config.rs` writes — is **read and normalized by V2**:
  `opencode debug config --standalone` answered
  `"mcp": {"servers": {"mnexis": {"type":"local","command":[…],"disabled": false}}}`.
  So `mnexis agent install --env opencode` does not need a V2 branch for the MCP entry.
- V2 discovers **both** a flat `plugins/x.js` and a `plugins/x/` package directory, loads
  it (`msg="loading plugin" … entrypoint=…` in `--print-logs`) and **runs `setup`**
  (verified with a probe plugin that appended `ctx.location.directory` to a file, in a
  throw-away `HOME`; see `test/host.smoke.test.js`).
- `Plugin.define` is a runtime passthrough — `export function define(plugin) { return plugin }`
  (`@opencode/plugin@2.2.5`, `dist/promise/plugin.js`) — so **a V2 plugin needs no
  dependency at all**: a plain object `{ id, setup }` default-exported is a plugin.

Consequence for us: a single installed file that both hosts load is possible, and it is
worth the effort, because the alternative is asking a user to know which opencode they run.

---

## 1. What a plugin can observe and change (the hooks that matter)

### V1 (`@opencode-ai/plugin@1.19.5`, `dist/index.d.ts`)

`Hooks` carries: `event`, `config`, `tool`, `dispose`, `auth`, `provider`,
`chat.message`, `chat.params`, `chat.headers`, `permission.ask`,
`command.execute.before`, `tool.execute.before`, `tool.execute.after`, `shell.env`,
`experimental.chat.messages.transform`, `experimental.chat.system.transform`,
`experimental.session.compacting`, `experimental.compaction.autocontinue`,
`experimental.text.complete`, `tool.definition`.

Event names (docs): `session.created|updated|deleted|idle|diff|error|status|compacted`,
`message.updated|removed`, `message.part.updated|removed`, `file.edited`,
`permission.asked|replied`, `command.executed`, `todo.updated`, `lsp.*`,
`installation.updated`, `server.connected`, `shell.env`, `tui.*`.

### V2 (`@opencode/plugin@2.2.5`, `dist/promise/*.d.ts`)

- `ctx.event.subscribe({ types?, signal? })` → `AsyncIterable` of the public event stream
  (`session.created`, `session.idle`, `session.deleted`, `session.compaction.{started,ended,failed}`, …).
- `ctx.session.hook("prompt" | "context" | "compaction" | "generate" | "title" | "retry" | "model.request" | "http.request" | …)`.
- `ctx.tool.hook("execute.before" | "execute.after")`, `ctx.tool.transform(editor => …)`.
- `ctx.permission.hook("evaluate", …)`, `ctx.shell.hook("create.before", …)`,
  transforms for agents/commands/skills/MCP/models/providers, and `ctx.storage`.
- Setup returns a cleanup function; every `hook`/`transform` returns a `Registration`
  with `dispose()`.

Hook callbacks may return `Promise<void>` — `Hooks<Spec>` is typed
`(input) => Promise<void> | void` — so an `await`ed CLI call inside a hook is supported.

### The two facts a memory integration needs

**Prompt capture.** V1 fires `chat.message` from the single `prompt()` path
(`packages/opencode/src/session/prompt.ts` at tag `v1.19.5`, `yield* plugin.trigger("chat.message", …)~
line 999) with `{ message: UserMessage, parts: Part[] }`; parts carry `synthetic: true`
when the host wrote them (agent-part hints, auto-continue). V2 states the rule outright:

> "Prompt hooks run under these conditions: … Synthetic messages, shell messages,
> compaction controls, and move controls do not [run the hook]. … Edits become the
> canonical persisted user input."
> — https://opencode.ai/v2/docs/build/plugins/server/

So the capture is host-driven in both models, which is exactly what the pi extension does
with its `input` hook, and exactly what the engine's `user_prompts` table needs (board
`odd/next.md` §11 and the capture-gap analysis: the only writer today is a tool the model
must be asked to call).

**Session end.** V1 has no per-session shutdown hook: `session.idle` fires after *every*
turn (not a session end), `session.deleted` only when deleted, and the plugin's `dispose`
runs when the plugin unloads. V2 adds `experimental.idle` (an idle loop, explicitly
experimental) and `session.deleted`; the setup cleanup runs on unload. So "session end"
is a best-effort, not an event with a promised seat — the same shape the pi extension has
with `session_shutdown`.

**What v2.2.5 really publishes (measured live, 2026-11-03, `bench-oc:verify`).** The
names this plugin used — `session.created` and `session.deleted` — are the **V1** names,
copied from `opencode.ai/docs/plugins` (which today documents the older generation). On
v2.2.5 the bus is alive and busy, and the plugin was listening to nothing: one headless
`opencode run` delivered **25** event types — `session.execution.started` /
`succeeded`, `session.step.*`, `session.text.*`, `session.inbox.*`, `session.usage.updated`,
`session.renamed`, and the rest — and **neither** `session.created` **nor**
`session.deleted` was among them. The union of publishable types
(`@opencode/client@2.2.5`, `dist/promise/client.d.ts`) *does* contain both names, so the
subscription was legal and the seat was dead: `session.deleted` is published only on an
explicit session removal ("Remove a temporary session with `remove`"), never at the end of
a run, and `session.created` is not published in a run at all. `session.idle` exists but
the schema marks it `// deprecated` in v2.2.5, and the host did not publish it.

The seat that fires and means "the session finished its work and goes idle" is
`ctx.session.hook("experimental.idle", fn)`, payload `{ sessionID, background }` — verified
live: it fires between `session.execution.started` and `session.execution.succeeded`. It is
the seat the host's own built-in `goal` plugin uses
(`packages/core/src/plugin/goal.ts@v2.2.5`). The paths the idle seat skips (a failure or an
interruption) are covered by `session.execution.failed` / `session.execution.interrupted`.

So V2's lifecycle is: **the pause refreshes the state** (the record the next session
resumes from is written again, saying the session is open — because it is), and **the end
is real where the host gives one**: the engine's session is ended when another session
starts, or when the plugin's teardown runs. There is no process-shutdown hook in v2.2.5
(open request `anomalyco/opencode#10524`).

**The unload carries no reason, on either model** (measured against the published types,
2026-11-03). V2's `setup` returns `Cleanup = () => Promise<void> | void`
(`@opencode/plugin@2.0.4`, `dist/promise/plugin.d.ts`), and V1's `dispose()` takes no
argument (`Hooks` above). pi skips its close when the shutdown reason is `reload`
(`extensions/mnexis-memory/index.ts:2524`); opencode has no such reason to skip on, and a
V2 hot reload runs the old plugin's cleanup on the way out (§3). A reload and a quit are
therefore the same event at the unload, and — because the engine never reopens an ended
session — ending there would kill a live session on every reload. The engine tolerates a
session ended early (measured on the binary: a prompt on an ended session is accepted, and
registering the same id again answers the ended row without reopening it), so V2 ends the
sessions it opened at the unload and the record it writes is corrected by the next pause
if the session turns out to be alive. **V1 keeps ending nothing at unload**: its host model
has an end signal of its own (`session.deleted`), and changing V1 is out of this feature's
scope.

### What cannot be done

- **No plugin-initiated consent dialog.** V1 exposes `permission.ask` (a plugin may answer
  a permission the host already raised) and V2 `permission.hook("evaluate")`; neither lets
  a plugin *ask the human a question*. A custom tool's `ctx.ask()` (V1 `ToolContext.ask`)
  exists only inside a plugin's own tool. Since this integration deliberately registers no
  tools (see §3), the gate cannot ask: it must **deny with instructions**, the way the pi
  extension behaves in a headless session.
- **No plugin-side rendering.** The TUI hardcodes its rendering per tool id, so a plugin
  cannot restyle an MCP tool (board `odd/next.md` §10, issues #21018, #33302; re-checked:
  the V1 TUI plugin API at `@opencode-ai/plugin/tui` is for TUI *parts*, not for the chat
  transcript). The portable part of the chrome — the shape, not the decoration — is
  therefore already what the CLI prints.
- **No structured logger in the V2 plugin context** (`ctx.app` is `{name, version, channel}`).
  V1 has `client.app.log({ body: { service, level, message, extra } })`. V2 has no logger at
  all, and its `ctx.storage` is a key/value store — `StorageDomain`:
  `get(key)`, `set(key, value)`, `remove(key)`, `scan(options)` with `Schema.Json` values
  (`@opencode/plugin@2.0.4`, `dist/promise/storage.d.ts`) — so a V2 plugin that needs to
  report a swallowed failure writes it there. The plugin journals one key per kind,
  `mnexis.report.<kind>`.
- **V1 cannot see a produced compaction summary.** `session.compacted` carries
  `{ properties: { sessionID } }` and `experimental.session.compacting` is called *before*
  the summary exists (it hands `{ context, prompt? }`). V2's `session.compaction.ended`
  event *does* carry `data.text` (the summary) plus `data.reason`
  (`@opencode/schema@2.2.5`, `dist/session-event.d.ts`), so "store the compaction summary
  as an episode" (pi's `session_compact`) is a **V2-only follow-up**, with the wording of
  `data.text` unverified at runtime. It is not shipped here: an unverified write into the
  memory store is worse than a missing one.

---

## 2. The MCP story — and the board's §11 is out of date

The engine ships `mnexis mcp serve` and the binary **already exposes the door**:

- `src/mcp/server.rs` defines **17** tools, `memory_context` among them (`#[tool(name = "memory_context", …)]`,
  `src/mcp/server.rs:851`), and `README.md` lists all 17.
- `do_context` answers the same envelope as `mnexis context --json` — the priced items,
  the session-state block, the coverage declaration and the `read_scope`
  (`src/mcp/server.rs:509-566`).

**So the extension is not blocked on the door.** Board `odd/next.md` §11 ("The door is not
in the MCP surface — the next thing for opencode") describes an earlier state of the
engine and should be corrected by the parent: item 11's first half has landed.

What §11 is still right about: the answers are not identical. The MCP door answers each
match with **a 160-character preview**, while the pi extension's `mnexis_context` answers
**whole facts** with no `full` switch. The pi extension README's claim that opencode "meets
one shape, not a second projection of the same facts" is therefore true for the envelope
and the session state, and not true for the item bodies. That is a documentation
correction plus an engine decision (does the MCP door carry whole facts?), not a plugin
fix — out of scope here, recorded so it is not silently lost.

MCP config, V1 and V2:

- V1: `mcp.<name> = { type: "local" | "remote", command?, enabled, environment? }`
  (https://opencode.ai/docs/mcp-servers/).
- V2: `mcp.servers.<name> = { type, command, environment?, disabled?, codemode?, timeout?, protocol? }`
  — "V2 does not place server names directly under `mcp`" (https://opencode.ai/v2/docs/mcp-servers/),
  but V1-shaped members inside `mcp` are normalized (measured, §0).
- Tool names are `<server>_<tool>` in both, normalized to `[A-Za-z0-9_-]`
  (V2 docs, "Names"), so the engine's `mnexis` entry yields `mnexis_memory_context`.

**One real gap V2 introduces:** `codemode` **defaults to true**, which exposes MCP tools
grouped behind Code Mode (`tools.mnexis.memory_context(...)`) instead of as direct tools.
For a memory door that is supposed to be one cheap call, that changes the cost and the
model's reach for it. `"codemode": false` on the `mnexis` entry keeps the direct tool.
Writing that field is **engine work** (`src/application/opencode_config.rs`, which would
have to decide by host version, and `codemode` is not a V1 field); until then the README
tells a V2 user the one line to add.

## 3. The skill and the install path the engine already has

`AgentEnvironment` knows `pi` and `opencode` (`src/application/agents.rs:25-64`), and
`mnexis agent install --env opencode` writes **the skill**
(`~/.config/opencode/skills/mnexis-memory/SKILL.md`) **and the MCP entry**
(`OpencodeConfig::install`, `src/cli/agent_cmd.rs:75-95`). It cannot write an extension:
`AgentInstaller::extensions_dir(AgentEnvironment::Opencode)` returns `None`
(`src/application/agents.rs:194-199`), because the engine only embeds the pi extension
(`crate::assets`).

That is the distribution blocker, and it is engine work (out of scope on this branch):
the extension ships here with its own `install.sh`, which copies one file into the host's
plugin directory for both models.

opencode's own distribution model, for the record:

- V1: files in `~/.config/opencode/plugins/` or `.opencode/plugins/` (discovery glob
  `{plugin,plugins}/*.{ts,js}`, `v1.19.5 packages/opencode/src/config/plugin.ts`), or npm
  packages named in `plugin` (installed with Bun, cached in `~/.cache/opencode/node_modules`).
- V2: `plugins` in `opencode.json(c)` with package *or directory* entries, plus
  auto-discovery of `.opencode/plugin/`, `.opencode/plugins/` and
  `<config>/plugins/`; `opencode plugin add|list|check|update|remove`; hot reload of
  watched config directories.

A **single flat file** at `~/.config/opencode/plugins/mnexis-memory.js` is auto-discovered
by both models, which is the only install shape that needs no config edit in either.

## 4. The one-file, two-model plugin — why it is possible

V1 accepts a **default-exported object** with `id` and `server()`:

```ts
// v1.19.5 packages/opencode/src/plugin/shared.ts, readV1Plugin()
const value = mod.default
if (mode === "detect" && !("id" in value) && !("server" in value) && !("tui" in value)) return
```

V2 accepts a **default-exported object** with `id` and `setup` or `effect`, and ignores
excess members — the module schema is an Effect `Schema.Struct({ id, setup })`
(`packages/core/src/plugin/module.ts`, `Module`), and `Schema.Struct` drops unknown keys by
default:

```ts
const Module = Schema.Struct({
  default: Schema.Union([
    Schema.Struct({ id: Schema.String, effect: … }),
    Schema.Struct({ id: Schema.String, setup: … }),
  ]),
})
```

So one default export `{ id, setup, server }` is a valid plugin for **both**: V1 detect-mode
returns the object (it has `id`) and calls `.server(input, options)`; V2 decodes `{id, setup}`
and runs `setup(ctx)`. `Plugin.define` being a passthrough means no import is needed at all,
so the file has no dependency and no `node_modules`.

This is a compatibility bet, and it is pinned two ways: a unit test asserts the shape both
hosts read, and an opt-in host test (`OPENCODE_BIN`) runs the real V2 loader over the file
and fails if it reports a load error.

## 5. What this integration ships, and what it deliberately does not

The tools come from the **MCP server** (17 tools, one shape, shared with every other host
and with the CLI). Re-registering them as plugin tools would put a second copy of every
tool in the model's context and let the two drift — the exact "second projection" the
engine's own rule forbids. So the plugin adds only what MCP and the CLI cannot do:
**host-side behaviour**.

| Rule | Why the host must carry it | V1 hook | V2 hook |
| --- | --- | --- | --- |
| **capture** — store what the person typed, verbatim | the only writer today is a tool the model must be asked to call (capture gap) | `chat.message` → text parts, `synthetic` skipped | `session.hook("prompt")` (host already excludes synthetic) |
| **session start** — register the session with the engine | the session must land in the namespace its memories land in | `event: session.created` (`properties.info.id`) | `session.hook("prompt")` — the seat that fires in a run (v2.2.5 publishes no `session.created`; measured 2026-11-03) |
| **session pause** — refresh the state the next session resumes from | a host that publishes a pause and not an end leaves the state unwritten, and a session nobody ends is a session nobody resumes from | — | `session.hook("experimental.idle")` + `event.subscribe({types:["session.execution.succeeded","session.execution.failed","session.execution.interrupted"]})` → the "estado al cierre" record, revised, saying the session is open |
| **session end** — end what the plugin opened, and leave the state it closed with | best effort, no promised seat in either model; the unload carries no reason, so a reload and a quit look the same there (§1) | `event: session.deleted` → `mnexis end` + the "estado al cierre" record; `dispose()` kills the in-flight children and ends nothing | `event.subscribe({types:["session.deleted"]})` → the same close; plus the end the host gives one way or another: a new session starts, or the plugin's cleanup runs (v2.2.5 has no end-of-session event; §1) |
| **the pointer** — point at the door, once per session | a session that never hears about the door never calls it | `experimental.chat.system.transform` | `session.hook("context")` |
| **the settle question** — ask once when work changed files and nothing was saved | the judgment is the model's; a session that is never asked never considers it | `tool.execute.after` + the system transform | `tool.hook("execute.after")` + the context hook |
| **compaction** — keep the project state in the summary | the state must survive the context that held it | `experimental.session.compacting` (`output.context`) | `session.hook("compaction")` (`event.system`) |
| **the gate** — block model-issued permission grants and profile edits | the human owns the permission; a plugin cannot ask (§1) | `tool.execute.before` (`bash`/`edit`/`write`) | `tool.hook("execute.before")` (`shell`/`edit`/`write`/`patch`) |

Composition rules, taken from the engine's own discipline:

- **The pointer is a pointer, not a projection.** One line per session: the namespace, its
  memory count, the name of the door, and the previous session's summary in one line.
  It does **not** re-render memories or the state block: the board measured that a state
  block injected into the context did not remove the verification round (P5 failed,
  `odd/next.md` §11), so shipping it as a second projection would buy tokens and nothing
  else. Cost: one `mnexis status --json` and one `mnexis recall --previous --json` per
  session, and ~127 tokens once (508 characters; the chars/4 estimate, not the engine's
  tokenizer).
- **The compaction block is the engine's own records, rendered from its own surface** —
  `mnexis list --type project_state,decision --full --limit 8 --json`, bounded, with the
  count of what did not fit. No invented selection rule, no second store.
- **Every call is bounded, and the writes retry a locked store.** A read gets ten
  seconds and a write thirty; `session`, `end`, `prompt` and the closing `save` are
  idempotent by construction, so a busy exit (10) is tried again with the waits 250 ms and
  750 ms inside the call's own budget, while a read pays one report. The capture has a 1.5 s
  deadline of its own, the pointer's two reads share one, and the close shares one 5 s
  budget (`SHUTDOWN_BUDGET_MS`, pi's R1) between the end and the estado al cierre; the
  unload kills the children still in flight and ends nothing (§1).
- **Every hook is best effort and never fails the turn.** A capture the store refuses is
  swallowed and journaled — V1 through `client.app.log`, V2 through `ctx.storage`
  (`mnexis.report.<kind>`), the only journal that host has (§1) — and never stops the
  human's message.
- **The gate mirrors the pi extension's two rules exactly** — the grant-command pattern and
  the profile-file pattern — with one wording change: opencode cannot ask the human from a
  plugin, so the refusal names the command the human must run instead of promising a
  confirmation dialog that does not exist on this host.
- **No tools, no renderer, no chrome.** Tools are MCP's; the chrome is pi's (§1).

## 6. What is blocked, and on what

1. **Distribution through the engine (engine work, `src/`).** `mnexis agent install --env
   opencode` installs the skill and the MCP entry but not an extension, because
   `AgentInstaller::extensions_dir(Opencode)` is `None` and the assets are embedded from
   `crate::assets`. Until that lands, `extensions/mnexis-opencode/install.sh` is the
   install path (it copies one file and undoes it cleanly).
2. **`codemode: false` for V2 (engine work, `src/`).** The engine writes the V1 `mcp`
   entry; V2 accepts it but hides the tools behind Code Mode by default. The engine would
   have to write `"codemode": false` when it writes for a V2 host. The README gives the
   one-line manual equivalent meanwhile.
3. **Whole facts in the MCP door (engine decision, `src/mcp`).** The door answers previews;
   the pi extension answers whole facts. Same envelope, different item bodies (§2).
4. **The compaction summary as an episode (V2-only, follow-up).** Needs a runtime check of
   `session.compaction.ended`'s `data.text` before anything is written to the store (§1).
5. **The board's §11 correction (docs).** The door is in the MCP surface; §11's premise is
   stale (§2).

Nothing in this list prevents the plugin from working today.

## 7. Confidence

| Claim | Confidence | Basis |
| --- | --- | --- |
| V2 is installed and is what many users run | high | `opencode --version` → 2.2.2; V2 docs + migration guide; brew/Arch default to V2 |
| V1 is still the npm `latest` and its plugin model is unchanged | high | npm dist-tags (`opencode-ai@1.19.5`), V1 type declarations |
| Both models discover one flat file and run it | high | V2: measured (`--print-logs` + a probe that wrote a marker from `setup`); V1: discovery glob read in the source |
| The dual-shape default export loads in both | high for V1 (source), high-but-a-bet for V2 (schema drops excess keys; pinned by an opt-in host test) | `readV1Plugin`, V2 `Module` schema |
| The engine's V1 MCP entry works in V2 | high | `opencode debug config` normalized it |
| `chat.message` fires for every submitted prompt and marks synthetic parts | medium-high | V1 source (`prompt.ts`) + V1 docs; not exercised on a live V1 binary here |
| `session.hook("prompt")` excludes synthetic messages | high | V2 docs state it |
| V1 has no plugin-side consent dialog | high | V1 type declarations: only `permission.ask` and tool `ctx.ask` |
| V2 has no structured logger in the plugin context | high | `@opencode/plugin@2.0.4` `Context` type (`app` = name/version/channel) |
| V2's setup cleanup receives no arguments, so the unload has no reason | high | `@opencode/plugin@2.0.4` `dist/promise/plugin.d.ts` (`Cleanup = () => Promise<void> | void`) |
| `ctx.storage` is a key/value journal | high | `@opencode/plugin@2.0.4` `dist/promise/storage.d.ts` (`get`/`set`/`remove`/`scan`) |
| `session.compaction.ended` carries the summary text | medium | schema field `data.text`; semantics unverified at runtime, so nothing writes it |

Two things could not be checked here and are named rather than guessed: a live **V1**
binary (none installed; V1 behaviour is read from the 1.19.5 source and type declarations),
and a live **V2 session** with a model (no model credentials in this worktree), so the
prompt/context/compaction hooks are exercised by unit tests against the documented event
shapes, not by a real turn.

---

## 8. The split from the pi extension, and the final port verdicts (T2-T4)

The plugin used to anchor its vocabulary on the pi extension: a comment pointed at the
`SETTLE_NUDGE` there, and `test/core.test.js` imported `extensions/mnexis-memory/index.ts`
to compare the two gloss lists. That anchor rotted the day the pi rewrite moved the glosses
into `mnexis_save`'s description, and it meant one host's file could break the other's
build. T2 severed it: the plugin keeps its own `TYPE_GLOSSES`, the seven NAMES are pinned to
the engine's own `memory_type` docs (`src/mcp/tools.rs`), and no code or test reads the
sibling extension tree. The engine — its type set, its MCP schema and its CLI — is the only
contract the two surfaces share.

T3 ported the pi feature set where this host can express it. The verdicts:

| pi capability | Verdict | Evidence |
| --- | --- | --- |
| Outcome-based settle nudge | **PORTED** | `tool.execute.after` (V1) / `ctx.tool.hook("execute.after")` (V2) feeds the session's outcomes; the ask rides `experimental.chat.system.transform` / `ctx.session.hook("context")`, once per session, only when a successful shell/edit call exists and no `mnexis_memory_save` landed (a failed attempt does not count). The text names the save and the seven `memory_type` names. opencode has no before-settle boundary, so the ask is delivered on the next model request instead of at the settle. |
| Taxonomy in the save description | **PORTED (V2) / NOT IMPLEMENTED (V1)** | `ctx.tool.transform` rewrites `mnexis_memory_save`'s description with the glosses (the engine's names, this host's wording). **V1 is a gap, not a host limit:** `tool.definition` is a V1 hook (§1), so the same curated descriptions can reach a V1 model; the plugin does not register it yet (follow-up, `odd/tasks/opencode-parity.md` L7), so a V1 model keeps reading the engine's description. |
| 5-tool loadout + deferred + `tool_search` + evidence activation | **N-A** | The tools are the MCP server's; the host exposes all of them. V2's `tool.transform` can rewrite a definition and its options (`codemode`), never its exposure — there is no seat to defer, hide or activate one. |
| Conditional `<mnexis-memory>` prompt section | **PORTED as the pointer** | `experimental.chat.system.transform` / `session.hook("context")`: one bounded block per session, only when the engine names a namespace. One difference kept: pi also requires `memory_count > 0`; here a bound namespace with zero memories still gets the pointer, because it teaches the door and the capture rule. |
| Per-tool `promptGuidelines` | **N-A** | pi's guidelines are a field of pi's own tool definition and pi renders them into its system prompt; opencode's catalog entries carry a description, the input schema and options (`tool.transform`) or a description and parameters (V1 `tool.definition`) — no guidelines field the host renders. The vocabulary rides the save's description (V2) and the settle ask (both) instead. |
| Prompt capture by the host | **PORTED (already present)** | `chat.message` / `session.hook("prompt")`; the host excludes synthetic messages. Now under a 1.5 s deadline and the busy retry. |
| Reliability: budgets, per-command busy retry, child registry, close deadline (R1) | **PORTED** | Per-call ceilings (read 10 s, write 30 s), busy exit 10 retried on `session`/`end`/`prompt`/`save` with 250/750 ms inside the call budget, `createDeadline` on the capture, the pointer and — R1 — the whole close (end + estado al cierre share one 5 s budget), `createChildRegistry` kills in-flight children before the unload. The plugin does spawn the CLI, so the registry applies. R1 was missing when the audit read this table and is implemented now. |
| Session lifecycle (registered start/end, idempotent close, the state a session closes with) | **PORTED** | Both models register the session and close on the host's own end signal: `mnexis end --json`, then the workspace's one "estado al cierre" `project_state` built from that answer (pi's title and shape, so a second close revises the record). The engine's verified semantics are read, not guessed: a second start is a no-op, a start after an end is reported as `already_ended` and never reopens, an unknown end (exit 3) is reported as `not_registered`, never thrown. **V2 was ported on the wrong event names** — `session.created`/`session.deleted` are the V1 names, and v2.2.5 publishes neither in a run (§1, measured 2026-11-03): the seat is now `session.hook("experimental.idle")` plus the `session.execution.*` events, the state is refreshed at the pause (saying the session is open), and the engine's session is ended where the host gives an end — a new session starts, or the plugin's cleanup runs. **V1 keeps its own seats and ends nothing at unload** (§1); the V1 population's `session.idle` behavior is unmeasured and stays a follow-up. |
| Journal renderers, footer counters, `/mnexis` card | **N-A** | opencode hardcodes transcript rendering per tool id; a plugin cannot restyle an MCP tool or draw a card (§1). V1's `client.app.log` is the only notification surface and the plugin already journals refusals there; V2 has no logger. A plugin TUI part, if the host ever exposes one for the transcript, is the future option. |
| Gate with timeout / tri-state | **N-A** | There is no dialog to time out: the gate refuses synchronously in the before-tool hook and names the human's command. The plugin keeps its inverted model (the human grants out of band). |
| Compaction summary stored as an episode (pi's `session_compact`) | **N-A** | Only V2's `session.compaction.ended` carries `data.text`, and its runtime semantics are unverified (§1, §6.4); an unverified write into the store is worse than a missing one. Follow-up when the field is verified. |
| Engine-side: taxonomy examples in the `memory_save` description | **recorded, out of scope** | The engine's `memory_save` tool description names a few kinds in prose ("a decision, a discovery, a convention, a user preference" — `discovery` and `convention` are not `memory_type` values) and gives no examples; the seven canonical names live in the `memory_type` argument's schema doc (`src/mcp/tools.rs`, carried by schemars). A sentence with the seven names and one example each would serve every host, including V1 where the plugin cannot curate. That is an engine change (`src/mcp/server.rs`), recorded in `odd/tasks/opencode-parity.md`'s Log. |

## Sources

Fetched on 2026-10-31 into `tmp/reference/` (HTML + text), unless a note says otherwise.

Docs — V1 (current `opencode.ai/docs`, documents the 1.x line):

- https://opencode.ai/docs/plugins/ — plugin dirs, npm plugins, hook/event list, custom
  tools, `experimental.session.compacting`
- https://opencode.ai/docs/mcp-servers/ — `mcp.<name>`, `type: local`, `enabled`
- https://opencode.ai/docs/skills/ — `~/.config/opencode/skills/<name>/SKILL.md`
- https://opencode.ai/docs/tools/ — tool ids (`bash`, `edit`, `write`, `read`)
- https://opencode.ai/docs/config/, /docs/custom-tools/, /docs/sdk/

Docs — V2 (`opencode.ai/v2/docs`):

- https://opencode.ai/v2/docs/plugins/ — configure/discover/manage/reload, `plugins` array,
  `opencode plugin add|list|check|update|remove`
- https://opencode.ai/v2/docs/build/plugins/ — parts (`server`, `tui`, `gui-experimental`,
  `rpc`), one package, load rules
- https://opencode.ai/v2/docs/build/plugins/server/ — events, hooks (`prompt`, `context`,
  `compaction`, `generate`, `title`, retry/HTTP/WS), prompt-hook conditions
- https://opencode.ai/v2/docs/build/plugins/migrate-v1/ — "V1 plugin implementations do not
  run in V2" + the V1→V2 hook map
- https://opencode.ai/v2/docs/mcp-servers/ — `mcp.servers.<name>`, `disabled`, `codemode`
  (default true), `<server>_<tool>` naming
- https://opencode.ai/v2/docs/migrate-v1/ — config compatibility (mixed V1/V2 `mcp` members)
- https://opencode.ai/v2/docs/skills/, /v2/docs/tools/ (tool id `shell` vs V1 `bash`),
  /v2/docs/config/, /v2/docs/api (event names)

Source (GitHub, `anomalyco/opencode`; the repo moved from `sst/opencode`):

- tag `v1.19.5`: `packages/opencode/src/config/plugin.ts` (discovery glob),
  `packages/opencode/src/plugin/shared.ts` (`readV1Plugin`, entry resolution:
  `exports["./server"]` → `main` → `index.*`), `packages/opencode/src/plugin/index.ts`
  (`applyPlugin`, legacy function exports), `packages/opencode/src/session/prompt.ts`
  (`chat.message` trigger site, synthetic parts)
- branch `dev` (V2): `packages/core/src/plugin/module.ts` (`Module` schema, legacy
  standalone file), `packages/core/src/plugin/source-directory.ts` (`plugin/` + `plugins/`,
  files and directories), `packages/plugin/src/README.md`,
  `packages/plugin/src/promise/*.d.ts`

Published packages (npm, `npm pack`, unpacked under `tmp/reference/pkgs/`):

- `@opencode-ai/plugin@1.19.5` — `dist/index.d.ts` (`Hooks`), `dist/tool.d.ts`
  (`ToolContext.ask`)
- `@opencode/plugin@2.2.5` — `dist/promise/{session,tool,event,plugin,registration}.d.ts`;
  `dist/promise/plugin.js` (`define` is a passthrough)
- `@opencode-ai/sdk@1.19.5` — `dist/gen/types.gen.d.ts` (`EventSessionCreated`,
  `EventSessionIdle`, `EventSessionCompacted`)
- `@opencode/schema@2.2.5` — `dist/session-event.d.ts` (V2 event `data` shapes, incl.
  `session.compaction.ended`)

Registry: https://registry.npmjs.org/-/package/opencode-ai/dist-tags (`latest` 1.19.5),
https://registry.npmjs.org/-/package/@opencode/plugin/dist-tags (`latest` 2.2.5).

Host measurements on this machine (opencode 2.2.2, throw-away `HOME`): plugin discovery
and `setup` execution (`--print-logs`), V1→V2 `mcp` normalization (`debug config`), and
the opt-in smoke test in `test/host.smoke.test.js`.
