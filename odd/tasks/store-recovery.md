# Store recovery, productized

**Feature:** `store-recovery` · **Branch:** `feat/store-recovery` · **Status:** planned
**Board:** `odd/next.md` item 18 (the enforcement arm)
**Evidence base:** `tmp/bench/skill-lean-report.md` `## The enforcement arm`
(R1 10/10 against 0/10 at +72 tokens per call), `odd/tasks/extension-enforcement.md` L4

The enforcement batch measured one mechanism that changed the store where four
batches of advice could not: when a second save creates a second live record
for one fact, the shipped extension's own `tool_call` hook repairs it. This
feature moves **that half only** into the shipped extension. The context
recovery is not productized: it answered nothing the advisory arm could not
answer and cost +179 tokens per call.

## Specs

**S1** — The behaviour is the one that was measured: on `mnexis_save`, when the
save would leave **two live records for one fact**, the extension supersedes
**every** qualifying live record onto the one the save created, so the fact ends
with **exactly one live record** and the superseded rows are kept. The model is
not asked and does not need to remember a rule.

*Amended after verification:* the first implementation retired only one twin per
save and read them from a page the engine returns oldest-first, which made the
repair silently inert past 50 records in the namespace and left N−1 twins alive.
"One live record" is the promise; retiring one of them is not it. See L4.

*Also declared, not hidden:* the candidate search is bounded — a ranked answer
of 100 records per value, and an exhaustive walk of the namespace that stops
after 2 000 records. In a namespace larger than that the check **declares
itself unfinished in the answer the model reads**, naming the engine's reason
and warning that a second live record may remain. Raising the ceiling is a
separate decision; claiming a repair that could not be verified is not an
option, and a silent ceiling is the very defect this feature was refused for
(see L5).

**S2** — "One fact" is tightened, not loosened, against the bench rule. The
bench identified a twin by one shared numeric token of three or more digits.
The product requires **all** of:

1. both records **active** and in the **same namespace** as the save;
2. both carrying the **same numeric value** (as measured);
3. **at least one shared keyword** (new: a narrowing that the bench did not
   need and the product does, because a shared number alone also matches
   unrelated facts such as a port and a worker count);
4. neither record being a compaction summary.

**S3** — The repair never deletes: it supersedes. A superseded row stays
**visible** — `mnexis list`, `mnexis show` and `mnexis timeline` report it as
`superseded` — and it is not touched outside the workspace's own bound,
writable namespace: the same guard the shipped `session_compact` hook already
applies (`workspace.bound`, `getWriteNamespaces`).

