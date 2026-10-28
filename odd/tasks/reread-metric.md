# The re-read metric — how many times the agent fetches a fact it already had

The first piece of the context-compression work is the measurement that does
not exist yet. The bench measures calls, tokens, quality and the invented
values; it never counts the redundancy: the calls that re-fetch a fact the
agent already had. This feature defines that metric, implements it over the
corpus that already exists, and reports the baseline it finds.

## Specs

- S1 (the user's direction, 2026-10-27, verbatim): "empieza por la metrica"
  — the metric comes first, before any change to the door, the skill or the
  state block: without it, every later improvement is priced by size and not
  by effect.
- S2 (the user's direction, 2026-10-27, verbatim): "recuerda que puedes usar
  podman si coupas hacer pruebas en entorno sandbox" — the sandbox is
  available for the batch; the metric itself needs none of it, because the
  corpus of the previous batches is already on disk.
- S3 (the premise, from the analysis of `billion-context`,
  `tmp/bench/../../billion-context` — the read of 2026-10-27). The
  competitor's metric set carries **re-fetch** as a first-class metric (the
  interaction cost of compression: how many times the model goes back for
  content it already had) and reports it next to the token bill and the
  recall probe. mnexis's bench has calls, tokens, quality, invented values
  and the write path — and no redundancy metric. The gap is not cosmetic:
  "the agent re-reads a fact" is the symptom of every failure mode the
  compression work targets (the door did not carry it, the door carried it
  and the agent did not believe it, the answer arrived in pieces).
- S4 (the definition of the metric — the object of this feature):
  1. A **record** is a memory as it appears inside a tool result: an object
     with an identity (`uuid` for mnexis, `sync_id` then `id` for engram) and
     optionally `content`, `preview` and `title`.
  2. A **delivery** is one record appearing in one tool result.
  3. A delivery is **full** when the record carries its whole `content`,
     **partial** when it carries only a `preview`, and **identity** when it
     carries neither.
  4. A delivery is **redundant-full** when that record's content was already
     delivered in full earlier **in the same run**; it is
     **redundant-partial** when only the record's identity or preview was
     delivered earlier. The state is per run: a record delivered in another
     run is never redundant.
  5. A **wasted call** is a memory-tool call in which *every* record it
     delivered was already delivered in full earlier in the same run —
     nothing new at all. A **half-wasted call** delivered at least one new
     record and at least one redundant one.
  6. Per run the metric reports: `memory_calls`, `deliveries`,
     `new_deliveries`, `redundant_full`, `redundant_partial`,
     `wasted_calls`, `half_wasted_calls` and `redundant_bytes` (the bytes of
     the redundant deliveries, counted as the length of the record's own
     serialised form).
  7. The **re-read rate** of an arm is `redundant_full / deliveries` — the
     share of the facts the agent was handed that it had already been handed.
     It is reported per arm with the medians and the totals, never as a
     single number without the runs behind it.
- S5 (the criterion, pre-registered — the metric is right, or it is not
  used):
  1. **Exactness.** On the hand-read transcripts the metric's counts match
     the manual reading, call by call. The report shows the manual reading
     next to the metric's numbers for those transcripts.
  2. **Arm-agnostic.** The metric computes on the mnexis arm and on the
     engram arm with no mnexis-specific rule: the identity field, the
     record shape and the tool-name family are all resolved at runtime.
  3. **No false positive.** A delivery of a record never delivered before in
     the run is never counted as redundant, and a record delivered in a
     different run is never redundant.
  4. **Reproducible and attributable.** The same corpus yields the same
     numbers, and every number in the report names the run that produced it
     (the transcript file), so a claim can be checked by reading one
     transcript.
  5. **The shared modules are not modified.** The metric imports `summarize`
     and `score-calls` as they are; nothing existing is edited to serve it.

## Tasks

- T1 (S4, S5) — this document: the definition and the criterion, written
  before any number is read. Route: inline. Commit: pending.
- T2 (S4, S5.2, S5.3, S5.5) — the scorer: `tmp/bench/run/score-reread.py`,
  arm-agnostic, importing the shared modules unmodified. Route: inline.
  Commit: pending (the harness lives outside version control; the archive
  copy is the artifact).
- T3 (S5.1, S5.4) — the run: the metric over the corpus that already exists
  (no model calls), plus the hand-read spot check whose numbers the report
  prints. Route: inline. Commit: pending. **Done** — the report is
  `tmp/bench/reread-report.md`.
- T4 (S3, S5.4) — the report (`tmp/bench/reread-report.md`) and the board
  (`odd/next.md`): the baseline the metric found, and what it means for the
  door, the skill and the state block. Route: inline. Commit: pending.
  **Done** — the board carries item 12.
- T5 (S2) — the batch, if the corpus cannot answer the question: the same
  arms, run in podman, with the criterion above unchanged. Route: inline,
  background. Commit: pending. **Not needed**: the corpus answered it, and
  a fresh batch would be an experiment, not a metric.

## Log

- L1 (2026-10-27, the user's request, verbatim): "empieza por la metrica
  recuerda que puedes usar podman si coupas hacer pruebas en entorno
  sandbox".
- L2 (2026-10-27): the metric is defined here before it is implemented, and
  the criterion (S5) is fixed before any transcript is read. The corpus
  (`tmp/bench/out`, 620 transcripts from the recheck, trigger, door,
  call-budget, scope and shape batches) is already on disk, so T3 costs no
  model calls; podman and the bench images (`bench-mem:*`) are present for
  T5 if the corpus turns out to be too thin to answer the question.
- L3 (2026-10-27): **the metric landed and the criterion holds** — S5.1 on
  three hand-checks (the JSON shape, the chrome's shape, the old batch's
  fan-out), S5.2 on the competitor's arm (its `sync_id` records and its
  markdown door answer are read without a mnexis-specific rule), S5.3 on
  nine self-test cases, S5.5 with `summarize.py` and `score-calls.py`
  imported and not modified. The baseline: **39.8 % of the delivered facts
  are repeats in the steering phase** (a median of 5.5 KB of repetition per
  run, a wasted call in 6 of 10 runs), 3.1 % in the old recall phase, and
  33.3 % for the competitor in the same phase. Two defects were found by
  the hand-checks and fixed before the numbers: a full delivery of a record
  seen only by identity was counted as redundant (it adds the content, so it
  is new), and the chrome's shape was invisible to the parser, which read
  as a **false zero** for the shape arm — a blind spot is now counted as
  `UNPARSED`, never as a zero. The report: `tmp/bench/reread-report.md`;
  the board: item 12 of `odd/next.md`.
- L4 (2026-10-27): **what the numbers change.** The waste is in the door,
  not in the payload: the door already carried the facts in full and the
  agent re-fetched them anyway (the `list --full` re-delivered 8 of its 10
  records). The cross-check round is a **coverage** problem, not a trust
  problem — so the door has to declare what it did not include. The order
  of the compression work moves: the door's answer before the skill.
