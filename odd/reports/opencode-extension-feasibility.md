# An opencode extension for mnexis — feasibility, from the evidence

**Analysis:** 2026-10-30 · **Status:** research only, requested by the human — *"investiga si
podemos mejorar la extension de opencode ya que esta esta abandonada… podemos hacer una
extension en lugar de solo una skill como hicimos con pi"* · **Method:** one read-only research
agent: the repo's own installer + the pi extension's surface, against opencode's documented
plugin/custom-tool/skill APIs, with the exact doc URLs.

## The premise, corrected first

opencode does **not** get only a skill today. `mnexis agent install --env opencode` writes the
skill **and** rewrites `opencode.json` with an MCP server entry
(`src/application/opencode_config.rs:127-165`:
`{"mcp":{"mnexis":{"type":"local","command":["<bin>","mcp","serve"],"enabled":true}}}`), and the
MCP surface already exposes **15 tools** (`src/mcp/tools.rs:305-319`). So the gap is not
"tools exist or not" — it is **everything the pi extension does around the tools**.

## The mapping (pi capability → opencode equivalent)

| pi extension does | opencode equivalent | verdict |
| --- | --- | --- |
| 14 registered tools (`index.js:1432-2175`) | MCP (shipped) or plugin `tool({description,args,execute})` | **covered** |
| `input` hook — verbatim prompt capture (A2) | `"chat.message"` hook, `(input:{sessionID…}, output:{message, parts})` — fires on submit, before the model | **covered** (text read from `parts`; verbatim-ness needs one runtime check) |
| `session_start`/`session_shutdown` (session axis) | `session.created` / `session.deleted`-ish events | **partial** — no reliable "session is ending" moment |
| `session_compact` (stores the produced summary) | `experimental.session.compacting` fires **before** generation | **gap** — no documented access to the generated summary |
| `tool_call` gate (blocks config/bind from the model) | `"tool.execute.before"` + `throw` | **covered** |
| tool renderers/status chrome (`lib/chrome.js`) | none documented (`tui.toast` only) | **gap** |
| human confirmation prompts | `permission.ask` (permission requests only) | **gap** |
| spawning the local binary | Bun `$` (BunShell) in `PluginInput` | **covered** |
| install | `~/.config/opencode/plugins/` auto-load, or `"plugin":[]` in opencode.json | **covered** — `agents.rs` needs a third asset target |

Sources: opencode.ai/docs/plugins (plugin format, `tool()`, `$`), the
`@opencode-ai/plugin@1.3.3` type declarations (hook names and payloads),
opencode.ai/docs/custom-tools, opencode.ai/docs/skills (skill + plugin **complement**),
opencode.ai/v2/docs/build/plugins/migrate-v1.

## The verdict, three lines

1. **CAN**: tool parity (MCP already ships 15 tools), `chat.message` for A2-style prompt
   capture, `tool.execute.before` for the namespace gate, Bun `$` for the CLI, plugin-dir
   delivery. A real opencode plugin covering the pi extension's core loop is **feasible**.
2. **CANNOT (documented today)**: chrome/renderers, a general human-confirmation UI, a
   reliable session-end hook, and post-hoc access to the compaction summary.
3. **The two real risks are decisions, not evidence gaps**: (a) **API instability** — the v1
   hook API coexists with a v2 `Plugin.define({id, setup})` shape whose docs say "V1 plugin
   implementations do not run in V2", plus a filed issue where documented local plugin loading
   fails; (b) **double surface** — plugin tools duplicating the already-wired MCP tools (the
   plugin should probably be hooks+chrome only, keeping MCP as the tool surface).

## What the next step would be (not taken — research only)

One runtime check settles the only factual unknown: whether `chat.message`'s `parts` carry the
human's text verbatim pre-expansion (inferred from the types, not stated in docs). After that,
the shape to propose is: **plugin = hooks + gate + (someday) chrome; MCP = the 15 tools** —
no duplication, and the skill stays as the advisory layer it already is on both hosts.
