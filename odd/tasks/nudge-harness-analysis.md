# The nudge's harness — the analysis, again, with everything now measured

Objective: redo the three-axis analysis of the settle nudge — CONTEXT, HARNESSES, AI
TECHNIQUE — with the evidence the last two sessions produced, aimed at **improving the harness
that carries a rule to the model at the settle boundary**. Branch
`docs/nudge-harness-analysis` (worktree `tmp/wt-nudge-analysis`), based on `main` at `4d8fb45`.
This unit is **research**: it produces a set of proposals with their falsifiers, not a change.

## Specs

- **S1 (the question, one line)** — *what harness could deliver a rule to the model at the
  settle boundary, in the population where the measured memory-quality gap actually lives
  (the sessions where the model already saved), without adding description?*
- **S2 (three axes, one proposal each)** — the same three axes as the previous round
  (CONTEXT, HARNESSES, AI TECHNIQUE), each answered by its own read-only agent, each
  returning **one** improvement to the nudge's harness, with its falsifier and its cost.
- **S3 (the dead ends, named, so nobody re-proposes them)** — the proposals of the previous
  round are **closed by measurement**, and the new proposals must not be them again:
  - **More description is null.** The enriched nudge (866 B: three survival examples + the
    seven type glosses) was **indistinguishable from the plain nudge on every gated metric**,
    at **1.14× cost** (`tmp/bench-enriched/enriched-report.md`, E-G2/E-G3/E-G5).
  - **The checklist cannot be measured as written.** The pilot proved why: the model **audits
    the tool it is asked to trust** (read `/usr/local/bin/ingest-check` in **10/10** sessions,
    probed it with a violating value in **5/10**) and then **records what it found in 10/10**
    (`odd/tasks/checklist-bench.md` L5; Appendix B of
    `tmp/wt-bench3/tmp/bench/checklist-measurement.md`). The fact's survival is already at the
    **ceiling**, so the gate's "≥ 2×" clause is unreachable and the measured batch was
    deliberately unspent.
  - **The nudge's population is tiny by construction.** It fires in **2/41** and **1/10**
    measured sessions, because the model saves its salient fact **before** the settle boundary
    in **40/41** sessions — so the nudge is silent in exactly the population where the trim
    bench's marginal-fact gap lives (H1-G3: 5/10 against 1/10).
  - **Naming the files** (the previous CONTEXT proposal) was predicted null by its own author's
    risk list, and it was never measured.
- **S4 (the bar every proposal must clear)** — a proposal is admissible only if it:
  1. **reaches** the sessions where the model already saved (the 40/41), not the ones where it
     saved nothing;
  2. **does not depend on more description** in the prompt (measured null);
  3. carries a **rule that is not the checklist** — and says why the checklist's shape fails
     now (the ceiling, not the wording);
  4. can be **measured** with the harness that exists (podman + pi, and the opencode host
     whose feasibility is being investigated), with a criterion written before the runs;
  5. states its **falsifier** — the measurement that would kill it.
- **S5 (the deliverable)** — one document with the three proposals side by side, each with:
   the mechanism, the evidence it rests on (`path:line` or a measured number), the population
   it reaches, the cost, the falsifier, and **what it would take to measure it**. Plus a
   synthesis: which of the three (if any) is worth building, and what the other two would have
   to be true for.
- **S6 (the honest frame)** — the board's own lesson cuts both ways: *more description cannot
  move what survives* (the enriched nudge, null at 1.14× cost) and *a decision rule the model
  did not already have can* (the untested hypothesis). The second half has never been
  measured, and the pilot just showed that the rule's own baseline can sit at the ceiling. So a
   proposal that cannot say **what its rule is** is not admissible.

## The shared context every agent gets

