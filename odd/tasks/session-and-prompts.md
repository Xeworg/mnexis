# session-and-prompts

Give the engine the two things a repository cannot hold: the session the
agent is running in, and what the user asked for. They are also where a
compaction lands, instead of dying with the context.

This is the structural gap the comparison with the other engine found: its
memory keeps sessions and prompts, ours keeps neither, and the control arm
showed that the repository is the channel an agent falls back to when the
engine has nothing to offer.

## Specs

- **S1** — "A session is something the engine knows about". The agent
  registers the session it runs in (`mnexis session start`) and ends it
  (`mnexis session end [--summary]`). The engine keeps the session: the
  caller's id (the agent never invents one), the namespace it belongs to,
  when it started, when it ended and the summary. A session that was never
  ended is still a session: the engine never guesses that a session is
  over.
- **S2** — "What the user asked survives". A user prompt is a stored,
  searchable entity: `mnexis prompt "<what the user said>"` stores it with
  its session and namespace, and `mnexis prompts --search "<query>"`
  finds it. The conversation is the one thing the repository does not
  hold: it is the context a future session needs and cannot get.
- **S3** — "The agent can see both". The CLI answers `mnexis sessions` and
  `mnexis prompts`; the MCP server answers `memory_session_start`,
  `memory_session_end`, `memory_save_prompt` and
  `memory_search_prompts`; the pi extension exposes the same four as
  `mnexis_session_start`, `mnexis_session_end`, `mnexis_save_prompt` and
  `mnexis_search_prompts`.
- **S4** — "The compaction has somewhere to land". When pi compacts a
  session, the extension saves the compacted summary into that session, so
  the next turn recovers it from the memory instead of from a context that
  no longer exists. The extension also opens and closes the session it
  runs in, so the engine knows the boundaries without the model having to
  remember them.
- **S5** — "The namespace rules do not bend". Sessions and prompts live in
  a namespace and obey the same allowlists as the memories: a write
  outside the allowlist is refused with the same error and exit code, and
  a read is bounded by the scope.

**Not in scope:** the conflicts layer, the cloud, the embeddings, the
pin/review lifecycle, and any change to the shape of a memory.

## Tasks

- **T1** — Migration 004: the `sessions` and `user_prompts` tables, the
  `user_prompts_fts` index and its triggers, the doctor's expected schema
  version, and the migration tests (a fresh profile, an upgraded profile
  and the no-downgrade rule). Route: inline. Commit: `feat(storage): the
  sessions and the prompts (migration 004)`.
- **T2** — The domain entities, the repository port, the storage adapter
  and the service: start a session, end it with a summary, store a prompt,
  search the prompts, list the sessions. Route: inline. Commit: `feat(
  sessions): the session and the prompt lifecycle`.
- **T3** — The caller-facing surface: the CLI commands and the MCP tools,
  with the help text and the docs. Route: inline. Commit: `feat(cli):
  sessions and prompts where the agent can reach them`.
- **T4** — The pi extension: the four tools, and the hooks that open,
  close and compact a session. Route: inline. Commit: `feat(extension):
  the session the agent runs in, and where a compaction lands`.
- **T5** — Verification: the suites, the migration against a profile that
  already holds memories, and the PR. Route: inline. Commit: `docs(odd):
  close session-and-prompts`.

## Log

- **L1** (2026-10-23) — The comparison with the other engine ended with a
  capability matrix where the structural gaps were ours: sessions, stored
  prompts, a conflict layer, compaction recovery. The control arm of the
  same run showed the consequence of not having them: the agent falls back
  to the repository, because the repository is the only thing it can count
  on.
- **L2** (2026-10-23) — The measured case for the protocol block (the
  other engine's channel) came back empty: an injected block buys nothing
  because the tool descriptions and the skill already say what to do. That
  is the reason this feature does **not** ship a protocol injection: the
  gap is not in the prompt, it is in what the engine can keep.
- **L3** (2026-10-23) — Branch `feat/session-and-prompts`, based on `main`
  at the merge of PR #22. It does not depend on PR #23 (the canonical
  binding) or PR #24 (the compact list): the tables, the entities and the
  tools are new. It is the first change that touches the schema, so the
  migration is additive only — new tables, new index, new triggers, and no
  change to any existing table.
- **L4** (2026-10-23) — The delivery, in five work units:

  | commit | unit | slice |
  | --- | --- | --- |
  | `4ea9c6e` | migration 004: the tables, the index, the triggers, the doctor | ~240 |
  | `5e668e8` | the entities, the repository port, the adapter and the service | ~460 |
  | `d637760` | the store helpers next to the store | ~30 |
  | `aaa08dd` | the CLI commands and the MCP tools | ~300 |
  | `5f72a7a` | the pi extension: the four tools and the three hooks | ~250 |

- **L5** (2026-10-23) — The evidence:
  - `cargo test`: 21 suites green; `cargo fmt --check` clean; clippy at the
    36-warning baseline.
  - The extension's own tests: 46 pass, 0 fail.
  - The installer suite: all installer tests passed (the extension's assets
    are embedded in the binary, and the installer installs them).
  - The migration: a profile at version 3 with a memory in it comes out of
    the migration with the memory and the new tables (a test), and the
    prompt index follows an insert and a delete (a test).
  - The live check, in the comparison's container, with the release binary:
    `mnexis session` → `mnexis prompt` → `mnexis prompts --search` (a
    hyphenated query included) → `mnexis end` → `mnexis sessions`, and
    `mnexis doctor` reporting every check passed.

**Risk: item 1** — this is the first change that writes to the schema: a
migration adds two tables, an index, a trigger pair and an FTS index. It is
additive (no existing table is touched, no memory is rewritten), it is
idempotent, and a profile that already holds memories keeps every one of
them — proven by a test. The no-downgrade rule still holds: a binary that
does not know version 4 refuses to open the profile (`UNSUPPORTED_VERSION`).

**Risk: item 3** — the MCP surface grows from ten tools to fourteen, and the
pi extension from five to nine. The new tools are additive; the existing
ones keep their shape. The `memory_prompts` read accepts a namespace
filter, and without one it stays inside the scope's read scope, exactly
like the memory search.
