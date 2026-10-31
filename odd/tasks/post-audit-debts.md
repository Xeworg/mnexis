# Post-audit debts — the tools block, the wire, the store, the reads

The 2026-10-30 close left a pending list; the user ordered it ("ve en ese orden") and asked for
worktrees. G5 is resolved (`tmp/bench/types-b1-round4-report.md`); this feature ships what remains:
the token bill of the tools block, the wire debts, the store debts, and the read paths.

## Specs

- **S1** — "recortar el bloque de tools de la extensión — 8 950 B de prosa ≈ 3.2k tokens por
  llamada, con ~1.7 KB repetido (enum de 7 tipos ×3, semántica de match ×3, \"mismo título\" ×3) y
  dos tools que se pelean por ser \"la primera\" (workspace vs context). Medirlo con el instrumento
  del round 4 (leer la respuesta, sin regex)."
- **S2** — "reparar el envelope de explain (relations, valid_from, valid_until)"
- **S3** — "cerrar RecallArgs (último args MCP abierto)"
- **S4** — "MemoryRepository::update que pierde title (no alcanzable desde producto)"
- **S5** — "keywords que no viajan en export/import"
- **S6** — "las optimizaciones de lectura C1/C2/C3/C5/C6 (N+1 de la puerta, llenado cuadrático del
  presupuesto que descarta 441 KiB por llamada en modo MCP, filtro por valor 21× filas, seis índices
  faltantes)"

## Tasks

| ID | Specs | Route | Branch / worktree | Commit |
| --- | --- | --- | --- | --- |
| T1 | S1 | worker, wave 1 | `fix/tools-trim` @ `tmp/wt-tool-trim` (task mvvw1jxf-1-qbrw) | `73003d7` |
| T2 | S3 (S2 → T4) | worker, wave 1 | `fix/mcp-wire` @ `tmp/wt-mcp-wire` (task mvvw1jxn-2-82ys) | `4bcf0d6` |
| T3 | S4, S5 | worker, wave 1 | `fix/store-wire` @ `tmp/wt-store-wire` (task mvvw32gz-3-1uo0; spec commit 9b728b1) | `e129687`, `7a315f9` |
| T4 | S2, S6 | worker, wave 2 (completed in two passes; ruling in L8) | `perf/read-paths` @ `tmp/wt-read-paths` (tasks mvvwh3kx-4-5b4b + mvvxap08-5-3gw2; spec commit 69a566a) | `249095d`, `971191f`, `f2c1949`, `f5ad359`, `16dc6d7` |
| T5 | all | parent: integration branch `integrate/post-audit` with the FULL suite green before any PR; closing PR re-lands what #106 carried (cherry-pick `odd/tasks/door-merged-measurement.md`, bump the version) | — | — |

