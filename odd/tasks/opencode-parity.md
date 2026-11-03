# opencode-parity — the opencode plugin decoupled from the pi extension, at feature parity where its host allows

The opencode plugin (`extensions/mnexis-opencode/index.js`, 924 lines, V2
hooks) shares no runtime imports with the pi extension — but it anchors its
vocabulary on the pi extension's file (the seven type glosses, a comment
pointing at `extensions/mnexis-memory/index.js`, and a test that reads the
sibling tree). That anchor rotted the day the pi rewrite moved the glosses
(ext-ts-rewrite T3). Per the human's decision, each host now owns its logic:
the ENGINE is the only shared contract. This feature severs the anchor and
brings the plugin to parity with the pi extension's new feature set where
opencode's host makes it possible.

## Specs

- S1: "la de open code se dano dado a que vi que usaba parte de la logica de
  la menxis-memory que es la skill de pi y esta tiene logica tambien" — the
  coupling is the defect; the stale gloss anchor is the proof it rots.
- S2: "es mejor que cada una tenga la suya" — each host owns its copy of the
  logic. No runtime import and no test read across `extensions/mnexis-*` trees;
  the engine (its seven type names, its MCP schema, its CLI) is the only
  contract both surfaces share.
- S3: "la skill de open code debera tener todas las funcionalidades que
  tenemos aca claro siempre y cuando sea posible se puede basar en la
  extension de pi para esto" — port the ext-ts-rewrite feature set to the
  plugin where opencode's host allows, using the pi implementation as the
  reference; what the host cannot express gets a written NOT-POSSIBLE with the
  reason, not a silent omission.
- S4: "haz una rama apartir de esta para eso luego hacemos merge cuando ya
  funcionen las dos en pr aparte, para no mezclar" — branch
  `feat/opencode-parity` FROM `feat/ext-ts-rewrite`, worktree
  `tmp/wt-ocparity`, separate PR stacked on the pi PR; nothing merges until
  both surfaces work.

## Design

### The anchor inversion (S1+S2)

The plugin keeps its own `TYPE_GLOSSES`, and the seven type NAMES are pinned
against the ENGINE's canonical list (the `memory_type` docs in
`src/mcp/tools.rs`, which mirror `MEMORY_TYPES` in
`src/domain/memory_type.rs`), never against the sibling extension. The gloss
text is plugin-local (the one-liners were authored on the pi side; each surface
may now evolve its own wording). The cross-extension parity test in
`test/core.test.js` (reading `../../mnexis-memory/index.ts`) is DELETED and
replaced by a local pin: the plugin's exported type list matches the engine's
names. Stale comments pointing at the pi tree are rewritten to name the engine
as the source.

### The feature mapping (S3) — verdicts filled from RESEARCH.md and the V2 plugin types

