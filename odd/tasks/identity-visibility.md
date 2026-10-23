# identity-visibility

The engine has the identity of a fact — the `topic_key`, unique per
namespace, derived from the title — and the relation that works
(`supersedes` + `was replaced by`). What it does **not** have is the
ability to **show** that identity or to **find** it.

The measurement of `near-duplicate-titles` said why the agent's own recall
usually saves the day and why it sometimes doesn't:

- `mnexis search` returns `{uuid, rank, namespace, memory_type, state,
  revision, created_at, valid_from, valid_until}` — **no title, no content,
  no topic_key**;
- `mnexis list` returns the `topic_key` and a preview **of the content** —
  **no title**;
- only `mnexis show` carries the title.

So the agent searches, gets uuids it cannot interpret, and opens them one
by one until it guesses. In the run that failed (`dup4-a1`) it opened three
wrong records before giving up, and the recall of that run cost 16 809
tokens hunting for the current value through the namespace, the database
and the git history.

And FTS5 indexes `content` only: a record whose content is `port = 8081`
does not come up when you search `puerto`.

This feature makes the identity **visible** and **findable** — and fixes the
two defects the measurement found on the way, one of which loses a fact.

## Specs

- **S1** — "The title travels with what the agent reads". `mnexis search`,
  `mnexis list` and `mnexis context` carry the title of every item they
  return, in the JSON envelope and in the human rendering. A search result
  says what it **is**, not only what it is not.
- **S2** — "The title is in the index". The full-text index covers the
  title and the topic key, not only the content: searching `puerto` finds
  the record titled `puerto del servicio` even when its content says
  `port = 8081`. The index is rebuilt, the doctor knows how to check it, and
  the reindex is the same operation everywhere.
- **S3** — "A save never writes into a record that is not alive". Saving
  with the title of a record that was superseded or deleted must not write
  the new value inside it: that record stays invisible to the search and to
  the index, and the fact is lost. The save refuses with an error that says
  what happened and what to do.
- **S4** — "The on-demand tools answer in the workspace's namespace".
  `mnexis duplicates` and `mnexis suggest-topic-key` resolve the namespace
  of the current workspace the way `mnexis save` does, instead of falling
  back to the profile's default namespace and being refused.

**Not in scope:** the topic entity, the graph, the concepts, any semantic
layer, and the second kind of relation. Three independent analyses of the
problem (engine, agent behaviour, identity architecture) said the same
thing: the defect that was measured is one of **recall and visibility**,
not of ontology, and the graph that exists today is 27 nodes with at most
one relation each.

## Tasks

- **T1** (S3) — The upsert refuses to write into a record that is not
  active, with an error that names the situation and the way out. Route:
  inline. Commit: `fix(save): a save does not write into a dead record`.
- **T2** (S4) — The extension's on-demand clients resolve the workspace
  namespace. Route: inline. Commit:
  `fix(extension): the on-demand tools answer in the workspace namespace`.
- **T3** (S1) — The title travels in the read responses. Route: inline.
  Commit: `feat(read): the title travels with what the agent reads`.
- **T4** (S2) — The title and the topic key in the full-text index:
  migration 006, the triggers, the reindex, the doctor. Route: inline.
  Commit: `feat(search): the index knows the title`.
- **T5** — Verification: the suites, and the experiment re-run with a new
  arm — **arm C: this branch, without the proposal**. The number that
  decides whether `near-duplicate-titles` is still needed: if the agent
  resolves the duplicate on its own with the title visible and findable,
  the proposal is redundant and the branch goes. Route: inline. Commit:
  `docs(odd): close identity-visibility with its measurement`.

## Log

- **L1** (2026-10-23) — The operator asked for the problem to be analyzed
  by three independent agents before taking a decision. Three explore
  agents read the same problem from three angles — the engine and its data,
  the agent's behaviour, and the identity architecture — and **converged**:
  the first move is the title in the index and in the read responses, the
  experiment that decides is an arm C without the proposal, and the topic
  graph with concepts is not justified by anything that was measured.
