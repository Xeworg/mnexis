# The shape experiment — can a visibly complete answer buy back the cross-check round?

The chrome landed (PR #74, PR #76) and the board's item 10 closed with a
rule that is now the portable contract: *the shape, not the decoration*. The
board's item 11 is product work; this feature is the measurement the chrome
opens — the only place where a **presentation** could pay the agent something.

The board's rule 6 says the agent cross-checks an answer before it trusts it,
and the call-budget batch measured the consequence: the `list+recall`
verification round survived in **every run of every arm**. A round is the
whole accumulated transcript billed again.

## Specs

- S1 (the user's direction, 2026-10-27, verbatim): "dale hazlo" — after the
  explanation of the board's item 2 ("explicame el 2"). The question is the
  board's, quoted from `odd/next.md` §11, verbatim: "**And the question the
  chrome opens** — the only place where a *presentation* could pay the agent
  something: the board's rule 6 says the agent cross-checks an answer before
  it trusts it, and the `list+recall` round survives in every run of every
  arm. A shape that makes the answer visibly complete might kill that round,
  and a round is the whole transcript billed again. That is a measurement
  with the criterion written before the numbers, not a change to guess at."
- S2 (the question). Two things are measured, separately, because they are
  separable: **(a) the block** — the door's answer does not carry the
  session state the tool's own description promises, and **(b) the shape** —
  the answer the model reads is fat JSON (`1. {…}` per memory) instead of the
  shape the chrome already draws for the human. The board's question is
  (b): does a *visibly complete* answer kill the verification round? (a) is
  measured in the same batch because a shape over a partial answer cannot
  answer (b) fairly, and because (a) is a promise the shipped extension
  breaks.
- S3 (the arms — one factor between neighbours, the same batch, the same
  model, the same seeds, N=10 each, phases K and S):
  - **A — `shape-a`: today.** The shipped binary (`main@7f4356f`). The door
    answers the budgeted items as fat JSON plus the declaration line. The
    session-state block is in the engine's envelope and **no client renders
    it**: this is what the user runs today.
  - **B — `shape-b`: the door keeps what it promises.** The branch binary
    with the door in `block` mode: the same items as fat JSON **plus** the
    session-state block as fat JSON (`Session state: {…}`), the records with
    their whole content. The only difference against A is the block.
  - **C — `shape-c`: the same content, in shape.** The branch binary with
    the door in `shape` mode: **the same items and the same block**, rendered
    in the shape — one physical line per memory, the kind of the fact next
    to its name, the identity of the record before its text, the text
    indented under it, the same icons the chrome draws for the human
    (`📘 knowledge`, `⚖️ decision`, `⭐ preference`, `🧭 procedure`,
    `📍 project_state`, `🎞️ episode`, `🧠 fact`), `#` plus the first eight
    characters of the uuid as the identity, no ANSI and no table borders
    (every box character is a token: the panel with borders costs 374, the
    same panel without them 81).
    The **only** difference against B is the rendering: same content, same
    order, same declaration line, same block. That is the comparison the
    board asks for.
  - B and C are the branch binary with an injected, env-read-once bench
    switch (`MNEXIS_BENCH_DOOR=block|shape`), the precedent of
    `resolveLeanTools` and of `MNEXIS_BENCH_SESSION_STATE`; with the
    variable absent the shipped answer is byte-identical to A's.
  - Nothing else moves: same image family (`bench-mem:now`), same pi
    (1.2.0), same model (`opencode-go/deepseek-v4.1-flash`), same fixture,
    same seeds, same prompts, same scorer. The block is **not** deduplicated
    against the items in either B or C: a record the retrieval already
    brought may appear twice, and that cost is the same in both arms, so the
    comparison stays about the rendering.
- S4 (the criterion, pre-registered — committed before any arm exists and
  not edited once the numbers arrive):
  1. **The verification round.** Defined once, on the transcript: a round
     **after the door round** that issues `mnexis_list` or `mnexis_recall`.
     A round is one assistant message plus the tool calls it issued (the
     definition `score-calls.py` already uses). The metric is the **presence
     rate**: how many of the 10 runs of phase S still carry one.
     - **The edge, settled before any run** (amended 2026-10-27, before the
       batch, after the manipulation check exposed it): a run that **never
       opened the door** carries no door round to be "after". For that run,
       **any** `mnexis_list`/`mnexis_recall` round counts as a verification
       round — the agent rebuilt the answer by hand, which is the round this
       metric is about — and a run that neither opens the door nor calls
       `list`/`recall` carries none. The denominator is the 10 runs in every
       arm: an arm is never rewarded for skipping the door.
     - **Two readings are reported, and P1 is judged on the primary one**
       (the edge rule above). The **strict** reading counts only the runs
       that opened the door, and the **door-call rate** per arm is reported
       next to it. **If the primary and the strict reading disagree about
       the verdict, the verdict is reported as not settled and nothing
       ascends** — a difference that rests on how many runs opened the door
       is not a difference in the rendering.
     - **P1 (the effect the board asks about):** arm C's presence rate is
       **≤ 2/10** and arm A's is **≥ 8/10**. The effect must exist in both
       directions: the shaped answer removes the round, today's answer
       keeps it.
     - **P5 (the block alone):** arm B's presence rate is **≤ 2/10**. If the
       block alone kills the round, the shape is not needed for the effect
       (it may still be the cheaper carrier — that is a separate read of the
       tokens).
  2. **The quality holds (phase S):** the arm states **≥ 2 of the 3**
     ground-truth state facts in **≥ 8/10** runs, with **0** invented state
     claims. Same rule the call-budget batch used.
  3. **The trap holds (phase K):** the arm keeps the stored decision
     (`staging-eu`) in **10/10** runs and invents **0** values.
  4. **The cost does not regress:** arm C's median input+cacheRead in phase
     S is **≤ A's** (ties allowed). A fatter answer that removes a round is
     a win; a fatter answer that keeps the rounds is a loss, and P1 without
     this rule is not a win in either direction.
  5. **The write path survives:** every run lands **≥ 8 titled records**
     with **0** untitled, in both phases' stores.
  6. **The rounds, per run:** the report shows each run's round-by-round
     tool sequence for every arm, so "which round disappeared" is visible
     and no win is claimed from a median alone.
- S5 (the disposition rule). The shape ascends to a product surface **only
  if arm C passes 1–4** (P5 is reported, not required, of the shape). The
  block ascends **only if arm B passes 1–3** — it is a promise the shipped
  extension already makes in the tool's description, and either the answer
  carries it or the description stops promising it. A partial pass is
  recorded as the next question, never merged. If no arm moves the
  presence rate, the verification round is not a presentation problem, and
  the next question is the door's contract, not another shape.
- S6 (the defect this feature found while exploring, recorded before the
  numbers). The shipped extension's `mnexis_context` description promises
  the model "PLUS the session state — the `project_state` and `decision`
  records with their whole content and the previous session's summary", and
  the answer carries **none of it**: `MemoryService::context` attaches the
  block as `response.session_state` (`src/application/service.rs`), the CLI
  serialises it, and **no client reads it** — not the pi extension
  (`extensions/mnexis-memory/index.js` renders `items` only), not the MCP
  surface (which has no `memory_context` tool at all — that is the board's
  item 11). The bench's arm B, by contrast, carried the state *inside* the
  items with their whole content, which is why the call-budget batch could
  measure "state facts 10/10" and a cross-check round at the same time. The
  shipped product therefore shows the model **less** than the batch that
  justified it. Arm B measures what that costs; the description's promise is
  a defect either way.

## Tasks

| ID | Spec | Route | Commit |
|---|---|---|---|
| C1 | S1–S6 | this document: the plan and the criterion, committed before any arm exists | docs | (this commit) |
| C2 | S3 | the door's experimental rendering in the extension behind the bench switch (`block`, `shape`), test-first; the shipped answer unchanged with the variable absent | feat branch | |
| C3 | S3, S4 | the bench wiring: the two branch images, the manipulation check (the model-facing text is what the arm claims, before the batch spends an hour on it), the runner, the scorer for the verification round | feat branch + `tmp/bench/run/` | |
| C4 | S4 | the batch: 3 arms × 2 phases × N=10, same model, same seeds, same images, the images and their shas recorded before it runs | artifacts: `tmp/bench/shape-measurement.md` (pre-numbers) + `out/` (gitignored) | 60/60 measured 2026-10-27, 0 invalid |
| C5 | S4, S5 | the report: the rules one by one, the numbers, the verdict, the one decision | `tmp/bench/shape-report.md` + board §11 | PR #83 |
| C6 | S5, S6 | the ascent or the null: what ships (the block, the shape, both or neither) — its own review, its own PR | the null: nothing ships, and the promise S6 recorded is the next decision | (see L4) |

## Log

- L1 (2026-10-27, user, verbatim): "dale hazlo" — the answer to "¿Con cuál
  seguimos?" was the measurement of item 2, after "explicame el 2" and
  "ya veo los dibujos" (the chrome's drawing confirmed live in the TUI,
  which closed the only pending verification of PR #74/#76).
- L2 (2026-10-27, the exploration). The harness was read end to end
  (`tmp/bench/run/run-all.sh`, `run-calls.sh`, `score-calls.py`,
  `dump-calls-rounds.py`): podman rootless, the opencode key, pi 1.2.0 in
  `bench-mem:now` (the same pi the user runs), N=10 per arm, phases K and S,
  the rounds defined as one assistant message plus its tool calls. The
  three images of the previous batch are still on disk.
- L3 (2026-10-27, the exploration). Two facts that shape the arms, both
  checked against the code and the artifacts rather than assumed:
  (1) the extension's model-facing text is `items` only — the session-state
  block is in the engine's envelope and in no client's answer (S6);
  (2) the call-budget's arm B carried the state *inside* the items with
  their whole content (`tmp/bench/out/s1-call-b-steering.jsonl`), which is
  why the batch could measure "state facts 10/10" **and** a cross-check
  round at the same time: a complete answer, in fat JSON, still got
  cross-checked. That is the premise S2 rests on, and it is now measured
  against the shipped product rather than against the branch that justified
  it.

- L4 (2026-10-27, the batch and the verdict). 60 measured transcripts, 0
  invalid, 3 arms × 10 runs × 2 phases, the same images and shas the
  measurement document recorded, the door check matching each arm's claim.
  **The verification round survived 10/10 in all three arms**, and the
  door-call rate was 10/10 everywhere, so the primary and the strict reading
  agree and the verdict is settled. Rule by rule: **P1 fails** — arm A keeps
  the round (10/10, needed ≥8/10) and arm C keeps it too (10/10, needed
  ≤2/10); **P5 fails** — arm B keeps it 10/10; **rule 2 holds** in B and C
  (≥2 of 3 state facts in 10/10, 0 invented); **rule 3 holds** (the trap
  `staging-eu` kept 10/10, 0 invented); **rule 4 holds** (C 36,214 and B
  37,290 median input+cacheRead tokens against A's 45,573 — both answer
  cheaper and both still pay the round); **rule 5 holds** (20/20 runs, 0
  untitled, at least 8 titled records per store). Per S5, **nothing
  ascends**: the shape fails rule 1 and the block fails P5, so the
  verification round is not a presentation problem, and the next question is
  the door's contract. The sequences say what the round is: the agent
  enumerates (`list`, often in the door's own round) and then recalls — a
  protocol, not a doubt about completeness — which is recorded as a caveat on
  the metric, not repaired after the numbers. The artifact record is
  `tmp/bench/shape-report.md` (gitignored, as C5 says); the durable record is
  the board's item 11. The one decision S6 leaves: the extension's
  `memory_context` description promises the session state and no client
  renders it — either the description stops promising it, or the block ships
  as a promise kept rather than as a measured win.
