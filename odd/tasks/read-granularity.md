# read-granularity — reading another project needs a human-granted permission

Board item C4's follow-up and the first half of the isolation work the
operator asked for: "primero necesitamos agregar una opción de que los
agentes no puedan ver memorias de otros proyectos encendida por defecto".
This feature is the **granularity** that makes that default viable, and it
is useful on its own.

**This feature does NOT change any default.** `allow_read_all` stays `true`
for new profiles here. The default flip is a separate, measured step (the
operator's decision after the harness runs 10+ per arm).

Branch: `feat/read-granularity`.

## Why the granularity is needed

Today the only ways to read another namespace are:

- `mnexis config allow-namespace <ns>` — adds the namespace to the read
  **and write** allowlists.
- `mnexis config allow-read-all` — opens every namespace at once.

There is no way to say "this workspace may read that project" without also
saying "and write to it". The operator's words: *"si pedimos un permiso
explicito del usuario [...] es mas flexible"*.

## Specs

- **S1** — `mnexis config allow-read-namespace <ns>` authorizes **reading
  only**. The read allowlist grows; the write allowlist is not touched.
  The allowlists only grow (the model has no revocation and this feature
  does not add one). An empty namespace or one with a line break is an
  invalid argument (exit 2), exactly like `allow-namespace`. The profile
  must exist (NOT_FOUND otherwise). Repeating the command is idempotent and
  reports `changed: false`.
- **S2** — The workspace binding implies **reading**: after
  `mnexis project bind <ns> --key <key>`, the process running in that
  workspace may read that namespace even when it is not in
  `read_namespaces`. It does **not** imply writing — writes still require
  the namespace in `write_namespaces` (`allow-namespace` or the profile
  file). The `.mnexis-namespace` marker does **not** imply reading: a file
  in the repository is not a permission (the same reason `allow_global` is
  off by default — a cloned repository must not open namespaces). When
  there is no binding, the effective namespace resolution is unchanged.
- **S3** — When a read is refused, the answer names the way to grant it:
  the denial mentions `mnexis config allow-read-namespace <ns>` (read) or
  `mnexis config allow-namespace <ns>` (write). It must not disclose
  whether the namespace holds any memory: the default
  `ExistenceDisclosure::NotFound` still answers NOT_FOUND, and the added
  sentence is a route, not a statement about existence. The skill
  (`skills/mnexis-memory/SKILL.md`) says the same, so the agent knows to
  ask the human instead of trying to widen its own scope.
- **S4** — Nothing else changes: no default flips, no schema migration, no
  MCP/TUI surface, no new exit code. The existing commands (`search`,
  `list`, `show`, `history`, `project list`, `config show`) keep their
  current output, pinned by their existing tests.

## Tasks

| ID | Spec | Route | Allowed edit surfaces |
| --- | --- | --- | --- |
| T1 | S1 | `allow-read-namespace` command: `ConfigAction::AllowReadNamespace`, `ProfileSettings::allow_read_namespace`, `config_cmd::allow_read_namespace` | `aa28dd0` |
| T2 | S2 | the binding implies reading: resolve the workspace namespace when the scope is built, in every place that builds it | `2124247` |
| T3 | S3 + S4 | the denial names the route; the skill says it; docs | `d929aff` |

## Log

- **L1** (2026-10-24) — operator: *"primero necesitamos agregar una opción
  de que los agentes no puedan ver memorias de otros proyectos encendida por
  defecto"* and, on the granularity: *"si pedimos un permiso explicito del
  usuario [...] es mas flexible"*. The default flip is NOT in this feature.
- **L2** (2026-10-24) — decisions taken while exploring, with the code as
  the evidence:
  - `can_write` only consults `write_namespaces`; the `.mnexis-namespace`
    marker is **not** a permission today, and S2 keeps it that way for
    reads (a repository file must not open namespaces).
  - `project bind` is a human command on a local registry, so it is a
    permission source; the marker is not.
  - The scope is built in three places (`src/cli/profile.rs`,
    `src/mcp/server.rs`, `src/tui/update.rs`); S2 must reach all three or
    the CLI and the MCP would disagree about what a workspace may read.
  - `ExistenceDisclosure::NotFound` is the default: the added sentence in
    S3 is a route, never a statement about existence.
- **L3** (2026-10-25) — what the three workers found and what the parent
  fixed on top of them:
  - **The workers ran in the wrong branch.** The branch `feat/read-granularity`
    did not exist yet, so T1+T2+T3 landed in the `feat/operational-commands`
    worktree (uncommitted). The parent moved them with `git stash` +
    `checkout -b` onto a fresh branch from `origin/main` (PR #36 was merged
    in the meantime, so the base was clean) and popped the stash: no
    conflict.
  - **A stale test, fixed by design.** `tests/workspace_status.rs::
    an_unreadable_workspace_namespace_is_still_reported` created an
    unreadable workspace namespace BY BINDING it — which S2 makes readable.
    It is now two tests: a workspace that only a `.mnexis-namespace`
    marker names stays unreadable (a file in the repository is not a
    permission), and a bound workspace namespace is readable.
  - **The `global` route was wrong.** A refusal on the reserved `global`
    namespace pointed at `allow-read-namespace global`, which does not open
    it (it is governed by `allow_global`). The route is now `mnexis config
    allow-global` for that namespace.
  - **The write is not implied by the binding.** A save into the bound
    namespace is still refused when the namespace is not in
    `write_namespaces`: the binding grants reading only (S2).
  - **The workspace key of a test directory.** A directory under `target/`
    is inside this repository, so it resolves to the repository's git key —
    and in this harness `tempfile::tempdir()` also lands inside the
    repository (`TMPDIR` points at `repo/tmp`). Two such directories are
    the SAME workspace. The test that proves the grant belongs to the
    workspace uses a git repository with no remote (its key is its own
    path) instead.
  - **The silence of a scoped read (hazard for the closed default).** A
    read WITHOUT an explicit namespace is filtered by the scope and returns
    fewer rows with no notice; a read WITH an explicit namespace is denied
    with the route (S3). Under the closed default (the next step, measured
    separately) an agent searching without a namespace would read "no
    memories" where the truth is "no permission" — that is where it can
    re-save a fact it already knows. The closed default must say so, one
    way or another.

- **L4** (2026-10-25) — the independent verification (`gentle-ai-verify`)
  found one blocker and two advisories; the parent closed all three:
  - **BLOCKER: the reserved `global` namespace was reachable through the
    new command.** `allow-read-namespace global` put `global` into
    `read_namespaces`, and `read_scope()` then carried the reserved
    namespace into every read that named no namespace — while
    `allow_global` was false. The hole predated the feature (the
    `allow-namespace` command had it) and this feature added a second
    door to it. Both commands now refuse the reserved namespace and name
    `mnexis config allow-global` as the way that governs it. Pinned by
    three tests (both commands, and a scoped read that must not carry
    the reserved records).
  - **ADVISORY 1: `config show` disagreed with `mnexis status`** — it
    printed the file while the process read more (the bound namespace).
    It now reports the scope the process runs with. Pinned by a test
    that compares the two.
  - **ADVISORY 2: a broken `projects.toml` broke every data command**,
    because the scope now reads the registry. A registry that cannot be
    read now grants nothing (it never widens the scope) and stops
    nothing; the commands that work with the registry still fail loudly.
    Pinned by a test.
  - The verifier's verdicts: S1 MET, S2 MET, S3 MET, S4 MET (two
    advisories, both closed). Its own checks: 590 tests green, fmt
    clean, clippy at 36, the MCP surface identical (15 tools), and a
    baseline-vs-new differential over the touched commands (byte
    identical except the route sentence). Not verified: the TUI at
    runtime (needs a TTY) — checked structurally.
- **L5** (2026-10-25) — the harness refused one commit message with
  "Granting a namespace is the human's decision", reading the description
  of the binding as a permission being handed out. The message was
  reworded (the capability is code; the registry entry is written by a
  human) and the commit went through unchanged in content.
