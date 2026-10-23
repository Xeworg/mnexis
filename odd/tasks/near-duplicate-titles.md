# near-duplicate-titles

The engine cannot tell that two **different** titles are the same fact
(`puerto del servicio` and `puerto de pagos` are two keys and two records).
This feature gives the engine the smallest thing that helps: a **shortlist of
the records that are probably the same fact**, so the agent — which does
understand language — can link them instead of writing a second record.

The measure came first, and it ruled out the obvious design. A threshold on
the title cannot work: on the key, `puerto-del-servicio` and
`puerto-de-pagos` (one fact) score 0.870 under Jaro-Winkler, while
`postgres-dsn-de-pagos` and `postgres-dsn-de-billing` (two facts) score 0.910.
A *shortlist* is a different question and it has an answer: over a namespace
of 24 titles, the record a new title duplicates is in the first three
candidates 13 times out of 17. So the engine
ranks and proposes; the agent judges.

## Specs

- **S1** — "The engine can rank the records that are probably the same fact".
  Given a title, the engine ranks the namespace's records by how likely they
  are to be the same fact, using IDF-weighted overlap over the titles: the
  corpus is the namespace's own titles, so the words everybody uses carry
  almost nothing, and **no stopword list exists**. It returns the top
  candidates with their title, key, uuid and state.
- **S2** — "The proposal carries the evidence, not a verdict". Every
  candidate says its score and the words it shares with the query, so the
  agent judges the proposal instead of trusting it. The engine never says
  "these are the same fact": it says "these are the records your title looks
  like".
- **S3** — "The save says what the namespace already holds". A save whose
  title is new carries the shortlist with it, so the agent learns about the
  record it is about to duplicate **at the moment it would create it**.
- **S4** — "The agent adjudicates with the relation that already exists".
  The proposal names the exact command to run
  (`mnexis supersede --older A --newer B --reason "..."`). No new kind of
  relation, no new storage: the fact's history stays in the supersede chain
  and the reason says why.
- **S5** — "The agent can ask on demand". `mnexis duplicates --title "<title>"`
  and the MCP tool `memory_scan_duplicates` return the same shortlist, for
  the agent that wants to look before it saves.
- **S6** — "The skill says what to do with a proposal". The skill tells the
  agent that a duplicate title is possible, what the proposal means, and
  that linking is a supersede with a reason.

**Not in scope:** embeddings, a second relation kind (the `memory_links`
CHECK allows only `supersedes`), the conflicts layer with verdicts, and
merging the content of two records (destructive, and the supersede chain
already carries both).

## Tasks

- **T1** (S1, S2) — The ranker, in the domain layer, pure and tested: the
  IDF-weighted overlap over titles, the score, the shared words, the order.
  Route: inline. Commit: `feat(topic): the records a title looks like`.
- **T2** (S1) — The repository read it needs (the titles of a namespace) and
  the service method that ranks them. Route: inline. Commit:
  `feat(service): the duplicates of a title`.
- **T3** (S3) — The save carries the shortlist. Route: inline. Commit:
  `feat(save): the save says what the namespace holds`.
- **T4** (S5) — `mnexis duplicates --title` and the MCP tool
  `memory_scan_duplicates`. Route: inline. Commit:
  `feat(cli): the duplicates of a title`.
- **T5** (S4, S6) — The skill, the extension and the docs: what a proposal
  is, and that the answer is a supersede with a reason. Route: inline.
  Commit: `docs(skill): a duplicate title is a link to make`.
- **T6** — Verification: the suites, and the experiment re-run (three runs
  per arm, records, links and recall tokens). **The measurement decides
  whether the feature is kept**: if the agent ignores the proposal, it goes
  the way the conflicts layer and the protocol block went. Route: inline.
  Commit: `docs(odd): close near-duplicate-titles with its measurement`.

## Log

- **L1** (2026-10-23) — The board's item 1, verbatim: "mnexis still cannot
  tell that two different titles mean the same fact. That is item 1 above."
  and "This is the small version of the conflicts layer that was measured
  and discarded in its candidates-and-verdicts form: not 'the agent judges
  every conflict', but 'the engine notices two titles that are probably the
  same fact and proposes the link'. Measure it with the same harness before
  building it: does the agent act on a proposed link?"
