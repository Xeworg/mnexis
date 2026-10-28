# mnexis-memory (pi extension)

Exposes the mnexis memory engine as native pi tools, so an agent can store
and recall project memory without being told to, and without deciding which
namespace to write into.

## Install

```sh
./install.sh extensions --env pi   # → <agent-dir>/extensions/mnexis-memory/
```

`<agent-dir>` is `$PI_CODING_AGENT_DIR` when set, otherwise `~/.pi/agent`.
Remove it again with `./install.sh uninstall --extensions --env pi`.

## Tools

| Tool | Purpose |
| --- | --- |
| `mnexis_workspace` | First call of a session: the namespace this workspace writes into, the allowlists, and the most recent memories — each with its title and a preview, never the whole record. |
| `mnexis_context` | The one-call door: the task in one line, and the memories this project recorded that matter for it, trimmed to a token budget — plus the **session state** (the live `project_state`/`decision` records with the content that fits the block's own 2048-token budget, the index of the live records that did not fit, and the previous session's summary), so one call answers "where did we leave off". The answer declares the namespace it searched and the tokens it spent. |
| `mnexis_save` | Store a durable, self-contained memory about the project, with the `type` of fact it is when the agent knows it: one of `knowledge`, `decision`, `preference`, `procedure`, `project_state`, `episode`, `fact` (optional, so the engine decides otherwise). |
| `mnexis_search` | Full-text search over the stored memories (ranked), narrowed by `type` when only some kinds matter. |
| `mnexis_list` | List what is already stored, narrowed by `type` when only some kinds matter. |
| `mnexis_show` | Read one memory by UUID. |
| `mnexis_history` | Every revision of one memory, and why each happened. |
| `mnexis_recall` | What a session was, in one answer: the session, its prompts, and the memories of its window (previews by default, `full` for the records). |
| `mnexis_supersede` | Replace an outdated memory with a newer one, keeping history. |

Every tool is declared to the model in every session: availability does not
depend on the model discovering a skill.

`mnexis_save` also takes `keywords`: the comma-separated words the fact is
called by in another language or another team's jargon (`puerto, port,
listen port`). They are the topic's vocabulary, never a value, and they
accumulate on the record across sessions, so the tool's answer shows the
words the memory carries now — read it and prune a word that is wrong.

## What the human sees

The tools draw themselves: pi renders the call and the result with the
extension's own chrome (`lib/chrome.js`), so the transcript reads like a
conversation instead of a log of JSON. A call names the tool and its
argument, an answer says what it is — the kind of the fact, its state, its
revision, its namespace — and the footer carries the engine's status while
it runs.

The drawing is **only** for the human. The text the model reads is exactly
what the CLI answered; the chrome reads the tool's `details` to draw what
you read, and never touches what the model gets. That is why it lives in
the extension and not in the CLI: the CLI's stdout is the machine channel
of every agent — this extension, opencode, and the codex and claude code
integrations to come — and a drawing there would be paid by every agent, in
tokens, for every memory it reads.

The door's panel draws the session state its answer carries: the records it
holds, with their content, then the index of the live records that did not
fit the block's budget — identity, kind, title and date — and the previous
session's summary. An answer from an engine that predates the block draws
nothing extra: the panel is the one it always was.

The kind of a memory has its own icon: 📘 knowledge, 🎯 decision, ⭐
preference, 📋 procedure, 📍 project_state, 🎉 episode, 🧠 fact. The
engine's own mark is 🤖, so a session with two memory engines stays
readable.

Every one of them is a bright, saturated emoji on purpose: the marks that
were dark (the scales, the compass, the filing cabinet) disappeared on a
dark terminal, and a mark the human cannot see is not a mark.

The icons appear when a memory carries its `type`. A memory the agent wrote
without one is a `fact` (🧠), so a store where most records are generic
shows mostly facts.

## Read across projects, write inside one

Reads are open: `mnexis_search` and `mnexis_list` reach every project
(projects share components) unless the model narrows them with a
`namespace` argument or a `type` argument — `type` being a comma-separated
list of the kinds (`knowledge`, `decision`, `preference`, `procedure`,
`project_state`, `episode`, `fact`), matched exactly — and `mnexis_show`
reads any memory the profile allows.

`mnexis_context` is the exception, and it is deliberate: it asks a
**project-shaped** question ("where did we leave off", "which port does
it listen on"), so it searches the workspace's own namespace — the one a
save lands in — and says which namespace it searched. An empty answer is
then never another project's silence. It takes `namespace` when the
question is about another project, and `token_budget` when the default
budget is not the one wanted.

Its answer also carries the **session state**, in a budget of its own: the
block holds the namespace's **live** `project_state` and `decision` records
(never the superseded, archived or proposed ones), newest first, with their
whole content while they fit the block's 2 048-token budget — and every live
record that did not fit follows as an **index entry**: identity, kind, title
and date, no content. What the budget left out is therefore never silent:
`mnexis_show` (or `mnexis show <uuid>` from a shell) pulls it. The most
recent finished session's summary rides with the block, and the answer says
how many records ride resident and what they cost, so both halves are
visible at a glance.

The block is the measured answer to the session's fan-out (the call-budget
experiment: three extra rounds rebuilding the same state by hand became one
call), so read it before asking the other surfaces — the state is already in
the first answer. Its budget is its own: the door's items keep their
selection, their order and their count whether the block is there or not,
which is the property the call-budget experiment measured. An empty block —
a project that recorded no state, or a namespace the profile may write but
not read — draws nothing at all: that answer is exactly what the door always
answered, never an error and never a `null`.

Writes are not: `mnexis_save` always targets the workspace namespace, and
`mnexis_supersede` refuses a memory that lives in another project. The
mnexis CLI enforces the same boundary for writes run from a bound
workspace, so the rule holds even when the agent uses the shell.

## How the namespace is resolved

The model never picks a namespace, and the engine owns the identity of the
workspace:

1. The engine publishes the canonical key of the workspace the session runs
   in (`mnexis project list --json` → `current_key`): `git:<normalized
   remote>`, with the protocol and the `.git` suffix removed, the whole URL
   lowercased, `/` and `:` replaced by `-` and the host's dots kept —
   `https://github.com/Xeworg/AegisPI.git` becomes
   `git:github.com-xeworg-aegispi`. Without a remote it is `path:<repo
   root>`, and outside a repository `path:<cwd>`.