Allowed edit surfaces, wave 1 (disjoint on purpose — the #111 lesson): T1
`extensions/mnexis-memory/**` + `tmp/bench/**`; T2 `src/mcp/**` + its tests; T3
`src/storage/memory_repo.rs` + `src/application/io.rs` + their tests. T4: `src/application/**`,
`src/storage/**`, `src/cli/**`, `src/retrieval/**`, `src/mcp/server.rs`.

## Log

- **L1 (2026-10-31, user)** — "por donde quedamos" → recap of the 2026-10-30 close; then "ve en ese
  orden" (the pending list in the order presented); then "pero que tal si usamos worktrees".
- **L2 (2026-10-31, evidence)** — G5 resolved, no code change: 14 honest `Saved` answers; the 2
  missing records were superseded later by the value-based twin repair firing false cross-type twins
  (sets `{0007, 2026}`); both pairings are refused by `sameKnownType` (`8f66146`, landed 11:38, run
  11:09). Residual risk recorded: same-type records with identical incidental numeric sets still
  pair. Report: `tmp/bench/types-b1-round4-report.md`.
- **L3 (2026-10-31, decision)** — worktree split by file ownership, two waves because T4 overlaps
  T2/T3 files; integration branch with full suite before PRs (the six-branch lesson). PR #106 stays
  the human's call; not part of this feature.
- **L4 (2026-10-31, T2 result)** — S3 verified done by the parent (focused test green on the
  branch): `4bcf0d6` closes `RecallArgs` with `deny_unknown_fields`; full suite 817/0, fmt clean.
  **S2 was mislocated**: the MCP server has no explain path (`SearchArgs` has no explain field;
  `do_search` already carries relations, valid_from and valid_until). The envelope S2 names is the
  CLI's — `src/cli/commands.rs::search()` explain branch — fed by `src/retrieval/explain.rs`, which
  does not read `m.valid_from`/`m.valid_until` while `fts5::search` does. S2 moves to T4 with the
  map: (a) `explain.rs` ExplainedHit gains the two fields (SELECT at :52-56, mapping at :170),
  tests in `tests/retrieval_explain.rs`; (b) `cli/commands.rs` explain branch adds the per-item
  fields and the envelope `relations` via `service.relations_between(&explained_uuids)` as plain
  search does at :523-527, RED test in `tests/cli_commands.rs`. Note: `relations` is empty for a
  supersede pair in both envelopes (the state filter hides the superseded end) — parity field.
- **L5 (2026-10-31, user decision)** — PR #106 closed ("es documentación, lo cerramos?"): the
  branch predates the types work and conflicts on the board. Rescue plan rides T5: cherry-pick the
  re-measurement doc, re-land the version bump. The local branch `docs/board-value-filter` stays
  until the cherry-pick, then joins the cleanup list.
- **L6 (2026-10-31, T3 result)** — S4+S5 verified by the parent (focused tests re-run green;
  the `title = ?1` renumbering reviewed) and committed as two units: `e129687` (storage: update
  keeps the title) and `7a315f9` (io: keywords travel; old files import as no words). Worker's
  suite: 821/0, fmt clean, clippy clean except one pre-existing warning. **Decision accepted
  (flagged by the worker): keywords are now canonical for import identity** — an old-era file
  re-imported under `reject` over a store whose row has words is a UuidClash, not an
  "identical" no-op; the T22 close gate (export from A into empty B) is unaffected. Lenient
  reading would be a one-line revert plus dropping test D. T4 launched early: T2+T3 freed the
  files and T1 only touches extensions/**.
- **L7 (2026-10-31, T1 result)** — S1 verified by the parent (5/5 new tests re-run green; the
  measurement reproduced) and committed: `73003d7`. 9 014 B → **5 490 B** description+parameter
  prose (14 tools; context 930, save 764). Each repetition said once and referenced: the enum in
  `mnexis_save.type`, match semantics in `mnexis_search.match`, same-title in `mnexis_save`;
  `mnexis_workspace` keeps the sole FIRST-call claim. All named contracts kept (namespace
  defaulting, read scope, value rule, keywords, block budget, supersede boundary); dropped text
  was non-contract guidance (rationales, "review before saving" tips). The ≤5 500 B test pins the
  bill with 10 B headroom — future prose additions must pay for themselves. Suite 181 tests,
  179 pass, 2 parity skips; schema keys and promptSnippets (695 B, unchanged) sit outside the
  measure. Wave 1 complete: T1 `73003d7`, T2 `4bcf0d6`, T3 `e129687`+`7a315f9`.
- **L8 (2026-10-31, T4 mid-result + ruling)** — S2 observed RED→GREEN (explain.rs validity window;
  CLI explain branch items + envelope relations); C1 (100 stmts → 1, 4.0×), C2 (34.9 MB re-serialised
  + 3.39 M discarded tokens per call → 394 KB / 38.3 k once; fill 61×, door 0.51 s → 0.11 s at 100
  items), C3 (value filter 21×n → 1×n rows), C5 (1.33 MB → 1.1 KB, 12×) — all differential-green
  (21/26 commands byte-identical; the 5 diffs are exactly the S2 keys on search --explain). C6
  designed and plan-verified but blocked: the port methods need src/domain/** and the 009-pattern
  migration needs doctor.rs EXPECTED_SCHEMA_VERSION 9→10 — both outside the parent-derived surface
  list (parent's omission, not a product boundary). **Option A authorized** (B rejected: unledgered
  indexes make the store shape unknowable; C rejected: the measured harness lives with the worker).
  Continuation task mvvxap08-5-3gw2 running; scratch /tmp/mnexis-read-paths.4CgYn3 stays until
  parent verification.
- **L9 (2026-10-31, T4 closed)** — Continuation finished C6: migration 010 with the six measured
  indexes, LATEST_SCHEMA_VERSION 10 as the single number doctor reads, schema assertions moved,
  index-presence + ledger-heal tests. Worker suite 823/0, fmt clean. Parent verification: the
  26-command differential re-checked on the scratch artifacts (exactly 6 diffs — the five explain
  key-additions and doctor's evidence 9→10); tree committed as FIVE units (C1+C5 share the port
  cluster: trait, adapter, service — splitting them bought no review clarity): `249095d` (S2),
  `971191f` (C3), `f2c1949` (C2), `f5ad359` (C1+C5), `16dc6d7` (C6). Full suite re-run on HEAD in
  the background before T5. Slice note: perf/read-paths is 1 131 authored lines across 6 commits —
  past the ~400-line slice; the delivery strategy is T5's integration PR, not more commits here.
  Stale line found: odd/next.md:70 still says schema 9 — folds into T5's version commit.
