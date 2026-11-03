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

## Amendments (2026-11-02 — three-lens explore review)

Three read-only explores reviewed the installed pi docs, the extension, and this
design: agent-behavior (mvzwxi8x-1-px76), reliability (mvzx0xps-2-r976),
human-UX (mvzx55bn-3-o3tm). Evidence is path:line in the extension or the
installed pi dist. Verdicts:

| # | Finding | Verdict |
|---|---|---|
| AB1 | `deferred` without an active `tool_search` = 7 dead ends: the call fails before any handler (`Tool X not found`), `tool_result` never fires, and the miss is invisible. `tool_search` registers `defaultActive: false` and nothing activates it for third-party extensions (only pi's MCP extension does, for its own). | **ACCEPT — blocking for B**: activate `tool_search` at `session_start` when `pi.getAllTools()` finds it (as pi's MCP extension does) + a reachability test. Never rely on host settings. |
| AB2 | The tools' own answers imply tail tools (context returns uuids + left-out count, index.js:2690-2790; search says "read one with mnexis_show"). | **ACCEPT**: context/search/save activate the 1-2 implied tail tools via `setActiveTools` inside `execute` (sticky per branch, one delta, no search round trip). |
| AB3 | The `before_agent_start` REJECT rests on a false premise: `systemPromptOptions` is mutable and pi diffs sections (agent-session.js:1308-1313) — cache-safe. Only returning `systemPrompt` busts caches. | **ACCEPT as A's complement**: one `<mnexis_memory>` section via `systemPromptOptions.sections`, emitted only when the workspace is bound AND has memories; never return `systemPrompt`. promptGuidelines stay the minimal always-on baseline. |
| AB4 | The 4 bullets contradict the surface: two competing FIRSTs (workspace's own description says "Call this FIRST", index.js:2665), "search or show" is a dead-end verb (show needs a UUID the model does not have), "same turn" contradicts the settle-nudge timing, and naming 4 hidden tools invites AB1's dead end. | **ACCEPT**: one start verb (`mnexis_context`; demote workspace's FIRST), bullet 2 → "search, then show the hit", bullet 3 → "before reporting the task done", bullet 4 teaches the verb (`tool_search`), not hidden names. |
| AB5 | The survival taxonomy (examples + 7 type one-liners) lives only in SETTLE_NUDGE (index.js:456-469); save's description carries bare names (index.js:2064-2078) — taught at the worst moment, once. | **ACCEPT**: move the taxonomy into `mnexis_save`'s description; shrink the nudge to one line + pointer. |
| AB6 | The nudge counts attempts (`tool_call`, index.js:1909-1914), not outcomes: a refused or failed save silences the only ask. | **ACCEPT**: count successful saves from `tool_execution_end` (`isError === false`); a denied save keeps the question open once, with the gate's reason in the text. |
| AB7 | `mnexis_save_prompt` double-writes: the input hook already captures every prompt (index.js:1858-1885); `deferred` keeps the tool tool_search-reachable → duplicate rows. | **ACCEPT**: register it `hidden` in pi (unreachable AND unsearchable); unchanged in the MCP surface. |
| AB8 | No `tool_result` teaching after the fact. | **DEFER**: superseded by AB2 (activation removes the dead ends the pointers patched); revisit with bench evidence of misses. |
| R1 | `session_shutdown` is awaited with NO timeout on /new, /resume, /fork, quit (agent-session-runtime.js:106-110); the close can spend ~60 s (end + save, each 30 s, lib/cli.js:26,57-63). | **ACCEPT**: wall-clock budget (~5 s) on shutdown work, journal the abandonment, keep the close re-runnable. |
| R2 | Busy-retry is keyed to save identity (lib/cli.js:101-130,467); `sessionStart/End`, `savePrompt`, `bind/allowNamespace` and the untitled compaction save never retry. `executionMode: "sequential"` serializes tool calls only — not hooks, other processes, or bash-spawned mnexis. | **ACCEPT**: retry becomes a property of the command (idempotent-by-construction commands always retry; only an untitled `save` must not). |
| R3 | Session identity is not a state machine: `sessionStart` fires on every reason incl. reload (index.js:1740-1775) with the error swallowed; start-after-end on resume of a quit-closed session; fork ends the source id the human can reopen. | **ACCEPT**: first verify the engine's answer to the four sequences (parity-style test against the binary); make start = upsert-by-id and end = idempotent where the engine disagrees; journal (never swallow) failures. |
| R4 | Compaction writes untitled (index.js:1985-1997) → one new row per compaction, never deduped; `session_compact_failed` invisible; branch summaries never captured. | **ACCEPT**: stable title/topic key per session (revises instead of accumulating); journal `session_compact_failed`; branch summaries stay REJECTED, now explicitly recorded. |
| R5 | The gate requires `mode === "tui"` (lib/gate.js:177,217) — RPC supports dialogs (`ctx.hasUI` true); the refusal text is false there and IDE clients can never authorize. | **ACCEPT**: gate on `ctx.hasUI` (tui + rpc), keep terminal-only chrome on `mode`; dialog `timeout`; tri-state refusal text (declined ≠ timed out). |
| R6 | No `session_tree` handler: the nudge latch leaks across branches (suppression or false fire after /tree). | **ACCEPT**: reset the evidence trio on `session_tree`; journals documented as branch data. |
| R7 | The input hook awaits a 30 s write on every user message (index.js:1858-1896). | **ACCEPT**: dedicated 1-2 s budget, journal abandonment; the capture never blocks the turn. |
| R8 | Hook calls carry no signal; children spawn detached (lib/cli.js:196-210) and can outlive the session, finishing writes into a closed session and holding the lock. | **ACCEPT**: session-scoped child registry, killed idempotently on shutdown; hooks get a registry signal. |
| UX1 | The 4 `appendEntry` journal kinds render NOTHING in chat without `registerEntryRenderer` (interactive-mode.js:3134-3138) — while index.js:1310-1312 and README.md:210-214 claim visibility. | **ACCEPT — overturns the by-omission rejection**: one entry renderer, collapsed line + expandable detail, 4 kinds; survives resume and exports. |
| UX2 | Adoption has no instrument: footer status exists only inside a tool execute (lib/chrome.js:296-339); a session with zero reads shows nothing at all. | **ACCEPT**: keyed footer counters set at `session_start`, updated on `tool_call` (`ctx/search/save`), always visible — the product's core question answered before the human asks. |
| UX3 | `/mnexis` as `notify` is a dim overwritten line for a 7-field card. | **ACCEPT (card)**: `appendEntry` + the UX1 renderer (durable, expandable) + `getArgumentCompletions` subcommands. **DEFER (grant path)**: letting /mnexis perform bind/allow touches the permission surface — its own decision, not folded into the rewrite. |
| UX4 | The settle nudge is `display: false` with no renderer — indistinguishable from agent spontaneity; the human cannot tell fired/fired-wrongly/never-fired. | **ACCEPT**: `display: true` + `registerMessageRenderer`, one collapsed line, expandable to the nudge text. |
| UX5 | `renderShell: "self"` opts out of error background, padding, and the expand keyHint (tool-execution.js:53-55,199-213; lib/chrome.js:304-364 ignores theme/outputPad). | **ACCEPT**: revert to the default shell (semantics for free); Panel stays as renderResult content; manual visual check in T7. |
| UX6 | The gate's blocking confirm has no timeout, and yes/no over a five-line paragraph. | **ACCEPT (timeout ~90 s + tri-state per R5)**. **DEFER (select shape)**: recorded for its own pass. |
| UX7 | `isPartial` wipes the call line with "…" (lib/chrome.js:258). | **ACCEPT**: keep the call line, append a state suffix. |
| UX8 | The audit table omitted `registerEntryRenderer`, `registerMessageRenderer`, `notify`; the UI REJECT row bundled five mechanisms in one sentence. | **ACCEPT**: fixed by this amendment; widgets, overlay, custom editor, and the autocomplete provider stay REJECTED with the split recorded. |

## Tasks

- T1 (S2): this spec + design document, committed on the feature branch. Route:
  parent inline.
- T2 (S3): TypeScript scaffold — rename modules to .ts, type-only imports,
  suite runs under the loader, installer ships .ts (test-first: loader runs RED
  on .ts imports, then GREEN; installer .ts test RED → GREEN). Route: parent
  inline (worker-executed under the context backstop; parent closed two
  out-of-surface references). Risk: none. Commit: 8ae9fbb.
- T3 (S2/S3): the agent-behavior set — AB1 guarded `tool_search` activation +
  reachability test, AB2 evidence-driven activation, AB4 bullet rewrite, AB5
  taxonomy into save, AB6 outcome-based nudge, AB7 save_prompt hidden, amended
  loadout B (5 declared + tool_search). Route: parent inline (worker-executed
  under the context backstop). Risk: none. Commit: bd23db8.
- T4 (S2/S3): the reliability set — R1 shutdown budget, R2 per-command retry,
  R3 session state machine (verify engine first, then upsert/idempotent + the
  four sequence tests), R4 compaction stable title + compact_failed journal,
  R5 RPC gate + tri-state, R6 session_tree reset, R7 input budget, R8 child
  registry. Route: parent inline (worker-executed under the context backstop).
  Risk: none. Commit: 5cd9792.
- T5 (S2/S3): the human set — UX1 entry renderer (4 kinds), UX2 footer counters,
  UX3 /mnexis durable card + completion, UX4 nudge visible, UX5 default shell,
  UX7 partial line, AB3 conditional `<mnexis_memory>` section. Route: parent
  inline. Risk: none.
- T6 (S2/S3): docs — README truth fixes (journal visibility claims), decision
  table completeness, the pi skill's overlap note. Route: parent inline. Risk:
  none.
- T7 (S3): install on the host, live verification: markers present, session
  registered, prompt captured, /mnexis renders, declared tools == 5 and
  tool_search reachability, journal lines visible in the transcript, footer
  counters live. Route: parent inline. Risk: item 5 (installer + host
  environment) → own commit.

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
- L4: "bien antes de seguir pon 3 agentes a leer la documentacion que di y
  analicen la extension de distintos puntos, para que te propongan mejoras o
  punto importantes que deberiamos tener en cuenta"; "usa explorers".
- L5: Three explore reports received (tasks mvzwxi8x-1-px76,
  mvzx0xps-2-r976, mvzx55bn-3-o3tm), read-only, path:line evidence; their
  substance is carried by the Amendments table.
- L6: Parent verdicts on all 24 findings: 21 ACCEPT (folded into T3-T5), 3
  DEFER with recorded reasons (AB8 teaching pointers — superseded by AB2;
  UX3 grant path — permission surface; UX6 select shape). B is amended by AB1:
  without guarded `tool_search` activation the deferred tail is unreachable
  and the misses invisible.
- L7: T2 evidence (8ae9fbb): node v22.22.2 strips types unflagged — plain
  `node --test` loads the .ts modules, no loader flag and no new dependency;
  the extension suite is invocation-cwd-sensitive (running it from the
  extension dir produces a FALSE failure in tools.test.js: the bind question
  names the repo root while the fake ctx names the invocation dir) — canonical
  invocation is from the repo root with MNEXIS_PARITY_BIN set; record it in
  T6's docs. The rename broke two references outside T2's planned surface,
  both fixed by the parent: src/tui/app.rs:1356 (an install assertion on
  index.js) and extensions/mnexis-opencode/test/core.test.js:17,262 (the
  sibling suite imports the pi extension). Suites: extension 243/243,
  opencode 0 fail (1 pre-existing skip), cargo all green (0 failed).
- L8: T3 evidence (bd23db8): extension suite 257/257 (+14 new tests: guarded
  activation, loadout table, sequential/annotation properties, taxonomy move,
  outcome-based nudge, hidden save_prompt), opencode 47/0, cargo agents 23/0.
  Parent ratifications: (1) the prose-budget pin now measures DECLARED tools
  (target 3 337 B against the audit's 5 500 B all-registered basis) — ratified
  because the budget guards the per-request cost and only declared tools pay
  it; the test's comment carries the reason. (2) pi emits an aborted tool call
  as isError:true in tool_execution_end (there is no abort flag), so AB6's
  "not aborted" is satisfied by isError alone — remember this in T4/T5.
  (3) the seven-gloss parity between the opencode plugin and the pi surface
  now anchors on mnexis_save's description; the nudge is one judgment line +
  pointer. Slice strategy recorded: each T-task is one work-unit commit and a
  slice boundary (T3 alone: 985 lines, ~60% tests). The live host still runs
  the OLD installed extension — the new loadout and nudge land at T7's
  install.
- L9: T4 evidence (5cd9792): extension suite 288/288 (+31), opencode 47/0,
  cargo agents 23/23. The ENGINE's session state machine, verified parity-style
  (no Rust change needed): a second start with the same id is a no-op (no
  error, no reopen); start-after-end never reopens; interleaved sessions are
  independent; `end` of an unknown id exits 3; a second `end` keeps the first
  timestamp. Decisions: the journal keeps its four entry KINDS and grows
  VARIANTS (action "noted"/"abandoned", step "start"/"end"/"save",
  not_registered; compaction gained action "failed", aborted compactions stay
  quiet) — T5's renderer must read action/step, never assume "skipped"; no
  `mode` remains anywhere — asking keys on ctx.hasUI only; the close kills the
  child registry BEFORE the budget work and on "reload" too; the deadline
  race returns the hook while the per-call timeout kills the child; the
  tri-state needed the extension's own AbortSignal because ui.confirm resolves
  false on decline, timeout AND abort. NEW DEFECT queued into T5 (found by T4,
  deliberately untouched): ensureWritable and bindOnConfirmation pass the long
  question as ui.confirm's TITLE and the short label as MESSAGE — swapped
  against pi's confirm(title, message) signature.