2. `mnexis project list` is consulted with that key: an existing binding is
   never renamed.
3. A workspace with no binding and no `.mnexis-namespace` marker has **no**
   namespace yet, and the engine offers the one it would take
   (`mnexis status --json` → `workspace.candidate_namespace`: the last
   segment of the git remote, lowercased, or the name of the directory that
   names the workspace (the repository root, or the working directory
   itself); `workspace.candidate_refusal` when it will not offer one). The
   extension never names a workspace itself: it puts that candidate to the
   human as one question, and a declined question writes nothing. A marker
   names the candidate too — the human wrote it down, so it wins over the
   derived name — but a marker grants nothing on its own: only the human's
   confirmation binds it.

## The permission gate

Permissions belong to the human, so the extension never grants itself one:

- **Bound and already authorized** → the write happens silently. The agent
  does not ask anything.
- **Unbound workspace, interactive session** → pi shows one question naming
  the namespace, the workspace key, the path and the two permissions a yes
  grants (the binding widens the read scope, the allowlist adds the write
  one). Confirming runs `mnexis project bind` and then
  `mnexis config allow-namespace` once; both persist in the profile, so
  later sessions write without asking again. Declining binds nothing and
  allows nothing.
- **Not authorized, interactive session** → pi shows one confirmation naming
  the namespace and exactly what it grants. Confirming runs
  `mnexis config allow-namespace <ns>` once; the grant persists in the
  profile, so later writes never ask again.
- **Unbound or not authorized, headless session** (`pi -p`, JSON, RPC
  without UI) → nothing is authorized. The tool fails with the exact
  commands a human must run, filled in with the candidate and the key:
  `mnexis project bind <ns> --key <key> --path <path>` and
  `mnexis config allow-namespace <ns>`.

The extension only drives the mnexis CLI; it never edits `mnexis.toml` or
`projects.toml` by hand.

## Configuration

| Variable | Meaning |
| --- | --- |
| `MNEXIS_BIN` | Path to the `mnexis` binary (default: `mnexis` on `PATH`). |
| `MNEXIS_DATA_DIR` | Profile directory, forwarded to the CLI unchanged. |

## Tests

```sh
node --test "extensions/mnexis-memory/test/*.test.js"
```

The unit tests inject a fake exec, so they need neither the binary nor a
profile.
