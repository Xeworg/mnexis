# Mnexis T14 — FTS5-only search benchmark and resource budgets

**Status:** E1 spike evidence per `PRD-Mnexis.md` v0.5 §15 and E1 items 5–6. These numbers
ground the numeric latency/RAM/disk budgets the spike must fix before closing design. This is
**not** the frozen E3 acceptance fixture (the curated 500-memory/150-query evaluation stays out
of T14 scope) and **not** an Engram baseline comparison (E3 scope).

**Scope:** SQLite/FTS5-only stack, Linux host. Non-Linux platforms are deferred until the MVP
by the user's decision. Historical vector-stack measurements (T6) do not apply.

## Environment

- Fedora 44, Linux 7.2.9, x86_64, 20 logical CPUs, 31 GiB RAM.
- Project filesystem: btrfs (118+ GiB free during all runs). `TMPDIR` for cargo-spawned
  processes is redirected into the repository (see `odd/tasks/e0-e1-recovery.md` L71–L72);
  no manual prefixes were used for any command below.
- rustc/cargo 1.95.0; rusqlite with bundled SQLite; rmcp for the MCP server.
- Benchmark DBs live under `target/t14-eval/` and `target/bench-data/` and are regenerable.

## Methodology

All numbers come from deterministic artifacts; no RNG is involved anywhere.

- **Store:** `memories` canonical table plus a plain `fts5(content)` table kept in sync by
  AFTER INSERT/UPDATE/DELETE triggers (PRD §10 trigger option). Connections open with WAL,
  `foreign_keys=ON`, 2 s busy timeout. Search: `MATCH` + `ORDER BY bm25(memories_fts)`,
  joining canonical columns, filtering `state='active' AND deleted=0` plus optional namespace,
  SQL fully parameterized.
- **Corpus** (`src/corpus.rs`): 25 curated ES/EN topic pools cycled to 200 pool indices.
  Document `i` composes 2–6 sentences (mean ≈ 300 chars) from its pool plus the unique token
  `mnx<i>`; states: superseded when `i%20==7`, soft-deleted when `i%20==13` (~5 % each);
  namespaces `proyecto-a`…`proyecto-e` by `i%5`. Corpus loading uses one transaction
  (`insert_memories`); single-memory writes stay one-transaction-per-operation per PRD §10.
- **Queries:** 200 relevant queries, each targeting one **active** document — the unique
  `mnx<i>` token plus one topic word taken from the template that document always includes,
  so the target matches every term while same-pool documents compete on the topic word.
  50 no-answer queries pair words from different pools **verified** never to co-occur inside
  any single pool's token set, so FTS5 implicit AND must return zero rows by construction.
  Every query term is a plain alphanumeric bareword: hyphenated tokens are invalid
  single-term FTS5 query syntax (the parser reads inner segments as column references).
- **Metrics:** Recall@5, MRR@5, Precision@5, no-answer false-positive rate, per-query latency
  p50/p95, cold-first-query latency, batched insert time, DB bytes on disk (post
  `wal_checkpoint(TRUNCATE)`).
- **Known bounds:** with exactly one relevant document per query, Precision@5 is structurally
  capped at 1/5 = 0.20 when recall is perfect — Recall@5 and MRR@5 are the meaningful quality
  signals here, as with A-08/A-09 calibration deferred to the frozen E3 fixture. "Cold" means
  the first query on a freshly opened connection with the OS page cache warm from corpus
  generation; it is never claimed disk-cold.

### Reproduction commands

```sh
cargo run --release --example t14_eval -- --memories 1000   --db target/t14-eval/1k.db
cargo run --release --example t14_eval -- --memories 10000  --db target/t14-eval/10k.db
cargo run --release --example t14_eval -- --memories 100000 --db target/t14-eval/100k.db
cargo bench --bench fts5_latency          # 10k/100k × cold/warm × short/long
```

## Results — quality and latency (t14_eval)

### 1 000 memories

| Metric | Value |
|---|---|
| recall_at_5 | 1.0000 |
| mrr_at_5 | 1.0000 |
| precision_at_5 | 0.2000 (structural 1/5 cap) |
| no_answer_fp_rate | 0.0000 |
| latency p50 / p95 | 0.0539 / 0.0601 ms |
| cold_first_query_ms | 0.1747 |
| insert_seconds (batched) | 0.0462 |
| db_bytes | 1 073 152 (~1.07 MiB) |

### 10 000 memories

| Metric | Value |
|---|---|
| recall_at_5 | 1.0000 |
| mrr_at_5 | 1.0000 |
| precision_at_5 | 0.2000 (structural 1/5 cap) |
| no_answer_fp_rate | 0.0000 |
| latency p50 / p95 | 0.0433 / 0.0561 ms |
| cold_first_query_ms | 0.2243 |
| insert_seconds (batched) | 0.3194 |
| db_bytes | 9 441 280 (~9.0 MiB) |

Content length: mean ≈ 299–300 chars, min 129, max 471. Relevant queries: 200; no-answer: 50.

### 100 000 memories

