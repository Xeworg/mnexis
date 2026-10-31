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
- **S4 (judgment, evidence-gated)** — the hooks capture the mechanical floor; the judgment
  ("did this session deserve a memory?") still depends on goodwill. At `agent_before_settle`, if
  the session shows evidence of unsaved work (file-writing tool calls happened AND zero
  `mnexis_save` this session), append a one-line nudge and request ONE continuation — at most
  once per session. The model evaluates once with the whole session in view; read-only or
  already-saving sessions pay nothing. No new tool prose.

## Tasks

| ID | Specs | Route | Branch / worktree | Commit |
| --- | --- | --- | --- | --- |
| T1 | S1–S3 | worker: read the pi extensions doc, then implement + test | `ext/pi-capture` @ `tmp/wt-pi-capture` (task mvvykzl7-7-jb9n) | `d473637` (S2) |
| T3 | S4 | worker continuation: the settle nudge, evidence-gated | same branch | — |
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
- **L4 (2026-10-31, T1 result, parent-verified)** — **S1 was already at base**: `a18a781`
  (2026-10-29) ships the `input` capture hook with its tests; the board's "nothing writes to the
  prompt store" was stale (seen through the host-drift window) and is corrected on main.
  **S2 shipped as `d473637`**: `session_shutdown` ends the engine session and saves one canonical
  `estado al cierre` project_state (same-title revisioning, per-namespace grain); reload is
  deliberately skipped (pi replaces the runtime, not the session); failures journal via
  `pi.appendEntry`, never reject shutdown. No narrative nudge: an `agent_before_settle`
  continuation costs a model call per settle; the narrative stays on the existing
  `session_end --summary` path. **S3 holds**: 14 tools, 5 490 B, pin green. Suite 189 tests /
  186 pass / 3 skips. Parent re-ran the 7 session-close tests and the pin. Branch stays UNMERGED
  for the human's decision.
