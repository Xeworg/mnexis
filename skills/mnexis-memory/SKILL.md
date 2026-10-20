---
name: mnexis-memory
description: Persist, recall, and supersede long-term memories with the mnexis CLI, scoped to the project the user is working on. Use when a task needs memory across sessions — recalling prior work, saving decisions, discoveries, or user preferences, or replacing an outdated stored fact without losing history.
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

## Scope to the current project (do this once per session)

Memories are namespaced, and the namespace for the current project is
recorded in the profile (`projects.toml`). Resolve it before saving:

1. **Compute the workspace key.**
   - Inside a git repository: key = `git:<normalized remote origin url>`
     (take the repo root with `git rev-parse --show-toplevel`, the remote
     with `git config --get remote.origin.url`; drop the protocol and the
     `.git` suffix, lowercase the host, and replace `/`, `:` and `.` with
     `-` — e.g. `git:github.com-xeworg-mnexis`). With no remote, use
     `path:<absolute repo root>`.
   - Without git: walk up from the working directory looking for a
     `.mnexis-namespace` marker; if one is found, the directory holding it
     is the project root and the key is `path:<absolute root>`. With no
     marker, the key is `path:<absolute working directory>`.
2. **Look it up:** `mnexis project list` prints `key -> namespace`. If the
   key is listed, use that namespace — never rename it.
3. **First visit (no binding):** choose a short, lowercase, hyphenated
   name from the workspace (usually the repository or directory name) and
   record it: `mnexis project bind <name> --key <key> --path <root>`.
   A key that is already bound to another namespace is refused; that
   refusal means the memories live under the old name — keep it, or ask
   the human before using `--force`.
4. **Authorize it:** a namespace only accepts writes when it is in the
   profile allowlist. `mnexis save --namespace <name>` fails with
   `NOT_FOUND namespace:<name>` until it is allowed. **Ask the human
   first**, then run `mnexis config allow-namespace <name>` — permissions
   belong to the user, not to the agent.

Then pass `--namespace <name>` on every write for this project, and use
`mnexis search --namespace <name>` for project-scoped recall. A bare
`mnexis search "<query>"` searches every namespace, which is what to use
when the answer may live in another project.

## The loop

1. **Recall first.** Before answering from scratch, look for prior context:
   `mnexis search "<keywords>"` (FTS5 ranked; `--namespace`, `--limit`),
   `mnexis list`, and `mnexis show <uuid>` to read one record.
2. **Save what matters.** After a milestone — a decision, a discovery, a
   user preference — store it immediately:
   `mnexis save --namespace <name> "<one self-contained sentence>" --tags a,b --topic-key <stable-key>`.
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
  (stdio; pass `--data-dir` in the server command line). The namespace
  resolution above still applies — the MCP tools take the namespace as an
  argument.
