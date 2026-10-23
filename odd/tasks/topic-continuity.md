# topic-continuity

Make the engine know which topic it already holds, so one fact stays one
record and the recall does not have to read two.

This is what the conflict measurement asked for. The measurement (three
arms, same container, same model, a fact that changes) showed that a
conflict layer would change nothing: every arm answered the current value.
What it changed was the **topic key**: when the agent reuses it, the store
keeps one record and the recall costs **1 363 tokens instead of 5 455**.
When it does not, the store keeps a superseded record and the agent has to
list and read both to find out which one is current.

## Specs

- **S1** — "The engine notices the topic". When a save's content overlaps a
  record the namespace already holds, the engine says so and names the
  record: its topic key and its uuid. It is a **notice in the answer**, not
  a refusal: the save still happens, and the agent may ignore it. Nothing
  that works today stops working.
- **S2** — "The agent can ask". `mnexis suggest-topic-key "<content>"`
  answers the same question on demand: the key of the record that holds
  this topic, if there is one.
- **S3** — "A replaced fact says so". A memory that was superseded says so
  where the agent reads it — in the search and list results — with the uuid
  of the memory that replaced it, and the newer one names what it
  replaced. The recall stops needing a `list` plus a `show` to find out
  which one is current.
- **S4** — "The agent is told what to do with it". The skill says what to
  do when the engine names a topic (use that key, so the record is
  updated instead of duplicated) and what a "superseded by" line means (the
  other one is the current one).

**Not in scope:** the conflicts layer of the other engine (candidates,
verdicts, relations): the measurement showed the recall is already right
without it, and this feature is what the measurement did find.

## The measurement that shaped it

| arm | what the agent was given | store at the end | recall tokens in |
| --- | --- | --- | --- |
| A | nothing | 2 records (the old one superseded, the new one with the **value inside the key**) | 5 455 |
| B | "the value changed, update the record" | 2 records (the new one with no key) | 8 568 |
| C | the topic key of the old fact | **1 record** (the engine's conflict refused the duplicate and the agent updated) | **1 363** |

All three answered the current value. The difference was the shape of the
store and what the recall cost. The finding: the agent **saw** the old key
in the `mnexis show` output and still saved the new value under
`payments-api/listen-port-9090` — a key with the value inside it guarantees
a second record the next time the value changes. And `TOPIC_CONFLICT` only
fires when the caller reuses the key **exactly**: today the engine never
notices "this is the topic you already have".

## Tasks

- **T1** — The overlap measure: how much of a content is already in
  another one, over the significant words, with a threshold and a floor.
  Pure, deterministic, tested. Route: inline. Commit: `feat(topic): the
  overlap between two memories`.
- **T2** — The notice and the suggestion: the save's answer names the
  record that holds the topic, and `mnexis suggest-topic-key` answers it on
  demand. Route: inline. Commit: `feat(save): name the topic the engine
  already holds`.
- **T3** — The supersede relation in the results: the search and list
  answers say which memory replaced which. Route: inline. Commit:
  `feat(search): a replaced fact says so where the agent reads it`.
- **T4** — The agent-facing surface: the extension's tools, the skill and
  the docs. Route: inline. Commit: `docs(skill): what to do with a topic
  the engine names`.
- **T5** — Verification: the suites, and the conflict experiment re-run
  against the new engine (the recall should cost what arm C cost).
  Route: inline. Commit: `docs(odd): close topic-continuity`.

## Log

- **L1** (2026-10-23) — The operator took the recommendation to measure
  before building. The measurement (in `tmp/bench/conflict-measurement.md`)
  answered the question and changed the plan: the conflict layer is not
  worth it, the topic is.
- **L2** (2026-10-23) — Branch `feat/topic-continuity`, based on `main` at
  the merge of PR #25.
