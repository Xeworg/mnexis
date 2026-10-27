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
| C1 | S1–S6 | this document: the plan and the criterion, committed before any arm exists | docs | `84eb092` |
| C2 | S4 | the session-state block in the `context` answer, behind the bench switch, test-first — the shipped answer unchanged with the variable absent | feat branch | `913504e` + review fix `feb579a` |
| C3 | S4 | the same block in the `status` payload, behind the switch, test-first | feat branch | `913504e` (same unit as C2: one injected switch, two carriers) |
| C4 | S5 | the batch: 3 arms × 2 phases × N=10, model calls as the primary metric, images and shas recorded before it runs | artifacts: `tmp/bench/calls-measurement.md` (pre-numbers) + `out/` (gitignored), copy in `lab/mnexis-bench/call-budget/` |
| C5 | S5, S6 | the report: the six rules one by one, the verdict, the one decision | `tmp/bench/calls-report.md` + board §9 (this commit) |
| C6 | S6 | the ascension: the block becomes the door's answer (no switch), `status` keeps its priced shape, the switch retires | `feat/door-session-state` |

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
- L4 (2026-10-26, user, verbatim): "recuerdas que te dije que habia un test
  mal, ya lo encontre era de otro proyecto puedes ejecutar los test sin
  problemas" — the suite runs again. 686 tests green; the only failure was
  `benches/fts5_latency` opening a stale cached corpus (`target/bench-data`,
  Oct 19, `user_version` 0, 10 000 rows sharing 200 topic keys — it predates
  the topic-key UNIQUE), regenerated and green. The gates were also restored
  after a toolchain update raised 23 clippy lints (nearly all in files this
  branch never touched) plus 4 fmt diffs: mechanical fixes, two lints taken
  properly (`ConflictPolicy` now implements `FromStr`; `Form::submit`
  returns `SubmitRejected` instead of `Err(())`), and this branch's dead
  `hit()` test helper removed — `073b17f`. fmt, clippy `-D warnings` and the
  full suite are green; the tree is ready for C4's images.
- L5 (2026-10-26): C4 complete and C5 written, nothing re-rolled. The memory
  guard killed the first batch during `call-c` run 9 (watchdog2,
  MemAvailable 1.3 GB); runs 9–10 resumed with the same image and seeds
  (runner gained only a `RUNS_FROM` knob). **The numbers** (N=10, same
  model, prefix 5 046 tok/call in every arm — payload, not schema): phase S
  median calls A 6.0 → B 5.0 / C 5.0; median tokens A 48 771 → B 37 121
  (−24 %) / C 41 752 (−14 %); B also −0.5 calls and −20 % tokens in phase
  K. Quality 3/3 in 10/10 (B, C), trap kept 10/10 everywhere, 0 invented,
  write path 20/20. **Rules 1–5 hold for both arms — rule 3 exactly at the
  boundary** (ceiling = this batch's A − 1 = 5.0; the doc's "≤ 4.0"
  parenthetical was a stale projection — A re-measured 6.0 with the same
  image). **The decision (S6): the block ascends on the door (B's carrier),
  only there** — B dominates C on every axis and the agent answers from
  the door 9/10; the rebuild rounds (show/search/history) are what
  disappeared, the `list+recall` verification round survives in every run.
  The ascent itself (ship the block on `mnexis context`/`mnexis_context`)
  is the next product step, with its own review.
- L6 (2026-10-27): C6, the ascension, done (`feat/door-session-state`).
  `MemoryService::context` attaches the block (`response.session_state`), so
  every caller of the door inherits it — the CLI command and the pi tool
  alike; the branch's carrier and the bench switch
  (`MNEXIS_BENCH_SESSION_STATE`, `SessionStateMode`) are gone, and
  `WorkspaceStatus` never gains the field (arm C lost). Test-first: the
  suite `tests/session_state.rs` was rewritten to the shipped contract and
  failed 5/6 before the change (the door without the block; the retired
  switch still steering). The differential against `11b8ea0` in a scratch
  worktree: `status`, `list`, `list --full`, `search`, `recall` and the
  budgeted `context` envelope byte-identical; the door's new `session_state`
  key is the only difference, which is what the experiment asked for. The
  extension's description, the README and the skill now say the first answer
  carries the state and that it is outside the budget — the contract the
  model reads, pinned by a new tools test (74/74). The task doc's own
  README/SKILL wording was updated with the block, so `assets_fidelity`
  keeps the shipped copy and the repository copy identical.