- **L2** (2026-10-23) — The operator's answer: "si dale".
- **L3** (2026-10-23) — The measurement of the detector, before any code
  (`tmp/bench/title-similarity-probe.py`, 19 labeled pairs):
  - Jaro-Winkler on the key: the same fact scores 0.532–0.973 and two
    different facts score 0.472–0.910. **The ranges overlap almost
    completely**: no threshold separates them. At 0.84 the measure keeps 5 of
    10 same-fact pairs and 5 of 9 different-fact pairs — a coin.
  - Normalized Levenshtein, segment Jaccard and segment containment: worse.
  - So the engine cannot *decide*. What it can do is *propose*.
- **L4** (2026-10-23) — The measurement of the shortlist, the question that
  a proposal actually has to answer (`tmp/bench/title-shortlist-probe.py`, a
  namespace of 24 titles, 17 titles that duplicate something):

  | generator | recall@1 | recall@3 | recall@5 | found |
  | --- | --- | --- | --- | --- |
  | Jaro-Winkler on the key | 6/17 | 11/17 | 11/17 | 17/17 |
  | shared words | 7/17 | 10/17 | 11/17 | 14/17 |
  | shared words, no function words | 9/17 | 13/17 | 13/17 | 13/17 |
  | IDF-weighted overlap (bm25) | 6/17 | 14/17 | 14/17 | 14/17 |
  | **+ the evidence rule** | 6/17 | **13/17** | **13/17** | 13/17 |

  The chosen one is the IDF-weighted overlap **with the evidence rule**: a
  word that more than half of the titles use is not evidence that two titles
  are about the same thing, and stops counting. The rule costs one true
  pair (`dsn de postgres` reaches `url de la base de datos` only through
  the word `de`) and it buys the absence of noise: without it, a title like
  `correo de soporte` — which shares nothing but `de` — proposes three
  records of the namespace; with it, it proposes none.

  The list of function words was dropped for the same reason #27 dropped
  it: a list that exists to make the numbers move is a list that will be
  tuned to the fixture. The corpus does the same work without a list.

  **The correction that the numbers went through**: the first pass counted
  a title with a score of 0 as "found", because the ranking sorted every
  title and the truth happened to land third among the zeroes. A score of 0
  is not a candidate. With that fixed, the numbers above are the honest
  ones — the recall is 13 of 17, not 15.
- **L5** (2026-10-23) — The design that follows from L3 and L4: the engine
  ranks and proposes, **the agent adjudicates**. It is not a verdict, it is
  a shortlist with the evidence (the score and the shared words), and the
  agent — which understands "puerto de pagos" and "puerto del servicio" —
  is the one that decides. The measure is a recall problem, not a precision
  problem: what matters is that the true record is *in* the shortlist.
- **L6** (2026-10-23) — Branch `feat/near-duplicate-titles`, based on `main`
  at `f3a590a`. No migration: the titles are already stored (migration 005)
  and the ranking happens over them in memory. The relation the agent uses
  to link is the one that already exists — `supersedes` — with the reason
  that says why. A second kind of relation would need a rebuild of
  `memory_links` (its CHECK allows only `supersedes`) for a word.
- **L7** (2026-10-23) — What the implementation settled:
  - the ranking lives in the domain (`src/domain/duplicates.rs`), pure and
    tested; the corpus is the namespace's own titles, and a word more than
    half of them use is not evidence (the rule that keeps the noise out);
  - the read the ranking needs carries only what it compares (uuid, title,
    key, state): the contents are not read, because the shortlist has to
    be cheap enough to travel with every save;
  - a shortlist travels only with a save that **created** a record: a save
    that updated the record already holding the key leaves one fact in one
    record, and there is nothing left to link;
  - the direction of the link: the candidate is the **older** record (it
    existed before) and the record just saved is the newer one, so the
    fact ends up in one record with both revisions kept;
  - the CLI envelope gained the `revision` it was missing: the pi
    extension already read it to tell "updated" from "saved", it never
    arrived, so the branch never fired.
