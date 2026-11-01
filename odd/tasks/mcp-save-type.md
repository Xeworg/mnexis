# The MCP save answer names the type that landed

Objective: the MCP `memory_save` answer names the type of the record that landed, so the
axis is visible on the host whose transcript is the host's (opencode renders no chrome of
ours). Branch `feat/mcp-save-type` (worktree `tmp/wt-a1`), based on `main` at `f93d473`.
Delivery strategy: ask-on-risk, one work unit. Test runner: `cargo test --test mcp_harness`,
then the full `cargo test`, plus `cargo fmt --check`.

## Specs

- **S1 (the type is named)** — the answer of the MCP `memory_save` tool names the type of the
  record that landed, in a field `memory_type`, read from the **saved record** — never from
  the request, which may have named no type at all.
- **S2 (one envelope with the CLI)** — the field is named `memory_type` because that is what
  `mnexis save --json` emits (since #107, commit `215d384`): *"mnexis save --json now emits
  revision and memory_type, read from the saved Memory — never from the request, which may
  have named no type at all."* The MCP answer and the CLI answer carry the same key for the
  same fact.
- **S3 (additive)** — the answer keeps `uuid`, `row_id` and `keywords` exactly as they are; no
  other MCP answer changes; no other surface changes.
- **S4 (the replay names the original)** — a save that is a replay of an idempotency key
  answers with the **original** record, so the type it names is the original's.
- **S5 (the checks)** — one RED test observed before the change, GREEN after it; the CLI's
  `mnexis save --json` and every other MCP tool answer byte-identical against the base commit
  `f93d473` (stdout, stderr and exit code); `cargo fmt --check` clean; the full suite green.
- **S6 (the documentation follows)** — the surface that documents the MCP answers names the
  field, so a reader of the docs and a caller of the tool see the same envelope.
- **S7 (the revision, too — the second half of the same envelope)** — the save answer names
  the **revision** of the record that landed, read from the saved record: `revision` is `1` for
  a create and higher for an upsert. The MCP `memory_update` answer already carries `revision`
  and `memory_type` (`src/mcp/server.rs:596-601`), so the save answer was the only one of the
  two that could not tell a create from an update — and the update is the path that rewrites a
  record's type.

## Tasks

| ID | Specs | Route | Branch / worktree | Commit |
| --- | --- | --- | --- | --- |
| T1 | S1, S2, S4 | inline (parent): the RED test in `tests/mcp_harness.rs` — the answer names the type that landed, the default when the request named none, the kind an upsert kept, and the original's on a replay | `feat/mcp-save-type` @ `tmp/wt-a1` | `b7beb53` |
| T2 | S1, S2, S3, S4 | inline (parent): the answer in `src/mcp/server.rs` | `feat/mcp-save-type` @ `tmp/wt-a1` | `b7beb53` |
| T3 | S5, S6 | inline (parent): the differential against `f93d473`, the suite, the docs | `feat/mcp-save-type` @ `tmp/wt-a1` | `b7beb53` |
| T4 | S7 | inline (parent): the RED test for the revision, the answer in `src/mcp/server.rs`, the differential, the suite | `feat/mcp-save-type` @ `tmp/wt-a1` | `b4d1beb` |

## Log

- **L1 (2026-11-02, board — the open item A1, verbatim)** — "A1 — the engine must name the type
  in the MCP save answer (`src/mcp/server.rs:256-260`; the CLI already does since #107) — it is
  what makes the type visible in opencode, where the transcript is the host's and there is no
  chrome of ours; falta worktree." (from `odd/tasks/nudge-enriched.md`, L5)
- **L2 (2026-11-02, human, verbatim)** — "por donde quedamos" / "usaremos worktrees y
  trabajaremos varios al mismo tiempo". The board was read, the four open items (A1, A2, B, C)
  were surfaced, and the human chose to run several of them at once, each in its own
  worktree.
- **L3 (2026-11-02, the scope, stated)** — the type only. The MCP answer also lacks
  `revision` (the CLI has it since #107), so an opencode model cannot tell a create from an
  upsert either — the same family of defect the CLI fix closed. That is **not** in this unit:
  it is a second field with its own argument about what the model should be told, and the
  board named only the type. It is recorded here so the decision is visible, not silent.
- **L4 (2026-11-02, T1 — RED, and what it taught)** — the new test failed on the base
  behavior with the answer it actually gives: `{"keywords":[],"row_id":1,"uuid":"…"}` — no
  `memory_type` at all. Two contract facts came out of writing the test, and both are now
  pinned:
  - **the idempotency hash covers the type** (`src/application/service.rs:189-212`), so the
    same idempotency key with a different type is a `TOPIC_CONFLICT`, not a replay. The
    replay path only exists for an identical payload; the test was corrected to replay the
    same payload.
  - **a save that names no type keeps the kind the record already has**, and the title rule
    is what lands the upsert on the same record. The first version of the test used a
    different title and therefore created a second record (`row_id` 3) instead of updating
    the first one — the answer was right, the test was wrong.
- **L5 (2026-11-02, T2–T3 — GREEN and the differential)** — GREEN: `cargo test --test
  mcp_harness` 3 passed, 0 failed. **The differential** (base binary built from `f93d473`
  against the change, the same data-dir path, the uuids and the timestamps normalized):
  - **the CLI surfaces are byte-identical** — 223 lines across 15 command invocations
    (`init`, `save` plain/typed/titled, `list` json/full/text, `search` json/text, `status`
    json/text, `doctor`, `stats`, `context`, and the refused `--type nonsense`): stdout,
    stderr and exit code, **identical**.
  - **the MCP surfaces differ in exactly one place**: the three `memory_save` answers carry
    the added `"memory_type"` key; `memory_list`, `memory_search`, `memory_workspace`,
    `memory_context`, the value-filtered `memory_list` and the invalid-argument error are
    byte-identical.
  - the whole suite: **830 tests passed, 0 failed** across 26 suites; `cargo fmt --check`
    clean.
- **L6 (2026-11-02, the doc, S6)** — `docs/projects.md` now names the MCP answer's shape next
  to the CLI's: the same `memory_type`, read from the saved record, so a host whose
  transcript is its own reads the axis instead of guessing it.
- **L7 (2026-11-02, the human, verbatim)** — "pero agrega una mejora?" and then "dale". The
  question was whether the change is an *improvement* or only parity. The honest answer was:
  parity and measurability — **no bench has measured any behavioural effect**, and the
  rationale inherited from #107 (a model told "Saved" re-saves to tidy the duplicate, and the
  same-title upsert is exactly the path that rewrites a record's type) is an argument, not a
  measurement. The human approved measuring it (the bench is
  `odd/tasks/opencode-type-bench.md`, branch `bench/opencode-type`) and approved closing the
  envelope's other half: **S7**, the revision.
- **L8 (2026-11-02, S7 — why the revision is the same unit and not a new one)** — the MCP
  `memory_update` answer already carries `revision` and `memory_type`
  (`src/mcp/server.rs:596-601`); the save answer was the only one of the two that could not
  tell a create from an update. On a host that reads the answer, that is the difference between
  "I created a record" and "I rewrote a record" — and rewriting a record is how a `decision`
  becomes a `fact`. The same envelope, the same source (the saved `Memory`), one more key.
- **L9 (2026-11-02, T4 — RED, GREEN and the differential)** — RED: the answer gave
  `{"keywords":[],"memory_type":"fact","row_id":1,"uuid":"…"}` with **no** revision. GREEN:
  `cargo test --test mcp_harness` **4 passed, 0 failed**; the whole suite **831 passed, 0
  failed**; `cargo fmt --check` clean. The differential against `f93d473`: the **CLI surfaces
  byte-identical** over 223 lines across 15 invocations, and the **MCP surfaces differ only in
  the three save answers**, which now carry `memory_type` **and** `revision`;
  `memory_list`, `memory_search`, `memory_workspace`, `memory_context`, the value-filtered
  `memory_list` and the invalid-argument error are byte-identical. The three saves' answers
  read, in order: `fact`/revision 1 (a create), `decision`/revision 1 (a create that names the
  type), `decision`/revision 2 (the upsert that names no type and keeps the record's kind).
  The work unit is `b4d1beb`.
