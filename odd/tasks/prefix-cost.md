# The prefix experiment — is the fixed per-call prefix the real cost lever?

The door re-measure and the steering experiment both ended at the same
sentence: the residual cost is the fixed prefix, not retrieval
(`tmp/bench/door-tool-report.md`, `tmp/bench/steering-report.md`). This
feature decides, by measurement, which part of that prefix earns its
place — and it corrects the door report's own claim about what the prefix
is.

## Specs

- S1 (the user's direction, 2026-10-26, verbatim): "dale" — proceed with
  the step proposed at the 2026-10-26 close: write the pre-registered
  criterion and move the work off the archived branch onto `main`.
- S2 (the user's standing rule, 2026-10-26, verbatim): "con el criterio
  escrito antes de los números" — the criterion below is committed
  **before** any arm exists and is not edited after the numbers arrive.
- S3 (the premise, measured — and corrected). `door-tool-report.md` says:
  "the fixed prefix — the 13 KB skill the agent reads in every session."
  The bench's own transcripts refuse the second half: across the 60
  steering transcripts the skill body is read in **3 of 60** runs (5 %),
  while the per-call prefix is **≈ 5 050 tokens** in every run (median of
  the first assistant call, all 60). Of that, the system prompt is
  ≈ 1 310 tokens (4 856 char) and the rest is the **tool-schema block**
  ≈ 3 740 tokens, of which mnexis's **14 tool schemas are ≈ 2 853 tokens**
  (chars/4 over name + description + parameters of every
  `pi.registerTool` call). The prefix is billed once per model call, and
  it is **63–69 %** of a session's whole token bill at 4–6 calls. So the
  lever this feature measures is the **tool surface**, and the skill body
  is measured only to close the door report's claim.
- S4 (the arms, one factor above the baseline each, the steering
  experiment's shape):
  - **A — today**: `main@68ee9fd`, the installed extension registers all
    14 tools, the shipped 14 819-byte skill. The control.
  - **B — the lean surface**: the same binary, the same store, the same
    prompts; the extension registers **5 tools** — `mnexis_workspace`,
    `mnexis_context`, `mnexis_search`, `mnexis_save`, `mnexis_show`
    (≈ 1 323 schema tokens, −1 530 per call). Behind a bench switch
    (`MNEXIS_BENCH_LEAN_TOOLS=1`, read once at registration and injected
    into the registration path, never a mutated global in tests) so the
    production surface is unchanged and A == B beyond the switch. The CLI
    stays available in every arm: this arm removes the schema block, not
    the capability.
  - **C — B + the short skill**: B plus a shortened skill body
    (≈ one third: the doors and the loop, the flag tables deferred to
    `--help`), installed as a file at setup so the repo's skill is
    identical in B and C. C exists to **falsify** S3's correction: if the
    body is read in 5 % of runs, C must not differ from B beyond noise.
- S5 (the criterion, pre-registered):
  1. **The tax holds (phase K):** the arm keeps the stored decision
     (`staging-eu`) in **10/10** runs and invents **0** values.
  2. **The prize is real (phase S):** the arm states **≥ 2 of the 3**
     ground-truth state facts in **≥ 8/10** runs, with **0** invented
     state claims.
  3. **The cost drops:** the arm's median input+cacheRead in phase S is
     **≤ A's − 15 %** (A = 29 493 → threshold 25 069), **and** its median
     call count is **≤ A's** (4). The prize is derived, not guessed:
     −1 530 tokens per call over 4 calls is −6 120 (−21 %); a leaner
     surface that buys an extra call gives it back (≈ +3.5 k per call).
  4. **The write path survives:** in phase 0 the arm's runs still record
     every seeded fact with a title, **10/10** runs — dropping tools may
     not quietly break saving.
  5. **The surface, per run:** the report shows which tools carried each
     answer, so a win that routes through the tools the baseline already
     prefers is a schema win and is named as one.
- S6 (the disposition rule): the lean surface ascends to a product
  decision **only if an arm passes 1–4**. A partial pass is recorded as
  the next experiment's question, never merged. If C ≈ B within noise,
  the door report's "13 KB skill" sentence is corrected in place and the
  skill body stays as it is.

## Tasks

| ID | Spec | Route | Commit |
|---|---|---|---|
| T1 | S1–S6 | this document: the plan and the criterion, committed before any arm exists | docs |
| T2 | S4, S5.4 | arm B: the lean surface behind the bench switch, test-first — the switch selects 5 tools, the shipped default still registers 14, the write path keeps its tests | feat branch |
| T3 | S4 | arm C: the shortened skill body as a setup file (the repo skill untouched) | bench |
| T4 | S4 | the run: 3 arms × 2 phases × N=10, image lineage and binary sha recorded before the batch | bench |
| T5 | S5, S6 | the report: the five rules one by one, the verdict, the one decision (lean surface ships, or retires) | bench + board |

## Log

- L1 (2026-10-26, user, verbatim): "dale" — after the 2026-10-26 close,
  the next step is the prefix experiment, starting with the criterion.
- L2 (2026-10-26): `main` fast-forwarded from `f7a8d95` to `68ee9fd`
  (PR #66); the work moved off `bench/steering-arms` (archived, 2 local
  docs commits ahead of its remote, never pushed).
- L3 (2026-10-26): the desk evidence that the premise needed correcting
  (S3): 60 transcripts, per-call prefix ≈ 5 050 tokens, 63–69 % of the
  bill, skill body read in 3/60; the 14 mnexis schemas ≈ 2 853 tokens by
  chars/4, the five-tool lean set ≈ 1 323. Sources: `tmp/bench/out/`,
  `extensions/mnexis-memory/index.js`.
- L4 (2026-10-26): the arm mechanism follows the steering precedent (one
  binary, the arms differ by the switch and by setup files), and the
  criterion lives here, in the repository, instead of only in `tmp/bench`
  — the steering criterion was written in `tmp/`, which is ignored, so
  the pre-registration was not durable.
- L5 (2026-10-26): T2 shipped in commit `94aa7b0` — the surface
  resolution in `extensions/mnexis-memory/index.js`
  (`resolveLeanTools(options, env)`: injected flag, then
  `MNEXIS_BENCH_LEAN_TOOLS=1`, then the shipped fourteen) with
  `extensions/mnexis-memory/test/surface.test.js`; extension suite
  77/77, Rust suite 673 passed 0 failed. The native review of that
  candidate (lineage `review-ee26900120ba335e`, tier medium, 246
  lines, lenses risk + reliability) is APPROVED and its authority
  burned; it left three informational WARNING findings, non-blocking
  and not corrected here: `R3-001` index.js:260-262, `R3-002`
  surface.test.js:56-78, `R3-003` surface.test.js:62-66 — follow-ups,
  not a reason to re-review this candidate.
- L6 (2026-10-26): T3 and T4 — the short skill body
  (`tmp/bench/build-context/short-skill.md`, 4 027 bytes = 27 % of the
  shipped 14 819), the arms verified inside their images (A registers
  14 with and without the switch; B and C register 5 with it and 14
  without), the images `bench-mem:prefix-a|b|c` and both binary shas —
  A `b7c1347072fd312095661da325ddfe9265ba03ee522c05ebd63d5f0190f18623`
  (main@68ee9fd), B and C
  `7dcfbdf16e42b2f6096ccf283ced68cea0b0592ba8984c4e3e5d004cd41b59c0` —
  and the batch: 60 transcripts, 40m49s, zero failed steps, 20 runs per
  arm, every lean run valid (no run called one of the nine dropped
  tools).
- L7 (2026-10-26): **the experiment ran and the answer is no.** The
  verdict and the deciding numbers (`odd/reports/prefix-cost-report.md`):
  prefix per call A 5 046 / B 3 359 / C 3 359; phase S median A 41 964
  at 5 model calls, B 48 739 at 7, C 44 615 at 7; quality 10/10 with
  3/3 facts and 0 invented in every arm; phase K kept 9/10 (A), 10/10
  (B), 10/10 (C); write path 20/20 everywhere. Rules 1, 2 and 4 hold
  for both lean arms; rule 3 fails for both, so no arm ascends (S6).
- L8 (2026-10-26): the disposition and the next question — the surface
  stays the shipped fourteen; the lever is the number of model calls
  and the accumulated transcript, not the schema block; the short skill
  is safe and is a separate later simplification, never merged as part
  of this result.
