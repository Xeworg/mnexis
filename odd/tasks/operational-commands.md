# operational-commands — stats, timeline, and a stable project list

Board item C4 (`odd/next.md` §3): "Inspecting a namespace without opening
the TUI." Three read-only operator commands; no behavior change to anything
that exists.

Branch: `feat/operational-commands`.

## Specs

- **S1** — Board, verbatim: "`mnexis stats`, `mnexis timeline <uuid>`,
  `mnexis projects list`. Inspecting a namespace without opening the TUI.
  Small." `mnexis stats`: store-level counts over the whole data dir —
  per-namespace memories (active, superseded, soft-deleted), totals
  (memories, supersede links, sessions, prompts), and the schema version.
  JSON (snake_case, typed struct) + a compact text table. Read-only, its own
  connection (the doctor pattern).
- **S2** — `mnexis timeline <uuid>`: one memory's whole life in one answer —
  its identity (title, topic key, namespace, type, current state), its
  revision events in order (from `memory_history`), and its supersede
  relations in both directions (from `memory_links`, with reason and date).
  An unknown uuid is NOT_FOUND (exit 3), not an empty list.
- **S3** — Comparison report §5 item 9, verbatim: "`mnexis project list
  --json` as a stable contract". The command exists (`project list`, the
  registry bindings); its JSON becomes a typed struct with pinned field
  names (the doctor-envelope test pattern), text output unchanged. The
  board's "projects list" and the repo's `project list` are the same
  command — the repo's spelling wins (report's wording).
- **S4** — Everything is read-only: no mutation of the store, no scope
  change, no MCP/TUI surface. Existing commands that share the touched code
  (`history`, `show`, `project list`) keep their current output, pinned by
  their existing tests.

## Tasks

| ID | Spec | Route | Commit |
| --- | --- | --- | --- |
| T1 | S1 | `stats` command (new `src/cli/stats_cmd.rs`, doctor-pattern connection), tests RED→GREEN | |
| T2 | S2 | `timeline <uuid>` (service wrapper over `history` + `links_for`, NOT_FOUND), tests | |
| T3 | S3 + S4 | `project list --json` typed contract + pinned field test; docs, board, issue + PR | |

## Log

- **L1** (2026-10-24) — operator: "dale ramas muertas y luego lo proximo
  del board" — the dead branches were deleted and C4 picked up.
- **L2** (2026-10-24) — scope source: `tmp/bench/report.md` §5 item 9 sizes
  this S; the board condensed it. Delivery follows the repo's new policy:
  one issue per PR, one `type:*` label.
