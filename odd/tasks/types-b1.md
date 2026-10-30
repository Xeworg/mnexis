# B1 — does the agent write the type? The measurement that gates the types programme

**Feature:** the Phase-1 gate of the memory-types plan (`odd/reports/memory-types-analysis.md`,
§6, board item 23). One bench arm, one pre-registered criterion, one verdict: **B2 (the
per-type cap) and B3 (the door organised by type) run only if B1 holds.**

**Status:** measurement in progress. The criterion lives in
`tmp/bench/types-b1-measurement.md`, written BEFORE the numbers, the way every batch of
this project wrote it.

## Specs

- **S1** (the report's own gate, `odd/reports/memory-types-analysis.md:224-230`):
  *"B1 — make the agent write the type. The measurement that decides whether the whole
  'structure the types' programme is admissible at all: the same fact keeps the same type
  in ≥ 8/10 runs (today ≤ 5/10), and the other facts are not typed the same way
  (≥ 8/10), and the median content of a typed record is ≤ 256 B. If the type is not
  written, no per-type rule is admissible — the structure would only be visible through
  free-text search, which already answers the steering question 10/10 for a median
  29 493 tokens."*
- **S2** (the board's restatement, `odd/next.md` item 23): *"the measurement that decides
  whether the whole 'structure the types' programme is admissible at all — does the agent
  write the type? Same fact, same type in ≥ 8/10 runs (today ≤ 5/10), the other facts not
  typed the same way, the median typed record ≤ 256 B. If the type is not written, no
  per-type rule is admissible."*
- **S3** (the engine's design premise being tested, `src/domain/memory_type.rs:3-9`):
  *"The type is an attribute the **agent writes**. The keywords lesson is the evidence: an
  attribute the agent sets is used (3 of 3 runs), while an attribute the engine guesses
  proposes noise."*

## Tasks

| ID | Spec | What | Route | Commit |
| --- | --- | --- | --- | --- |
| T1 | S1-S3 | the pre-registered criterion, before any number of the new arm | inline | this commit |
| T2 | S1 | `patch-extension-types.py`: ONE intervention — the `type` parameter description of `mnexis_save`, taught with the engine's own definitions (`memory_type.rs:16-27`), applied to the installed extension inside the run's home | inline | tmp/bench (gitignored) |
| T3 | S1-S2 | `score-types.py` + the baseline matrix from the merged arm's own transcripts (same scorer, both arms) | inline | tmp/bench (gitignored) |
| T4 | S1 | the batch: N=10 × (setup → patch → phase0 → store --json), image `bench-mem:merged`, model `opencode-go/deepseek-v4.1-flash`, the fixture and the prompt verbatim | inline | tmp/bench (gitignored) |
| T5 | S1-S2 | score the arm, apply the three gates, write the verdict | inline | tmp/bench (gitignored) |
| T6 | S1-S2 | the report `tmp/bench/types-b1-report.md`, the board item 23 verdict, the commit | inline | this commit |

## Log

- **L1** (2026-10-30, the user): *"dale"* — to *"¿Arrancamos con B1?"*, after the resume
  that showed the Phase 0 (A1-A4) delivered and B1 as the next step in order.
- **L2** (the parent, before the batch): the baseline is NOT a new measurement — it is the
  merged arm's own data (`tmp/bench/out/s<i>-merged-trigger-store.txt`, the door re-measure
  batch), read by the same scorer that will read the new arm. The new arm changes exactly
  one thing: the `type` parameter description. Everything else (binary, skill, model,
  fixture, prompts) is the merged arm's.
- **L3** (the verdict, 2026-10-30): **B1 FAILS the pre-registered criterion.** The agent
  writes the type **64/64** (the literal question is a yes; the shipped description's
  "the engine decides" was the corpus's defect, not the model's willingness). G2 holds
  9/10 (baseline 1/10 — the run-mood is gone), G3 holds (median 148 B), but **G1 fails:
  `deploy` `decision` 7/10 — the coached fact — and `contact` 6/10**; 8/10 and 7/10 under
  the collision-free reading, so the verdict is FAIL either way. **Consequence, as
  pre-registered: no per-type rule is admissible; B2 and B3 do not run.** Full numbers:
  `tmp/bench/types-b1-report.md`.
