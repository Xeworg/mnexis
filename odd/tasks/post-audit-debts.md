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
| T1 | S1 | worker, wave 1 | `fix/tools-trim` @ `tmp/wt-tool-trim` | — |
| T2 | S2, S3 | worker, wave 1 | `fix/mcp-wire` @ `tmp/wt-mcp-wire` | — |
| T3 | S4, S5 | worker, wave 1 | `fix/store-wire` @ `tmp/wt-store-wire` | — |
| T4 | S6 | worker, wave 2 (after T2+T3 integrate: touches `memory_repo.rs`, `mcp/server.rs`) | `perf/read-paths` @ `tmp/wt-read-paths` | — |
| T5 | all | parent: integration branch `integrate/post-audit` with the FULL suite green before any PR | — | — |

Allowed edit surfaces, wave 1 (disjoint on purpose — the #111 lesson): T1
`extensions/mnexis-memory/**` + `tmp/bench/**`; T2 `src/mcp/**` + its tests; T3
`src/storage/memory_repo.rs` + `src/application/io.rs` + their tests. T4: `src/application/**`,
`src/storage/**`, `src/cli/**`, `src/mcp/server.rs`.

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
