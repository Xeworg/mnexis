# The door carries whole facts again

**Feature:** `door-whole-content` · **Branch:** `feat/door-whole-content` ·
**Status:** to start

PR #92 priced the door's items (identity + a 160-character preview) and the
re-read metric was run against the merged binary. **The verdict was
negative** (`tmp/bench/reread-priced-measurement.md` and
`tmp/bench/reread-priced-report.md`): the re-read *rate* fell from 47.2 % to
31.6 %, but the session's median tokens grew **35 %** (37 290 → 50 464) and
its median memory calls **doubled** (4.5 → 9.0), because the preview cut the
facts and the agent fetched each one it needed — one `mnexis_show` per fact,
where the fat door's enumeration was **one** `list --full`. The redundant
**bytes** did not move (6 418 → 6 484): the agent read the same twice, and
paid for it in calls.

So the pricing is **reverted**. What the measurement *does* support stays:
the coverage declaration and the relevance order.

## Specs

**S1** — The door's item carries the **whole fact** again: the item's
`content` is the record's whole text, the `preview` field is gone, and the
`--full` flag is gone (there is nothing left to ask for). The door's budget
decides what fits — **whole items, never a cut fact** — exactly as before
PR #92.

**S2** — The **coverage declaration stays** (`coverage`: `candidates`,
`carried`, `left_out`, `left_out_items` with their uuid, title, kind and
size). With the whole content back, the declaration is the *only* way the
agent learns that the budget cut something — which is exactly its job: it
turns "the door answered three of six" into "the door answered three, and
these three did not fit, pull them with `mnexis show`".

**S3** — The **relevance order stays**: bm25 ASC, then `created_at` ASC. The
door carries the most relevant facts that fit, and what it leaves out is the
least relevant.

**S4** — The text answer prints the item's whole content and the
declaration. The uuid no longer rides the item line (nothing is cut any
more); the declaration's entries carry theirs.

**S5** — The extension renders the same answer, and its **description says
what the answer is**: the facts that match the task, whole, the session
state, and the declaration of what the budget could not keep.

**S6** — The documentation follows: the skill, the extension's README, the
root README and the board say the door carries whole facts again; the
`--full` flag is gone from all of them.

**S7** — Nothing else moves: the differential against the base leaves every
other surface byte-identical, and only the door's two modes change.

## The re-measurement (the criterion, written BEFORE the numbers)

The arm: `whole` — the reverted tree, the same container family, the same
fixture, the same prompts, the same model, N = 10. The baseline: the
**shape-b** arm (the fat door with the state block), whose numbers the
scorer still reproduces: median **37 290** tokens, median **4.5** memory
calls, a `mnexis_show` fan-out in **2/10** runs, re-read rate 47.2 %,
median redundant bytes 6 418.

