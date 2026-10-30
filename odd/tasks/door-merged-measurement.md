# The door, re-measured against the merged binary — the criterion, written BEFORE the numbers

**Date:** 2026-10-30 · **Status:** the plan and the criterion, written before the
batch, the way every batch of this project wrote it.

## What this is

The board's last open debt (item 21): *"The door's re-measurement against the
merged binary. The door was priced, measured, lost and reverted (items 15 and
16); the revert was measured once, against the branch tree. Re-measuring
against the released binary needs the bench environment (docker image plus the
model provider)."*

The revert's measurement (the arm `whole`, N = 10) ran against **PR #96's
branch tree**. Since then `main` moved: the store recovery in the shipped
extension (#99), the lean skill (#100 inside #101), the door in the MCP surface
(#94), the state block, and the value filter (#105). The door's own answer did
not change — the whole-fact door with the coverage declaration is still what
ships — but **the extension that carries it did**, and the extension's shape
is part of what the agent reads. So the number the board carries is a number
measured against a tree that no longer exists.

This run re-measures the door **against the merged binary**: the same arm, the
same protocol, the same fixture, the same prompts, the same model, the same
scorer, and the criterion written **before** the batch.

## The arm

`merged` — the **merged** tree: `main` = `e5f429c` (PR #105, the value
filter) plus the version bump `6f39ead7` (`0.6.0-dev`), built as a release
binary and shipped in the bench image `bench-mem:merged` (FROM
`bench-mem:now`, with exactly one file replaced — the engine — so the
lineage is the one every other arm ran on).

The protocol, per run, unchanged: `setup` → `phase 0` (the agent writes the
knowledge facts) → `seed-state` (the three typed state records, deterministic)
→ `phase K` (the production trap: the tax) → `phase S` (a fresh session asks
*"¿por dónde quedamos?"*: the prize). **N = 10** runs, the model
`opencode-go/deepseek-v4.1-flash`, the `payments-api` fixture, the prompts
verbatim.

## The baselines

Both are the numbers the scorer still reproduces from their own transcripts:

| | the fat door (`shape-b`) | the whole door (`whole`, the branch tree) |
| --- | --- | --- |
| steering median tokens | 37 290 | 38 902 |
| steering median memory calls | 4.5 | 5.0 |
| `mnexis_show` fan-out | 2/10 runs | 1/10 runs |
| re-read rate | 47.2 % | 48.5 % |
| median redundant bytes | 6 418 | 7 530 |
| ≥2 state facts stated | 10/10 | 10/10 |
| invented values | 0 | 0 |
| the trap kept | 10/10 | 10/10 |

The bar is the **whole** arm's level, not the fat arm's: the shipped door is
the whole door, and the question is whether the merged tree moved it.

## The criterion (written before any run of the new arm)

The four points of `odd/tasks/door-whole-content.md` ("The re-measurement"),
applied to the merged binary:

- **P1 — the cost comes back.** The steering phase's median tokens
  (input+cacheRead) are **≤ 40 000** (the fat arm's 37 290; the whole arm
  measured 38 902). The bar is the fat arm's level, not below it.
- **P2 — the fan-out does not come back.** The `mnexis_show` fan-out appears
  in **≤ 2 of 10** runs (the fat arm: 2/10; the priced arm: 7/10).
- **P3 — the door still answers.** ≥2 state facts stated in **10/10** runs,
  **0** invented values, the stored default kept in the trap phase in
  **10/10**, and no run without an answer.
- **P4 — the delivery is honest.** The scored transcripts are the ones the
  arm produced; a phase the model never answered is quarantined by the runner
  and counted as a failure of the run, never dropped from the data.

**P5 — the door's weight against the fixed prefix.** The steering run
(item 20) narrowed what a re-measurement can change: the lever it tested was
negative, so what is left is the door's **weight** — the answer rides in the
transcript of every later call, so a heavier answer is a tax on the rest of
the session. The scorer reports the **fixed prefix** (the tokens every call
pays before the answer) separately from the answer's own weight; this run
reads both, and states them side by side with the `whole` arm's. This is a
**reading**, not a pass/fail: the criterion's pass/fail points are P1-P4.

## What the verdict will not claim

**N = 10 with one model is a measurement of a direction, not of a number.**
The board already says it: a rate near 45 % with n = 10 carries roughly
±15 points at one sigma. The verdict will therefore be written as the
direction of the change, whether P1-P4 held, and the size of the difference
**with its noise attached** — never as "the agent now does X".

## What is out of scope

- **No product change.** This is a measurement; anything that follows it is a
  separate feature with its own document and its own criterion.
- No re-running of the baseline arms (they were measured in this lineage; a
  re-run would measure the model's variance).
- The pruning lever (the door carrying less when the task is not about the
  facts) is a **different experiment** — it needs a product change, and the
  board says so.

## The artifacts

- the criterion (this document, written before the batch): the local copy is
  `tmp/bench/door-merged-measurement.md`
- the report: `tmp/bench/door-merged-report.md` (local) and the result below
- the transcripts: `tmp/bench/out/s<i>-merged-{trigger,steering}.jsonl`
  (with the per-run store and workspace snapshots)
- the scorer: `tmp/bench/run/score-priced.py merged` (the shape experiment's
  own rules over the re-read metric, unchanged) and
  `tmp/bench/run/score-reread.py` (only a batch pattern was added, as the
  `priced` and `whole` arms did)
- the image: `bench-mem:merged` (FROM `bench-mem:now`, the engine replaced)

## The result

**The steering phase (the prize) — the three arms side by side:**

| | fat (`shape-b`) | whole (the branch tree) | **merged (`main`)** |
| --- | --- | --- | --- |
| median tokens (input+cacheRead) | 37 290 | 38 902 | **40 495** |
| min–max | 28 022 – 55 062 | 30 346 – 46 872 | **21 169 – 56 763** |
| the spread | 27 040 | 16 526 | **35 594** |
| median model calls | 4.5 | 4.5 | **5.0** |
| median memory calls | 4.5 | 5.0 | **6.0** |
| `mnexis_show` fan-out | 2/10 runs | 1/10 runs | **0/10 runs** |
| the door called | 10/10 | 10/10 | **10/10** |
| ≥2 state facts stated | 10/10 | 10/10 | **10/10** |
| invented values | 0 | 0 | **0** |
| re-read rate | 47.2 % | 48.5 % | **44.7 %** |
| median redundant bytes | 6 418 | 7 530 | **5 275** |
| **the fixed prefix** (tok/call) | 5 135 | 5 269 | **5 341** |

**The trigger phase (the tax):** median tokens 44 361 / 55 607 / **46 101**;
the trap kept 10/10 in every arm; the write phase 20/20 with 8 records.

**The criterion, applied.**

- **P1 — the cost comes back: does not hold, and the miss is inside the
  noise.** The bar is a median of ≤ 40 000 tokens; the merged arm measures
  **40 495** — over by **495 tokens (1.2 %)**. Its own runs span 21 169 →
  56 763 (a spread of **35 594**, the widest of the three arms), and the
  paired view against the `whole` arm is **4 of 10 runs cheaper, 6 of 10
  dearer**, median delta **+3 885**, individual deltas from −22 319 to
  +18 930. A 1.2 % difference is a fortieth of the spread, and the paired
  view does not agree on a sign.
- **P2 — the fan-out does not come back: holds, and better than the
  reference.** **0 of 10** runs (the whole arm 1/10, the fat arm 2/10, the
  priced arm that made the mechanism famous 7/10).
- **P3 — the door still answers: holds.** 10/10 with the facts stated
  (median 3 of 3), 0 invented values, the trap kept 10/10, the door called
  10/10, no run without an answer.
- **P4 — the delivery is honest: holds.** 20 measured phases, 0 without an
  answer, 0 quarantined, none dropped.
- **P5 — the weight against the fixed prefix (a reading).** The prefix moved
  from 5 269 to **5 341** tok/call (+1.4 %; +4.0 % against the fat arm): the
  merged tree's own surface is heavier than the branch tree's, and that is
  paid by every call whatever the door answers. The **answer** did not get
  heavier: the trigger phase is *cheaper* on the merged arm in 6 of 10 runs
  (median delta −7 603) and its median is back at the fat arm's level — the
  branch arm's 55 607 (max 95 474) was the outlier.

**The verdict: the door does not move on the merged binary.** Three of the
four points hold outright and the fourth misses the bar by 1.2 %, inside the
noise the criterion itself warns about. The board's debt is closed.

**What it does not settle, and what it hands to the next experiment.** The
pruning lever — the door carrying the facts when the task is about the facts
and less when it is not — is a **different experiment** (it needs a product
change). What this run gives it is the base to beat: on the merged binary
the door's answer costs **40 495** tokens in the steering phase (median,
N = 10, one model, spread 35 594) and its **fixed prefix is 5 341 tok/call**,
paid whatever the answer says.

## Log

**L1** — the human's instruction, verbatim: *"hazlo tengo tiempo"*, answering
the board's last debt ("the door's re-measurement against the merged binary").
The environment it needed — the bench image plus the model provider — was
available this time.

**L2** — the criterion was written **before** the batch, and the order is on
the record: the criterion file's timestamp is 18:24:02 and the first
transcript's is 18:29:06 — 304 seconds apart. Nothing in the criterion was
edited after a number was read.

**L3** — the bench's key had expired and the human gave a new one
("la api key de test ya expiro toma esta"), saved outside the repository
(`~/.config/opencode/api-key`, mode 600, exported by the shell profile) and
copied into the bench's own `tmp/bench/secrets.env`. The key never reaches
the repository and never reaches a memory store.

**L4** — the first attempt used the wrong runner and was **restarted from
zero**: `run-steering.sh` does not snapshot the store after each measured
phase, and the scorer reads that snapshot (`score-priced.py` reads
`<stem>-store.txt` for the write measure and `<stem>-workspace.txt` for the
deploy verdict). `run-shape.sh` — the runner the `whole` arm itself used —
snapshots both. The aborted attempt's files were removed and the batch was
re-launched with `run-shape.sh` (N = 10, 17 minutes, 0 quarantined phases).

**L5** — one scorer change, the same one the `priced` and `whole` arms got:
a batch pattern in `score-reread.py` (`^s(\d+)-(merged)-(trigger|steering)$`).
The metric itself is untouched.