- **L8** (2026-10-23) — A gate the implementation needed and the
  specification did not have: a namespace the caller may **write but not
  read** stays closed. The proposal is a courtesy of the save, and it must
  never turn a save the caller was allowed to make into a read of a
  namespace its scope hides, nor leak the titles it holds. The
  differential check found it: a titled save into a write-only namespace
  used to fail with `NOT_FOUND`.
- **L9** (2026-10-23) — The house gates, for the record: `cargo fmt
  --all -- --check` clean, `cargo clippy --all-targets` reports (the
  repository carries a warning baseline and does not gate on `-D
  warnings`), `cargo test --locked` green (21 suites), and the extension's
  own node suite green (52 tests). The two new behaviours were checked
  RED first: without the implementation, the test that pins the save
  message and the test that pins the new tool both fail; the test that
  pins the *unchanged* common path stays green in both states, which is
  the point of it.
- **L10** (2026-10-23) — The delivery slices of the branch, for the
  review: 0438ed6 (the ranker), c02ac4b (the read and the service), 
  88772b0 (the save carries the shortlist), 4648fd6 (the command and the
  tool), da18b4f (the skill, the extension and the docs), 8397bbb (the
  corpus of one title). Each one is a work unit of about 400 authored
  lines, with the tests and the docs alongside the behaviour.
- **L11** (2026-10-23) — **The measurement** (three runs per arm, four
  scenarios, same container, same model; the full record in
  `tmp/bench/duplicates-measurement.md`):

  | | arm A (today) | arm B (the shortlist) |
  | --- | --- | --- |
  | the agent linked the two records | 8/12 | **12/12** |
  | the fact ended in one current record | 11/12 | **12/12** |
  | the proposal reached the model | — | 11/12 |

  The answer to the board's question is **yes**: the agent acts on the
  proposal, and in the runs of the first scenario it used the command
  verbatim — the reason it wrote was the proposal's reason. That is the
  opposite of what `topic-continuity`'s notice did, which was read and
  ignored.

  But the measurement also said something the board did not expect: **in
  three of the four scenarios the proposal is redundant**. Today's engine
  already ends with the fact in one current record: the agent searches,
  lists, shows, and either links the old record or saves with its title
  again (the upsert of `memory-title`). The proposal is a second path to a
  judgment the agent already makes.

  It earns its place in the fourth scenario — a busy namespace, a content
  that does not repeat the title's words, and no hint that the fact exists
  — because **the search indexes the content, not the title**: a record
  whose content is `port = 8081` cannot be found by searching `puerto`.
  There, arm A linked 2/3 and left the fact in two records with
  contradictory values (the recall of that run cost 16 809 tokens, hunting
  through the namespace, the database and the git history), and arm B
  linked 3/3.

  The honest limit: n = 3 per arm per scenario and one model. The overall
  difference is **one run in twelve** — a tendency, not a proof. What is
  proven is narrow and real: the proposal closed the failure in the
  scenario it was built for, and the agent acts on it.

- **L12** (2026-10-23) — The verdict: the feature stays. It is the only
  mechanism in the engine that compares **titles** (the search compares
  contents), it costs one query per save over the namespace's titles, and
  it is what kept the fact in one record in the scenario where the agent
  could not be thorough. What it does **not** do is replace the agent's
  own recall discipline, and the report says so: in three of the four
  scenarios it is the slower path.
- **L13** (2026-10-23) — Two defects the measurement found in the
  harness, both fixed before the numbers were read: the store dump used a
  variable that was not in scope (and `set -e` took the script down with
  it), and the dump was truncated at the 20 records `mnexis list` cuts at
  by default, which in the busy namespace hid the very records the
  measurement was about. And one defect the measurement found in the
  **engine**, fixed before the measurement ran: the rule that a word
  everybody uses is not evidence needs a corpus of at least two titles to
  say anything, and with one title in the namespace it said nothing — the
  one record of the namespace was never proposed, which is the case the
  feature exists for.
