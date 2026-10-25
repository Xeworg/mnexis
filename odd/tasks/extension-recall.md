# extension-recall — the one-call door, opened and priced

The re-comparison (odd/next.md §6) measured the recall at the other engine's
level but at twice its cost: the pi extension does not expose the recall, so
the agent rebuilds it by hand (45 `mnexis_show` calls across ten sessions),
and a recall that answers with 100 full memories would be a context bomb.
This feature opens the door and prices what comes through it.

## Specs

S1 (price — odd/next.md §6, move 2, quoted verbatim):

> **Price the recall in context**: identity and preview by default, the full
> content on request — the shape `list` got in #24/#26 and `memory_search`
> already has. A recall that answers with 100 full memories is not the cheap
> door the extension would open.

S2 (expose — odd/next.md §6, move 1, quoted verbatim):

> **Expose the recall in the pi extension** (and `memory_workspace` and
> `memory_history`, which are missing there too). The door is built; the
> agent cannot see it.

## Tasks

| ID | Specs | Route | Commit |
| --- | --- | --- | --- |
| T1 The recall is priced: `memories_in_window` answers with identity + preview by default; `--full` (CLI) / `full` (MCP) asks for the whole records. One projection (`SessionRecall::payload`), both surfaces | S1 | inline | 3ed1f45 |
| T2 `mnexis_recall` in the pi extension: client method + tool + tests | S2 | inline | 182b7f3 |
| T3 `mnexis_workspace` + `mnexis_history` in the extension, and the docs that name the tools (SKILL.md, extension README) | S2 | inline | |

## Log

L1: "dale al item 6" (the operator, 2026-10-26) — implement board item 6
(odd/next.md §6): the two moves the re-comparison produced.
