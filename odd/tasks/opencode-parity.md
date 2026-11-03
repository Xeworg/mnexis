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
against the ENGINE's canonical list (the `memory_type` enum in
`src/mcp/tools.rs`), never against the sibling extension. The gloss text is
plugin-local (the one-liners were authored on the pi side; each surface may
now evolve its own wording). The cross-extension parity test in
`test/core.test.js` (reading `../../mnexis-memory/index.ts`) is DELETED and
replaced by a local pin: the plugin's type list, read from its own file,
matches the engine's names. Stale comments pointing at the pi tree are
rewritten to name the engine as the source.

### The feature mapping (S3) — candidates the implementer confirms against RESEARCH.md

| pi capability | opencode possibility |
|---|---|
| Outcome-based settle nudge (counts successful saves; one-line ask + pointer to the taxonomy) | Port onto the plugin's existing shell/edit observation gate: count successful `mnexis_save` MCP calls, keep the one-ask latch, carry the pointer to the taxonomy's home |
| Taxonomy in the save description | The save description lives in the ENGINE's MCP schema (shared by both hosts) — plugin-side N/A; verify the engine's `memory_save` description carries the seven types (it names them; if the examples are wanted there too, that is an engine-side change, out of scope here, record it) |
| 5-tool loadout + deferred + tool_search + evidence activation | NOT POSSIBLE: the tools come from the engine's MCP server and opencode has no exposure/activation control — record as N/A with the reason |
| promptGuidelines / conditional `<mnexis-memory>` prompt section | Research: whether the V2 plugin hooks can add system/instructions content (RESEARCH.md §1); if yes, port the conditional-section idea (bound + has memories) with plugin-local state; if no, N/A with the reason |
| Prompt capture by the host | The plugin's pointer/state machinery (POINTER_ENV, chat.message) already touches this area; port the capture only if a message hook exists (RESEARCH.md "the two facts a memory integration needs"); otherwise keep the engine's CLI capture path and record it |
| Reliability: call budgets, per-command busy-retry, child registry | Port the budget/retry patterns to every CLI call the plugin makes; a child registry only if the plugin spawns (it mostly intercepts; verify) |
| Session lifecycle (registered start/end, idempotent close) | Port to the plugin's session hooks if V2 exposes them; the engine's verified semantics (start-by-id is a no-op; start-after-end never reopens) are the contract |
| Journal renderers, footer counters, /mnexis card | opencode UI has no equivalent — N/A; the plugin's pointer system is its own surface, extend it only if RESEARCH.md shows a hook |
| Gate with timeout/tri-state | The plugin's gate philosophy is inverted (it BLOCKS the model from granting; the human grants out of band) — keep the plugin's model, port only the timeout idea if dialogs exist |

The implementer fills this table's verdicts from RESEARCH.md and the V2
`@opencode/plugin` types, implementing every POSSIBLE row with tests and
recording every N/A with its reason in this document.

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
