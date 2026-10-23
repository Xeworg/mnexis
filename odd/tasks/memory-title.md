# memory-title

Make one fact stay one record, deterministically: a memory gets a **title**,
the topic key derives from it, and saving with a key the namespace already
uses **updates that record** instead of writing a second one.

This replaces the content-based refusal of `topic-continuity` (#27). The
measurement said why: the refusal was a probability that cost friction
(three to five refused retries per run) and still left two records in two of
three runs. A derived key and an upsert are an equality, not a guess.

Reading the other engine's implementation settled the design: it never
compares contents. `mem_suggest_topic_key` slugifies the **title** — the same
title always yields the same key, whatever the content says — and saving
with a key that exists **updates** the row (`revision_count` goes up, the id
stays). No stopwords, no document frequency, no thresholds, no false
positives. That is the mechanism this feature adopts, in our own engine.

## Specs

- **S1** — "A memory has a title". A memory carries a short, stable label
  next to its content (`--title` in the CLI, `title` in the MCP tool and the
  pi extension). The column is additive and defaults to empty, so every
  existing memory and every existing caller keeps working.
- **S2** — "The key comes from the title". When a save carries a title and
  no topic key, the key is the slug of the title: lowercase, runs of
  anything that is not a letter or a digit become `-`, trimmed. The same
  title always yields the same key, in any language.
- **S3** — "The charset follows the language". A topic key accepts any
  lowercase letter or digit (not only ASCII) plus `.`, `-` and `/`. A title
  written in Japanese, Greek or Cyrillic yields a usable key instead of an
  empty one; the rejection message names the whole allowed set.
- **S4** — "Saving with a key that exists updates the record". A save whose
  key is already recorded in the namespace updates that memory: content and
  title are replaced, the revision goes up, the previous content is kept in
  the history, and the answer says which record was updated. A fact stays
  one record, with its history.
- **S5** — "The refusal goes away". The content-based refusal of
  `topic-continuity` is removed: with a derived key and an upsert there is
  nothing to guess about, so nothing to refuse. `--new-topic` disappears
  with it.
- **S6** — "What worked in #27 stays". The relations travel with the search
  and list answers (`was replaced by`), and `suggest-topic-key` survives —
  answering from the title now, and saying whether the key is already
  recorded.

**Not in scope:** the conflicts layer (candidates, verdicts, relations as a
workflow), the cloud, embeddings, and the pin/review lifecycle.

## Tasks

- **T1** (S1, S3) — Migration 005: the `title` column, the doctor's expected
  version, and the topic-key charset that accepts any lowercase letter.
  Route: inline. Commit: `feat(storage): a title for every memory
  (migration 005)`.
- **T2** (S2) — The derivation: the slug of a title, in the domain layer,
  pure and tested. Route: inline. Commit: `feat(topic): the key a title
  yields`.
- **T3** (S4, S5) — The save becomes an upsert on the key, in one
  transaction, and the content-based refusal is removed. Route: inline.
  Commit: `feat(save): the same key updates the record`.
- **T4** (S1, S6) — The surface: `--title` in the CLI, `title` in the MCP
  tool and the extension, `suggest-topic-key` answering from the title, and
  the help text. Route: inline. Commit: `feat(cli): the title where the
  agent writes it`.
- **T5** (S6) — The skill and the docs: one fact, one record, and the title
  is what keeps it that way. Route: inline. Commit: `docs(skill): the title
  is the topic`.
- **T6** — Verification: the suites, and the conflict experiment re-run
  (three runs, records and tokens) against the derived key and the upsert.
  Route: inline. Commit: `docs(odd): close memory-title with its
  measurement`.

## Log

- **L1** (2026-10-23) — The operator asked what the other engine does about
  the stopword problem. Investigated against its binary:
  - `mem_suggest_topic_key` slugifies the **title** and ignores the content
    entirely (same title + different content → same key; different title +
    same content → different key);
  - saving with an existing key **upserts** (same id, `revision_count` 1→2);
  - without a key, a changed value produces a second observation, and
    `normalized_hash` only catches exact duplicates;
  - near-duplicates are found **afterwards** by `conflicts scan`, which
    inserts a `pending` relation for the agent to adjudicate (optionally
    with an external LLM).
  No stopword list, no document frequency, no similarity anywhere.
- **L2** (2026-10-23) — That is cheaper and more precise than what #27
  shipped, so the operator's decision is to keep what worked (#27's
  relations and `suggest-topic-key`) and replace the content-based refusal
  in a new feature.
- **L3** (2026-10-23) — Branch `feat/memory-title`, based on `main` at the
  merge of PR #27.
- **L4** (2026-10-23) — Writing the slug found where the multilingual
  ceiling really was: not in the slug but in the charset. `'サ'.is_lowercase()`
  is false, because kana have no case, so a Japanese title produced a key the
  validator refused. The rule is now "alphabetic and not uppercase" instead of
  "lowercase": scripts without case have no lowercase form and their letters
  are good in a key, while an uppercase letter stays out because a key is
  lowercase by contract.
- **L5** (2026-10-23) — With the comparison gone, so goes everything it
  needed: the stopword list, the overlap measure, the document-frequency idea
  and the `--new-topic` escape hatch. `src/domain/topic.rs` becomes
  `src/domain/title.rs` and keeps only the slug. The refusal that #27 shipped
  is gone with them — there is nothing to guess about, so nothing to refuse.
- **L6** (2026-10-23) — The measurement, three runs per state of the engine
  (same experiment, same container, same model; the full record in
  `tmp/bench/memory-title-measurement.md`):

  | engine | records after the change | recall tokens in |
  | --- | --- | --- |
  | before the feature | 2 · 2 · 4 | 5 455 / 4 541 / 3 514 |
  | + the notice | 1 · 2 · 4 | 1 024 / 4 541 / 3 514 |
  | + the refusal | 1 · 2 · 2 | 1 534 / 5 523 / 2 620 |
  | **+ the title** | **1 · 1 · 1** | 2 584 / 1 318 / 1 657 |

  **Three of three** is the first time the store ends with one record every
  run, and the record's revision is 2: the fact was **updated**, not
  duplicated. The recall is also the cheapest it has been — ~1 853 input
  tokens on average against ~4 503 before, with one record to read instead of
  two or four.
- **L7** (2026-10-23) — What the engine still cannot do: tell the agent
  that two *different* titles mean the same fact (`puerto del servicio` and
  `puerto de pagos` are two keys). That is where the other engine's
  *afterwards* scan earns its place — relations proposed after the fact and
  adjudicated by the agent. mnexis has the relations and the supersede chain;
  it does not have the scan.