| pi capability | opencode possibility | Verdict |
|---|---|---|
| Outcome-based settle nudge (counts successful saves; one-line ask + pointer to the taxonomy) | Port onto the plugin's existing shell/edit observation gate: count successful `mnexis_save` MCP calls, keep the one-ask latch, carry the pointer to the taxonomy's home | **PORTED.** V1 `tool.execute.after` and V2 `ctx.tool.hook("execute.after")` feed the session's outcomes; the ask rides the pointer's seat (`experimental.chat.system.transform` / `session.hook("context")`) once per session, only when a successful shell/edit call exists and no `mnexis_memory_save` landed (a failed attempt is not a save). The ask names the save and the seven `memory_type` names (the taxonomy's home). No before-settle boundary exists on this host, so the ask arrives on the next model request. Tests: `test/plugin.test.js` (v1 and v2 settle cases), `test/core.test.js` (`SETTLE_NUDGE` is exported and pinned to the engine's names). |
| Taxonomy in the save description | The save description lives in the ENGINE's MCP schema (shared by both hosts) — plugin-side N/A; verify the engine's `memory_save` description carries the seven types (it names them; if the examples are wanted there too, that is an engine-side change, out of scope here, record it) | **PORTED (V2) / NOT IMPLEMENTED (V1).** `ctx.tool.transform` rewrites `mnexis_memory_save`'s description with the glosses. **The V1 half of this verdict was wrong when the audit read this table:** `tool.definition` IS a V1 hook (RESEARCH.md §1, the V1 `Hooks` list), so a curated description can reach a V1 model too — the plugin simply does not register that hook yet. That is an implementation gap recorded as a follow-up (L7), not a host limit. The engine's `memory_save` tool description names NO types and NO examples; the seven names live in the `memory_type` argument's schema doc (`src/mcp/tools.rs`, carried by schemars). Engine-side follow-up recorded in the Log (L4): names + one example each in the description would serve every host. |
| 5-tool loadout + deferred + tool_search + evidence activation | NOT POSSIBLE: the tools come from the engine's MCP server and opencode has no exposure/activation control — record as N/A with the reason | **N-A.** The host exposes the MCP server's whole tool list; V2's `tool.transform` rewrites a definition and its options (`codemode`), never its exposure. No seat to defer, hide or activate a tool. |
| Conditional `<mnexis-memory>` prompt section | Research: whether the V2 plugin hooks can add system/instructions content (RESEARCH.md §1); if yes, port the conditional-section idea (bound + has memories) with plugin-local state; if no, N/A with the reason | **PORTED as the pointer.** Both models can add system content (`experimental.chat.system.transform` / `session.hook("context")`), and the pointer is the conditional section: one bounded block per session, only when the engine names a namespace. Difference: pi also requires `memory_count > 0`; here a bound namespace with zero memories still gets the pointer because it teaches the door and the capture rule. |
| Per-tool `promptGuidelines` (pi renders each tool's guidelines into its system prompt) | pi's guidelines are a field of pi's own tool definition; opencode's catalog entries carry a description, the input schema and options (`tool.transform`) or a description and parameters (V1 `tool.definition`) — no guidelines field the host renders into the prompt | **N-A.** There is no seat for per-tool guidelines: the only prompt-shaped seat is a system part, and the plugin spends it on the pointer and the settle question. The vocabulary the guidelines would carry reaches the model through the save's description instead (V2) and the settle ask (both models). **Only the conditional-section half of this row is ported** — the row overstated it when the audit read this table (L7). |
| Prompt capture by the host | The plugin's pointer/state machinery (POINTER_ENV, chat.message) already touches this area; port the capture only if a message hook exists (RESEARCH.md "the two facts a memory integration needs"); otherwise keep the engine's CLI capture path and record it | **PORTED (already present).** `chat.message` (V1) / `session.hook("prompt")` (V2); the host excludes synthetic messages. Now under a 1.5 s deadline and the busy retry. |
| Reliability: call budgets, per-command busy-retry, child registry, shutdown deadline (R1) | Port the budget/retry patterns to every CLI call the plugin makes; a child registry only if the plugin spawns (it mostly intercepts; verify); and one wall-clock budget for the whole close, because a host awaits it with no timeout of its own | **PORTED.** Every call carries a ceiling (read 10 s, write 30 s); `session`, `end`, `prompt` and the closing `save` are idempotent by construction and retry a busy store (exit 10) with 250/750 ms inside the call budget; `createDeadline` bounds the capture, the pointer's two reads and — R1 — the whole close (the end and the estado al cierre share one 5 s `SHUTDOWN_BUDGET_MS`, overridable with `MNEXIS_OPENCODE_SHUTDOWN_BUDGET_MS`), whose abandonment is journaled and re-runnable; `createChildRegistry` kills in-flight children before the unload returns. **R1 was missing when the audit read this table** — the unload had no deadline — and it is implemented now (L7). Tests: `test/core.test.js` (budgets, retry, deadline, registry, the closing save), `test/plugin.test.js` (the unload kills and ends nothing; a hung close is abandoned inside its budget and reaches the journal). |
| Session lifecycle (registered start/end, idempotent close, the state a session closes with) | Port to the plugin's session hooks if V2 exposes them; the engine's verified semantics (start-by-id is a no-op; start-after-end never reopens) are the contract | **PORTED.** Both models register `session.created`, and close on the host's own end signal, `session.deleted`: `mnexis end --json`, then the workspace's one "estado al cierre" `project_state` built from the engine's answer (the same title and shape pi writes, so a second close revises that record instead of adding one). The engine's semantics are read, not guessed: a second start is a no-op; a start after an end is reported as `already_ended` and never reopens; an unknown end (exit 3) is reported as `not_registered`, never thrown. **The unload ends nothing** — neither model's unload carries a reason (V2's cleanup is `() => Promise<void> | void`, V1's `dispose()` takes no argument), so a reload and a quit look the same there and an end would kill a session a V2 hot reload only meant to replace (L7). Tests: `test/plugin.test.js` (v1 already-ended, v1 exit 3, v1/v2 deleted, the closing record, the unload ending nothing, an abandoned close retried). |
| Journal renderers, footer counters, /mnexis card | opencode UI has no equivalent — N/A; the plugin's pointer system is its own surface, extend it only if RESEARCH.md shows a hook | **N-A.** opencode hardcodes transcript rendering per tool id; a plugin cannot restyle an MCP tool or draw a card (RESEARCH §1). V1's `client.app.log` is the only notification surface, and the plugin already journals refusals there; V2 has no logger. A plugin TUI part, if the host ever exposes one for the transcript, is the future option. |
| Gate with timeout/tri-state | The plugin's gate philosophy is inverted (it BLOCKS the model from granting; the human grants out of band) — keep the plugin's model, port only the timeout idea if dialogs exist | **N-A.** There is no dialog to time out: the gate refuses synchronously in the before-tool hook and names the human's command. The inverted model is kept. |
| Compaction summary stored as an episode (pi's `session_compact`) | — (added while walking the pi feature set) | **N-A.** Only V2's `session.compaction.ended` carries `data.text`, and its runtime semantics are unverified (RESEARCH §1, §6.4); an unverified write into the store is worse than a missing one. Follow-up when the field is verified. |

## Tasks

- T1 (S1-S4): this spec. Route: parent. 
- T2 (S1+S2): sever the anchor — plugin-local glosses, engine-pinned names,
  cross-extension test reads deleted, comments rewritten. Test-first.
  Route: worker in tmp/wt-ocparity.
- T3 (S3): the port — every POSSIBLE mapping row with its tests; the table's
  final verdicts written back here. Route: worker.
- T4 (S1-S4): docs — the plugin's README and RESEARCH.md updated to the new
  shape (decoupled, what each surface owns). Route: worker.
- T5 (S4): verification — the opencode suite green from the worktree root,
  the pi extension suite still green (it shares the branch base), cargo
  agents green; a live probe only if an `opencode` binary is available
  headless. Route: parent.

## Log

- L1: "ok la de open code se dano dado a que vi que usaba parte de la logica
  de la menxis-memory que es la skill de pi y esta tiene logica tambien, aca
  hay que tomar una desicion separar la logica de la extension o hacer la
  logica en cada una dado a que cada programa trabaja distinto y tener la
  misma logica en un solo lugar puede limitar las posiblidades es mejor que
  cada una tenga la suya, ahora pon un agente a haga lo que ya hicimimos con
  la skill de pi pero en un worktree haz una rama apartir de esta para eso
  luego hacemos merge cuando ya funcionen las dos en pr aparte, para no
  mezclar, la skill de open code debera tener todas las funcionalidades que
  tenemos aca claro siempre y cuando sea posible se puede basar en la
  extension de pi para esto"
- L2: Coupling assessed (2026-11-03): no runtime imports across the trees; the
  coupling is the gloss anchor (comment at index.js:433-438 pointing at the pi
  SETTLE_NUDGE — already stale after ext-ts-rewrite T3 moved the glosses into
  mnexis_save's description) and test/core.test.js reading
  `../../mnexis-memory/index.ts` (re-anchored there by T3). Branch
  feat/opencode-parity created from feat/ext-ts-rewrite at 9f0a92f in worktree
  tmp/wt-ocparity.
- L3: T2 done (severed the anchor). `TYPE_GLOSSES` is plugin-owned; its seven
  NAMES are pinned by test/core.test.js to the engine's own `memory_type` docs
  in `src/mcp/tools.rs`; the sibling import and the two tests that read
  `extensions/mnexis-memory/index.ts` are deleted; the stale comments name the
  engine as the only shared contract. RED observed by renaming `preference` in
  the plugin and watching the pin fail, then restoring it. The plugin stays
  JavaScript (the TS rewrite was pi-side).
- L4: T3 done (the port). Verdicts are in the mapping table above; the plugin
  gained the settle question, per-call budgets, the busy retry, the deadline,
  the child registry, `session.deleted` handling and the engine's session-close
  semantics. Engine-side follow-up recorded, NOT done here: the engine's
  `memory_save` tool description (`src/mcp/server.rs`) names a few kinds in
  prose — "a decision, a discovery, a convention, a user preference", and
  `discovery`/`convention` are not `memory_type` values — and gives no examples.
  The seven canonical names are only in the `memory_type` argument's schema doc
  (`src/mcp/tools.rs`, via schemars). A sentence with the seven names and one
  example each would serve every host, including opencode 1, where the plugin
  has no catalog seat. That is a separate engine change (`src/mcp`), out of this
  branch's edit surface.
- L5: Deviations recorded: (a) opencode has no before-settle boundary, so the
  settle question rides the next model request's system part instead of the
  settle; (b) a successful shell call counts as "work that may have changed a
  file" — the plugin has no git baseline on the per-request path, so a read-only
  session that ran a shell command can be asked once; (c) the pointer's
  `memory_count > 0` condition is not applied, a bound namespace with zero
  memories still gets the pointer; (d) pi's "estado al cierre" record is written
  on the host's real end signal, not at unload — see L7.
- L6: T4 done: README and RESEARCH.md describe the decoupled shape (engine-pinned
  vocabulary, what this host owns, the ported features, every N-A with its
  reason).
- L7: Audit round (an independent audit of L3-L6, path:line evidence). Three rows
  of the mapping table overstated what shipped, and two cheap behavioral gaps
  were authorized as the only fixes:
  (1) **The table tells the truth.** R1 (the shutdown deadline) was claimed
  PORTED and was not (`index.js`'s `shutdown()` had no deadline); it is ported
  now (fix 3 below) and the row says so. The V1 half of the taxonomy row claimed
  "no catalog seat", contradicting RESEARCH.md §1 (`tool.definition` IS a V1
  hook): corrected to NOT IMPLEMENTED, and the gap — registering V1's
  `tool.definition` so a V1 model reads the curated descriptions — is recorded
  as a follow-up, NOT done here. The `promptGuidelines` row conflated two pi
  features; it is split: the conditional `<mnexis-memory>` section is PORTED as
  the pointer, per-tool guidelines are N-A (no seat renders a per-tool guidelines
  field on either model). README and RESEARCH §5/§8 repeated the same three
  claims and are corrected too.
  (2) **The reload skip and the close record.** pi skips its close when the
  shutdown reason is `reload` (`index.ts:2524`). Neither opencode model gives the
  unload a reason — V2's `setup` returns `Cleanup = () => Promise<void> | void`
  (`@opencode/plugin`'s `plugin.d.ts`) and V1's `dispose()` takes no argument —
  so a reload and a quit are the same event there, and the old end-on-unload
  killed a live session on every V2 hot reload (the engine never reopens an
  ended session). The unload now kills the in-flight children and ends nothing
  (`RELOAD_SKIP_REASON`, and the returned report says what was skipped); the
  close runs on the host's own end signal, `session.deleted`, where the
  workspace's one "estado al cierre" `project_state` is written from the
  `end --json` answer through the same `run()` path (write budget + busy retry).
  Known cost, stated in the README: a quit that never deletes its session leaves
  the row live, which the engine's verified start-after-end semantics make
  harmless.
  (3) **The shutdown deadline (R1) and the missing journal (3a).** The close's
  steps share one 5 s budget (`SHUTDOWN_BUDGET_MS` /
  `MNEXIS_OPENCODE_SHUTDOWN_BUDGET_MS`), a step that does not fit is abandoned,
  the abandonment is journaled, and the session stays tracked so the next close
  signal retries it under the same record title. The audit also found V2's
  `report()` was a no-op (no logger in the plugin context); `ctx.storage` is a
  real key/value store (`StorageDomain`: `get`/`set`/`remove`/`scan`,
  `@opencode/plugin`'s `storage.d.ts`), so a report is now written there as
  `mnexis.report.<kind>` — the abandonment is observable instead of swallowed —
  and the close's own report is also its return value.
  Verification: the opencode suite 70 pass + 1 opt-in skip + 0 fail (8 new
  tests), the pi extension suite 327/327, `cargo test agents` 24/24. A
  differential run of the base plugin against this one, over capture, pointer,
  compaction, gate, close and unload, differs only where fixes 2 and 3 intend it
  to (the close's record, and the unload's end).
