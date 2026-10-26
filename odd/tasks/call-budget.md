# The call-budget experiment — can the session answer in fewer model calls?

The prefix experiment priced the tool schema and lost
(`odd/reports/prefix-cost-report.md`): the saving per call is real, and the
bill is set by the number of model calls. This feature asks the question
that experiment left open, with the same discipline: a criterion written
before any arm exists.

## Specs

- S1 (the user's direction, 2026-10-26, verbatim): "dale haz el experimento"
  — the next experiment is the call count, the lever the prefix experiment
  isolated.
- S2 (the user's standing rule, 2026-10-26, verbatim): "con el criterio
  escrito antes de los números" — the criterion below is committed before
  any arm exists and is not edited once the numbers arrive.
- S3 (the measured premise, `odd/reports/prefix-cost-report.md`). Each model
  call bills the fixed prefix (≈ 5 046 tokens measured) **plus the whole
  accumulated transcript**, so a call late in the session costs ≈ 8 400
  tokens: the control's phase S is 5.0 model calls for 41 964 tokens. The
  lean-tool arms saved 1 687 tokens on every prefix and still cost more,
  because they needed two extra calls. The control does not answer from one
  surface: it fans out in rounds — `workspace` + `context` (round 1) →
  `list` + `recall` (round 2) → `search_prompts` + `search` (round 3) →
  `show` / `history` (round 4) → the answer (round 5). Every surface holds a
  part of "where we left off", so the agent keeps asking.
- S4 (the arms, one factor above the baseline each, the earlier shape):
  - **A — today**: `main@68ee9fd`, the shipped surfaces. The control.
  - **B — the session-state block in the door**: `mnexis context`'s answer
    gains one block: the `project_state` and `decision` records **with their
    content** (not the priced preview the other surfaces return), plus the
    most recent session summary, plus the newest records as previews. Behind
    a bench switch (`MNEXIS_BENCH_SESSION_STATE=door`; with the variable
    absent the shipped answer is unchanged), read once and injected, never a
    mutated global in tests — the precedent of `resolveLeanTools`.
  - **C — the same block in `workspace`/`status`**: the surface the control
    opens first in 10 of 10 runs, carrying the same block
    (`MNEXIS_BENCH_SESSION_STATE=workspace`). B and C differ only by which
    surface carries it; the two together answer the design question the door
    re-measure left open.
  The block is a payload change, not a prompt trick and not a new tool: the
  question is whether one complete answer stops the fan-out.
- S5 (the criterion, pre-registered):
  1. **The quality holds (phase S):** the arm states **≥ 2 of the 3**
     ground-truth state facts in **≥ 8/10** runs, with **0** invented state
     claims.
  2. **The trap holds (phase K):** the arm keeps the stored decision
     (`staging-eu`) in **10/10** runs and invents **0** values.
  3. **The prize is a round, not a token:** the arm's **median model calls**
     in phase S is **≤ A's − 1** (A measured 5.0, so ≤ 4.0). One round is
     the smallest effect this N can honestly see, and it is worth ≈ 8 400
     tokens.
  4. **The cost does not regress:** the arm's median input+cacheRead in
     phase S is **≤ A's** (ties allowed). A fatter answer that removes a
     round is a win; a fatter answer that keeps the rounds is a loss, and
     rule 3 without rule 4 is not a win in either direction.
  5. **The write path survives:** every run lands **≥ 8 titled records**
     with **0** untitled, in both phases' stores.
  6. **The rounds, per run:** the report shows each run's round-by-round tool
     sequence, so "which round disappeared" is visible and a win cannot be
     claimed from a median alone.
- S6 (the disposition rule): the block ascends to a product surface **only
  if an arm passes 1–5**. A partial pass is recorded as the next question,
  never merged. If neither arm moves the call count, the fan-out is not a
  payload problem — the agent does not trust one answer, and the next
  question is the envelope's contract, not another field.

## Tasks

| ID | Spec | Route | Commit |
|---|---|---|---|
| C1 | S1–S6 | this document: the plan and the criterion, committed before any arm exists | docs |
| C2 | S4 | the session-state block in the `context` answer, behind the bench switch, test-first — the shipped answer unchanged with the variable absent | feat branch |
| C3 | S4 | the same block in the `status` payload, behind the switch, test-first | feat branch |
| C4 | S5 | the batch: 3 arms × 2 phases × N=10, model calls as the primary metric, images and shas recorded before it runs | bench |
| C5 | S5, S6 | the report: the six rules one by one, the verdict, the one decision | bench + board |

## Log

- L1 (2026-10-26, user, verbatim): "dale haz el experimento" — after the
  prefix experiment answered no, the user sent the next experiment.
- L2 (2026-10-26): the premise, measured by the previous feature: A 5.0
  model calls / 41 964 tokens in phase S, prefix 5 046 per call, lean arms
  −33 % per call and +16 % per session because of 2 extra calls. The open
  lever is therefore the number of calls, not the prefix.
- L3 (2026-10-26): branch `bench/call-budget` off `main@68ee9fd`; the
  extension's `resolveLeanTools` is the precedent for an injected,
  env-read-once bench switch.