- **L2** (2026-10-23) — What the three analyses found that nobody knew,
  and what this feature is made of:
  1. **The engine hides the identity.** `search` returns no title, no
     content and no topic key; `list` returns the key and a content
     preview, but no title. Only `show` has it. The agent searches and
     gets uuids it cannot interpret.
  2. **A save writes into a dead record** (verified at runtime): the
     occupier of a key is found "any state, any deletion status" and the
     upsert keeps the state, so saving with the title of a superseded
     record writes the new value inside it — invisible to the search and
     to the index. The value 7070 was not findable: 0 hits. The current
     fact was lost.
  3. **The on-demand tools do not resolve the workspace namespace**: 26
     calls were refused with FORBIDDEN across the 24 runs of phase 2. The
     agent worked around it by passing the namespace by hand.
  4. **The skill is read in 10 of 24 runs** of phase 2. A mechanism that
     depends on the agent *knowing* a rule is fragile: what gets obeyed is
     what arrives inside the tool's answer with the command ready.
  5. Smaller: the doctor compares row counts and not the schema, so a
     stale content-only index passes; the reindex is not atomic outside
     `doctor --repair`; the migrations do not wrap the DDL and the ledger
     in one transaction; the proposal only travels with a save that
     *creates*; the shortlist reads every title of the namespace with no
     bound on every new save.
- **L3** (2026-10-23) — The semantics of S3, decided before writing it:
  the chain `X → Y` (X was replaced by Y) means Y is the current record of
  that fact, so reviving X would contradict the chain — and the cycle check
  that already exists would refuse it. Releasing the key instead would
  create a second record for a fact the engine knows about, which is the
  duplicate this cycle spent three features on. So the save **refuses**,
  and the error says what happened and what to do. The refusal is rare: it
  only fires when the key is held by a record that is not alive.
- **L4** (2026-10-23) — Branch `feat/identity-visibility`, based on `main`
  at `f3a590a`. The proposal branch (`feat/near-duplicate-titles`, 8
  commits) stays where it is: it is arm B of the experiment. If the
  experiment says the proposal is still needed, it is rebased onto this
  branch; if it says it is not, it is archived with its measurement.
- **L5** (2026-10-23) — **The measurement** (four scenarios, three runs per
  arm, paired: the same runs, the same day, the same model; the report in
  `tmp/bench/visibility-measurement.md`):

  | | A: main | C: this branch |
  | --- | --- | --- |
  | the agent linked the two records | 7/12 | **11/12** |
  | the fact ended in one current record | 9/12 | **11/12** |
  | the recall answered the current value | 12/12 | 12/12 |
  | tokens in the recall (mean) | 3 054 | 3 103 |

  In the hard scenario — a busy namespace, a content that does not repeat
  the title's words, no hint that the fact exists — today's engine left
  the fact in two records in **three of three** runs; this branch, in two
  of three.

  The honest limits: n = 3 per arm per scenario, the difference is two
  runs out of twelve, and the variance is large — the same image and the
  same prompts measured 11/12 in the earlier measurement and 9/12 in this
  one. What the direction says across four independent scenarios is more
  than what one scenario's number says.

- **L6** (2026-10-23) — What this says about the parked proposal: the
  **visibility is worth more than the proposal**, and it removes the
  reason the proposal existed. The agent can see what it found (the title
  travels with every answer) and find it by the title's words (the index
  covers it). The proposal's shortlist compares titles with a similarity
  the measurement already showed does not separate; the visibility does
  not have to separate anything — it shows the truth and lets the agent
  judge it.
- **L7** (2026-10-23) — The two defects this branch fixed are of a
  different kind, and both were found by the measurement of the *previous*
  feature: the upsert that wrote into a dead record (a fact lost, 0 hits)
  and the on-demand tools that answered in the wrong namespace (26 refused
  calls). Neither needed a measurement to justify the fix: both had a
  correct behaviour that was simply not implemented.
