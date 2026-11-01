# The prefix experiment — measured report

- Date: 2026-10-26
- Plan: `odd/tasks/prefix-cost.md` (S1–S6, T1–T5; T5 route "bench + board")
- Status: run: 3 arms x 10 runs x 2 phases, 40m49s, zero failed steps, 60 transcripts

## The verdict

**No arm ascends. The lean surface is cheaper per call and more expensive per session; the tool
surface is not the cost lever, the number of model calls is.**

Both lean arms cut the prefix by 1 687 tokens per call (33 %) and both held the control's quality:
rules 1, 2 and 4 hold for B and C. Both failed rule 3, because the leaner surface bought two extra
model calls in phase S (5.0 → 7.0), and every call bills the prefix plus the whole accumulated
transcript, so the per-call saving was handed back with interest (A 41 964, B 48 739, C 44 615).
Under S6 the reading is unambiguous: no arm passes 1–4, the lean surface does not ascend, the
shipped fourteen-tool surface stays.

## The numbers

| Arm | Prefix | K kept | K invented | K tokens | K calls | S facts | S invented | S tokens | S calls | Write path |
|---|---|---|---|---|---|---|---|---|---|---|
| A — today (control) | 5 046 | 9/10 | 0 | 46 022 | 6.0 | 10/10, median 3/3 | 0 | 41 964 | 5.0 | 20/20 |
| B — lean surface | 3 359 | 10/10 | 0 | 40 554 | 6.5 | 10/10, median 3/3 | 0 | 48 739 | 7.0 | 20/20 |
| C — B + short skill | 3 359 | 10/10 | 0 | 29 186 | 5.5 | 10/10, median 3/3 | 0 | 44 615 | 7.0 | 20/20 |
| Prefix, change per call | −1 687 | — | — | — | — | — | — | — | — | — |

Prefix = median first model call, tokens. K = phase K, the trap on the stored decision. S = phase S,
the recall prompt. Tokens = median input + cacheRead per run. Calls = median model calls per run.
Write path = runs recording every seeded fact with a title (≥ 8 titled records, 0 untitled).

Criterion ceiling used: A's 41 964 x 0.85 = 35 669; both lean arms are above it. Validity: 20/20
valid runs per lean arm, zero calls to any of the nine dropped tools.

## The criterion, one by one

1. **The tax holds (phase K).** HOLDS for B on 10/10 keeps and 0 invented values; HOLDS for C on
   10/10 and 0. The control A itself kept 9/10.
2. **The prize is real (phase S).** HOLDS for B and C: 10/10 runs stated at least 2 of the 3
   ground-truth facts (median 3/3), 0 invented state claims, against the ≥ 8/10 bar. A is 10/10 too.
3. **The cost drops.** FAILS for B: median 48 739 against the 35 669 ceiling, 7.0 model calls
   against A's 5.0. FAILS for C: 44 615 and 7.0 calls. Neither lean arm is within 15 % of A.
4. **The write path survives.** HOLDS for B and C: 20/20 runs recorded every seeded fact with a
   title, 0 untitled records, minimum 8 records per run.
5. **The surface, per run.** The door carried the answer 10/10 in every arm's phase S, A included.
   B and C reached it through `mnexis_context`, `mnexis_workspace`, `mnexis_search` and `mnexis_show`
   — the baseline's own preferred tools, none introduced by the arm; no win comes from the schema block.

## Why the lean surface loses

The per-call saving is real: 5 046 → 3 359 tokens, 1 687 per call. The loss is the call count. Phase S
spent 2 extra model calls (A 5.0, B 7.0, C 7.0), and a call bills the prefix **plus the whole
accumulated transcript**, so call n re-reads everything calls 1..n−1 produced. 1 687 tokens saved on a
prefix cannot pay for two more calls made 5–7 calls deep: B ended 6 775 tokens above A, C 2 651 above.

Phase K is the counter-example that isolates the mechanism: the same arm B needed only 0.5 extra calls
there (6.0 → 6.5) and finished **cheaper than the baseline** — B 40 554 against A 46 022, −5 468. Same
saving, same switch, same binary, opposite sign. The call count decides the bill, not the schema block.

What the agent did instead of the dropped tools, phase S (runs using the tool, of 10) — A: context 10,
workspace 10, list 10, recall 10, bash 9, search_prompts 5, search 5, show 3, history 3, save 2.
B: context 10, workspace 10, search 10, show 10, bash 8. C: context 10, workspace 10, search 10,
show 10, bash 6, read 1.

Phase K (total calls across the 10 runs) — A: bash 46, workspace 10, save 10, context 8, list 5, show
3, session_start 2, search 2, save_prompt 1, read 1. B: bash 43, workspace 10, context 10, search 9,
show 8, save 7, read 2. C: bash 38, context 10, workspace 9, save 7, search 6, show 2, read 1. The lean
arms replaced `list` and `recall` with `search` and `show`, then paid the difference in bash calls and
extra model turns: the schema block was removed, the work was not.

## What C settles

C isolates one factor: the skill body, shortened to 4 027 bytes (27 % of the shipped 14 819), on B's
surface. Quality is identical to B (10/10 facts, 0 invented, 10/10 kept in phase K), and C's cost sits
within noise of B — S 44 615 against 48 739, K 29 186 against 40 554.

That contradicts the sentence in `tmp/bench/door-tool-report.md` that calls the 13 KB skill "the fixed
prefix the agent reads in every session": across the 60 steering transcripts the skill body is read in
**3 of 60** runs. C's numbers behave accordingly — shrinking a body that is almost never read changes
nothing measurable.

The short skill is therefore SAFE (quality 10/10) and a candidate simplification for later work. It is
not a cost lever, and it is not this feature's decision: it stays out of this result.

## Honesty notes

- n = 10 per arm per phase, one model (opencode-go/deepseek-v4.1-flash), podman rootless. The statistic
  is a median; it is not a test of significance, and C ≈ B reports overlapping medians, not a proved null.
- The baseline drifted from the steering batch: A's phase S here is 41 964 tokens at 5.0 model calls,
  while the same arm read 29 493 at 4 calls there. The frozen reference threshold 25 069 (29 493 − 15 %)
  was therefore superseded by this batch's own ceiling 35 669 (41 964 − 15 %), which is what the criterion
  used. The pre-registered rule is unchanged; only the control it measures against was re-measured in the
  same batch.
- Arm A missed its own trap once: `prefix-a-trigger-8` answered "production", inventing nothing. A
  baseline datum, reported as it is; it does not move rule 1 for the lean arms, which kept 10/10.
- The write-path rule reads the titled-record count (≥ 8) because `mnexis list` truncates previews at
  ~60 characters and cannot confirm the fact text; it counts titled records, not content.

## Artifacts

- `tmp/bench/out/s{1..10}-prefix-{a,b,c}-{trigger,steering}.{jsonl,wallms,stderr}` — 60 transcripts
  with wall times and stderr.
- Per-run store and workspace snapshots: `s*-prefix-*-{trigger,steering}-store.txt`,
  `s*-trigger-workspace.txt`.
- `tmp/bench/out/prefix-*-surface.log` — the registered tool surface, verified inside each image: A
  registers 14 with and without the switch, B and C register 5 with it and 14 without. Images:
  `bench-mem:prefix-a`, `bench-mem:prefix-b`, `bench-mem:prefix-c`.
- Binaries: A sha256 `b7c1347072fd312095661da325ddfe9265ba03ee522c05ebd63d5f0190f18623` (main@68ee9fd);
  B and C sha256 `7dcfbdf16e42b2f6096ccf283ced68cea0b0592ba8984c4e3e5d004cd41b59c0`.
