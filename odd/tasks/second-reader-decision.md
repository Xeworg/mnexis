# The mechanism that reaches the 40/41 — the decision brief

Objective: give the human the evidence to decide **which** mechanism reaches the population
where the measured memory-quality gap lives — the sessions where the model **already saved**
(40 of 41), where the nudge never fires. Branch `docs/second-reader-decision` (worktree
`tmp/wt-docs`), based on `main` at `f93d473`. This unit is **research**: it produces a
decision brief, not a change. Test runner: none (no code changes).

## Specs

- **S1 (the decision, verbatim)** — "B1/B2 — decisión de producto sobre el mecanismo que
  alcanza el 40/41: el segundo lector (la extensión decide qué sobrevive: tres preguntas
  abiertas — doctrina, costo por sesión, slop) o la variante chica (el nudge pregunta
  \"guardaste X, ¿faltaba algo?\", más barata pero dispara en toda sesión con cambios)."
- **S2 (the second reader's own investigation, verbatim)** — "3. **The second reader, as its
  own feature with its own spec** — biggest lever (it is the only one that reaches the
  \"saved once, missed the second\" case) and the biggest shift (the extension, not the
  model, decides what to survive). It needs its own investigation: credentials (the 401
  class is measured), cost per session (it would fire on every session that changed files,
  not only the unsaved ones), and the slop risk (gated: ≤ A+2 records, zero change-log
  records)."
- **S3 (the three questions, answered with evidence)** — each of the three open questions
  the board names gets an answer with `path:line` evidence, the measured numbers that
  already exist, and what remains unmeasured: **doctrine** (what rule decides what survives
  — the second reader needs one, and the checklist bench says more description cannot move
  what survives), **cost per session** (how often it would fire and what it costs), **slop**
  (what it would write that the session did not establish, and what gate bounds it).
- **S4 (the cheap variant, measured)** — the variant that changes the nudge's gate so it
  asks "you saved X, is anything missing?" in every session that changed files: what it
  costs per session, how many sessions it would reach, and what it cannot do (it still
  cannot decide **what** was missed; it can only ask again).
- **S5 (the recommendation, honest)** — one recommendation with its price, the gates that
  would admit it, and the measurement that would falsify it. If the evidence does not
  settle it, the brief says so and names the one experiment that would.
- **S6 (the brief is durable)** — the brief lives in this document; the board
  (`odd/next.md`) points at it.

## The brief

The evidence, the answers and the recommendation. Every number below is quoted from the
file that carries it; where a number does not exist, it says so.

### The decision, in one line

Two mechanisms can reach the 40/41 — the sessions where the model **already saved**, where
the nudge is silent. The **second reader** (a settle-time reviewer model call that decides
what survives) is the only one that can decide *what* was missed; the **cheap variant** (the
nudge asks "you saved X, is anything missing?" in every session that changed files) can only
ask again. **The measurement says the cheap variant's mechanism is null**; **the second
reader's mechanism is unmeasured and its doctrine does not exist yet**.

### S3 — the second reader

**(a) Doctrine — there is no rule today, and every rule-shaped candidate that exists has
already been measured.**

| candidate source of truth | where | what the measurement says |
| --- | --- | --- |
| the nudge's own text | `extensions/mnexis-memory/index.js:456-472` (866 B, capped at 1 200 B by `test/settle-nudge.test.js:254-261`) | **null**: "The enrichment is **not distinguishable from the plain nudge on any gated metric**", at 1.14× cost (`tmp/bench-enriched/enriched-report.md`, E-G2/E-G3/E-G5) |
| the seven types | `src/domain/memory_type.rs:44-52` (default `fact` at `:58`, closed set validated `:76-90`) | **labels, not a rule** — the engine never infers. Per-type rules are inadmissible by measurement: G1 failed (`deploy` keeps `decision` in 7/10), so B2/B3 never ran (`odd/next.md:1361-1367`); round 2 stabilized the typing but collapsed the diversity — "the axis CAN be made stable by rules but does not discriminate" (`:1376-1379`). The store holds **zero** procedures (`:1384-1386`) |
| the engine's title/similarity | item 1c | **cannot separate** two titles of one fact from two titles of two facts (0.53–0.97 against 0.47–0.91) |
| the transcript-residue rejection clause | `odd/next.md:1449-1456` | **never measured**; its support is external (Bedrock's skip criteria are rejections; contrastive demos; +5.8 abs agreement on the holistic→atomic substitution) |
| the type distribution | `odd/next.md:1384-1386` | a **target** (procedure/knowledge), not a rule — and the marginal-fact gap is exactly the knowledge/procedure fact H1-G3 measured (`tmp/bench/trim-nudge-report.md`) |

So the rule a reviewer would apply has to be **rejection-first and atomic** — two YES/NO
questions over a *digest* (not the transcript), with enumerable local rejects (the diff, the
tool output, a restatement of the prompt) and an accept that must be checkable against the
digest alone. That is the checklist's shape, and the checklist has **never been measured**: the
bench fired the nudge in **2 of 41** sessions against a pre-registered ≥4/10
(`tmp/wt-bench3/tmp/bench/checklist-measurement.md:369`).

**Where it would live:** next to `SETTLE_NUDGE` (`index.js:456`), because the vocabulary is
already a duplicated, test-pinned pair — the opencode plugin carries `TYPE_GLOSSES` verbatim
(`~/.config/opencode/plugins/mnexis-memory.js:433-449`) and two tests pin the two copies
(`extensions/mnexis-opencode/test/core.test.js:266-271`, `test/plugin.test.js:635-640`).
**Caveat:** the opencode plugin has **no settle seat at all** (prompt capture `:755`, session
start `:876-881`, compaction `:777-786` and `:899-903`), so a rule added there is a third copy
with no firing path.

**(b) Feasibility — yes, and this is the load-bearing fact.**

- `docs/extensions.md:118` — "`turn_end` and `agent_before_settle` are actionable boundaries.
  Their handlers can chain proposed `custom`, `custom_message`, `context_edit`, or `compaction`
  entries and **return `continue: true` for one next model request**. Guard continuation
  conditions because an unconditional continuation can loop."
- `docs/extensions.md:215` — "Use `ctx.modelRegistry.streamSimple()` for provider-neutral
  nested model calls."
- `docs/extensions.md:5` — "An extension runs inside the Pi process with the same
  operating-system permissions. It can inspect prompts, tool calls, files, **credentials**,
  and session history." `docs/extensions.md:137` — nested calls must report their `usage`;
  `:126` — `ctx.signal` for nested work owned by an active turn.
- **What it receives:** `AgentBeforeSettleEvent extends BoundaryState` = `{entries, continue,
  context, outcome}`, and `context` is `BoundaryContextPreview` = `{contextEntries,
  contextMessages, llmMessages, pendingMessages, canContinue}`
  (`@earendil-works/pi-coding-agent/dist/core/extensions/types.d.ts:779-799`). The digest the
  board cites exists: `event.context.llmMessages`.
- **What it can return:** only `BoundaryResult = {entries?, continue?}` (`types.d.ts:792-795`).
- **What it cannot do:** it cannot block, revert, edit or delete what the session model
  already saved; it cannot edit the session's messages; and it cannot run after the settle
  (`agent_settled` is "final and notification-only", `docs/extensions.md:68`).
- **The working precedent** is the cited example: `examples/extensions/custom-compaction.ts:29-97`
  — `ctx.modelRegistry.find("google","gemini-2.5-flash")` then
  `ctx.modelRegistry.complete(model, {messages}, {maxTokens, signal, cacheRetention: "none",
  sessionId: uuidv7()})`, returning `usage` with the entry (`:88-96`). **Caveat:** that hooks
  `session_before_compact`, whose event carries `event.preparation` (`messagesToSummarize`,
  `turnPrefixMessages`, `tokensBefore`, `firstKeptEntryId`, `previousSummary`) — a **different
  event shape** from `agent_before_settle`. The extension's current handler ignores its event
  entirely (`async (_event, ctx)`, `index.js:1917`), so the digest is available and unused today.

**(c) Credentials and cost.**

- **Credentials:** the reviewer would use the session's own provider through
  `ctx.modelRegistry` ("Model registry for API key resolution", `types.d.ts:224-225`). Today
  the extension needs **no** provider credential: every call spawns the local `mnexis` CLI
  (`index.js:2082`, `extensions/mnexis-memory/lib/cli.js`). The second reader introduces the
  extension's **first** provider dependency.
- **The measured 401 class** (`odd/tasks/trim-nudge-bench.md:58-71`): "every pre-flight session
  died with `401` from provider `opencode-go` in ~1.1 s (zero model turns)", root-caused to a
  **stale environment key** (two `oc_sk_…` keys; the env's sha256[:12] `0594eb858387` rejected,
  the live one in opencode's credential DB `7946d6f2eee3`), and "the invalid 401 pre-flights are
  NOT measurements". Resolved 2026-11-01 with a fresh key. **The 401 *rate* is not measured** —
  and a settle-time reviewer has no user present to rotate a key, so a silent 401 is a silent
  loss of the write.
- **Measured bills:** the enriched bench — $0.002931 (nonudge) / $0.002705 (plain) / $0.003094
  (enriched) per run; the trim bench — $0.002817 / $0.002373 / $0.002205 with 11.0 / 8.9 / 9.6
  model calls per run; the checklist pilot — 41 sessions for **$0.1103** and **442 model calls**
  ≈ $0.00269 per session ≈ 10.8 calls per session ≈ **$0.00025 per model call** (derived).
- **The reviewer's own cost is NOT measured.** Nothing in the repository sizes a digest, a
  reviewer prompt or a reviewer arm. What would have to be measured: the digest's bytes per
  session, one reviewer call's billed tokens, and the fire rate.
- **How many sessions would fire:** with the files-changed gate fixed, **~41 of 41** — every
  pilot session changed the repo — against **2/41** today. The reviewer's fire population is
  about **twenty times** the nudge's, and it pays in the 40/41 where the nudge costs nothing.

**(d) Slop.**

- The enriched doctrine text produced 12 saves (`decision×2, knowledge×8, episode×1,
  project_state×1`) with E-G4's human read finding **zero fabricated facts** at n=10 — but
  that text was still filtered by the session model. A reviewer that writes by itself removes
  that filter: **unmeasured**.
- The known failure mode the board names is **change-log records**: paths and diff-shaped
  inputs "invite `episode` saves" (`odd/next.md:1438-1441`).
- **A concrete duplicate hazard:** the extension already writes a code-written `project_state`
  titled `estado al cierre` at shutdown (`index.js:404`, `SESSION_CLOSE_TITLE`), and "the
  title is the only handle that tells a code-written record from a model-written one"
  (`tmp/bench/trim-nudge-report.md`, pre-flight finding 3). A reviewer free to write
  `project_state` collides with that seat unless it is told not to.
- **The bounds that already exist:** `idempotency_key` ("Makes retries safe", `index.js:2069`);
  `topic_key` (`:2061`) and the title rule ("the same title always lands on the same record,
  so a changed fact updates it", `:2041-2044`), which turn a repeat into an update;
  `keywords` accumulate (`:2055-2060`); the store's type set is closed
  (`src/domain/memory_type.rs:44-52`, `:76-90`).
- **The board's gate** is **≤ A+2 records, zero change-log records** (`odd/next.md:1476`); the
  checklist's sealed criterion was tighter (records/run ≤ A+1, zero fabricated facts,
  `checklist-measurement.md:231-236`). The pilot writes **1.37 saves per session**
  (`checklist-measurement.md`, A.3): a "+2" bound would roughly double the write volume of the
  measured population.

### S4 — the cheap variant

**(a) The gate, exactly.** `extensions/mnexis-memory/index.js:1929`:

```
if (settleNudgeSent || !sessionFilesChanged || sessionSaveCalls > 0) { return undefined; }
```

`sessionFilesChanged` is set for a successful `write`/`edit` only (`:1899-1907`,
`FILE_WRITING_TOOLS` at `:475`), widened by the workspace-state comparison (`:1920-1928`);
`sessionSaveCalls++` on any `mnexis_save` call (`:1909-1915`); both reset at `:1743-1746`; the
single fire returns one `custom_message` and `continue: true` (`:1930-1944`). The variant edits
**exactly one clause**: `sessionSaveCalls > 0`.

**(b) What it would cost.** The nudge is 866 B, capped at 1 200 B by
`test/settle-nudge.test.js:254-261`, and it is priced **per fire as one extra model request**
(`index.js:1895-1896`: "`continue: true` is one more model request"; `:1893`: "no entry, no
continuation"). The measured price of the nudge *text* is the enriched bench's **+14 % per run**
($0.003094 against $0.002705, 1.14×, E-G5) — paid only in the firing runs (5/10). With a
files-changed gate, that premium moves from **2 sessions in 41 to nearly all of them**, and
from ~zero (a silent nudge costs nothing) to **one extra request per session**. The absolute
money for one more call is **not measured**; the derived figure from the pilot is ≈$0.00025.

**(c) What it cannot do.** It asks the session model to re-open a judgment it already closed:
"the nudge fires **after** the model has already concluded its judgment of the session, so it
is not eliciting a judgment — it asks the model to re-open one… **more description cannot
help; a decision rule the model did not already have can**" (`odd/next.md:1457-1462`). And for
the case that matters most, "**no wording can fix that**" (`:1446-1448`) — which is why the
trim bench's H1-G3 gap (the marginal fact 5/10 against 1/10) sits precisely in the 40/41
population the nudge is silent in (`:1512-1518`). The variant does not decide *what* was
missed: it re-asks the model that just decided nothing else deserved memory, in the same
context, and the enriched measurement already showed that adding words to that question moves
no gated metric at +14 % cost.

### S5 — the recommendation, and what would falsify it

**The cheap variant is dominated by the measurement.** It buys a bigger population for a
mechanism that has already been measured null (more description moves nothing), at a cost that
goes from ~zero to per-session. Shipping it is a bet against the project's own evidence.

**The second reader is the only lever that reaches the 40/41 *and* can decide what was missed**,
but three of its four prerequisites are missing: the doctrine (no rule exists), the credential
path (the extension's first provider dependency, with a measured 401 class and no user present
to rotate a key), and the cost (never measured).

**So the decision is not "which of the two". It is: the second reader is the mechanism, and
its missing piece is the doctrine.** The cheapest honest next step is therefore not a
mechanism but a **rule, written and measured in the harness that already exists** — the same
harness, the same gates, the same fixture, with the reviewer's arm measured against the
session model's own rate on the marginal fact. That bench is running now (the checklist arm,
`bench/checklist-nudge`), and the second reader is the natural fourth arm of it.

**The gates the second reader must pass, pre-registered before any run:**

1. **Reach.** It fires in the 40/41 population, not the 2/41 one — the population is the whole
   point, and it is the only mechanism that can be there.
2. **The marginal fact.** It records the knowledge/procedure fact the session model missed in
   **≥ 2×** the session model's own rate and in **≥ 4/10** sessions — the checklist's sealed
   C-G2 gate, applied to the population that actually holds the gap. A reviewer that fires
   everywhere and records nothing new is a cost, not a mechanism.
3. **No slop.** ≤ A+2 records per session, **zero** change-log records, zero fabricated facts,
   and no collision with the code-written `estado al cierre` seat.
4. **The cost, bounded and stated.** One reviewer call per changed session, with the digest's
   size and the call's billed tokens **measured** (neither exists today), and a 401 that
   **fails loudly** instead of silently losing the write.
5. **The axis stays honest.** The records it writes carry a type the seven-kind set accepts,
   and the human read of every record it writes says the session established it.

**What would falsify the second reader:** the same bench, with the reviewer as an arm, failing
C-G2 — the reviewer recording the marginal fact no more often than the session model does,
once it is in the 40/41 population with the digest in front of it. At that point the
mechanism is dead, the axis stays a label, and the honest product is the cheap door (the
priced answer) plus the discipline the skill teaches.

**The questions that are the human's, and cannot be answered by evidence alone:**

- **How much is a session's memory worth?** One extra model request per changed session is a
  budget decision, not a technical one. The measured reference points: ≈$0.00025 per model
  call (derived), ≈$0.00269 per session for the session itself.
- **Whose credential?** The session's own provider through `ctx.modelRegistry` (no new key,
  but it inherits the 401 class and the host's provider policy) or the extension's own key
  (a new secret to manage and rotate, and the same 401 class with nobody watching).
- **Who decides when the reviewer is wrong?** The hook cannot block or revert what the session
  model saved. A reviewer that writes a wrong record leaves a wrong record in the store, and
  the doctor's repair is the only thing that takes it out.

## Tasks

| ID | Specs | Route | Branch / worktree | Commit |
| --- | --- | --- | --- | --- |
| T1 | S1–S4 | delegated: `gentle-ai-explore` (read-only) — the evidence for the three questions and the cheap variant, `path:line`. **Done**: the evidence is in the brief above | `docs/second-reader-decision` @ `tmp/wt-docs` | — |
| T2 | S1–S6 | inline (parent): the brief, written from the explorer's evidence. **Done**: the brief, the recommendation and the five gates | `docs/second-reader-decision` @ `tmp/wt-docs` | — |

## Log

- **L1 (2026-11-02, board — the open item B, verbatim)** — "B — decisión de producto sobre el
  mecanismo que alcanza el 40/41: el segundo lector (la extensión decide qué sobrevive: tres
  preguntas abiertas — doctrina, costo por sesión, slop) o la variante chica (el nudge pregunta
  \"guardaste X, ¿faltaba algo?\", más barata pero dispara en toda sesión con cambios)."
- **L2 (2026-11-02, human, verbatim)** — "usaremos worktrees y trabajaremos varios al mismo
  tiempo". The human did not pick between the two mechanisms; this unit exists so the
  decision can be made on evidence rather than on a summary.
- **L3 (2026-11-02, the explorer's evidence, in)** — the three questions, answered with
  `path:line`: **doctrine** — no rule exists; every rule-shaped candidate in the repository has
  already been measured (the nudge's text null at 1.14× cost, the seven types labels and their
  per-type rules inadmissible, the similarity unable to separate one fact from two), and the
  only rule-like candidate — the checklist — has never been measured. **Feasibility** — yes:
  `agent_before_settle` is an actionable boundary (`docs/extensions.md:118`), `ctx.modelRegistry`
  makes a provider-neutral nested call (`:215`), the digest is `event.context.llmMessages`
  (`types.d.ts:779-799`), and the hook can only return `{entries?, continue?}` — it cannot block
  or revert what the session model saved. **Cost** — the reviewer would fire in ~41 of 41
  sessions (≈20× the nudge's population), the extension would gain its first provider
  dependency, and the reviewer's own cost is **not measured**; the measured 401 class is a stale
  key, and the 401 *rate* is not measured either.
- **L4 (2026-11-02, the recommendation)** — the brief's S5: the cheap variant is dominated by
  the measurement (bigger population, mechanism already measured null, cost from ~zero to
  per-session); the second reader is the only lever that reaches the 40/41 **and** can decide
  what was missed, but its doctrine does not exist yet. So the decision is not "which of the
  two" — it is that the second reader is the mechanism and the **rule** is the missing piece,
  to be written and measured in the harness that already exists. Five gates are pre-registered
  in the brief, and the falsifier is named: the reviewer arm failing the checklist's own C-G2
  gate in the 40/41 population.
- **L5 (2026-11-02, the structure, stated)** — the brief lives in this document, between the
  `## Specs` and the `## Tasks`, so the canonical order (header, Specs, Tasks, Log) is kept and
  the log stays last.