- **P1 — the cost comes back.** The median tokens (input+cacheRead) of the
  steering phase are **≤ 40 000** (the fat arm's 37 290). The reverted door
  carries more text than the fat one (it has the coverage's lines and the
  relevance order), so the bar is the fat arm's level, not below it.
- **P2 — the fan-out does not come back.** The `mnexis_show` fan-out
  appears in **≤ 2 of 10** runs (the fat arm: 2/10; the priced arm: 7/10).
  This is the mechanism the measurement identified, so it is the criterion
  that matters.
- **P3 — the door still answers.** ≥2 state facts stated in **10/10** runs,
  **0** invented values, the stored default kept in the trap phase in
  **10/10**, no run without an answer.
- **P4 — the declaration is honest.** The scored transcripts are the ones
  the arm produced; a phase the model never answered is quarantined by the
  runner and counted as a failure, never as a data point.

The verdict will be written with the noise attached: **N = 10 with one
model** carries roughly ±15 points on a rate near 45 %, so the numbers
speak about a **direction**, never about "the agent now does X".

## Tasks

| ID | specs | route | commit |
| --- | --- | --- | --- |
| T1 | S1, S4 | inline | `feat(context): the door carries whole facts again` |
| T2 | S2, S3, S5 | inline | `feat(context): the coverage and the relevance order stay` |
| T3 | S6 | inline | `docs: the door carries whole facts again` |
| T4 | S7 + the re-measurement | inline | `test: the differential and the re-measurement of the whole-fact door` |

## Log

- **L1** — The human's decision, after the numbers: *"si realmente a largo
  plazo no aporta beneficio tomare la opción b, talvez luego podamos volver
  a experimentar con esto y si nos pueda aportar un beneficio"*. Option B
  is "revert the pricing, keep the coverage declaration" — the measurement's
  own recommendation.
- **L2** — The measurement that decided it: `tmp/bench/reread-priced-report.md`
  (the plan and the criterion: `tmp/bench/reread-priced-measurement.md`).
  The numbers: the re-read rate 47.2 % → 31.6 % (partly definitional — the
  metric only counts a redundancy when the fact was delivered **in full**,
  and a preview does not arm it), the redundant bytes flat (6 418 → 6 484),
  the median tokens +35 % and the median memory calls doubled, the
  `mnexis_show` fan-out 2/10 → 7/10, the quality untouched.
- **L3** — **The finding that reorders the product's beliefs**: the
  redundancy the re-read metric counted as waste was not waste. Reading N
  facts costs **one** call (`list --full`), not N (`mnexis_show` × N), and
  every extra model call re-bills the prefix plus the whole accumulated
  transcript — the regression over the ten paired runs gives **+10 574
  tokens per extra model call**. The lever is the number of calls (the
  call-budget experiment), not the size of the answer.
- **L4** — **T1, T2, T3 done** (S1–S6), implemented by two delegated
  workers and reviewed by the parent. The item carries the whole fact
  again; the `preview` field, the `ContextRequest.full` field and the
  `--full` flag are **gone from the code**, not merely unused; the coverage
  declaration and the relevance order are untouched (their tests pass
  before and after). The text answer prints each fact whole, with no uuid
  handle (nothing is cut any more) and the declaration's entries carry
  theirs. The documents (the skill, the extension's README, the root
  README) say what the door answers now; the skill's length did not grow.
  A doc worker caught that the declaration's own wording still said "the
  least relevant first" — the truth is the opposite (the omitted
  candidates are listed in rank order, so the ones that just missed the
  cut come first), and the string was fixed in the same change, in the CLI,
  in the extension and in their tests.
  Suites: 718 Rust tests, 137 node tests, clippy and fmt clean.
- **L5** — **T4 done** (S7): the differential against the base
  (`origin/main`, the priced door) leaves **twelve surfaces
  byte-identical** and moves only the door's two modes. And the
  **re-measurement** (the criterion written before the batch): the arm
  `whole` (N = 10, the same fixture, prompts and model) holds **all four
  points** — the median tokens of the steering phase are **38 902** (under
  the 40 000 bar, back at the fat arm's 37 290), the `mnexis_show`
  fan-out appears in **1 of 10** runs (the priced arm: 7/10), the facts
  are stated in 10/10 with 0 invented values and the trap kept in 10/10,
  and no run failed to answer. Run by run, the whole door is cheaper than
  the priced one in **8 of 10** steering runs.
  The honest caveat, written in the report: the **trigger phase** shows
  the whole door dearer in 7 of 10 runs against the priced door, with a
  spread that overlaps the other arms entirely (the difference is inside
  the noise) — and the direction makes sense: the door's answer rides in
  the transcript of every later call, so a bigger answer is a tax on the
  rest of the session. The door's answer has **two prices** — what it
  carries (the facts the agent would otherwise fetch one by one) and what
  it weighs — and the pricing bought the second and paid the first.
  Report: `tmp/bench/whole-content-report.md`.
- **L6** — **The board's final state comes after the merges**: PR #94 (the
  door in the MCP surface) and PR #95 (the board with the priced door's
  negative measurement) are open, and this branch is cut from `main`
  without them. The board's item 14/15 text has to be rewritten once (the
  pricing was measured, reverted, and the door carries whole facts); that
  is a separate docs PR, after the three are merged.
