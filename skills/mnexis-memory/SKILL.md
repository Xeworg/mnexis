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
mnexis context "extend the snake game with a wrap mode"   # context for this task + the session state
mnexis search "wrap mode borde"                            # ranked search, FTS5
mnexis list                                                # everything this project recorded
mnexis status                                             # where may I write, and what is there
```

`mnexis context "<the task in one line>"` returns the memories that matter
for that task, already trimmed to a token budget: use it as the first call of
a session. It searches the namespace this workspace writes into — the one a
save lands in — and the answer names the namespace it searched, so an empty
answer means this project has nothing, not that the door looked elsewhere. If
it returns nothing, the project has no memory yet — carry on
and record what the session produces (see "The loop").

**Its answer already carries the session state.** The `session_state` block
holds this project's `project_state` and `decision` records **with their
whole content**, newest first, plus the summary of the most recent finished
session. Read it before reaching for `list`, `search` or `recall` for the same
question: it is the measured answer to "where did we leave off" — one call
instead of the three or four rounds an agent spends rebuilding the same state
by hand. The block is outside the token budget and belongs to the namespace
the door searched; when the project has recorded no state it answers with an
empty list and a null session, not an error.

`mnexis status` answers the other half in one call: the namespace this
workspace writes into, the read and write allowlists, how many memories the
scope can see, and the most recent ones — each as identity and preview, so
the call stays cheap; open a record with `mnexis show <uuid>` when it is
the one you need. Run it before the first save, and
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
`mnexis_list`, `mnexis_show`, `mnexis_history`, `mnexis_workspace`,
`mnexis_context`, `mnexis_recall`, `mnexis_supersede` — and resolves the
namespace and the permission gate by itself. `mnexis_workspace` is the
first call of every session (the namespace, the allowlists, the recent
memories), and `mnexis_recall --previous` answers what the session before
this one was in one call — identity and preview by default, the whole
records with `full`. `mnexis_context` is the **one-call door**: give it
the task in one line and it answers with the memories this project
recorded that matter for it, already trimmed to a token budget, naming
the namespace it searched — and with it, the **session state** (the
`project_state`/`decision` records with their whole content and the
previous session's summary). It is the cheapest way to ask the project a
question ("where did we leave off", "which port does it listen on"):
read the state it returns before walking the namespace by hand with
`list` and `show`.
When those tools are available
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

**A refused read is a question for the human.** When a read of another
namespace comes back denied — `NOT_FOUND namespace:<name>` by default,
`FORBIDDEN` when the profile asks for it — the answer names
`mnexis config allow-read-namespace <name>`. That command is the human's to
run: ask them, say which namespace and what the read is for, and wait for
the answer. Never widen your own reach to get past a refusal — not with
`mnexis config allow-read-namespace`, not with `mnexis config
allow-read-all`, and never by editing `mnexis.toml`: a permission the agent
grants itself is not a permission. If no interactive answer is possible,
report that the access is pending and answer from what the scope already
holds. And never go around the refusal by opening another profile: the
scope binds the profile you were given, not the files on the disk, so a
second `mnexis.toml` is not a permission either — it is the same reach with
a different name.

## The session and what the user said

Two things a repository cannot hold, and the engine keeps both:

- **The session.** `mnexis session --id <id>` registers the session you are
  running in, with the id you were given (never invented); `mnexis end
  --id <id> --summary "what happened"` ends it. The summary is what the next
  session reads first, so it must make sense with no surrounding chat. The
  extension does both for you when it runs inside pi.
- **What the user said.** `mnexis prompt "<what the user said>"` stores a
  constraint, a preference or a goal the user stated, verbatim;
  `mnexis prompts --search "<query>"` finds it again. Use it when the user
  says something that has to outlive the conversation: the chat is not in
  the files, and this is where it goes. A prompt is not a memory — it has
  no revision and no topic key — but it obeys the same namespace rules.

## The loop

1. **Recall first.** Before answering from scratch, look for prior context:
   `mnexis search "<keywords>"` (FTS5 ranked; `--namespace`, `--limit`),
   `mnexis list`, and `mnexis show <uuid>` to read one record. The bare
   forms reach every project, because projects share components; add
   `--namespace <name>` to stay inside this one. The recall of the session
   start is not optional: a project with no memory reads like a project
   that never decided anything.
   Write the query the way you would say it: every word counts, a hyphen
   is part of a word (`staging-eu` is one search, not FTS5 syntax), and
   `--match all` narrows it to memories that hold every word, and
   `--type decision,preference` narrows it to the kinds of thing you
   ask about.
2. **Save what matters.** After a milestone — a decision, a discovery, a
   user preference — store it immediately, in this project's namespace:
   `mnexis save --namespace <name> --title "<what the fact IS>" --keywords "<the words it is called by>" --type <kind> "<one self-contained sentence>" --tags a,b`.
   The content must make sense months later with no surrounding chat.
   `--type` says what kind of thing the fact is, and the kinds are a
   closed set: `knowledge` (a fact about the world), `decision` (a choice
   that was made), `preference` (a stated taste or constraint),
   `procedure` (how something is done here), `project_state` (where the
   project is right now), `episode` (what happened, once) or `fact` (the
   generic one). Leave it out and the record keeps the type it already
   has — a save that says nothing about the type never demotes a typed
   fact.
   `--idempotency-key` makes retries safe.
3. **One fact, one record — the title is what does it.** The **title** is
   what the fact *is*; the content is what it says *right now*. The topic
   key derives from the title, so the same title always lands on the same
   record: saving the same fact again with the same title **updates** that
   record (its revision goes up and the previous content stays in the
   history) instead of adding a second one next to it.
   Write the title as the topic, never with the value inside it:
   `puerto del servicio`, not `puerto 9090` — a title that carries the
   value guarantees the duplicate next time the value changes.
   `mnexis suggest-topic-key "<title>"` answers which key a title yields
   and whether the project already records it.
   `--topic-key` still exists for the cases where you want to place the
   memory yourself; it accepts lowercase letters, digits, `.`, `-` and `/`.
4. **The words of the fact are its other names.** `--keywords "puerto,
   port, listen port"` holds the words the same fact goes by in another
   language or another team's jargon: that is how it is found when it is
   asked for that way, and the index covers them. The words are the topic's
   **vocabulary, never a value** — `puerto`, not `8081`, for the same reason
   the title has: a word that carries a value makes the memory match a value
   that is no longer true. A save **adds** its words to the ones the record
   already has, so a session that does not remember them cannot erase them;
   when a word is wrong, replace the whole list with `--replace-keywords`.
   The answer to a save shows the accumulated words, so read it and prune.
5. **A replaced fact says so.** When a search or a list answer carries
   `was replaced by`, that memory is the old one and the other is current.
   Answer with the current value, and do not read the replaced one.
6. **Replace facts by superseding, never by deleting.** Outdated but
   historically true facts keep their record: `mnexis save` the newer fact,
   then `mnexis supersede --older <old-uuid> --newer <new-uuid> --reason "why"`.
   The supersede is atomic, revision-checked on both sides, cycle-free, and
   persists the reason. `mnexis history <uuid>` shows the chain. Both
   memories must live in this project's namespace.
7. **Correct a record in place** only for typos/formatting:
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
