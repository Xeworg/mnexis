# Mnexis E2 — Núcleo durable y CLI textual (design proposal)

> Status: planning only. No code, tests, benches, Cargo.toml, or other odd/* files are edited
> by this document. The proposal is reviewed by the parent agent and then by a second agent before
> the user is asked for a go-ahead.
>
> Branch: `feat/mnexis-e0-e1`
> Evidence base: PRD v0.5 §E2 + close tests §16, e0-e1-recovery.md L52/L73–L82, T12 contracts,
> T13/T14/T10 spike evidence, `docs/t14-fts5-benchmark.md`.
> Spike code is **evidence**, not the production E2 core.

---

## 1. Scope

### E2 deliverables (PRD §E2)

E2 implements the durable SQL core and the textual CLI. Item by item from the PRD §E2 close
criteria:

1. **Migrations** and short-transaction SQL repository — embedded, numbered, applied at `open_sqlite`.
2. **Save / get / list / update / history / supersede** with validation, optimistic revision
   (`expected_revision`), topic-key and idempotency-key rules, and state transitions enforced in
   the application service.
3. **Scopes** enforced in the application service, not only in MCP. Every read and write route
   authorises against the process's immutable `AccessScope`.
4. **FTS5** maintained in every write transaction (trigger approach, not a separate service).
   Default retrieval returns only `active`, non-deleted, currently-valid records.
5. **Logical delete / restore / purge** with configurable retention window and the `purge_after`
   date.
6. **CLI commands** corresponding to the full PRD §8 surface, sharing the application service
   with MCP.
7. **Stable error set** from PRD §8, mapped to MCP JSON-RPC codes and CLI exit codes.
8. **Persistence after restart**, concurrent writer safety (WAL + optimistic revision),
   scope enforcement on IDs and cursors, atomic supersede, and FTS5 consistency after
   delete/restore — all verified by the E2 close tests.

### Explicitly deferred to E3+

- Reranking (neuronal or learned; PRD §4 P1).
- Explicit `memory_links` for general graph relations beyond `supersedes` (PRD §4 P2; MVP
  keeps `memory_links` only for the `supersedes` relation).
- Agent harness, compactación/pruning, subagents, multi-client MCP (second distinct client;
  PRD §E4).
- Sync and multi-device (PRD §4 P2).
- Embeddings, vector search, reranker, LanceDB, Ollama, or any form of semantic similarity beyond
  lexical FTS5 (PRD §21: FTS5-only MVP).
- UI web or desktop.
- Automatic contradiction detection.
- Session summarisation (P1; `session_summary` type stays as a manual user-asserted memory type).

### E2 does not close

- A-07 (tokenizer-backed budget enforcement in `memory_context`). Token counting infrastructure
  is built but the E2 budget mode is `estimated` with a hard byte ceiling, labelled as such.
  Exact-token enforcement is E3.
- A-08 / A-09 quality metrics on the frozen 500-memory/150-query evaluation fixture. That
  fixture is E3 scope. The T14 eval harness is productionised as a development tool (see §10).
- A-19 (second distinct MCP client; E4 scope).
- A-20 (first-time configuration in < 10 min). The `init` command and profile setup are E2,
  but the E4 client integration step is out of scope.

---

## 2. Production schema

All tables live in the same SQLite database as the benchmark's `memories` / `memories_fts` pair.
The benchmark schema is replaced; the production schema is designed for correctness and the PRD
§6/§10 invariants.

### `memories` — canonical table (replaces the benchmark `memories`)

```sql
CREATE TABLE memories (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,

    -- Identity (PRD §6)
    uuid            TEXT    NOT NULL UNIQUE,
    topic_key       TEXT    NOT NULL DEFAULT '',   -- concept key; unique per namespace when set
    revision        INTEGER NOT NULL DEFAULT 1,

    -- Namespace and classification
    namespace       TEXT    NOT NULL,
    memory_type     TEXT    NOT NULL DEFAULT 'fact',
    state           TEXT    NOT NULL
                        CHECK (state IN (
                            'proposed','active','disputed','superseded','archived')),

    -- Content
    content         TEXT    NOT NULL,

    -- Provenance (PRD §5)
    actor           TEXT    NOT NULL,              -- caller identity, not a user-facing name
    channel         TEXT    NOT NULL,             -- 'cli' | 'mcp' | 'import' | 'other'
    assertion_basis TEXT    NOT NULL
                        CHECK (assertion_basis IN (
                            'user_stated','observed','inferred','imported')),

    -- Validity window (PRD §6; UTC)
    valid_from      TEXT,   -- ISO-8601, NULL = no lower bound
    valid_until     TEXT,   -- ISO-8601, NULL = no upper bound

    -- Soft-delete (PRD §12)
    deleted         INTEGER NOT NULL DEFAULT 0,
    deleted_at      TEXT,   -- ISO-8601; set when deleted=1
    purge_after     TEXT,   -- ISO-8601; set from retention policy at delete time

    -- Bookkeeping
    created_at      TEXT    NOT NULL
                        DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    updated_at      TEXT    NOT NULL
                        DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),

    -- Constraints
    UNIQUE (namespace, topic_key)
        WHERE topic_key != ''
);
```

**Field justifications:**

| Field | PRD § justification |
|---|---|
| `uuid` | Stable identity across revisions; not the rowid. |
| `topic_key` | Conceptual identity (PRD §6); unique per namespace to prevent silent collision. Empty string means no concept. |
| `revision` | Monotonically increasing integer; required for optimistic concurrency (PRD §6). |
| `namespace` | Per-process scope unit (PRD §7). |
| `memory_type` | Controls eligibility in `memory_context` filters (PRD §9 example). |
| `state` | Enumerated set matches PRD §6 exactly; no other values admitted by CHECK. |
| `content` | Primary payload; 16 KiB limit enforced at the application layer, not in DDL. |
| `actor` / `channel` / `assertion_basis` | Provenance minimum per PRD §5. |
| `valid_from` / `valid_until` | Optional temporal validity window; MVP does not support bi-temporal queries (PRD §6). |
| `deleted` / `deleted_at` / `purge_after` | Soft-delete with configurable retention and purge eligibility (PRD §12). Default retention is 30 days; `0` disables (record becomes purge-eligible immediately). |
| `created_at` / `updated_at` | Audit trail; `updated_at` advances on every write. |
| `UNIQUE (namespace, topic_key) WHERE topic_key != ''` | Prevents duplicate concept registration per namespace without forbidding multiple memories without a concept key. |

### `memory_history` — revision log

```sql
CREATE TABLE memory_history (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    memory_uuid     TEXT    NOT NULL,
    revision        INTEGER NOT NULL,

    -- Snapshot of the record at this revision
    memory_type     TEXT    NOT NULL,
    content         TEXT    NOT NULL,
    state           TEXT    NOT NULL,
    valid_from      TEXT,
    valid_until     TEXT,
    assertion_basis TEXT    NOT NULL,

    -- Who changed it and why
    changed_by     TEXT    NOT NULL,     -- actor
    change_reason  TEXT    NOT NULL,     -- free text, never memory content
    changed_at      TEXT    NOT NULL
                        DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),

    FOREIGN KEY (memory_uuid) REFERENCES memories(uuid)
);
```

Every write to `memories` appends a row to `memory_history` inside the same transaction before
committing. The history row captures the **previous** state of the record. The current record
is always the one with the highest revision for a given `uuid`.

### `memory_links` — supersedes relation only

```sql
CREATE TABLE memory_links (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    older_uuid      TEXT    NOT NULL,
    newer_uuid      TEXT    NOT NULL,
    link_type       TEXT    NOT NULL DEFAULT 'supersedes'
                        CHECK (link_type = 'supersedes'),  -- MVP: only this type

    created_at      TEXT    NOT NULL
                        DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),

    UNIQUE (older_uuid, newer_uuid, link_type),

    FOREIGN KEY (older_uuid) REFERENCES memories(uuid),
    FOREIGN KEY (newer_uuid) REFERENCES memories(uuid)
);
```

The MVP uses this table only for the `supersedes` relation. General graph links are E3+.

### `memory_tags` — normalised tag index

```sql
CREATE TABLE memory_tags (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    memory_uuid     TEXT    NOT NULL,
    tag             TEXT    NOT NULL,   -- lowercase, trimmed; normalised externally
    created_at      TEXT    NOT NULL
                        DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),

    UNIQUE (memory_uuid, tag),

    FOREIGN KEY (memory_uuid) REFERENCES memories(uuid)
        ON DELETE CASCADE
);
```

Tags are written explicitly on save/update; the service does not auto-generate them. Maximum 32
tags per memory enforced at the application layer.

### Purge behavior

`purge` deletes the canonical memory and all of its associated rows — `memory_history`,
`memory_links`, and `memory_tags` for the purged `uuid` — in a single SQLite transaction.
The application service drives the deletes explicitly: there is no `ON DELETE CASCADE` on
`memory_history` or `memory_links` because purge must be deliberate, not silent. The audit
trail (`memory_history`) and the link graph (`memory_links`) stay under explicit control.
The only declared cascade is on `memory_tags` (for tag-row consistency), and the service
issues the explicit deletes to keep all four tables in lockstep. PRD §12's "purge eliminates
content, revisions, tags, references" is satisfied by this transaction.

### `idempotency_records` — deduplication

```sql
CREATE TABLE idempotency_records (
    id_key          TEXT    NOT NULL,    -- idempotency_key
    namespace       TEXT    NOT NULL,
    request_hash    TEXT    NOT NULL,    -- SHA-256 of canonical request JSON
    result_uuid     TEXT    NOT NULL,    -- uuid of the memory that was (or would be) created
    created_at      TEXT    NOT NULL
                        DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),

    PRIMARY KEY (id_key, namespace)
);
```

Retention of idempotency records is bounded to 7 days by the application service (configurable);
old records are pruned on startup and after each write.

### `schema_migrations` — migration ledger

```sql
CREATE TABLE schema_migrations (
    version         INTEGER PRIMARY KEY,
    applied_at      TEXT    NOT NULL
                        DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);
```

Every numbered migration inserts a row here. Missing rows indicate incomplete migrations.
The service refuses to open a database whose recorded version is higher than its own.

### `memories_fts` — FTS5 content table

```sql
CREATE VIRTUAL TABLE memories_fts USING fts5(content);
```

**Plain `fts5(content)`, not external-content or contentless.** T13 proved that plain
`fts5(content)` tables maintain the index correctly across UPDATE and DELETE and survive reopen
without a rebuild. This is the production choice with no change from T14.

### Production triggers (replaces and extends the T14 triggers)

T14's benchmark triggers are the correct starting point; they are extended as follows:

```sql
-- AFTER INSERT — index new active content
CREATE TRIGGER memories_after_insert_content
AFTER INSERT ON memories
WHEN new.deleted = 0
BEGIN
    INSERT INTO memories_fts(rowid, content) VALUES (new.id, new.content);
END;

-- AFTER UPDATE of content — re-index if still active
CREATE TRIGGER memories_after_update_content
AFTER UPDATE OF content ON memories
WHEN new.deleted = 0
BEGIN
    UPDATE memories_fts SET content = new.content WHERE rowid = new.id;
END;

-- AFTER UPDATE — FTS5 maintains the invariant
-- (state='active' AND deleted=0) ⇔ row present in memories_fts.
-- The T14 benchmark already used a single combined `AFTER UPDATE OF state, deleted`
-- trigger that unconditionally DELETEd from FTS (see `src/retrieval.rs:106–110`).
-- The production triggers SPLIT that into two mutually-exclusive paths so that
-- restore (UPDATE deleted 1→0 while state='active') and the proposed→active
-- transition both re-index via the into_active path, and exits from the
-- active/non-deleted region remove via the out_of_active path. The service
-- never issues manual FTS DML; the triggers own the invariant.
CREATE TRIGGER memories_after_update_into_active
AFTER UPDATE OF state, deleted ON memories
WHEN new.state = 'active' AND new.deleted = 0
BEGIN
    DELETE FROM memories_fts WHERE rowid = new.id;
    INSERT INTO memories_fts(rowid, content) VALUES (new.id, new.content);
END;

CREATE TRIGGER memories_after_update_out_of_active
AFTER UPDATE OF state, deleted ON memories
WHEN NOT (new.state = 'active' AND new.deleted = 0)
BEGIN
    DELETE FROM memories_fts WHERE rowid = new.id;
END;

-- AFTER DELETE — always remove
CREATE TRIGGER memories_after_delete
AFTER DELETE ON memories
BEGIN
    DELETE FROM memories_fts WHERE rowid = old.id;
END;
```

The T14 benchmark used a single combined `AFTER UPDATE OF state, deleted` trigger that
unconditionally DELETEd from FTS. The production triggers split that into two
mutually-exclusive paths (above) to maintain the invariant
`(state='active' AND deleted=0) ⇔ row in memories_fts`. The T14 spike did not exercise
restore or state transitions out of `proposed` into `active`, so the combined trigger
worked there; E2 is the first place restore and full state-transition coverage are
required, and the combined trigger would have broken both (restore would DELETE instead of
re-index; proposed→active would DELETE the row it just inserted).

`memory_supersede` runs entirely through the application service's normal write path:
it deletes the older memory and inserts the newer one in a single SQLite transaction. The
AFTER DELETE and AFTER INSERT triggers (above) maintain the FTS5 index automatically — the
application service does **not** issue any manual `INSERT/UPDATE/DELETE` against
`memories_fts`. This guarantees that FTS reflects the new state after the commit and is
consistent with the single-transaction FTS5-update rule in §5.

### Relation to T12 contract shapes

The production `Memory` struct (the in-memory Rust type) maps to the canonical `memories` table
as follows:

| T12 contract field | Production field | Notes |
|---|---|---|
| `id` | `uuid` (TEXT, UUID v4) | Stable across revisions; not the SQLite rowid |
| `topic_key` | `topic_key` | Exact match |
| `namespace` | `namespace` | Exact match |
| `memory_type` | `memory_type` | Exact match |
| `content` | `content` | 16 KiB enforced at service layer |
| `state` | `state` | Exact match; CHECK constraint enforces enumerated values |
| `revision` | `revision` | Integer; incremented on each update |
| `assertion_basis` | `assertion_basis` | Exact match |
| `valid_from` | `valid_from` | ISO-8601 or NULL |
| `valid_until` | `valid_until` | ISO-8601 or NULL |
| `created_at` | `created_at` | ISO-8601; set once at insert |
| `deleted_at` | `deleted_at` | Set when `deleted` becomes 1 |
| `purge_after` | `purge_after` | Set from retention policy at delete time |
| `actor` / `channel` | added | Required by PRD §5 provenance minimum |
| `tags` | `memory_tags` | Separate table, joined by uuid |
| `idempotency_key` | `idempotency_records` / application layer | Not a table column; checked at service layer |

The `AccessScope` (T12 `scope.schema.json`) is loaded by the application service from
configuration; it is never accepted from callers.

---

## 3. Module layout

### Current crate structure (T13/T14 spike)

```
src/
  lib.rs          # open_sqlite, SpikeError, module re-exports
  retrieval.rs    # BenchmarkStore, Memory struct, insert_memory, search
  mcp.rs          # SpikeServer, ping tool, serve_stdio
  main.rs         # clap: McpServe subcommand only
  corpus.rs       # Deterministic corpus generator (benchmark scaffold; RETIRED in E2)
```

### Production module layout (E2)

```
src/
  lib.rs              # Crate root: re-exports, library-level public API

  domain/             # Pure domain; NO rmcp, NO rusqlite, NO tokio
    mod.rs
    memory.rs         # Memory, MemoryHistory, MemoryLink, MemoryTagValue
    scope.rs          # AccessScope, Operation
    error.rs          # MnexisError (the stable error set)
    repository.rs     # Repository traits (ports); impls are in storage/
    idempotency.rs    # IdempotencyKey, IdempotencyRecord

  application/        # Use-case orchestration; stateless; depends on domain + repository traits
    mod.rs
    service.rs        # MemoryService: save, get, list, update, history, supersede,
                      #   soft_delete, restore, purge, export, import, reindex
                      # Owns all validation, revision checks, scope enforcement, state transitions
    budget.rs         # Budget accounting; lexical-estimated mode in E2, exact mode in E3

  storage/            # SQLite adapter; implements repository traits
    mod.rs
    sqlite.rs         # open_sqlite, SqlitePool, connection pragmas
    migrations.rs     # Migration runner (numbered, embedded)
    memory_repo.rs    # Repository impl for memories + history + links + tags
    idempotency_repo.rs # Repository impl for idempotency_records

  retrieval/          # FTS5 search; stateless; called by application service
    mod.rs
    fts5.rs           # search(query, namespace_filter, limit) -> Vec<SearchHit>
                      #   (the spike's search() becomes this module's public API)
    explain.rs        # search_explain: debug/doctor view of rankings and exclusions

  mcp/                # MCP server; thin wrapper over application service
    mod.rs
    server.rs         # McpServer: implements rmcp ServerHandler
    tools.rs          # Tool definitions (memory_save, memory_get, etc.)
    transport.rs      # stdio transport glue

  cli/                # CLI; thin wrapper over application service
    mod.rs
    commands.rs       # clap subcommands (version, init, save, update, …, doctor, eval)
    output.rs         # Text and JSON output formatters
    config_cmd.rs     # config show / set

  config/             # Configuration loading
    mod.rs
    settings.rs       # Settings: data_dir, scope, retention_policy, tokenizer

  diagnostics/        # Doctor and repair commands
    mod.rs
    doctor.rs         # doctor: check SQLite integrity, FTS5 availability, WAL health
    repair.rs         # doctor --repair: rebuild FTS5 from canonical data

  lib.rs re-exports
```

### Module disposition summary

| Spike module | E2 fate |
|---|---|
| `src/lib.rs` (open_sqlite, SpikeError) | Split: `storage/sqlite.rs` (open_sqlite with pragmas), `domain/error.rs` (MnexisError replacing SpikeError). `SpikeError` is retired; the `Fts5Unavailable` variant survives as `MnexisError::IndexUnavailable`. |
| `src/retrieval.rs` (BenchmarkStore, search) | Becomes `retrieval/fts5.rs`. The benchmark `Memory` struct is replaced by `domain/memory.rs`. |
| `src/mcp.rs` (SpikeServer, ping) | Extended to `mcp/server.rs` + `mcp/tools.rs`. SpikeServer → `McpServer`. |
| `src/main.rs` | Grows the full CLI via `cli/commands.rs`. The `McpServe` subcommand delegates to `mcp::serve_stdio`. |
| `src/corpus.rs` | **Retired.** Moved to `docs/historical/corpus.rs.txt` (same pattern as `docs/historical/t4_crash_replay.rs.txt`). The deterministic corpus and eval harness are replaced by the productionised `examples/t14_eval.rs` for development use only. |
| `examples/t14_eval.rs` | Productionised as a dev-only eval tool; not shipped as a user command. |
| `benches/fts5_latency.rs` | Retained as a dev benchmark. Criterion stays a dev dependency. |

### Domain independence guarantee

`domain/` contains zero imports from `rmcp`, `rusqlite`, or `tokio`. All I/O is behind
traits defined in `domain/repository.rs`. The `MnexisError` enum is defined here and
covers only domain-level failure modes (no `rusqlite::Error` variants).

---

## 4. Migrations

### Migration strategy

Migrations are numbered, embedded as string constants in `storage/migrations.rs`, and applied
at `open_sqlite` time before any other operation.

Numbering starts at `001` (the baseline that creates all tables). Each migration is
idempotent: it checks `schema_migrations` before running and skips if already applied. The
migration number stored in `schema_migrations` is the **target** schema version, not a
sequence counter.

### Migration list (E2)

| # | Description | Destructive? |
|---|---|---|
| `001` | Create all production tables and triggers (memories, memory_history, memory_links, memory_tags, idempotency_records, schema_migrations, memories_fts) | No — uses `IF NOT EXISTS`; idempotent and safe to re-run |
| `002` | (Future: add column, add index, etc.) | TBD |

Migration `001` is the baseline. If `schema_migrations` is empty, the runner applies `001`
and records the version. If `schema_migrations` contains the current version, the runner does
nothing.

### Baseline migration (001)

```sql
-- Run only when schema_migrations is empty
BEGIN;

-- Canonical table
CREATE TABLE IF NOT EXISTS memories (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    uuid            TEXT    NOT NULL UNIQUE,
    topic_key       TEXT    NOT NULL DEFAULT '',
    revision        INTEGER NOT NULL DEFAULT 1,
    namespace       TEXT    NOT NULL,
    memory_type     TEXT    NOT NULL DEFAULT 'fact',
    state           TEXT    NOT NULL
                        CHECK (state IN ('proposed','active','disputed','superseded','archived')),
    content         TEXT    NOT NULL,
    actor           TEXT    NOT NULL,
    channel         TEXT    NOT NULL,
    assertion_basis TEXT    NOT NULL,
    valid_from      TEXT,
    valid_until     TEXT,
    deleted         INTEGER NOT NULL DEFAULT 0,
    deleted_at      TEXT,
    purge_after     TEXT,
    created_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    updated_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    UNIQUE (namespace, topic_key) WHERE topic_key != ''
);

-- History
CREATE TABLE IF NOT EXISTS memory_history (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    memory_uuid     TEXT    NOT NULL,
    revision        INTEGER NOT NULL,
    memory_type     TEXT    NOT NULL,
    content         TEXT    NOT NULL,
    state           TEXT    NOT NULL,
    valid_from      TEXT,
    valid_until     TEXT,
    assertion_basis TEXT    NOT NULL,
    changed_by      TEXT    NOT NULL,
    change_reason   TEXT    NOT NULL,
    changed_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    FOREIGN KEY (memory_uuid) REFERENCES memories(uuid)
);

-- Links
CREATE TABLE IF NOT EXISTS memory_links (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    older_uuid      TEXT    NOT NULL,
    newer_uuid      TEXT    NOT NULL,
    link_type       TEXT    NOT NULL DEFAULT 'supersedes'
                        CHECK (link_type = 'supersedes'),
    created_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    UNIQUE (older_uuid, newer_uuid, link_type),
    FOREIGN KEY (older_uuid) REFERENCES memories(uuid),
    FOREIGN KEY (newer_uuid) REFERENCES memories(uuid)
);

-- Tags
CREATE TABLE IF NOT EXISTS memory_tags (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    memory_uuid     TEXT    NOT NULL,
    tag             TEXT    NOT NULL,
    created_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    UNIQUE (memory_uuid, tag),
    FOREIGN KEY (memory_uuid) REFERENCES memories(uuid) ON DELETE CASCADE
);

-- Idempotency
CREATE TABLE IF NOT EXISTS idempotency_records (
    id_key          TEXT    NOT NULL,
    namespace       TEXT    NOT NULL,
    request_hash    TEXT    NOT NULL,
    result_uuid     TEXT    NOT NULL,
    created_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    PRIMARY KEY (id_key, namespace)
);

-- Migration ledger
CREATE TABLE IF NOT EXISTS schema_migrations (
    version         INTEGER PRIMARY KEY,
    applied_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);

-- Drop the spike's legacy probe table if it exists (idempotent in-place upgrade from spike-era DBs).
DROP TABLE IF EXISTS mnexis_fts_probe;

-- FTS5
CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(content);

-- Triggers (replace any pre-existing ones atomically)
CREATE TRIGGER IF NOT EXISTS memories_after_insert_content
AFTER INSERT ON memories WHEN new.deleted = 0
BEGIN INSERT INTO memories_fts(rowid, content) VALUES (new.id, new.content); END;

CREATE TRIGGER IF NOT EXISTS memories_after_update_content
AFTER UPDATE OF content ON memories WHEN new.deleted = 0
BEGIN UPDATE memories_fts SET content = new.content WHERE rowid = new.id; END;

CREATE TRIGGER IF NOT EXISTS memories_after_update_into_active
AFTER UPDATE OF state, deleted ON memories WHEN new.state = 'active' AND new.deleted = 0
BEGIN DELETE FROM memories_fts WHERE rowid = new.id; INSERT INTO memories_fts(rowid, content) VALUES (new.id, new.content); END;

CREATE TRIGGER IF NOT EXISTS memories_after_update_out_of_active
AFTER UPDATE OF state, deleted ON memories WHEN NOT (new.state = 'active' AND new.deleted = 0)
BEGIN DELETE FROM memories_fts WHERE rowid = new.id; END;

CREATE TRIGGER IF NOT EXISTS memories_after_delete
AFTER DELETE ON memories
BEGIN DELETE FROM memories_fts WHERE rowid = old.id; END;

-- Record
INSERT INTO schema_migrations(version) VALUES (1);

COMMIT;
```

### Schema mismatch and doctor

If the recorded `schema_migrations.version` is **higher** than the binary's known migrations,
`open_sqlite` returns `MnexisError::UnsupportedVersion` and refuses to open the database. This
prevents a newer binary from silently degrading on an older database.

If the version is **lower**, the runner applies pending migrations in order.

`mnexis doctor` checks: (1) `schema_migrations` exists and has a row, (2) the recorded
version matches expectations, (3) all canonical tables exist with expected columns, (4)
`memories_fts` is readable, (5) WAL is functioning (no `-wal` without `-shm`), (6) no obvious
integrity errors (`PRAGMA integrity_check`). `--repair` rebuilds FTS5 from canonical data
(`memories_fts` dropped and recreated, triggers re-created, all non-deleted content
re-inserted via the application service).

### Backup before destructive migrations

A destructive migration (dropping a column, renaming a table) must create a WAL checkpoint
and copy `mnexis.db` to `mnexis.db.pre-<version>.bak` in the same directory before
executing. The application service checks that `mnexis.db` and its WAL/SHM are intact after
the migration commits.

### Testing migrations

- **Forward-only test**: start from an empty DB, apply `001`, assert all tables and triggers
  exist and `schema_migrations.version = 1`.
- **Idempotency test**: apply `001` to a DB that already has version 1; assert `001` is a
  no-op and the version remains 1.
- **Integrity test**: after applying `001`, run `PRAGMA integrity_check` and assert `ok`.
- **No downgrade test**: opening a DB whose recorded version is 999 (unknown to this binary)
  must return `MnexisError::UnsupportedVersion`.

---

## 5. Application service

The `MemoryService` in `application/service.rs` owns all business rules, validation, and
state transitions. It is a stateless struct that holds a repository handle; it is shared by
both the CLI and the MCP server.

### Use-case surface (closed set for E2)

| Operation | Description |
|---|---|
| `save` | Insert a new memory; enforce idempotency, topic_key uniqueness, limits |
| `get` | Fetch a memory by uuid; scope-authorise; check deleted |
| `list` | Paginated list with namespace/state/tag filters; scope-filtered |
| `update` | Update content/state/tags; require `expected_revision`; write history |
| `history` | Return revision log for a uuid; scope-authorise |
| `supersede` | Atomic link of two memories; state transitions; FTS5 update via triggers |
| `soft_delete` | Set deleted=1, deleted_at, purge_after; remove from FTS5 via trigger |
| `restore` | Clear deleted fields; re-index in FTS5; check post-supersede conflicts |
| `purge` | Hard delete of eligible records; cascade to history/links/tags |
| `export` | Snapshot selected memories as JSONL; exclude deleted unless `--include-deleted` |
| `import` | Parse JSONL; resolve conflicts via policy; transactionally write |
| `reindex` | Rebuild FTS5 from canonical data; used by doctor --repair |
| `search` | Delegates to `retrieval::fts5::search`; scope-authorises namespace filter |

`memory_context` is present in the service (E2 partial: lexical-estimated budget mode only)
and will be completed in E3 (exact tokenizer mode). The budget struct and selection logic live
in `application/budget.rs`.

### Validation rules

| Rule | Error on violation |
|---|---|
| `content` length ≤ 16 384 bytes UTF-8 | `INVALID_ARGUMENT` |
| `task` / query text ≤ 8 192 bytes | `INVALID_ARGUMENT` |
| `topic_key` format: lowercase, dots/hyphens/alphanumeric, max 128 bytes | `INVALID_ARGUMENT` |
| `namespace` non-empty string | `INVALID_ARGUMENT` |
| Tag count ≤ 32 per memory | `INVALID_ARGUMENT` |
| FTS5 query syntax: bareword alphanumeric tokens only (hyphenated tokens like `uso-after-free` are invalid single-term FTS5 syntax; the service rejects them with `INVALID_ARGUMENT` before sending to FTS5) | `INVALID_ARGUMENT` |
| `expected_revision` must match current revision (for `update`) | `REVISION_CONFLICT` |
| `expected_old_revision` AND `expected_new_revision` must each match (for `supersede`) | `REVISION_CONFLICT` |
| `topic_key` unique per namespace (if non-empty) | `TOPIC_CONFLICT` |
| `idempotency_key` + same request hash → same uuid (ok); same key + different hash → conflict | `TOPIC_CONFLICT` |
| Caller has write access to the target namespace | `FORBIDDEN` |
| Caller has read access to the target namespace | `FORBIDDEN` |
| Target record not deleted (for get/list/update/history) | `NOT_FOUND` |
| Supersede: both uuids exist and are active; no cycle | `INVALID_ARGUMENT` / `NOT_FOUND` |
| Restore: not already restored; no newer superseding memory | `INVALID_ARGUMENT` |

### State transitions

```
new memory       → proposed | active
proposed         → active | disputed | archived
active           → disputed | superseded | archived | deleted (soft)
disputed         → active | superseded | archived | deleted (soft)
superseded       → (no direct transitions; link points to the newer one)
archived         → active | deleted (soft)
soft-deleted     → restored | purged
restored         → active (same rules as active)
```

Transitions are enforced by a `next_states()` function. Any transition not in this map returns
`INVALID_ARGUMENT`.

### Optimistic revision

`expected_revision` is required for `update`. For `supersede`, both `expected_old_revision` and
`expected_new_revision` are required and must each match the current revision of the
respective record (this matches the T12 fixture `superseded-decision.json` which pins
`expected_old_revision` and `expected_new_revision`). The service executes:

```sql
UPDATE memories SET revision = revision + 1, content = ?, ... WHERE uuid = ? AND revision = ?
```

The affected row count is checked: 0 means `REVISION_CONFLICT` (returns current revision in
the error payload); 1 means success.

### Idempotency

On `save`, the service hashes the canonical request (namespace + idempotency_key +
content + memory_type + assertion_basis) with SHA-256. If an `idempotency_records` row exists
with the same `id_key + namespace` and the same hash, it returns the stored `result_uuid`
without writing. If the hash differs, it returns `TOPIC_CONFLICT`. Idempotency records older
than 7 days are pruned on every service call (before checking, then after writing).

### Scope enforcement

`AccessScope` is passed to `MemoryService::new` as an immutable reference. Every public
method begins with an authorization check: the caller's namespace is verified against
`read_namespaces` (for read operations) or `write_namespaces` (for write operations), and the
operation name is checked against `allowed_operations`. The `all` namespace literal is
expanded to the concrete list of `read_namespaces ∪ write_namespaces` before filtering.
`NOT_FOUND` vs `FORBIDDEN` is a service-level policy (the default is `NOT_FOUND` to avoid
revealing existence; this is configurable in `config/settings.rs`).

### Single-transaction FTS5 updates

Every write operation wraps canonical table writes + FTS5 index maintenance in one SQLite
transaction. The triggers (defined in §2) handle FTS5 updates automatically. The service does
not issue manual FTS5 INSERT/UPDATE/DELETE statements; it relies on the trigger firing within
the transaction. This matches the PRD §10 prescription and the T14 evidence.

### `memory_context` in E2

`memory_context` (PRD §9) is partially E2: the service implements the retrieval pipeline
(step 1–6 of PRD §9), the FTS5 candidate retrieval, the diversity/deduplication heuristic,
and the budget accounting. E2 budget mode is `estimated`: the service counts response bytes
and compares against `token_budget` as a soft upper bound (the byte budget is derived from the
token budget using a conservative 4 bytes/token estimate). If the response exceeds the budget,
items are dropped from the result until it fits. The `budget_enforcement` field in the output
is `"estimated"` and a `warnings` entry `"lexical_estimation"` is added. Exact tokenizer
counting (enabling `budget_enforcement: exact`) is E3.

---

## 6. Scopes

### Configuration source

`AccessScope` is loaded from the configuration file (`mnexis.toml` in the data directory) at
process startup. It is **immutable** for the lifetime of the process. The MCP server and CLI
both receive the same `AccessScope` value.

### Fields and semantics

| Field | Type | Semantics |
|---|---|---|
| `read_namespaces` | `Vec<String>` | Exact allowlist; only these namespaces are readable |
| `write_namespaces` | `Vec<String>` | Exact allowlist; only these namespaces are writable |
| `default_namespace` | `String` | Must be in both read and write namespaces; used when caller omits namespace |
| `allow_global` | `bool` | Explicit permission to resolve the literal `global`; never implicit |
| `allowed_operations` | `Vec<Operation>` | Closed set of operations the process may perform |

The literal `global` is **not** an alias for all namespaces in the profile; it is an explicit
namespace name that requires `allow_global = true`. When `allow_global = false`, the literal
`global` is treated as a non-existent namespace (returns `NOT_FOUND`).

`all` is **not** a wire value. Where the PRD uses `all` in prose it means "the concrete
union of `read_namespaces ∪ write_namespaces` for this process".

### Enforcement points

Every `MemoryService` method enforces scope before any I/O:

```
fn get(uuid, scope)          → check read_namespaces contains memory.namespace
fn list(filters, scope)     → inject namespace filter from read_namespaces
fn save(req, scope)         → check write_namespaces contains req.namespace
fn update(req, scope)       → check write_namespaces contains memory.namespace
fn supersede(req, scope)    → check write_namespaces contains both namespaces
fn soft_delete(req, scope)  → check write_namespaces contains memory.namespace
fn restore(req, scope)      → check write_namespaces contains memory.namespace
fn search(req, scope)       → check read_namespaces; inject namespace from request or default
fn history(uuid, scope)     → check read_namespaces contains memory.namespace
fn export(scope)            → check read_namespaces; honour namespace filter
```

The MCP server passes the `AccessScope` into the service; it does not re-check after the
service returns. The CLI does the same.

### NotFound vs Forbidden policy

When a caller requests a resource in a namespace they are not authorised to read, the
service returns `NotFound` by default rather than `Forbidden`. This avoids disclosing the
existence of records the caller cannot see. The policy is configurable per process in
`config/settings.rs` (`existence_disclosure: forbid | not_found`, default `not_found`).
`Forbidden` is the opt-in alternative when the operator explicitly wants the caller to know
they were denied. The same policy applies to the `NotFound` returned for soft-deleted
records to non-authorised callers. PRD A-04 (no out-of-scope content or ID disclosure)
asserts the deny-without-leak behaviour; the configurable default makes it the default
posture.

### Cursors and ID-based enumeration

Cursors are opaque base64-encoded strings that carry the effective `read_namespaces` filter.
If a caller manipulates a cursor to reference a namespace they are not authorised for, the
cursor decoding yields a filter that produces zero results (the cursor does not reveal the
underlying namespace). This matches the T12 `abstain-without-cross-namespace-disclosure`
scenario.

### Relation to T12 contract

`scope.schema.json` is the wire contract. The application service reads this JSON at
configuration time and constructs an `AccessScope` value. The service does not accept a scope
from a caller. The T12 fixtures (`no-answer-and-scope.json`, `two-agents-shared-profile.json`)
are reused as the contract layer's test suite in §10. **Reconciliation note for T17:**
`fixtures/e0/no-answer-and-scope.json` pins `FORBIDDEN` for an out-of-scope `memory_get`,
while this plan defaults `existence_disclosure: not_found`. Both are non-leaking
configurations; T17 runs the contract suite against both settings (the service supports
either via the `existence_disclosure` config) so the fixture and the default agree on the
non-leaking guarantee without conflicting on the exact error code.

---

## 7. CLI

### Commands (PRD §8)

| Command | Description |
|---|---|
| `mnexis version` | Print version and exit |
| `mnexis init [--data-dir DIR]` | Initialise a new profile; create `mnexis.db` and `mnexis.toml` |
| `mnexis mcp serve` | Run the MCP stdio server (same as today) |
| `mnexis save [--namespace NS] [--type TYPE] [--basis BASIS] [--idempotency-key KEY] [--valid-from F] [--valid-until U] [--tags TAG[,...]] [--state STATE] CONTENT` | Save a new memory |
| `mnexis update UUID --content CONTENT --expected-revision N --reason REASON` | Update a memory |
| `mnexis supersede --older UUID --newer UUID --reason REASON` | Supersede an older memory with a newer one |
| `mnexis search [--namespace NS] [--limit N] [--explain] QUERY` | FTS5 ranked search |
| `mnexis context [--namespace NS] [--token-budget N] [--tokenizer-id ID] [--max-items N] [--budget-mode exact\|estimated] TASK` | Retrieve budgeted context |
| `mnexis list [--namespace NS] [--state STATE] [--tag TAG] [--limit N] [--cursor CURSOR]` | List memories |
| `mnexis show UUID [--revision N]` | Show a memory (current or specific revision) |
| `mnexis history UUID [--limit N] [--cursor CURSOR]` | Show revision history |
| `mnexis delete [--force] UUID` | Soft-delete (prompts for confirmation unless `--force`) |
| `mnexis restore UUID` | Restore a soft-deleted memory |
| `mnexis purge [--dry-run] [--before ISO-DATE]` | Purge eligible memories (default: dry-run) |
| `mnexis export [--namespace NS] [--include-deleted] [--output FILE]` | Export memories as JSONL |
| `mnexis import [--dry-run] [--policy reject\|skip\|remap] [--input FILE]` | Import JSONL |
| `mnexis reindex [--repair]` | Rebuild FTS5 from canonical data (doctor --repair is the preferred command) |
| `mnexis doctor [--repair]` | Check integrity and optionally repair FTS5 |
| `mnexis config show` | Print effective configuration |
| `mnexis eval --query-file FILE --memories N` | Run the T14-style eval harness on a generated corpus (dev-only; see §10) |

`mcp serve` remains the only MCP subcommand; all MCP tools are accessed via the MCP protocol.

### Clap integration

- `structopt` / `clap_derive` with `ArgAction::Count` for `-v` verbosity flags.
- Subcommand tree: `Command::new("mnexis")` with nested `Command`s for each group.
- Global `--data-dir` and `--json` flags on the root command.
- `--json` switches output formatting to `serde_json::to_string_pretty`; text is the default.
- Exit codes: 0 = success, 1 = application error, 2 = `INVALID_ARGUMENT`, 3 = `NOT_FOUND`,
  4 = `FORBIDDEN`, 5 = `REVISION_CONFLICT` / `TOPIC_CONFLICT`, 6 = `INDEX_UNAVAILABLE`,
  7 = `BUDGET_TOO_SMALL`, 8 = `CURSOR_EXPIRED`, 9 = `UNSUPPORTED_VERSION`, 10 = `BUSY`.

### Admin commands

`purge`, `import`, `export`, `reindex`, and `doctor` are administrative. The CLI shares the
application service with MCP and enforces the same `AccessScope`. If the process is started
without a scope (e.g., `mnexis init`), a default permissive scope (`allow_global: true`,
`allowed_operations: all`, `read_namespaces: ["default"], write_namespaces: ["default"]`)
is used. The `config show` command reveals the effective scope without exposing data.

### Flags

- `import --dry-run`: parse and validate but do not write; report conflicts.
- `doctor --repair`: drop and recreate `memories_fts`, re-create triggers, re-index all
  non-deleted content. Runs inside a transaction; aborts on failure.
- `search --explain`: print per-result rank, bm25 value, matched terms, and exclusion reason
  (for authorised results only). Does not expose non-authorised candidates.

### Output format

Text (default):
```
mnexis list --namespace proyecto-a
  [proyecto-a] abc123-…  decision/active  "Accept Rust as primary language"  rev=3  2025-01-15
  [proyecto-a] def456-…  fact/active      "SQLite is the canonical store"    rev=1  2025-01-16
```

JSON (`--json`):
```json
{
  "items": [
    {
      "uuid": "abc123-…",
      "namespace": "proyecto-a",
      "memory_type": "decision",
      "state": "active",
      "content": "Accept Rust as primary language",
      "revision": 3,
      "created_at": "2025-01-15T…"
    }
  ],
  "count": 2,
  "namespace": "proyecto-a"
}
```

---

## 8. Errors

### Stable error set (PRD §8)

| MnexisError variant | Description | MCP code | CLI exit |
|---|---|---|---|
| `InvalidArgument { field, message }` | Validation failure | `-32050` | 2 |
| `NotFound { resource }` | Record or resource not found | `-32050` | 3 |
| `Forbidden { operation, reason }` | Caller not authorised | `-32050` | 4 |
| `RevisionConflict { current_revision }` | Optimistic lock failure | `-32050` | 5 |
| `TopicConflict { topic_key, namespace }` | Duplicate topic_key or idempotency mismatch | `-32050` | 5 |
| `Busy { message }` | SQLite BUSY after timeout | `-32050` | 10 |
| `IndexUnavailable { reason }` | FTS5 unavailable or broken | `-32050` | 6 |
| `BudgetTooSmall { budget, minimum_envelope }` | Response exceeds budget | `-32050` | 7 |
| `CursorExpired { reason }` | Opaque cursor no longer valid | `-32050` | 8 |
| `UnsupportedVersion { version }` | DB schema version newer than binary | `-32050` | 9 |

**MCP code convention.** All `MnexisError` variants map to the JSON-RPC server-error code
`-32050`. The specific failure label is carried in `data.error_code` (see the response
shape below). The standard JSON-RPC code `-32602` (Invalid params) is **reserved for
protocol-level malformed requests** (e.g. a tool call that does not match its input schema);
domain validation failures use `-32050` + `data.error_code: "INVALID_ARGUMENT"`. This keeps
the protocol code stable for MCP clients while preserving the fine-grained error semantics
the service already produces.

### MCP JSON-RPC error response shape

```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32050,
    "message": "RevisionConflict: expected revision 3, current is 4",
    "data": {
      "error_code": "REVISION_CONFLICT",
      "current_revision": 4
    }
  }
}
```

### Logging constraints

Error logs (tracing `ERROR` level) contain the `error_code`, field names, and revision numbers,
**never** the memory `content` or `uuid`. The `forbidden` variant logs the operation and
namespace, not the target uuid. Example: `WARN memory::update: forbidden namespace=proyecto-a`.
Memory content in an error context is a `DEBUG` trace at most, gated behind a feature flag.

---

## 9. FTS5 strategy

### Confirmation of the T14 approach

The T14 spike confirmed that a plain `fts5(content)` table with AFTER INSERT/UPDATE/DELETE
triggers is the correct production choice. Evidence:
- T13 proved plain FTS5 tables maintain the index correctly across UPDATE (including content
  replacement) and DELETE and survive reopen without rebuild.
- T14 demonstrated that bm25-ranked search over this table achieves Recall@5 = 1.0 and
  MRR@5 = 1.0 on a deterministic corpus of up to 100k memories.
- The trigger approach avoids the complexity of external-content tables (which require a
  shadow table and manual reindexing) and contentless tables (which cannot be queried directly,
  only used for token lookups).

External-content tables were not chosen because they require maintaining a shadow canonical
table and running `WHERE rowid = ?` lookups on every query, which doubles read latency and
adds schema complexity. Contentless tables were not chosen because the MVP needs to return
full content, not token lists.

### Production search API

`retrieval::fts5::search` replaces the spike's `retrieval::search`. The signature becomes:

```rust
pub fn search(
    conn: &Connection,
    query: &str,
    namespace: NamespaceFilter,
    state_filter: StateFilter,   // default: only 'active'
    deleted_filter: DeletedFilter,  // default: only non-deleted
    valid_window: Option<(Option<&str>, Option<&str>)>,  // (valid_from, valid_until)
    limit: usize,
) -> Result<Vec<SearchHit>, MnexisError>
```

Where `NamespaceFilter` is `Any` or `Namespace(&str)` (from the spike, retained) and
`StateFilter` / `DeletedFilter` default to the active/non-deleted policy.

The SQL uses fully parameterised queries throughout:

```sql
SELECT m.uuid, bm25(memories_fts) AS rank, m.namespace, m.memory_type,
       m.state, m.revision, m.created_at, m.valid_from, m.valid_until
FROM memories_fts
JOIN memories AS m ON memories_fts.rowid = m.id
WHERE memories_fts MATCH ?1          -- parameter: query text
  AND m.namespace = ?2              -- parameter: namespace
  AND m.state = 'active'            -- implicit filter
  AND m.deleted = 0                 -- implicit filter
  AND (?3 IS NULL OR m.valid_from IS NULL OR m.valid_from <= ?3)
  AND (?4 IS NULL OR m.valid_until IS NULL OR m.valid_until >= ?4)
ORDER BY rank ASC
LIMIT ?5                            -- parameter: limit
```

The `bm25()` function is called without explicit parameters; SQLite uses the default
`k1=1.2, b=0.75` settings. Return values are **negative**: bm25 in FTS5 is negated internally
so that **lower (more negative) values are better (more relevant)**. A hit with `rank = -2.5`
is more relevant than one with `rank = -0.8`. The search result includes the rank value so
callers can log or display it; it is not used for any business logic in E2.

### WAL pragmas

The connection opened by `open_sqlite` sets the following pragmas before any other
operation, carried from the T13/T14 evidence base (PRD §11 + the spike's
`src/lib.rs:18–27` / `src/retrieval.rs:135–139`):

- `journal_mode = WAL` — concurrent readers with a single writer; crash-safe.
- `foreign_keys = ON` — enforces the `FOREIGN KEY` constraints in `memory_history`,
  `memory_links`, and `memory_tags`.
- `busy_timeout = 2000ms` — short writers wait up to 2 seconds on `SQLITE_BUSY` before
  returning `MnexisError::Busy`.

Dropping any of these pragmas on the production migration rewrite would be a regression
versus the spike.

### Rebuild from canonical data

`doctor --repair` drops `memories_fts`, re-creates it, re-creates the triggers, then
iterates all non-deleted `active` memories in batches of 1000 and issues `INSERT INTO
memories_fts(rowid, content) VALUES (id, content)` via the canonical table's id. This is the
correct rebuild procedure for a plain `fts5(content)` table.

---

## 10. Test strategy

### Unit tests (per-module)

| Module | Test focus |
|---|---|
| `domain/error.rs` | Every error variant round-trips through `thiserror`. |
| `domain/memory.rs` | Memory construction, revision increment, state transition map. |
| `domain/scope.rs` | Scope parsing, `all` expansion, `global` resolution, operation matching. |
| `storage/migrations.rs` | Forward-only, idempotency, integrity check, no-downgrade. |
| `storage/memory_repo.rs` | CRUD round-trips, history append, tag upsert, link upsert, conflict detection. |
| `application/service.rs` | Each use case: save, get, update, supersede, delete, restore, purge, export, import. |
| `retrieval/fts5.rs` | search round-trip with BM25 rank ordering, namespace/state/deleted filters, empty query. |
| `mcp/tools.rs` | Tool request → service call mapping, scope propagation. |
| `cli/output.rs` | Text and JSON formatting of all result types. |

### Integration tests

- **Application service end-to-end**: open a fresh SQLite DB, run a sequence of save/update/
  supersede/delete/restore operations, assert the canonical table, FTS5, and history are all
  consistent. Simulates a crash after every step by reopening the connection.
- **Scope enforcement**: run each use case with an out-of-scope namespace and assert the
  correct error variant (not a panic or generic 500).
- **MCP interop**: extend the T10 test (`tests/mcp_interop.rs`) from a ping smoke to a
  sequence of `memory_save` / `memory_get` / `memory_search` across the stdio boundary,
  using the official rmcp client.
- **CLI integration**: invoke the binary with each command via `assert_cmd`, capture stdout/
  stderr, assert exit codes and output shapes. Use `tempfile::tempdir_in("target")`.

### PRD §E2 close tests

These are the minimal tests that must be green to call E2 done (derived from the PRD §E2
"Pruebas de cierre"):

| Test | What it proves |
|---|---|
| `persist_after_restart` | Close and reopen the SQLite DB; all memories and revisions are intact. |
| `concurrent_update_conflict` | Two processes update the same memory with the same `expected_revision`; one succeeds, one gets `REVISION_CONFLICT`; no data loss. |
| `history_complete` | After three updates, `history` returns all three previous revisions in order. |
| `scope_forbidden_on_unauthorised_namespace` | A call with a namespace not in `read_namespaces` returns `FORBIDDEN` (or `NOT_FOUND` per policy) without leaking content. |
| `scope_forbidden_on_id` | A call with a `uuid` whose namespace is out of scope returns `FORBIDDEN` / `NOT_FOUND`. |
| `supersede_atomic` | `supersede` atomically sets older=superseded, newer=active, inserts the link; a crash between the two does not leave a partial state. |
| `delete_removes_from_fts` | After `soft_delete`, `search` does not return the deleted memory. |
| `restore_reindexes` | After `restore`, `search` returns the restored memory. |
| `doctor_detects_missing_fts` | With FTS5 unavailable, `doctor` returns `INDEX_UNAVAILABLE` and the search path degrades explicitly. |

| `cursor_manipulation_does_not_disclose_unauthorised_namespace` | A tampered opaque cursor that encodes a namespace outside `read_namespaces` returns zero results and never the underlying record. | A-04, §7 cursor filtering |
| `concurrent_update_three_processes` | Three processes perform save/update/delete concurrently on the same profile: no loss, no superseded records visible, no deleted records returned. | A-14, §11 gate |
| `partial_failure_no_partial_writes` | A forced failure mid-transaction during save/update/delete leaves the canonical table and FTS5 in exactly the prior state. | A-12 |

### PRD §15 quality gates (E3 scope, noted here)

The frozen 500-memory/150-query fixture and the A-08/A-09 metrics are E3 acceptance criteria,
not E2 criteria. The T14 eval harness (`examples/t14_eval.rs`, `benches/fts5_latency.rs`) is
productionised as a **development tool** for measuring quality during development. It is not
shipped as a user command and is available only via
`cargo run --example t14_eval …` (the `mnexis` binary never dispatches to it).
`criterion` stays a `[dev-dependency]` and is therefore excluded from release builds
by Cargo's default behaviour. The reproduction commands from
`docs/t14-fts5-benchmark.md` are the quality-gate template for the E3 fixture.

### Logical delete / restore / purge testing

- `soft_delete`: assert `deleted=1`, `deleted_at` set, `purge_after` computed from retention
  policy, FTS5 returns 0 results for the uuid.
- `restore_before_purge`: assert `deleted=0`, `deleted_at=null`, FTS5 returns the memory again.
- `restore_after_purge`: assert `NOT_FOUND` (record no longer exists).
- `purge_eligible`: create a memory, soft-delete with `purge_after` in the past, call purge,
  assert record and all history/links/tags are gone.
- `purge_respects_namespace`: a purge call scoped to `namespace=A` must not touch
  `namespace=B` even if B's records are eligible.

### T12 contract tests

The existing `tests/e0_contracts` suite (which validates the JSON schemas against the fixtures)
is extended with Rust-level tests that exercise the application service using the same
fixture data as the contract examples. This ensures the service honours the contract without
being a pure schema validator.

---

## 11. Sequencing into sub-tasks

### Sub-task order and dependencies

```
[T15] Domain types and error set
  Depends: nothing (first task)
  Owns: domain/memory.rs, domain/error.rs, domain/scope.rs, domain/idempotency.rs
  Size: ~400 lines; 2–3 days
  Close gate: all domain unit tests green

[T16] Storage adapter — migrations, repository impl, open_sqlite
  Depends: T15
  Owns: storage/sqlite.rs, storage/migrations.rs, storage/memory_repo.rs,
        storage/idempotency_repo.rs
  Size: ~600 lines; 3–4 days
  Close gate: migration tests green, repository CRUD integration tests green

[T17] Application service — use cases, validation, scope enforcement
  Depends: T16
  Owns: application/service.rs, application/budget.rs (estimated mode only)
  Size: ~800 lines; 4–5 days
  Close gate: all E2 close tests green

[T18] FTS5 retrieval module
  Depends: T16
  Owns: retrieval/fts5.rs, retrieval/explain.rs
  Size: ~300 lines; 2 days
  Close gate: search integration tests green, FTS5 rebuild via doctor --repair works

[T19] CLI commands
  Depends: T17, T18
  Owns: cli/mod.rs, cli/commands.rs, cli/output.rs, config/settings.rs
  Size: ~500 lines; 3–4 days
  Close gate: CLI integration tests green, all commands render correct output

[T20] MCP server — full tool surface
  Depends: T17, T18
  Owns: mcp/server.rs, mcp/tools.rs, mcp/transport.rs
  Size: ~400 lines; 3 days
  Close gate: MCP interop test green, all tools respond correctly

[T21] Diagnostics — doctor, repair, reindex
  Depends: T18
  Owns: diagnostics/doctor.rs, diagnostics/repair.rs
  Size: ~200 lines; 1–2 days
  Close gate: doctor detects FTS5 unavailability; doctor --repair rebuilds correctly

[T22] Import / export
  Depends: T17, T21
  Owns: import/export in application/service.rs, CLI subcommands
  Size: ~300 lines; 2 days
  Close gate: round-trip test: export from DB A, import to empty DB B, assert identical;
              import --dry-run reports correct conflict counts

[T23] End-to-end close tests
  Depends: T15–T22
  Owns: integration tests for all E2 close tests (§10)
  Size: ~300 lines; 2 days
  Close gate: all E2 close tests green; no regressions in T13/T14 tests
```

### First task

**T15 — Domain types and error set.** This task has zero dependencies and establishes the
public API surface. It defines `Memory`, `MemoryHistory`, `MemoryLink`, `AccessScope`,
`MnexisError`, and the repository traits. It is the foundation for all subsequent tasks and
can proceed immediately.

### Minimum viable E2 gate (subset to call E2 done)

The following tasks must be green before the parent considers E2 closed, even if T20–T23
are still in progress:

1. T15 (domain) + T16 (storage) + T17 (service) + T18 (retrieval) → all E2 close tests pass.
2. T19 (CLI) → at least `save`, `list`, `show`, `history`, `search`, `delete`, `restore`,
   `doctor` commands work end-to-end.
3. T21 (diagnostics) → `doctor` and `doctor --repair` work.

MCP tools (T20) and import/export (T22) can be verified in a subsequent review round if
needed to unblock E3.

### Deferred to E3

- Exact tokenizer-backed `memory_context` (E3: token counting, `budget_enforcement: exact`).
- The frozen 500-memory/150-query evaluation fixture and A-08/A-09 metrics.
- Reranking.
- `memory_links` beyond `supersedes`.
- Multi-client MCP (second distinct client).
- Embeddings or any semantic search beyond FTS5.

---

## 12. Decisions to confirm with the user

The following choices are made in this proposal. Each requires explicit user confirmation
before implementation begins.

### D1 — Exact `Memory` struct field set and types (HIGH PRIORITY)

The proposal defines 20 fields on `Memory`. Confirm:
- `actor` (TEXT, required) and `channel` (TEXT, required) are the right provenance fields.
  Alternatives: a single `provenance` JSONB column, or a separate `provenance` table.
- `assertion_basis` uses an enumerated CHECK constraint (`user_stated | observed | inferred |
  imported`). Confirm this covers all cases or suggest additions.
- `memory_type` is a free-text string with a default of `'fact'`; the T12 contract uses
  strings like `"decision"`, `"fact"`, `"preference"`. Confirm this is sufficient or whether
  an enumerated set is preferred.

### D2 — `valid_from` / `valid_until` semantics edge cases (HIGH PRIORITY)

- When `valid_until` is in the past, the memory is ineligible for `memory_context` but still
  visible via `get` and `list`. Confirm: should `search` return expired memories by default?
  (Proposal: no — `memory_search` filters by validity window by default; an explicit
  `--include-expired` flag may be added in E3.)
- When `valid_until` is NULL, the memory is valid indefinitely. Confirm.
- When `valid_from` is NULL, the memory is valid from creation. Confirm.

### D3 — Naming: `SpikeError` / `SpikeServer` historical names (LOW PRIORITY, COSMETIC)

The spike uses `SpikeError` and `SpikeServer`. The proposal renames these to `MnexisError`
and `McpServer` in E2. This is purely cosmetic. If the user prefers to preserve the
historical names as-is for the production service (they carry no semantic meaning), confirm
that preference.

### D4 — Admin CLI surface scope (MEDIUM PRIORITY)

The proposal makes `purge`, `import`, `export`, `reindex`, and `doctor` CLI commands under
user authority (no separate admin role in MVP). Confirm:
- Should `purge` require an explicit `--confirm` flag? (Proposal: yes, with a default dry-run
  that shows what would be purged.)
- Should `import` support a `--conflict-policy` flag with values `reject | skip | remap`?
  (Proposal: yes, as described in PRD §13.)
- Should `config set` be a writeable command, or read-only (`config show`) in MVP?
  (Proposal: read-only in MVP; configuration is done by editing `mnexis.toml` directly.)

### D5 — Schema migration strategy: clean vs. from T14 benchmark schema (HIGH PRIORITY)

The proposal creates a clean production schema (migration `001`) and does not migrate from
the T14 benchmark schema. **The T14 benchmark databases live under `target/`, which is
gitignored and never leaves the host** — there is no production data to migrate, only code
that can be replayed. This makes "clean start" the only safe choice: a migration path adds
`001` complexity (read the benchmark `memories`, transform rows into the production shape,
drop benchmark columns) for no real benefit, and the benchmark schema lacks the provenance,
idempotency, and tag fields the production service requires (so the transformation would
fail or lose data anyway).

**Recommendation: clean start.** If a user has spike-era data they wish to preserve, the
only honest path is to re-export it through the spike (T14's eval harness is not a
production exporter) and re-import it under E2 via the `import` command with a
hand-written JSONL. This is acceptable because the spike is an E1 evidence tool, not a
data-collection product.

**Decision needed: confirm clean start (recommended).**

### D6 — Legacy `mnexis_fts_probe` table (LOW PRIORITY, CLEANUP)

The spike's `open_sqlite` creates a probe table `mnexis_fts_probe` to verify FTS5
availability. The production `open_sqlite` in `storage/sqlite.rs` does not need this table.
Confirm: should `storage/sqlite.rs` explicitly drop `mnexis_fts_probe` if it exists (e.g., on
a DB created by the spike), or should it be left untouched?

### D7 — Branch strategy: continue `feat/mnexis-e0-e1` or new branch (HIGH PRIORITY)

The proposal is authored on `feat/mnexis-e0-e1`. E2 implementation will land on this branch.
Confirm: should E2 commits continue on `feat/mnexis-e0-e1`, or should a new branch
`feat/mnexis-e2` be created? (Per the ODD recovery log, `feat/mnexis-e0-e1` has been used for
both E0/E1 evidence work and the spike. Continuing on this branch is the proposal.)

### D8 — Budgeted-output tokenizer in E2 vs E3 (HIGH PRIORITY)

The proposal implements `memory_context` in E2 with `budget_enforcement: estimated` only.
Exact tokenizer-backed enforcement requires a token counting implementation (the PRD §3
mentions `tokenizers` as a potential dependency). Confirm:
- E2 uses byte-counting as a proxy (4 bytes/token estimate). Is this acceptable for the
  initial E2 CLI/MCP surface?
- Should the `tokenizers` crate be added as a dependency in E2 (and hence E3), or is the
  lexical-estimated approach preferred until E3?

### D9 — FTS5 trigger approach: keep combined `state, deleted` UPDATE trigger (LOW PRIORITY)

The proposal combines the T14 benchmark's separate `AFTER UPDATE OF state` trigger and the
new `AFTER UPDATE OF deleted` logic into a single `AFTER UPDATE OF state, deleted` trigger.
This is a minor simplification. Confirm this is acceptable, or whether keeping them separate
is preferred for clarity.

### D10 — `corpus.rs` retirement path (LOW PRIORITY)

`src/corpus.rs` is ~1000 lines of benchmark scaffold. The proposal retires it to
`docs/historical/corpus.rs.txt` following the same pattern as `docs/historical/t4_crash_replay.rs.txt`. Confirm: should the file be preserved as a historical artifact, or is it acceptable to simply delete it from the repository? (The T14 eval harness and benches already exist as separate productionised artifacts.)

---

## 13. Risks and dependencies on E1 evidence

### Verification debt summary

| Task | Evidence status | Impact on E2 confidence |
|---|---|---|
| T13 — FTS5/rmcp stack isolation | **PASS** (independent verification) | HIGH: anchor proves SQLite WAL + FTS5 + rmcp works as a foundation |
| T14 — FTS5 ranked search + budgets | **PASS** (independent verification post-cleanup) | HIGH: anchor proves bm25 ranking, trigger-based index maintenance, latency/RAM/disk budgets |
| T12 — E0 contracts v0.5 reconciliation | **Unverifiable** (Node -122, 2 user-attempted retries) | MEDIUM: contracts are committed by explicit user decision (53e19c3); the service must honour them anyway |
| T10 — real rmcp-client stdio interop | **Green** (verification unavailable, Node -122) | MEDIUM: the MCP server implementation (T20) must be validated against real clients; T10 green by assertion is not the same as a PASS |

T13 and T14 are the **E2 anchors**. Their independent verification status gives confidence in
the storage and retrieval foundations. The E2 production service builds on these foundations
and must not break the T13/T14 tests.

### `/tmp` quota mitigation

The `.cargo/config.toml [env] TMPDIR = { value = "tmp", relative = true }` mitigation applied
in L72 redirects cargo-spawned processes to the repository `tmp/` directory. This resolves
the EDQUOT failures that blocked L19, L64, and multiple `gentle-ai-verify` attempts.

**Additional mitigation needed:** The T14 verification harness creates `mnexis-verify-*`
workspace directories under `target/` (documented in e0-e1-recovery.md L82). Each such
workspace can consume ~13 GB, filling the 12.7 GiB per-user tmpfs quota. Add
`target/mnexis-verify-*` to the safe-cleanup list: these directories should be removed by the
harness after each run, or excluded from quota accounting by moving them entirely into the
repository (e.g., `target/verify-*`). The user is responsible for cleaning these manually
until the harness is updated.

### PRD §15 reminder on budgets

The numeric budgets in `docs/t14-fts5-benchmark.md` (warm search ≤ 0.5 ms, cold ≤ 5 ms,
batched ingestion ≥ 5 000 memories/s, `mcp-serve` RSS ≤ 32 MiB, binary ≤ 8 MiB) are
**spike proposals, not acceptance verdicts**. They are grounded in measured numbers but on a
single Linux host. The PRD §15 requirement is that the spike "fixes numeric budgets before
closing design." These proposals are the fix; they must be confirmed or revised during E2
implementation (particularly the `mcp-serve` RSS after adding the full tool surface).

### Schema migration risk

Starting from a clean schema (migration `001`) is the safest approach: no data transformation
bugs can corrupt existing data. The risk is that users with spike-era data must re-import
their memories. The spike does not support a migration path because it lacks the provenance,
idempotency, and tag fields. If the user has significant spike-era data, D5 must be resolved
in favour of a migration path.

### FTS5 availability risk

The production service must degrade explicitly when FTS5 is unavailable (PRD §8
`INDEX_UNAVAILABLE`). The `doctor` command checks FTS5 availability and reports it. The risk
is that a corrupt FTS5 index silently returns no results, violating A-11. Mitigation: `doctor
--repair` rebuilds the index from canonical data (A-13). The E2 close tests include the
`INDEX_UNAVAILABLE` degradation test.
