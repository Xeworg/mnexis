# agent-query-contract

Make the engine understand what the agent asks: a query in plain language
brings results, a hyphen never breaks a search, a bad query says what is wrong
with it, and the workspace identity has exactly one form.

## Specs

- **S1** — "Una query en lenguaje natural trae resultados (0/5 → 5/5)". A
  multi-word query must not require *every* word: FTS5 reads it as an implicit
  AND, so a paraphrase that drops one word finds nothing. The five paraphrased
  queries the benchmark used (`puerto del servicio`, `base de datos`, `espera
  entre reintentos`, `ambiente de despliegue`, `contacto de soporte`) must find
  the memories that hold those words. The engine compiles the query into a safe
  FTS5 expression; the caller chooses the mode.
- **S2** — "Un guion en la query no rompe la búsqueda". `mnexis search
  "staging-eu"` and `mnexis search "payments-api"` must return results, not
  fail. A hyphen is part of a word, not FTS5 syntax.
- **S3** — "Un error de parseo FTS5 tiene su propio error kind, no BUSY". A
  query the engine cannot parse must fail as an invalid argument
  (`INVALID_ARGUMENT`, exit 2) naming the query, not as `BUSY` (exit 10) with
  an index-repair message that is not the problem.
- **S4** — "La key del workspace se normaliza en un solo lugar". The Rust
  engine owns the rule: a key is stored, looked up and displayed in one
  canonical form, so `git:github.com/example/payments-api`,
  `git:https://github.com/example/payments-api` and
  `git:github.com-example-payments-api` are the same workspace.
- **S5** — "El bind humano y el de la extensión dicen lo mismo". The human's
  `mnexis project bind` and the pi extension's binding must use the same key,
  and the extension must never invent a namespace: a namespace comes from a
  binding or from a `.mnexis-namespace` marker the human wrote, never from the
  name of a directory.

**Not in scope:** the storage schema, the store format, the agent protocol
(skill and MCP tool descriptions), and the permission model.

## Tasks

- **T1** (S1, S2) — The query contract: a `MatchMode` (`any`, `all`, `phrase`)
  and a compiler that turns a user query into a quoted, safe FTS5 expression.
  Route: inline. Commit: `feat(query): compile the agent's query into a safe expression`.
- **T2** (S1, S2, S3) — Wire the compiler into the retrieval path and map a
  query the engine cannot parse to its own error kind. Route: inline.
  Commit: `feat(search): answer plain-language queries and own the parse errors`.
- **T3** (S1) — The caller-facing surface: `mnexis search --match`,
  `mnexis context --match`, the MCP `memory_search` tool, the help text and
  the docs. Route: inline. Commit: `feat(cli): expose the match mode where a
  query is written`.
- **T4** (S4) — One canonical workspace key: the registry stores, finds and
  displays it in canonical form, and `mnexis project list --json` publishes the
  canonical key of the current workspace. Route: inline. Commit:
  `feat(projects): one canonical workspace key`.
- **T5** (S5) — The pi extension asks the engine for the canonical key and
  stops inventing a namespace. Route: inline. Commit:
  `fix(extension): use the engine's workspace key and never invent a namespace`.
- **T6** — Verification: the suite, the installer suite, the extension tests,
  and the benchmark's own retrieval check re-run against the new binary in the
  container (0/5 → 5/5). Route: inline. Commit: `test(benchmark): the
  retrieval check passes with the new engine`.

## Log

- **L1** (2026-10-23) — "quiero que ahora hagas una comparacion con engram en
  un contenedor comprara el funcionamiento de nuestro software vs engram el
  cual ya esta muy avanzado aprende lo que puedas de el podriamos encontrar
  puntos de mejora puedes usar pi esta vez cuando ocupes la api key me la
  pediras creare una temporal para que puedas usarla sin riesgos,"
- **L2** (2026-10-23) — "es de opencode go usala con deepseek".
- **L3** (2026-10-23) — "propon un nombre no no estamos compiando codigo al
  final son caracteristicas que se implementaran en el motor propio sugiere un
  nombre". The branch and this feature are named `agent-query-contract`: the
  features are implemented in our own engine, and the name says what they
  change — the contract between what the agent asks and what the engine
  answers.
- **L4** (2026-10-23) — The benchmark found the two defects this feature
  fixes: `mnexis search` passes the query straight to FTS5 (a hyphen fails with
  `busy: fts5::search collect: no such column: api`, a multi-word query is an
  implicit phrase and matches nothing), and `mnexis project bind --key` stores
  the key verbatim while the extension computes a normalized one, so a human
  binding never matches and the extension dead-ends on an auto-bound namespace
  the allowlist does not know. Evidence: `tmp/bench/report.md`,
  `tmp/bench/out/`.
- **L5** (2026-10-23) — The agent-facing behaviour was measured before the
  change: 0/5 natural queries hit; with the terms joined by `OR` by hand, 5/5
  hit. The gap is the query compilation, not the index.
- **L6** (2026-10-23) — The independent verification reproduced both defects
  against the released binary and corrected one of my explanations: a bare
  multi-word FTS5 query is an implicit **AND**, not a phrase (SQLite 3.53.2),
  so the paraphrases missed because every word had to be present, not because
  they had to be adjacent. The hyphen failure is a column-resolution error
  (`no such column: eu`), not a syntax error, and it lands on the `BUSY` path
  because the search maps every `query_map`/`collect` error to it. The fix
  covers both readings: quoting stops the parse error, `any` stops the AND.
