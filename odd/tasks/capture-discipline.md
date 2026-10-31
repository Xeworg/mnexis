# Capture discipline in the pi extension — hooks, not prose

The 2026-10-31 dogfood measured it (board, `c32a822`): during a working session neither memory
engine gave the agent one useful read — the writes are insurance for the next resume — and the
only difference was write discipline (a compelled protocol got ~8 boundary saves; an advisory
skill got 1). This feature makes the pi extension's capture **mechanical**.

## Specs

- **S1 (prompt capture)** — an `input`/`before_agent_start` hook writes the user's prompt to the
  engine's prompt store. The capability already exists and nothing exercises it (the store is
  empty): this hook is the free win.
- **S2 (session close)** — a `session_shutdown` hook keeps exactly **one live "estado al cierre"
  `project_state` per workspace**, revisioned through the engine's same-title rule. Code writes
  the mechanical parts (the engine's session commands); a narrative nudge at
  `agent_before_settle` is optional and only if it costs nothing per call.
- **S3 (no new prose)** — the tools block just paid 3 524 B to say less: no new tool descriptions,
  no new tools; the ≤ 5 500 B cost pin stays green. Discipline is mechanical, not asked for.

## Tasks

| ID | Specs | Route | Branch / worktree | Commit |
| --- | --- | --- | --- | --- |
| T1 | S1–S3 | worker: read the pi extensions doc, then implement + test | `ext/pi-capture` @ `tmp/wt-pi-capture` | — |
| T2 | all | parent: verify, commit, report for the merge decision | — | — |

## Log

- **L1 (2026-10-31, user)** — "pon un agente a leer esto https://pi.dev/docs/latest/extensions …
  el que lea el documento de pi la mejore para mejorar la diciplina como lo haz mencionado antes
  hazlo en dos ramas aparte luego vemos si le hacemos merge o no".
- **L2 (design source)** — the board entry `c32a822` ("the capture gap the dogfood measured —
  hook-driven, not prose") is the design of record.
- **L3 (references)** — `tmp/reference/pi-extensions.md` in the worktree is the runtime-matching
  copy of the URL above (structure verified equal on 2026-10-31), plus selected official examples
  under `tmp/reference/pi-examples/`. The existing extension already uses `before_agent_start`
  (workspace resolution) and `tool_call` (recovery) — follow their patterns.