*Amended after verification:* this spec first claimed `mnexis_history` still
shows the superseded row. It does not — the engine writes no `memory_history`
row on supersede (`src/storage/memory_repo.rs:700-705`, "no history row, by
parity") and answers `no history for <uuid>`. The claim was false in production
and a test had locked it in, because the test's fake fabricated the row the
engine never writes. See L4.

**S4** — The repair is observable, never silent: the tool result the model
reads names both records and the shared value, and the extension appends one
line per intervention to its existing diagnostic path. A user who dislikes the
behaviour can see it, and a future measurement can count it.

**S5** — Symmetry with what was measured: the *newer* record survives, which is
the engine's own "the latest write is current" rule and exactly what R1
measured. Changing which side wins would make the product behave differently
from the thing that was verified, and the measurement would stop applying.

**S6** — Test-first: every rule of S2 gets a test that fails without it, and the
three failures the bench produced are the regression corpus — the twin that must
be repaired, the value change that must **not** be repaired (a conflict, not a
twin), and a shared number with no shared keyword that must **not** be touched.

**S7** — Only the extension moves. No CLI, engine, schema or skill change is in
scope, and the shipped skill body keeps whatever it says today.

## Tasks

| id | what | specs | route | commit |
| --- | --- | --- | --- | --- |
| T1 | the twin rule and its guards, test-first | S1-S3, S6 | worker | feat(extension): repair the twin a second save creates |
| T2 | observability: the tool result names both records, one log line per repair | S4 | worker | (same commit) |
| T3 | the three-case regression corpus | S6 | worker | (same commit) |
| T4 | independent verification of the data effect | S1-S3, S5 | verify | — |
| T5 | board and README line | S4, S7 | inline | docs: the extension repairs a twin |

## Log

**L1** — the human's instruction, verbatim: *"aplica la mejora"*. In context it
answers the open question of item 18 — productize the store half of the
enforcement arm — and not the other two (ship `fat`/`head`, correct the
doctrine scorer's artifacts), which stay open.

**L2** — why the rule is hardened instead of copied. The bench's twin signature
is one shared numeric token, and its supersede direction is older→newer. Copied
verbatim, a save that mentions "8081" as a worker count could silently retire an
unrelated record that mentions the port 8081, and nothing in the store would
warn. S2 adds the keyword requirement — a narrowing only — and S5 keeps the
direction that was measured. The cost of the narrowing is stated plainly: the
product may miss a twin that the bench would have caught, and that is the
correct failure mode for something that writes to the user's memory.

**L3** — what is deliberately not shipped: the context recovery (R3 bought no
answer and cost +179 tokens per call), the prompt injection (four batches say
it buys nothing), and any change to the skill.

**L4** — independent verification (gentle-ai-verify, real engine 0.5.0-dev in an
isolated data dir) refused the first implementation with four blockers, and it
was right on all four:

- **B1, the fatal one.** `mnexis list --limit 50` answers the **50 oldest** rows
  (`src/storage/memory_repo.rs:99-104`, `newest_first = false`). The repair
  looked for the record the save had just written in that page, so in any
  namespace with more than 50 records it found nothing and returned silently:
  no supersede, no entry, no line. Reproduced with 64 records and a real save —
  **five live records for one fact, no repair**. The code comment claiming a
  newest-first page was factually wrong.
- **B2.** Same root cause: the twin it retired was the oldest in the page, not
  the one the save stands beside.
- **B3.** One repair per save leaves N−1 live twins. S1 promised one live
  record; the code delivered "one fewer".
- **B4.** A test locked in a fiction — the fake's `history` fabricated a row the
  engine never writes, so S3's "still visible to `mnexis_history`" passed in the
  test and failed in production.

The verifier also reported that the fake mirroring the engine (list order,
history, supersede rules) made the whole corpus weaker than it looked, and that
a failed repair is entirely silent. Both are in scope for the fix. What the
verification confirmed as sound, and is not being redesigned: the hardened
predicate (identical numeric signature, shared keyword, namespace, active,
summary excluded), the supersede direction, the permission guards, the
observability through the tool result and `pi.appendEntry`, and the bounded read.

**L5** — the second verification round closed B2, B3 and B4 and left **B1 open at
its edge**: the exhaustive walk stops after 2 000 records, and a twin newer than
that boundary stayed live in a 2 001-record probe (two live records, one
journaled failure). The decision taken here is not to raise the number — every
number has a boundary — but to **declare the ceiling**: the answer the model
reads now says the duplicate check did not finish, names the engine's reason,
and warns that a second live record may remain. That turns a silent gap into a
declared one, which is the difference between the defect that refused the first
implementation and a documented limit. Raising the ceiling (an engine filter on
value instead of a walk) is a separate feature, and it is on the board.

The round also confirmed as sound: the value-based candidate query (a short
ranked answer is a complete one), the every-twin repair loop (three twins → one
live record, proven against the real binary), the fake now mirroring the engine
(oldest-first `list`, no history row on supersede, the engine's three supersede
refusals), failure journaling for search, show and supersede, and no collateral
change to a save that needs no repair. `mnexis_list`'s description claimed
"newest first", which the engine does not do — the belief that produced B1 —
and it is corrected here.
