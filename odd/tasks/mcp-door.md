# The door in the MCP surface

**Feature:** `mcp-door` · **Branch:** `feat/mcp-door` · **Status:** to start

The one-call door exists in the CLI and in the pi extension, and **not** in
the MCP surface. The MCP server the binary ships has sixteen tools, and
`memory_context` is not one of them: it has `memory_workspace` (the priced
identity and preview) and `memory_recall`, but not the door — and therefore
not the session state that rides it, and not the coverage declaration.

So in opencode — and in the codex and claude code integrations still to
come — the agent has to **rebuild the answer by hand**, which is exactly
what PR #59 fixed for pi, and it never receives the block that made one
complete answer buy the round back (board item 9).

## Specs

**S1** — The MCP server exposes the door as `memory_context`: the task in
one line, and the answer is **the door's answer** — the same envelope the
CLI prints with `--json`: the priced items (identity + preview), the
session-state block, the coverage declaration and the declarations of the
namespace it searched, the tokens it spent and how far the read reached.

**S2** — **One implementation, two surfaces.** The MCP tool calls the same
`MemoryService::context` the CLI calls; the answer is not a second
projection of the same facts. The door's answer has one shape everywhere the
agent meets it.

**S3** — The namespace is the **workspace's own** — the one a save lands in
— resolved by the engine (`workspace_status`, the same rule `memory_workspace`
answers with), never another project's. A caller may name a namespace the
scope may read.

**S4** — The tool's description says what the answer carries: the priced
items, the session state (the live `project_state`/`decision` records with
the content that fits the block's budget, the index of what did not fit, and
the previous session's summary), and the declaration of what the answer did
not include. A description that promises what the tool does not deliver is a
promise the agent reads and cannot keep.

**S5** — The documentation follows: the skill, the extension's README and
the root README name the door in the MCP surface, and the count of the
server's tools (sixteen → seventeen) is corrected everywhere it is written.

**S6** — Nothing else moves. Every other MCP tool answers **byte-identically**
before and after; the CLI's surfaces are untouched.

## Tasks

| ID | specs | route | commit |
| --- | --- | --- | --- |
| T1 | S1, S2, S3 | inline | `feat(mcp): the door answers in the MCP surface` |
| T2 | S4, S5 | inline | `docs(mcp): the door is in the MCP surface, and the tool count is seventeen` |
| T3 | S6 | inline | `test(mcp): every other tool answers byte-identically` |

## Notes for the implementer

- The service call is `MemoryService::context(&ContextRequest)` — the same
  one the CLI uses (`src/cli/commands.rs::context`). The `ContextRequest`
  carries `task`, `namespace`, `match_mode`, `token_budget`,
  `tokenizer_id`, `max_items`, `budget_mode`, `full` and the `SearchFilter`.
- The MCP server resolves the workspace's namespace with
  `crate::application::workspace_status::workspace_status(&svc,
  &self.data_dir, …)` — the same call `do_workspace` makes. The door's
  answer must declare the namespace it searched (the CLI's rule, #57): an
  answer that names no namespace cannot be told apart from one that looked
  in another project.
- The MCP payload is JSON (the other tools answer with
  `serde_json::to_string_pretty`). The door's envelope is already a typed
  contract (`ContextResponse`), so the tool serializes it as it is.
- The tool's arguments: `task` (required), `namespace` (optional),
  `token_budget` (optional, default 1024), `max_items` (optional, default
  20), `match` (optional: any | all | phrase).
- The MCP has its own test harness (`tests/mcp_harness.rs`,
  `tests/mcp_interop.rs`): a new tool needs a test that calls it and one
  that proves the other tools' answers did not move.

## Log

- **L1** — The board's item 11: "The door is not in the MCP surface — the
  next thing for opencode". The order the board imposes: the door first
  (M3 + M4, PR #92, merged), then **M5** (the lean skill, with its own
  experiment), then this.
- **L2** — The measurement that puts this in the plan: the re-read metric
  (`tmp/bench/reread-report.md`) measured the door and the enumeration as
  the same machine — the agent rebuilds the answer by hand because the
  surface it has does not carry it.
- **L3** — **T1 done** (S1, S2, S3), implemented by a delegated worker and
  reviewed by the parent. `memory_context` calls the same
  `MemoryService::context` the CLI calls and serialises the typed envelope
  as it is (the only addition is `read_scope`, inserted exactly the way
  `src/cli/commands.rs::context` inserts it), so the two surfaces cannot
  drift into two projections of the same facts. The namespace is the
  workspace's own, resolved with the same `workspace_status` call
  `memory_workspace` makes.
  RED: the three new tests failed with `tool not found` and the tool list
  showed exactly sixteen tools. GREEN: 34 mcp_interop, 1 mcp_harness,
  clippy and fmt clean.
  Two review points the worker raised and the parent accepted: the
  namespace is resolved **fresh per call** (the server also caches it at
  startup; the fresh resolution matches the CLI's semantics and costs one
  query plus a workspace probe per door call — once per session), and the
  four CLI door options the tool does not expose (`memory_type`,
  `budget_mode`, `full`, `tokenizer_id`) are fixed to the CLI's defaults so
  the parity test compares like with like.
- **L4** — **T3 done** (S6): the differential (`tmp/diff-mcp-door.sh`, base
  `origin/main`, the MCP stdio transport, the same seeded profile) — the
  **tool list moved** (`memory_context` appears, and it is the only
  difference) and the six read tools' answers are **byte-identical**:
  `memory_workspace`, `memory_list`, `memory_search`,
  `memory_search_prompts`, `memory_suggest_topic_key`, `memory_recall`.
- **L5** — **T2 done** (S4, S5), written by a delegated worker and checked
  by the parent: the tool's description names the priced items with their
  handle, the three parts of the session state and what the coverage
  declares; the skill, the extension's README and the root README name the
  door in the MCP surface. The count of the server's tools was written as
  **fourteen** (and fourteen names) — it is **seventeen**, and the stale
  Rust doc comments ("9 memory_* tools", "exposes 15 tools") are corrected.
- **L6** — **A promise the answer was making, now made true.** The
  declaration said "the least relevant first", and the truth is the
  opposite: the omitted candidates are listed in rank order, so the ones
  that **just missed the cut** come first (verified on the binary: the most
  relevant omitted fact is named first, then the rest in rank order). The
  wording in the CLI's text answer, in the extension's answer and in the
  documents is now "the closest to the cut first" — the promise the agent
  reads is now the promise the engine keeps. The worker that wrote the
  documentation is the one who caught it.
