# The enriched-nudge bench — does the enriched question convert judgment into typed saves?

The trim-nudge bench could not measure the nudge: its fixture saturated (the baseline saved
9/10 on its own; the nudge fired 0/10). The enriched nudge itself now ships on
`ext/nudge-enriched` (`3d18acc`). This bench measures it on a **de-saturated fixture** —
work that changes files, has genuinely memorable content, but forces no decision and no ADR.

## Specs

- **S1 (verbatim, human, 2026-11-01)** — "primero pruebalo haz un bench para esa rama": measure
  the enriched nudge BEFORE any merge decision on `ext/nudge-enriched`.
- **S2 (the fixture, de-saturated)** — `tmp/bench/fixtures/sh-check/`: a small repo whose
  `make check` FAILS because the Makefile recipe uses bash-only syntax (`[[ ... ]]`) while
  `/bin/sh` runs recipes. The prompt asks exactly: fix `make check` so it passes, without
  changing what it checks. No decision, no version pin, no ADR, no preference stated. The
  memorable content is the *discovery* (make recipes run sh, not bash) — precisely the kind the
  enriched nudge names ("a discovery about how this repo actually works"). The prompt never
  mentions memory.
- **S3 (the arms, one variable)** — same image lineage, same engine (built from `f3c4b83`, which
  carries the trust batch), one fixture, one prompt, one model (`opencode-go/deepseek-v4.1-flash`).
  The arm is the installed `extensions/mnexis-memory/index.js`:
  - `nonudge`  <- `80c3882` (trimmed tools block, no S2, no S4) — sha256 `4148ee85…`, the file
    preserved from the trim-nudge bench
  - `plain`    <- `f3c4b83` (main: S2 close record + the plain settle nudge)
  - `enriched` <- `3d18acc` (ext/nudge-enriched: S2 + the enriched nudge, 866 B)
- **S4 (the gates, pre-registered before any run — copied verbatim into the criterion doc)**:
  - **E-G0 (de-saturation).** The `nonudge` arm saves in **≤ 6/10** runs. If it saves ≥ 7/10 the
    fixture is saturated again: report, no nudge verdict. If it saves 0/10 the baseline is a
    floor: the nudge's effect is measured against zero and reported as such.
  - **E-G1 (the boundary is reached).** Nudge entries appear in **≥ 3/10** transcripts of EACH
    nudge arm. Fewer means the model pre-empts the question: report, no enrichment verdict.
  - **E-G2 (conversion).** Share of runs with ≥ 1 model save: **plain − nonudge ≥ 0.3 AND
    enriched − nonudge ≥ 0.3.** A nudge that does not convert is not saved by better wording.
  - **E-G3 (the enrichment buys typing).** Among MODEL saves in `enriched`, **≥ 2/3 arrive
    typed** (an explicit type other than the `fact` default, or `fact` chosen by name).
    `plain`'s typed share is reported for contrast, not gated.
  - **E-G4 (no noise).** Human read of every save in `enriched`: **zero fabricated facts** and
    **≥ 2/3 state something this session established** (the sh-not-bash discovery, the shape of
    the check, the fix).
  - **E-G5 (the bill).** mean cost(enriched) ≤ **2 ×** mean cost(plain).
- **S5 (save counting, unchanged from the trim-nudge bench)** — a save is a successful
  `mnexis_save` the MODEL made (transcript `tool_execution_end`, `isError: false`); refusals
  counted separately; the canonical `estado al cierre` close record (S2, code-written) is
  excluded from every count. N = 10 per arm, 30 measured sessions; a transcript with no
  assistant answer is not data and is re-run once.

## Tasks

| ID | Specs | Route | Branch / worktree | Commit |
| --- | --- | --- | --- | --- |
| T1 | S2, S3 | worker: criterion doc FIRST, fixture, arms, engine image, runner, pre-flight | `bench/enriched-nudge` @ `tmp/wt-bench2` | — |
| T2 | S1, S4, S5 | parent: verify the criterion preceded the runs, run the measured batch, score, report | — | — |

## Log

- **L1 (2026-11-01, user)** — "primero pruebalo haz un bench para esa rama": the bench comes
  before the merge decision on `ext/nudge-enriched`.
- **L2 (evidence carried over)** — `tmp/bench/trim-nudge-report.md`: ceiling at 9/10 baseline
  saves, nudge fired 0/10, silence free (C cheapest arm), S2 close record 10/10. The engine at
  `f3c4b83` adds the trust batch (A8 silence, A9 timeout, S1 ambient-value twin repair).
- **L3 (2026-11-02, T2 result, parent-verified)** — 30/30 measured runs, 0 NO ANSWER, end state
  10/10/10 in all three arms. **E-G0 FAILS by its letter** (nonudge saved 7/10, the guard says
  ≥7 = saturated = no nudge verdict) — but the raw data contradicts the conclusion it protects
  against: **the nudge fired in 8 runs across the two nudge arms and converted 8 of 8** (3/3 plain,
  5/5 enriched), which is only possible because the model had saved nothing in them. E-G1 passes
  (3/10, 5/10), E-G2 passes at the metric's CEILING (both deltas exactly +0.300, and with a 7/10
  baseline 0.300 is the maximum expressible), E-G3 passes but is uninformative (typed share 100%
  in ALL THREE arms — the tools block already exposes the types), E-G4 passes (human read: zero
  fabricated facts; every save states what the session established), E-G5 passes (1.14× ≤ 2×).
  **Reading: the S4 MECHANISM works (first positive evidence for the nudge); the ENRICHMENT is
  indistinguishable from the plain nudge on every gated metric, at +14 % cost, with one
  measurable difference — type diversity (4 kinds vs 2: the enriched arm produced an episode and a
  project_state the plain arm never did), N=10 so a signal, not a proof.** Formal verdict per the
  criterion: E-G0 fails = no nudge verdict. Recommendation: the enriched nudge stays (neutral, mildly
  positive diversity); a decisive plain-vs-enriched comparison needs a fixture with a ~5/10
  baseline (lifts E-G2's ceiling) and E-G0 re-specified to measure headroom directly (runs where
  the nudge fired) instead of the baseline's save share. Full report: `tmp/bench/enriched-report.md`.