The nudge today (`extensions/mnexis-memory/index.js`): `SETTLE_NUDGE` (866 B) fires at the
settle boundary, only when the session **changed files** (a `write`/`edit` tool call, or — since
`4865f51` — the workspace's own git state changed since the session started) **and** the session
made **no** `mnexis_save` call. It is one extra model request (`continue: true`), and its text
is pinned under 1 200 B (`extensions/mnexis-memory/test/settle-nudge.test.js:254-261`). The
gate is at `index.js:1929`. The opencode plugin has **no settle seat** (only prompt capture,
session start and compaction), so the nudge is a pi-only mechanism today.

Sources to read (the board first): `odd/next.md` (the three explorations of the settle nudge;
the checklist bench's outcome), `odd/tasks/nudge-enriched.md`, `odd/tasks/checklist-bench.md`
(L4 and L5), `odd/tasks/second-reader-decision.md` (the brief: the doctrine gap, the
feasibility facts, the five gates), `extensions/mnexis-memory/index.js` (the nudge, the gate,
the settle handler), the pi docs at
`/home/xeworg/.npm-global/lib/node_modules/gentle-pi/node_modules/@earendil-works/pi-coding-agent/docs/extensions.md`
(hooks: `:118` `agent_before_settle`, `:215` `ctx.modelRegistry`, `:5` what an extension can
inspect) and `.../examples/extensions/custom-compaction.ts` (the nested model call). Artifacts:
`tmp/bench-enriched/enriched-report.md`, `tmp/bench/trim-nudge-report.md`,
`tmp/wt-bench3/tmp/bench/checklist-measurement.md` (Appendix B is the pilot). `tmp/` is
gitignored: read those paths explicitly.

## The three briefs

### Axis 1 — CONTEXT: *what the agent can see, and what it must see, at the settle boundary*

The nudge asks the model to re-open a judgment with **no new information** — it re-describes
the session it just closed, and that is measured null. The extension, meanwhile, knows things
the model does not: the **workspace's diff**, the **tool calls' results**, the **facts the
session already saved** (in the store, with their types), the **type distribution** of the
namespace. The question: what **state** — not prose — would let a rule move what survives in
the 40/41, and could the extension compare what the session *did* against what it *saved*
(a diff that says "you changed three bounds, and you saved none of them")? Deliver **one**
mechanism: where the data comes from, its shape, why it reaches the population the nudge
cannot, what it costs, and its falsifier.

### Axis 2 — HARNESSES: *the vehicle and the moment*

The vehicle is pi's `agent_before_settle` and its one extra request; the rule's author is the
model that just closed its judgment. The question: what **harness** could deliver a rule to
the model in the 40/41 — a different moment (`turn_end`, the session's close, the compaction),
a different seat (the opencode plugin, which has none), a different speaker (a reviewer the
extension calls, with `ctx.modelRegistry`), or a different contract (the extension decides and
writes, and the hook's `{entries, continue?}` is all it can return)? Deliver **one** harness
change with the API facts quoted, what it costs per session, what it can and cannot do (it
cannot block or revert what the session model saved), and its falsifier. The second reader is
**not** the proposal: the doctrine gap is the open question, so a harness without a rule is
not admissible — say what the rule would have to come from.

### Axis 3 — AI TECHNIQUE: *the rule*

Not the checklist: the pilot killed that shape — the model audits the tool it is asked to
trust and records what it finds, so the fact's survival is at the ceiling and the rule has
nothing left to catch. The question: **what class of fact survives this model, and what class
does not** — and therefore what a rule may accept, and what it must reject. The measurements
give the raw material: diagnoses are saved before the settle (40/41); procedures about tools
are saved after the model reads the tool (10/10); the marginal facts the trim bench caught
(5/10 against 1/10) were `knowledge`/`procedure` facts; the store holds **zero** procedures;
the seven types are labels, and their per-type rules are inadmissible by measurement. Deliver
**one** rule — with its accept set, its reject set, the evidence for each side, how it differs
from the checklist, what it costs (bytes in the nudge, or a reviewer's prompt), and its
falsifier.

## The synthesis — the three proposals, and what they make together

The three agents were asked for one improvement each, with the dead ends named. What came back
are **not three candidates for the same slot**: they answer three different questions, and
none of them clears the bar alone.

| axis | the question it answers | the proposal | what it needs from the others |
| --- | --- | --- | --- |
| HARNESSES | **WHEN** does the rule get spoken | **the moment**: `turn_end` of the turn that changed the workspace, not the settle boundary | the rule (imported from AI TECHNIQUE) |
| CONTEXT | **WHAT** does the rule speak about | **the coverage gap**: the extension diffs what the session changed against what it saved and names the uncovered values | a seat that reaches the 40/41 (HARNESSES) |
| AI TECHNIQUE | **WHICH** facts may survive | **the output-pointer rule**: a fact about how a tool behaves is saved only with the output line that shows it — a citation the extension can verify | a seat where the output lines are in the event (HARNESSES) |

### The mechanism, assembled

**`turn_end` carries `toolResults`** (`types.d.ts:828-832`) — the turn's own tool outputs, a
payload `agent_before_settle` does not expose per turn. That single fact makes the two rules
composable in one event:

- the **pointer rule** needs the output lines to check a citation against, and they are in the
  event;
- the **coverage rule** needs the workspace's diff, and the extension already snapshots the
  workspace's git state at the session start (`index.js:1751`) and can run any bounded `git`
  command (`lib/cli.js:385-395`, no whitelist, a timeout).

So the assembled mechanism is: **at the end of the first turn that changed the workspace, the
extension computes what that turn left uncovered — the values it moved that no record carries,
and the output lines that show a behaviour the session read but never tripped over — and asks
the model, while its judgment is still open, to save the invariant it established, quoting the
line that shows it.** No `sessionSaveCalls > 0` clause (that is the clause that silences the
nudge in the 40/41, `index.js:1929`), no description of the session (measured null at 1.14×
cost), no re-judgment (the model already closed it, 40/41).

### Why the three measured facts make it admissible — and what is still unmeasured

**The class exists and is under-served.** The model **writes what it executed and drops what it
merely read**: the `--explain` procedure appeared in **10/10** transcripts and was saved
**0/10**, while the same session's typo diagnosis was saved **10/10** (R5, pilot A.5). The trim
bench prices the same gap: the secondary fact ("how to run / what `make check` validates")
survived **5/10** under the fat block and **1/10** under the trim. The dogfood corpus holds
**zero procedures**. That class — the **affordance**, the behaviour a later session would have
to re-derive by auditing the tool again — is the one with a **sub-ceiling baseline**, which is
exactly what the checklist lacked (its baseline sat at 9–10/10, so the "≥ 2×" clause was
unreachable and the batch was correctly unspent).

**Still unmeasured, and it is a lot:** the real fire rate under the new gate; whether the
model saves differently when asked at a turn; the pointer rule's own effect (never run — its
accept-set evidence rests on **one pilot at N=10**, one model, one image); the bytes and the
billed tokens of the entry; and the cost of an extra request at `turn_end` (the only reference
is the derived **$0.00025 per call**, **$0.00269 per session**, 41 sessions for $0.1103).

**The cost moves.** Today the nudge is silent in 41 of 41 sessions, so it costs ~zero. The
assembled mechanism fires in ~41 of 41: one extra model request per changed session
(≈ **+9.3 %** of the calls, ≈ **+$0.00025/session**) — and the text gets *shorter* (the
examples and the seven glosses go), so it does not buy the enriched bench's 1.14× premium. The
firing rate stops being the constraint; **the judgment becomes it**: the precision risk is
that the coverage set is non-empty in most sessions, and the rule then has to be right about
what is worth keeping.

**opencode is not covered.** The plugin has no settle seat, and its V1 analogue
(`session.idle`, which fires after *every* turn) is named but **unmeasured**
(`extensions/mnexis-opencode/RESEARCH.md:110-111`). The mechanism is pi-only until someone
measures the opencode seat — and the opencode bench now running measures a different
question (the answer's content), not this one.

### The falsifier, pre-registered

Same bench, on the **trim fixture** (`ingest-contract`) — the only fixture with a measured
marginal-fact gap, whose control baseline is ≤ 5/10; the checklist fixture is unusable because
its baseline sits at the ceiling. Arms: **A** = the shipped settle nudge, **A′** = no nudge
(the control), **B** = the assembled seat + rules. Criterion, sealed and hashed before the
runs: the marginal fact present in the store at close in **≥ 2× A′ and ≥ 4/10**; records/run
**≤ A+1**; **zero** fabricated facts; **zero** change-log-only records; cost/run **≤ 1.15×**.
**The kill:** if B records the marginal fact no more often than A′ — and no better than the
shipped nudge — then the "wrong moment, wrong author" diagnosis is refuted and the settle
boundary was never the problem. Secondary kills: B's fire rate **< 4/10**, or records/run
**> A+1**.

### What the human decides

1. **The seat**: pi-only (`turn_end`), or pi + the opencode analogue once its feasibility is
   measured — the opencode bench's pilot is running now and will say whether that host is even
   reachable.
2. **The cost ceiling**: one extra request per changed session is a budget decision, not a
   technical one (reference: ≈$0.00025 per call, ≈$0.00269 per session).
3. **Whether to spend it**: the bench is ~30 sessions; the criterion above is what it buys.

## Tasks

| ID | Specs | Route | Branch / worktree | Commit |
| --- | --- | --- | --- | --- |
| T1 | S1–S4, the CONTEXT brief | delegated: `gentle-ai-explore` (read-only) — **delivered**: the uncovered-change gap | `docs/nudge-harness-analysis` @ `tmp/wt-nudge-analysis` | — |
| T2 | S1–S4, the HARNESSES brief | delegated: `gentle-ai-explore` (read-only) — **delivered**: the moment (`turn_end`) | `docs/nudge-harness-analysis` @ `tmp/wt-nudge-analysis` | — |
| T3 | S1–S4, the AI TECHNIQUE brief | delegated: `gentle-ai-explore` (read-only) — **delivered**: the output-pointer rule | `docs/nudge-harness-analysis` @ `tmp/wt-nudge-analysis` | — |
| T4 | S5, S6 | inline (parent): the three proposals side by side and the synthesis | `docs/nudge-harness-analysis` @ `tmp/wt-nudge-analysis` | — |

## Log

- **L1 (2026-11-02, the human, verbatim)** — "mientras haces eso vuelve a hacer el analisis con
  los 3 agentes, a el nudge para mejorar el harnesses". The same three axes as the previous
  round, run again with the evidence the last two sessions produced: the enriched nudge's null,
  the checklist's ceiling, and the brief's doctrine gap.
- **L2 (2026-11-02, the three proposals, delivered)** — **HARNESSES**: the moment — `turn_end`
  of the turn that changed the workspace, with the turn's `toolResults` in the event
  (`types.d.ts:828-832`), dropping the `sessionSaveCalls > 0` clause that silences the nudge
  in the 40/41; the rule it carries is "a change implies a constant — save the invariant the
  turn established, not the act"; cost ≈ +1 call/session. **CONTEXT**: the coverage gap — the
  extension diffs what the session changed (and read) against what it saved and names the
  uncovered values; the data is already there (`runGit` is unrestricted, the baseline is
  snapshotted at `:1751`, the tool results are discarded at `:1903-1909`, the session's saves
  are in memory). **AI TECHNIQUE**: the output-pointer rule — a fact about how a tool behaves
  is saved only with the output line that shows it, a citation the extension can verify in
  `event.context.llmMessages`; its accept set (the affordances) is the one class with a
  sub-ceiling baseline (R5: 10/10 read, 0/10 saved; the trim's marginal fact 5/10 against
  1/10), and its reject set is the session's own writing, `project_state`, and restatements.
- **L3 (2026-11-02, the parent's spot checks)** — the citations of the two proposals that
  carry the mechanism were verified against the code and the artifacts: `runGit` passes its
  args to git with no whitelist (`lib/cli.js:385-395`), the workspace baseline is snapshotted
  at the session start (`index.js:1751`), the tool-call handler reads only `isError` and
  `toolName` and discards the result (`index.js:1903-1909`), `numericValues` /
  `sameNumericSignature` exist as measured helpers (`index.js:515-556`), and the trim bench's
  H1-G3 reads exactly as quoted ("the secondary fact ... is recorded by A in 5/10 runs and by B
  in 1/10", `tmp/bench/trim-nudge-report.md`). R5 was checked in the pilot record
  (`checklist-measurement.md:382,422-424,437`): the `--explain` procedure appeared in 10/10
  transcripts and was saved 0/10, the typo diagnosis 10/10.
- **L4 (2026-11-02, the synthesis)** — the three are **complementary, not competing**: WHEN
  (the seat), WHAT (the state), WHICH RULE (the acceptance). None clears the bar alone; the
  assembly does, because `turn_end`'s `toolResults` are exactly the material the pointer rule
  needs to verify a citation, and the workspace diff is exactly the material the coverage rule
  needs — one event, both rules. The honest frame: the class is real and under-served, and
  **nothing about the assembly is measured yet**; the fire rate moves from ~2/41 to ~41/41, so
  the cost moves from ~zero to per-session, and the judgment — not the firing — becomes the
  constraint. The falsifier and the arms are pre-registered in the synthesis above, on the
  trim fixture, whose control baseline is ≤ 5/10.

