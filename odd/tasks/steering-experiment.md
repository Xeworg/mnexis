# The steering experiment — does a typed priority earn its place in retrieval?

Issue #55 part two. The type axis shipped (#66); whether it ascends to a
retrieval input is decided by this experiment, not by argument.

## Specs

- S1 (issue #55, direction 3, verbatim): "The experiment: three arms
  (today; typed priority; typed priority + type-filtered search), N=10,
  kill criteria written first — **plus a steering-shaped phase** ("where
  did we leave off") whose ground truth lives in project_state/decision
  records, because that is the real recurring question and the bench
  never asked it."
- S2 (issue #55, direction 3, verbatim): "Type ascends to a retrieval
  input only if that arm wins."
- S3 (issue #55, acceptance, verbatim): "The steering experiment exists
  as bench artifacts (criterion first) before any priority semantics
  ship."
- S4 (the design constraint the door re-measure added,
  `tmp/bench/door-tool-report.md`): "the steering arms must aim at the
  door the agent actually opens" — typed priority that lives only in
  `mnexis_context` reaches 5/10 runs; the arms cover the surfaces the
  agent opens (`context` ranking, `list`/`workspace` order, the skill's
  pointer) or the experiment measures the wrong surface.
- S5 (the user's direction, 2026-10-26, verbatim): "si eso es importante
  a que le daremos prioridad al recuperar y debe ser lo siguiente de
  debemos hacer".

## Tasks

| ID | Spec | Route | Commit |
|---|---|---|---|
| T1 | S1, S3, S4 | the plan and the criterion, written before any arm exists: `tmp/bench/steering-measurement.md` | docs |
| T2 | S1, S4 | arm B/C semantics behind a bench switch: the typed tier in the door's ranking (B), plus the skill teaching the type filter for steering-shaped questions (C); test-first, isolated state | feat branch |
| T3 | S1 | the fixture grows typed `project_state`/`decision` records (same store for every arm); the steering prompt is the real recurring question | bench |
| T4 | S1 | the run: three arms × two phases × N=10, image lineage + sha recorded | bench |
| T5 | S2, S3 | the report with the verdict per criterion and the one decision: type ascends, or stays an attribute and a filter | bench + board |

## Log

- L1 (2026-10-26, user, verbatim): "si eso es importante a que le
  daremos prioridad al recuperar y debe ser lo siguiente de debemos
  hacer" — the steering experiment is the next step, after the door
  re-measure.
- L2 (2026-10-26): the door re-measure ran first (`door-tool-report.md`):
  2/5 criteria; the rebuild is gone but the priced pair displaced it;
  the door reached 5/10; baseline for this experiment: 10/10 quality,
  5/10 door, 7 calls, 44 237 tokens, binary sha `b7c1347…`.
- L3 (2026-10-26, user): confirmed the bench tested the current
  development version (`main@68ee9fd`, post-v0.3.0 by eight merges); the
  version string still says 0.3.0 — the sha is the identity until a bump
  is decided.
- L4 (2026-10-26): correction after native review R2-001/R3-001/R3-002.
  Tier ordering behind `MNEXIS_BENCH_STEERING_TIER=1`, default ranking
  unchanged; arm B bakes that env var; arm C adds the pointer as a file
  appended to the installed skill at setup, so the repo skill is B == C.
- L5 (2026-10-26): correction 2 — the bench switch is injected into the
  pipeline instead of mutated through the process environment, so tests
  never touch global state (review finding R3-001 on lineage
  review-d72a00e7b8c98455). `build_response` reads the variable once and
  delegates to `build_response_with_tier(..., bench_tier)`; the tier
  tests pass the flag directly, with no lock, no `set_var`/`remove_var`
  and no serialization.
