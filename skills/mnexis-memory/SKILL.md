---
name: mnexis-memory
description: Persist, recall, and supersede long-term memories with the mnexis CLI. Use when a task needs memory across sessions — recalling prior work, saving decisions, discoveries, or user preferences, or replacing an outdated stored fact without losing history.
license: MIT
---

# mnexis memory

mnexis is a local memory engine: one binary, SQLite + FTS5, no daemon, no
cloud. Memories are plain text rows with namespace, tags, revision history,
and supersession chains.

## Setup

`mnexis --version` must succeed. If no profile exists yet, `mnexis init`
creates it at `$HOME/.local/share/mnexis` (`MNEXIS_DATA_DIR` overrides).
Every command accepts `--data-dir <DIR>` to target a different profile.

## The loop

1. **Recall first.** Before answering from scratch, look for prior context:
   `mnexis search "<keywords>"` (FTS5 ranked; `--namespace`, `--limit`),
   `mnexis list`, and `mnexis show <uuid>` to read one record.
2. **Save what matters.** After a milestone — a decision, a discovery, a
   user preference — store it immediately:
   `mnexis save "<one self-contained sentence>" --tags a,b --topic-key <stable-key>`.
   The content must make sense months later with no surrounding chat.
   `--topic-key` is unique per namespace: reuse it to group facts about one
   topic. `--idempotency-key` makes retries safe.
3. **Replace facts by superseding, never by deleting.** Outdated but
   historically true facts keep their record: `mnexis save` the newer fact,
   then `mnexis supersede --older <old-uuid> --newer <new-uuid> --reason "why"`.
   The supersede is atomic, revision-checked on both sides, cycle-free, and
   persists the reason. `mnexis history <uuid>` shows the chain.
4. **Correct a record in place** only for typos/formatting:
   `mnexis update <uuid> --expected-revision <rev> --reason "why" [--content "..."]`.

Soft-delete only on request: `mnexis delete <uuid> [--force]`, reversible
with `mnexis restore <uuid>`.

## Semantics that matter

- Optimistic revisions: writers send `--expected-revision`; on
  `REVISION_CONFLICT` (exit 5) re-read with `show` and retry with the
  current revision.
- Exit codes: 2 invalid argument, 3 not found, 4 forbidden, 5 revision or
  topic conflict, 6 index unavailable, 7 budget too small, 8 cursor expired,
  9 unsupported version, 10 busy. 0 is success; 1 is infrastructure.
- `--json` on read commands gives machine-readable output — prefer it when
  parsing uuids or revisions.
- MCP hosts: instead of the CLI, point the host at `mnexis mcp serve`
  (stdio; pass `--data-dir` in the server command line).
