# agent-keywords

A memory has a **title** (what the fact *is*) and a **content** (what it
says now). It has no **vocabulary**: the words the same fact is called in
another language, in another team's jargon, in another document.

That is the gap the title leaves and the title-in-the-index does not
close. A record titled `listen port` with the content `port = 8081` is not
found by searching `puerto`, and the title is in exactly one vocabulary.

The fix is the one thing that does not need a new authority: **the agent
already knows the vocabulary**. It is the actor that writes the memory, it
understands the domain, and the measurement showed it is the semantic judge
the engine relies on (it linked two records of the same fact in 12 of 12
runs when it was proposed). So the agent says which words belong to the
fact, and the engine stores them — deterministically, with no analyzer, no
similarity, no stopwords and no thresholds.

This is also the cheap replacement for the "Concept" entity of the
identity architecture that was evaluated and not adopted: the words live on
the memory, the index connects them, and nobody has to maintain a global
vocabulary.

## Specs

- **S1** — "A memory carries the words of its fact". The agent supplies the
  vocabulary when it saves: `--keywords "puerto, port, listen port"` in the
  CLI, `keywords` in the MCP tool and in the pi extension. The words are
  part of the record, in its JSON, in its history and in the index.
- **S2** — "The words accumulate". A save that carries words **adds** them
  to the ones the record already has: the agent of a new session does not
  remember the words of the old session, and replacing them would erase
  the vocabulary. `--replace-keywords` (and `replace_keywords` in the MCP
  tool) replaces them instead, for the case of a word that is wrong.
- **S3** — "The words are findable". The full-text index covers them, so a
  memory is found by any of its words: `puerto` finds `listen port`. The
  index is rebuilt by the same one rebuild function, and the doctor tells a
  stale index from a good one.
- **S4** — "The words travel with what the agent reads". Search, list,
  context and the MCP equivalents carry the words, and the save's answer
  carries the accumulated result — so the agent sees the effect of what it
  wrote, and can prune a word that is wrong.
- **S5** — "The words are the topic's vocabulary, never a value". The skill
  says it, in the same breath as the rule that a title is never a value: a
  word that carries a value (`8081`) makes the memory match a value that is
  no longer true.

**Not in scope:** a global alias table, a concept entity, embeddings, and
any analysis of the words by the engine.

## Tasks

- **T1** (S1, S2) — The domain and the storage: the words on the memory,
  the union on the upsert, the replacement, the limits. Route: inline.
  Commit: `feat(save): the words of a fact`.
- **T2** (S3) — The index: the words join the title and the topic key in
  the FTS table (migration 007), with the triggers, the rebuild and the
  doctor's shape check. Route: inline. Commit:
  `feat(search): the index knows the words`.
- **T3** (S1, S4) — The surfaces: the CLI flag, the MCP argument, the
  extension's tool, and the words travelling with the read answers. Route:
  inline. Commit: `feat(cli): the words where the agent writes them`.
- **T4** (S4, S5) — The skill and the docs: the vocabulary of the fact,
  and the rule that a word is never a value. Route: inline. Commit:
  `docs(skill): the words of the fact`.
- **T5** — Verification: the suites, and the experiment with a new arm —
  **arm D: this branch**. The scenario that decides it: the old record is
  titled `Puerto de escucha del servicio` (content `port = 8081`) and the
  new one is titled `listen port` — **not one word in common**, so the
  title in the index cannot help and only the words can. Route: inline.
  Commit: `docs(odd): close agent-keywords with its measurement`.

## Log

- **L1** (2026-10-23) — The operator's idea, in their words: "hacer que el
  agente nos diga palabras que podrian tener relacion con la memoria cuando
  la guarda ya que siempre tendremos este problema con palabras muy
  tecnicas o palabras traducidas... ocuparia un nuevo campo pero conforme
  el agente va agregando palabras a una memoria esta tendra las de la
  anterior".
- **L2** (2026-10-23) — The operator took the recommendation: the words
  **accumulate**, with an explicit way to replace them. And the cold start
  is not a concern: there are no users yet, so no backfill and no data to
  migrate.
- **L3** (2026-10-23) — The design, and why it is where the semantic
  knowledge already is:
  - the **agent** is the judge the engine depends on (12 of 12 runs linked
    when it was proposed), so the vocabulary comes from it and not from an
    analyzer: no similarity, no thresholds, no stopword list, no concept
    table to maintain;
  - the words live **on the memory**, not in a global table: the index
    already connects two memories that share a word, and a global alias
    space would be a second source of truth to maintain;
  - **accumulate** because the amnesia is the common case (a new session
    does not remember the old words) and because a lost word is invisible
    while a stale one is visible — the accumulated list travels with the
    read answers, so the agent can prune it;
  - the rule that keeps this honest is the one the title already has: the
    words are the topic's **vocabulary**, never a value.
- **L4** (2026-10-23) — Branch `feat/agent-keywords`, based on
  `feat/identity-visibility` (stacked: the identity-visibility work has to
  land first — it is the one that makes the title visible and findable, and
  this one adds the vocabulary to the same index). The proposal branch
  (`feat/near-duplicate-titles`) stays parked: the arm C measurement (the
  identity-visibility branch, without the proposal) decides whether it is
  still needed, and this feature changes that question again.
- **L5** (2026-10-23) — **The measurement** (the vocabulary scenario: the
  old record titled `Puerto de escucha del servicio` with the content
  `8081`, the new one titled `listen port` with the prompt in English — so
  the title in the index cannot answer and only the words can; three runs
  per arm; the report in `tmp/bench/keywords-measurement.md`):

  | | A: main | C: visibility | D: visibility + words |
  | --- | --- | --- | --- |
  | the fact in one current record | 3/3 | 4/6 | 3/3 |
  | the recall answered the current value | 3/3 | 5/6 | 3/3 |
  | tokens in the recall | 1 159–1 311 | 1 088–13 704 | 1 512–2 660 |

  **The agent writes the words: 3 of 3** (`puerto, port, listen port`,
  `port, listen port, server port`, …). That is the thing the previous
  cycle could not get: a mechanism the agent uses without being pushed.
  The skill says what the words are for, the tool takes them, and the
  answer shows the accumulated result.

  **The outcome cannot be resolved with n = 3.** The visibility arm was
  measured **twice** with the same image and the same prompts: 1/3 in one
  sample and 3/3 in the other. Every difference between the arms is of
  that size. So the mechanism is proven and the effect is not.
- **L6** (2026-10-23) — What survives the measurement, and what does not:
  - **proven**: the agent acts on a proposal whose answer carries the
    command (12/12); the agent writes the words when the skill and the tool
    ask for them (3/3); the engine's similarity cannot separate two titles
    of one fact from two titles of two facts (0.53–0.97 against 0.47–0.91
    — a property of the strings, not of the sample);
  - **not proven**: any ranking between the arms. The proposal's 12/12, the
    visibility's 11/12 and the title's 3/3 are all inside the band a re-run
    of the same configuration produces (the visibility arm measured 1/3 and
    3/3 on the same day, with the same image and the same prompts).

  The decisions rest on the mechanisms, not on the counts. That is the
  correction this cycle earns: **n = 3 with one model is a smoke test, not
  a measurement.**
