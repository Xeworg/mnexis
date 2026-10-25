# session-recall — what happened in the session before this one

Board item 5: *"the agent can save a prompt and search prompts, but cannot
ask 'what happened in the session before this one'"*. The provenance with
git rides along: the session is where the working context belongs, and the
branch is part of that context.

Branch: `feat/session-recall`.

## Why now

The reads are closed by default (#42): an agent no longer sees other
projects' memories, so its own session is the only bridge to what it knew
before. That makes the recall of a session load-bearing, not a convenience.

## Specs

- **S1** — `mnexis recall` answers what a session was, in one answer:
  - `mnexis recall --session <id>` — that session.
  - `mnexis recall --previous` — the most recent **finished** session of the
    namespace the workspace writes into (the session before this one). No
    previous session is NOT_FOUND (exit 3) with a message that says so, not
    an empty answer.
  - The answer carries: the session itself (id, namespace, directory,
    started_at, ended_at, summary), **its prompts in order**, and **the
    memories of that namespace created inside the session's window**
    (from `started_at` to `ended_at`, or to now when it is still open).
  - The window is a window, not a link: the engine does not know which
    session wrote which memory, so the answer says **"the memories of the
    namespace in the session's window"** and never claims the session wrote
    them. A memory of another session that overlapped the window is in the
    list, and that is the honest answer.
  - The MCP tool `memory_recall` answers the same thing, for the agent.
- **S2** — The session carries its working context: migration 008 adds
  `sessions.branch`, `sessions.branch_sha` and `sessions.branch_remote`.
  When a session starts in a git repository, the engine records the branch,
  the HEAD commit and the **canonical** remote key (`git:<normalized>`) —
  the same identity the workspace uses, never the raw URL (a URL can carry
  credentials). No git, no repository, or a failing git is not an error:
  the three columns stay empty and the session is still a session.
- **S3** — The state of that branch is **validated when it is read**, never
  stored: `merged` (the recorded commit is an ancestor of the base),
  `not_merged` (it is not), or `unknown` (it could not be checked). The
  answer says **which base** it compared against and **when** it checked.
  `unknown` is a first-class answer and never turns into `not_merged`: the
  engine does not lie about what it could not verify. There is no
  "pending" answer: whether the work is still pending is the agent's
  judgment about evidence, not the engine's.
- **S4** — Everything is read-only except the session that records its own
  context: no default changes, no new exit code, and the existing session
  commands (`session`, `end`, `sessions`, `prompt`, `prompts`) keep their
  shape.

## Tasks

| ID | Spec | Route | Allowed edit surfaces | Commit |
| --- | --- | --- | --- | --- |
| T1 | S1 | the recall: a service method, `mnexis recall`, the MCP tool | `d0a7527` |
| T2 | S2 | the provenance: migration 008, the capture at session start | `src/storage/sqlite.rs`, `src/application/workspace.rs`, `src/application/service.rs`, `src/domain/session.rs`, `src/storage/memory_repo.rs`, `src/diagnostics/doctor.rs`, `src/cli/mod.rs`, `src/cli/stats_cmd.rs`, `src/mcp/tools.rs`, `tests/session_recall.rs`, `tests/sqlite_fts5.rs`, `README.md` | `3468667` |
| T3 | S3 + S4 | the validator, and the recall showing it | `src/application/git_state.rs`, `src/application/workspace.rs`, `src/application/session_recall.rs`, `src/cli/recall_cmd.rs`, `src/mcp/server.rs`, `tests/session_recall.rs` | |

## Log

- **L1** (2026-10-26) — the operator: *"2"* (the thread from the beginning:
  the recall of a session and the provenance with git). The branch is the
  working context of a session, and the session is the right place to keep
  it: a branch is volatile, and a session describes the past, so the
  provenance does not rot.
- **L2** (2026-10-26) — decisions taken while designing, with the code as
  the evidence:
  - **The recall is a new command, not a new shape for `session`.** `mnexis
    session --id X` exists and works; turning `session` into a subcommand
    would break it. The new command is `mnexis recall` (`--session <id>` /
    `--previous`).
  - **The window is a window.** `memories` has no `session_id` and the
    engine does not know the active session of a namespace (the MCP server
    is a process, the CLI is another), so linking a memory to a session
    would need a second migration and a rule about which session is active.
    The window answers the same question with the data that exists, and it
    says what it is.
  - **The canonical remote, not the URL.** `canonical_workspace_key` already
    normalizes the remote into `git:<normalized>`; a raw URL can carry
    credentials (`https://user:token@host/...`) and it is not needed to
    identify the repository.
  - **The state is derived, never stored.** "Merged" is a fact about the
    repository *now*, not about the session; storing it would rot the same
    way a stored branch name rots. The recorded anchor (branch, commit,
    remote) is what does not rot: the commit is still that commit.
  - **`unknown` is a real answer.** The repository may not be on this
    machine, the base may be undecidable, git may be missing. The engine
    reports what it could not check and never turns it into a negative.
- **L3** (2026-10-26) — what is NOT in this feature: the provenance **per
  memory** (an anchor the agent asks for when it saves). The session is the
  context; the memory is the fact. A per-memory anchor is a separate
  feature, and it belongs to the same measurement the rest of this cycle
  learned to ask for: does the agent use it when it matters?

- **L4** (2026-10-26) — **T1 is done** (`d0a7527`, on `feat/session-recall`,
  pushed): the recall answers, the CLI and the MCP tool work, the window is
  labelled as a window, and the read obeys the scope. Checks: 22 cargo test
  binaries green, fmt clean, clippy at the 36-warning baseline, plus a
  differential against the base commit over the session commands (byte
  identical) and a manual smoke of both forms.

  **T2 and T3 are NOT done**, and the operator asked to stop here. Their
  decisions are already written above (L2), so the next session starts
  without losing the design:

  - **T2** — migration 008 adds `sessions.branch`, `sessions.branch_sha`,
    `sessions.branch_remote`; the engine captures them when a session
    starts in a git repository, using the canonical remote key
    (`git:<normalized>`) and never the raw URL. No git is not an error: the
    columns stay empty.
  - **T3** — the validator re-checks the recorded anchor when it is read and
    answers `merged` / `not_merged` / `unknown`, with the base it compared
    against and the instant of the check. `unknown` is a first-class answer
    and never becomes `not_merged`. There is no "pending": whether the work
    is pending is the agent's judgment about evidence.

  **Not done either, and deliberately:** the provenance **per memory** (an
  anchor the agent asks for when it saves) — the session is the context, the
  memory is the fact, and that one needs the measurement this cycle learned
  to ask for: does the agent use it when it matters?

- **L5** (2026-10-26) — **T2 is done** (`3468667`, on `feat/session-recall`):
  the session records its working context. The decisions the implementation
  took, with the code as the evidence:
  - **The capture is in the service, not in the CLI.** The CLI and the MCP
    server are two doors into the same service, so a capture in the CLI
    would leave the agent's own sessions (the MCP door) without an anchor.
  - **The directory the session names is the source; the process's own
    directory is the fallback.** `mnexis session --id X --directory D`
    anchors to `D`, and a session that names none anchors to the directory
    the process runs in. The recorded `directory` field itself is
    unchanged: it stays what the caller said, empty included.
  - **The anchor is the repository's, not the directory's.** A session that
    happens in a subdirectory is anchored to the repository it belongs to
    (`git rev-parse --show-toplevel`), because that is the thing a branch
    lives in.
  - **A detached HEAD has no branch.** `git symbolic-ref --short HEAD`
    fails when the HEAD is detached, and the engine records no branch
    instead of recording the word `HEAD` as if it were one. The commit is
    still recorded: it is the part of the anchor that does not rot.
  - **The three columns are probed one by one.** `ALTER TABLE ... ADD
    COLUMN` adds one column at a time and cannot be conditional in SQL, so
    migration 008 probes each column and adds only the missing one
    (migration 007's pattern). A half-applied migration heals on the next
    run instead of failing.
  - **The schema version moves to 8**, and the doctor, the stats and the
    migration tests follow it: a store at 8 is not "newer than this
    binary".

  The **shape of the session record changed**: `mnexis session` and
  `mnexis sessions` now carry `branch`, `branch_sha` and `branch_remote`
  (null when there is nothing to record). The anchor has to be readable
  somewhere, and the session record is where it lives. The human rendering
  of both commands did not change.

  Checks: 23 cargo test binaries green (7 new tests: 5 in
  `tests/session_recall.rs` and 2 on the **upgrade path** of the migration —
  a profile that already holds sessions, and a migration that failed
  halfway), fmt clean, clippy at the same 36-warning baseline, and a
  **differential against the base commit** (`main`) over the session
  commands: the only differences are the three new fields, the schema
  version and the help text of `--directory` (which said "informational" and
  now says that the working context is recorded from it).

  **T3 is NOT done.** The validator still has to answer `merged` /
  `not_merged` / `unknown` when the anchor is read, and the recall has to
  show it. The decisions for it are already written (L2).

  **Delivery note:** the branch is now three commits and about 1,600
  authored lines ahead of `main` (T1 + T2). The slice boundary is T1 |
  T2 | T3, one commit per task, and the delivery that fits the repo's
  precedent is **one PR per task slice, stacked** (the way `doctor-contract`
  went out as #31 + #34). The PRs are the operator's call at the close of
  the feature.
