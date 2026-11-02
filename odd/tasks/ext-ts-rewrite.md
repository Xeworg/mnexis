# ext-ts-rewrite — the pi extension, redesigned on the full extension API, rewritten in TypeScript

The pi extension (extensions/mnexis-memory, 4604 lines of ESM JS) works but uses a
fraction of the extension API the official docs document. This feature redesigns it
against the full API surface (decision by decision), then rewrites it in TypeScript
with the accepted improvements, without changing the engine contract: the text the
model reads stays exactly what the CLI answers.

## Specs

- S1: "dejemos de lado el cambio que tenia estaba planeando" — the file-anchored
  memory queue (P0 pilot, `reason` field, analysis-doc commit) stays parked; nothing
  of it is lost, none of it is worked on here.
- S2: "quiero que disenes la skill primero analizando todas sus mejoras" — the
  design comes first: every capability of the pi extension API
  (https://pi.dev/docs/latest/extensions, read in full on 2026-11-02) is analyzed
  and decided APPLY or REJECT with a reason, before any code.
- S3: "para que luego las implementes reescribiendo a typescript" — the
  implementation is a rewrite of extensions/mnexis-memory to TypeScript; behavior
  the design does not change must survive the rewrite (suite green, parity intact).

## Design

### What the audit already confirmed applied (keep, do not redo)

promptSnippet on all 14 tools · custom renderers/chrome + footer status via
withStatus · abort-aware exec (kills the spawned process tree) · gate aware of
ctx.hasUI/ctx.mode (asks only in interactive TUI) · session journal via
pi.appendEntry (4 entry kinds) · the settle nudge on agent_before_settle (the
correct final actionable boundary) · session_compact journaling · busy-retry on
locked saves · no background resources in the factory.

### The decision table (full API surface → APPLY/REJECT)

| Capability | Verdict | Reason |
|---|---|---|
| `promptGuidelines` | **APPLY (A)** | 0 uses today. Rule bullets in the `<rules>` section of the system prompt, code-owned, every session — the same authority class as the injected protocols that displaced the skill three measured times (memory `adoption.injected-protocol-displaces-mnexis`). The cheapest lever against the adoption problem P0 wanted to measure. |
| `exposure: "deferred"` + built-in `tool_search` | **APPLY (B)** | 14 tools always declared; long descriptions pay tokens on every request. Defer the long tail; `tool_search` finds them; a guideline points at it. Availability risk is the falsifiable half (see Measurement). |
| `defaultActive: false` | **APPLY (B)** | `mnexis_session_start`/`mnexis_session_end`: in pi the extension itself runs the lifecycle (session_start handler + session_shutdown hook). Declaring them to the model pays tokens for calls the model should not make. Registered but inactive. |
| `executionMode: "sequential"` | **APPLY (C)** | Parallel calls contend on the SQLite lock; busy-retry rescued 8 losses in 30 bench runs but is a patch. Sequential is the doc's intended mechanism. Applied to mutating tools only (save, supersede, save_prompt, session_start, session_end); reads stay parallel (WAL allows them). Busy-retry stays: it also covers cross-process contention (bash-spawned mnexis) that sequential cannot. |
| `annotations` | **APPLY (D)** | readOnlyHint on the 9 read tools; mutating tools declare none-false honestly. Cheap, semantic, enables permission extensions. |
| `registerCommand` | **APPLY (E)** | A `/mnexis` status card: workspace key, bound namespace, read/write allowlists, session id + state, prompts captured, memories count, nudge state. QoL and the debugging surface for the adoption problem. Read-only. |
| TypeScript rewrite | **APPLY (F)** | The user asked for it. Zero-build: jiti loads .ts without compilation. Type-only imports of `ExtensionAPI`/`ExtensionContext` (erased at load, no npm install at runtime). Raw JSON-schema `parameters` objects stay (no new runtime deps, no typebox). |
| `registerMcpServer` | REJECT | Registering `mnexis mcp serve` would duplicate the surface without chrome, gate, or the nudge. MCP is opencode's path; the extension is pi's, by design. |
| `before_agent_start` prompt injection | REJECT | promptGuidelines is the designed mechanism; replacing the prompt per-run busts caches. |
| `turn_end` / `agent_settled` | REJECT | The nudge needs the actionable boundary (agent_before_settle), already used; agent_settled is notification-only. |
| `session_tree` state reconstruction | REJECT | Our tools are stateless (CLI calls); gate counters track the live session's mechanical work, not branch history. |
| `prepareArguments` | REJECT | No schema migration needed; schemas are stable. |
| `outputSchema`/`structuredContent` | REJECT | Text is the contract with the model; codemode scripts are not a consumer today. |
| `withFileMutationQueue` / `terminate` / `usage` / `namespace` grouping / `constrainedSampling` | REJECT | No file mutation, no early-stop, no nested LLM calls, no codemode listing need. |
| Timed dialogs, widgets, overlay, custom editor, autocomplete | REJECT | The gate asks once per namespace (already mode-guarded); footer status suffices; no evidence a widget earns its lines. |
| `resources_discover` / `project_trust` / provider & context hooks / `user_bash` | REJECT | Skill installation stays via `mnexis agent install`; trust is pi's; message filtering and prompt hooks are risk without benefit here. |

### A. promptGuidelines — the exact bullets (each names its tool)

On the active mnexis tools' definitions:

1. "At the start of a session in an existing project, call mnexis_context once
   with a one-line task summary before doing anything else."
2. "Before changing code that already exists, call mnexis_search or mnexis_show
   to check the decisions and discoveries already recorded about it."
3. "When you learn a durable decision, discovery, or user preference, record it
   with mnexis_save in the same turn."
4. "The mnexis engine has more tools (list, history, supersede, recall); use
   tool_search to find and enable them when a task needs them."

Bullet 4 exists to de-risk B (the deferred tail's discoverability).

### B. The loadout (14 tools)

| Tool | Today | Design |
|---|---|---|
| mnexis_workspace / mnexis_context / mnexis_search / mnexis_show / mnexis_save | active | **active** (the door + the loop) |
| mnexis_history / mnexis_list / mnexis_supersede / mnexis_suggest_topic_key / mnexis_recall / mnexis_search_prompts / mnexis_save_prompt | active | **deferred** (tool_search reaches them; bullet 4 names them) |
| mnexis_session_start / mnexis_session_end | active | **inactive** (`defaultActive: false`; the extension runs the lifecycle itself) |

Token effect: 14 declared schemas → 5 per request. The MCP surface (other hosts)
is untouched — this is the pi loadout only.

### Measurement (the falsifiable half of B)

The deferred tail is a hypothesis: tokens down, availability preserved. If a
future bench shows the model failing to reach history/supersede/recall when a
task needs them (tool_search + bullet 4 not enough), the fallback is re-activating
the specific tool. Not measured in this feature; recorded so the decision is
reversible with evidence.

### F. TypeScript, concretely

- `index.ts` + `lib/{cli,gate,workspace,chrome}.ts`, same module boundaries and
  behavior; `import type` only for pi types (erased; no runtime dep on the pi
  package, no npm install).
- Raw JSON-schema `parameters` stay as they are (typed locally).
- Tests: the 15 files move or stay JS importing the .ts modules through a
  loader — prefer node's native type stripping (`node --test
  --experimental-strip-types`, erasable syntax only: no enums, no namespaces)
  if the host node is >= 22.6; otherwise a dev-only `tsx` dependency. Parity
  tests keep spawning the real binary unchanged.
- The installer (`mnexis agent install --env pi`) must ship `.ts` files: the
  copy logic and its `install_never_ships_the_extension_tests` pin get a sibling
  test asserting `.ts` files land in the agent dir.

## Tasks

- T1 (S2): this spec + design document, committed on the feature branch. Route:
  parent inline.
- T2 (S3): TypeScript scaffold — rename modules to .ts, type-only imports,
  suite runs under the loader, installer ships .ts (test-first: loader runs RED
  on .ts imports, then GREEN; installer .ts test RED → GREEN). Route: parent
  inline. Risk: none.
- T3 (S2/S3): capabilities A–D (guidelines, loadout, sequential, annotations)
  with one test each proving the registered definition carries them, plus a
  loadout table test. Route: parent inline. Risk: none.
- T4 (S2/S3): the `/mnexis` status command (E) with its test. Route: parent
  inline. Risk: none.
- T5 (S2/S3): README of the extension + the pi skill's overlap note updated
  (the skill stays the advisory layer; guidelines are the code layer). Route:
  parent inline. Risk: none.
- T6 (S3): install on the host, live verification: markers present, session
  registered, prompt captured, /mnexis renders, declared tool count == 5.
  Route: parent inline. Risk: item 5 (installer + host environment) → own
  commit.

## Log

- L1: "si estoy viendo que hay muchos metodos utiles que pueden mejorar el
  funcionamiento de la skill con la reescritura dejemos de lado el cambio que
  tenia estaba planeando, quiero que disenes la skill primero analizando todas
  sus mejoras para que luego las implementes reescribiendo a typescript"
- L2: The file-anchored memory queue is parked by L1 (P0 pilot, `reason` field,
  analysis-doc commit). `odd/tasks/file-anchored-memory-analysis.md` stays
  untracked, untouched.
- L3: Audit evidence (2026-11-02): pi.dev extensions doc read in full
  (~157k chars); extension grep confirms promptSnippet 14/14, promptGuidelines
  0, executionMode/annotations/exposure/defaultActive 0, no registerCommand;
  abort-signal and hasUI/mode handling already correct. Recorded as memory
  `defect.extension-pi-gaps` (namespace mnexis).
