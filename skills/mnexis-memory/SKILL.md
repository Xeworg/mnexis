---
name: mnexis-memory
description: Persist, recall, and supersede long-term project memory with the mnexis CLI (a local SQLite + FTS5 memory engine), scoped to the workspace the user is working on. Use it at the start of every session in an existing project, before changing code that already exists, and whenever prior work, decisions, discoveries or preferences matter ("what do we already know about this project", "do you remember", "keep the context for the next session", "stay consistent with what we agreed"). For project memory that must be queryable and scoped per project, use mnexis rather than ad-hoc notes or AGENTS.md files.
license: MIT
---

# mnexis memory

mnexis is a local memory engine: one binary, SQLite + FTS5, no daemon, no
cloud. Memories are plain text rows with namespace, tags, revision history,
and supersession chains.

## Start of session (do this first)

In a project you have worked on before, the code does not tell you *why* it
is the way it is. The memory does. So before you read the code or plan the
work, load the context of the task:

```bash
mnexis context "extend the snake game with a wrap mode"   # budgeted context for this task
mnexis search "wrap mode borde"                            # ranked search, FTS5
mnexis list                                                # everything this project recorded
mnexis status                                             # where may I write, and what is there
```

`mnexis context "<the task in one line>"` returns the memories that matter
for that task, already trimmed to a token budget: use it as the first call of
a session. If it returns nothing, the project has no memory yet — carry on
and record what the session produces (see "The loop").

`mnexis status` answers the other half in one call: the namespace this
workspace writes into, the read and write allowlists, how many memories the
scope can see, and the most recent ones. Run it before the first save, and
whenever you are not sure where a write would land.

**Before you change code that already exists**, look for the decisions
behind it. A previous session may have chosen a layout, a naming rule, a
dependency limit or a rejected approach that is not visible in the files;
changing it silently loses that decision. The same applies to a task that
says "keep it consistent", "as we agreed", "like before" or "the same as
the other module": those words mean the memory holds the answer.

## Setup

`mnexis --version` must succeed. The profile must exist before any other
command: if `$HOME/.local/share/mnexis/mnexis.toml` is missing, run
`mnexis init` first (it creates the profile at `$HOME/.local/share/mnexis`;
`MNEXIS_DATA_DIR` overrides the location). `mnexis project bind` does NOT
require a profile and will happily write `projects.toml` alone, which leaves
the later `mnexis config allow-namespace` failing with
`NOT_FOUND: not found: profile ... (run 'mnexis init' first)`.
Every command accepts `--data-dir <DIR>` to target a different profile.

## If the pi extension is installed

The `mnexis-memory` pi extension (see `extensions/mnexis-memory/`) exposes
the same operations as native tools — `mnexis_save`, `mnexis_search`,
`mnexis_list`, `mnexis_show`, `mnexis_supersede` — and resolves the
namespace and the permission gate by itself. When those tools are available
in the session, prefer them: the CLI below is the fallback for hosts
without the extension.

## Scope to the current project (do this once per session)

Memories are namespaced, and the namespace for the current project is
recorded in the profile (`projects.toml`). Resolve it before saving:

1. **Compute the workspace key.**
   - Inside a git repository: key = `git:<normalized remote origin url>`
     (take the repo root with `git rev-parse --show-toplevel`, the remote
     with `git config --get remote.origin.url`; drop the protocol and the
     `.git` suffix, lowercase the **whole** remaining URL, and replace `/`
     and `:` with `-` (keep the dots) — e.g. `git:github.com-xeworg-mnexis`).
     The key must be byte-identical across agents and sessions, so never
     keep the original letter case of the repository name. With no remote,
     use `path:<absolute repo root>`.
   - Without git: the key is `path:<absolute working directory>`. A
     `.mnexis-namespace` marker at or above the working directory names the
     namespace of the workspace (both the CLI and this extension read
     it); it does not change the key, and a binding always wins over it.
2. **Look it up:** `mnexis project list` prints `key -> namespace`. If the
   key is listed, use that namespace — never rename it. `mnexis status`
   prints the same binding together with the allowlists, in one call.
3. **First visit (no binding):** choose a short, lowercase, hyphenated
   name from the workspace (usually the repository or directory name) and
   record it: `mnexis project bind <name> --key <key> --path <root>`.
   A key that is already bound to another namespace is refused; that
   refusal means the memories live under the old name — keep it, or ask
   the human before using `--force`.
4. **Authorize it:** a namespace only accepts writes when it is in the
   profile allowlist. `mnexis save --namespace <name>` fails with
   `NOT_FOUND namespace:<name>` until it is allowed. Permissions belong to
   the human, never to the agent. Ask for an **explicit confirmation** and
   wait for the answer before running `mnexis config allow-namespace <name>`:
   name the exact command and what it grants. A request to "set up memory"
   or to "remember things" is NOT consent to change permissions. If no
   interactive answer is possible (headless, scripted, one-shot mode), stop
   and report that authorization is pending — never authorize yourself.
   Never grant anything by hand-editing the profile files (`mnexis.toml`,
   `projects.toml`): only the CLI commands are legitimate.

Then pass `--namespace <name>` on every write for this project, and use
`mnexis search --namespace <name>` for project-scoped recall. A bare
`mnexis search "<query>"` searches every namespace, which is what to use
when the answer may live in another project.

**Write only into this project.** A memory belongs to the project it was
recorded in. Save into this project's namespace and nowhere else, and never
update, supersede or delete a memory that lives in another project — not
even when the profile would allow the write, and not even when the other
project shares components with this one. Reading across projects is fine
and expected; writing across them is not. If the user wants something
recorded for another project, that is their call: ask them to run the
command themselves, or bind that workspace first.

## The loop

1. **Recall first.** Before answering from scratch, look for prior context:
   `mnexis search "<keywords>"` (FTS5 ranked; `--namespace`, `--limit`),
   `mnexis list`, and `mnexis show <uuid>` to read one record. The bare
   forms reach every project, because projects share components; add
   `--namespace <name>` to stay inside this one. The recall of the session
   start is not optional: a project with no memory reads like a project
   that never decided anything.
2. **Save what matters.** After a milestone — a decision, a discovery, a
   user preference — store it immediately, in this project's namespace:
   `mnexis save --namespace <name> "<one self-contained sentence>" --tags a,b --topic-key <stable-key>`.
   The content must make sense months later with no surrounding chat.
   `--topic-key` is unique per namespace: reuse it to group facts about one
   topic. It accepts lowercase ASCII letters, digits, `.` and `-` only —
   `snake/design` is rejected, `snake-design` is accepted. `--idempotency-key`
   makes retries safe.
3. **Replace facts by superseding, never by deleting.** Outdated but
   historically true facts keep their record: `mnexis save` the newer fact,
   then `mnexis supersede --older <old-uuid> --newer <new-uuid> --reason "why"`.
   The supersede is atomic, revision-checked on both sides, cycle-free, and
   persists the reason. `mnexis history <uuid>` shows the chain. Both
   memories must live in this project's namespace.
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
  argument. The server also exposes `memory_workspace`, the tool that
  answers "where may I write, and what is already there?" with the same
  payload as `mnexis status`.