| Metric | Value |
|---|---|
| recall_at_5 | 1.0000 |
| mrr_at_5 | 1.0000 |
| precision_at_5 | 0.2000 (structural 1/5 cap) |
| no_answer_fp_rate | 0.0000 |
| latency p50 / p95 | 0.1949 / 0.3482 ms |
| cold_first_query_ms | 0.3870 |
| insert_seconds (batched) | 5.2131 |
| db_bytes | 92 917 760 (~88.6 MiB) |

Content length at 100k: mean 301.0, min 129, max 472 chars. Scaling is linear and
unremarkable: per-query latency grows ~4.5× from 1k to 100k while remaining sub-millisecond
at p95; DB size grows ~10× per 10× memories (~0.93 KiB per memory); batched ingestion holds
~19 200 memories/s.

## Results — Criterion latency benches

`cargo bench --bench fts5_latency`, 200 samples per benchmark, Criterion 0.5. "Cold" opens
a fresh connection per measurement — including the idempotent schema/trigger batch every
connection runs — before the first query; "warm" reuses one connection. Short/long select
different deterministic queries; with token-plus-topic-word queries both lengths behave
alike.

| Benchmark | Lower / mean / upper |
|---|---|
| search_10k_cold/short | 299.96 / 301.87 / 303.70 µs |
| search_10k_cold/long | 308.28 / 310.19 / 312.07 µs |
| search_10k_warm/short | 49.58 / 51.02 / 52.89 µs |
| search_10k_warm/long | 44.24 / 44.60 / 45.01 µs |
| search_100k_cold/short | 377.78 / 379.15 / 380.56 µs |
| search_100k_cold/long | 419.36 / 421.76 / 424.16 µs |
| search_100k_warm/short | 107.46 / 108.14 / 108.85 µs |
| search_100k_warm/long | 106.13 / 106.74 / 107.37 µs |

The cold-vs-warm gap (~250–320 µs) is dominated by per-connection setup (open, WAL/foreign
keys/busy-timeout pragmas, idempotent schema and trigger batch), not by FTS5 query planning;
warm search grows ~2.1× from 10k to 100k (51 → 108 µs mean) while staying far below a
millisecond. The in-process eval p50/p95 (43–348 µs across 1k–100k) is consistent with the
warm benchmarks.

## Results — build and runtime resources

Fresh release build in a clean `CARGO_TARGET_DIR=target/t14-clean` (TMPDIR redirected into
the repository; no manual prefixes):

| Metric | FTS5-only stack (this run) | Historical vector stack (T6) |
|---|---|---|
| Wall-clock build | 1 min 11.46 s | 10 min 59.39 s |
| Peak build RSS | 758 376 KiB (~741 MiB) | 7 265 268 KiB (~6.9 GiB) |
| Release binary | 2 562 680 B (~2.44 MiB) | 135 546 232 B (~129 MiB) |
| Clean target dir | 258 907 119 B (~247 MiB) | 2.0 GiB |

Runtime (same binary):

| Measurement | Value |
|---|---|
| `mnexis --help` startup | 0.00 s, 4 072 KiB RSS |
| `mnexis mcp-serve` with immediate stdin EOF | 0.00 s, 4 156 KiB RSS, exit status 1 |

`mcp-serve` starts at ~4 MiB RSS and exits immediately with a non-zero status when stdin
closes before `initialize` — the expected behavior for a stdio server whose transport closed;
no hang and no leak window.

## Proposed resource budgets

Proposed from the measured numbers above with explicit headroom for slower hosts; these are
spike proposals to confirm at design freeze, not acceptance verdicts (PRD §15 requires the
spike to fix numeric budgets before closing design).

| Budget (Linux host, FTS5-only stack) | Proposed ceiling | Measured basis |
|---|---|---|
| Warm ranked search, 100k memories | ≤ 0.5 ms | 108 µs mean (4.6× headroom) |
| Cold first search incl. connection setup, 100k | ≤ 5 ms | 380–424 µs (12× headroom) |
| Batched ingestion throughput | ≥ 5 000 memories/s | ~19 200/s at 100k |
| DB size per memory | ≤ 1.5 KiB | ~0.93 KiB measured |
| DB size at 100k memories | ≤ 150 MiB | 88.6 MiB measured |
| `mcp-serve` startup RSS | ≤ 32 MiB | ~4.1 MiB measured |
| Binary startup time | ≤ 100 ms | 0.00 s measured |
| Fresh release build (declared host) | ≤ 3 min wall, ≤ 1 GiB peak RSS | 1 min 11 s, 741 MiB |
| Release artifact size | ≤ 8 MiB | 2.44 MiB |

Single-memory writes stay one-transaction-per-operation (PRD §10) and are correctness-bound,
not throughput-budgeted.

## Limitations

- Synthetic corpus with by-construction relevance: it measures the mechanics of ranked FTS5
  retrieval (matching, ranking under competition, abstention, filters), not curated human
  relevance judgments; A-08/A-09 calibration belongs to the frozen E3 fixture.
- Single-relevant-document queries cap Precision@5 at 0.20 structurally.
- "Cold" queries run with a warm OS page cache, and include per-connection schema
  verification (every connection runs the idempotent schema/trigger batch).
- Single Linux host; single developer workstation profile. PRD §15 requires the declared
  equipment to be recorded: this report's host is the one above.
