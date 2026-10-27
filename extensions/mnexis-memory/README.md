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
| `mnexis_context` | The one-call door: the task in one line, and the memories this project recorded that matter for it, trimmed to a token budget — plus the **session state** (the `project_state`/`decision` records with their whole content and the previous session's summary), so one call answers "where did we leave off". The answer declares the namespace it searched and the tokens it spent. |
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

The kind of a memory has its own icon: 📘 knowledge, ⚖️ decision, ⭐
preference, 🧭 procedure, 📍 project_state, 🎞️ episode, 🧠 fact.

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

Its answer also carries the **session state**, outside the token budget:
the `project_state` and `decision` records of that namespace with their
whole content, newest first, plus the summary of the most recent finished
session. That block is the measured answer to the session's fan-out (the
call-budget experiment: three extra rounds rebuilding the same state by
hand became one call), so read it before asking the other surfaces — the
state is already in the first answer. It carries an empty list and a
null session when the project has recorded nothing, and never another
namespace's records; a namespace the profile may write but not read
yields the empty block.

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
   namespace. The extension does not invent one — the name of a directory is
   not a namespace — so the write is refused and the message names the two
   commands the human has to run: the bind command with the canonical key,
   and the allow-namespace command for the namespace. A marker names one
   (the human wrote it down), and only then is the binding recorded.

## The permission gate

Permissions belong to the human, so the extension never grants itself one:

- **Already authorized** → the write happens silently. The agent does not
  ask anything.
- **Not authorized, interactive session** → pi shows one confirmation naming
  the namespace and exactly what it grants. Confirming runs
  `mnexis config allow-namespace <ns>` once; the grant persists in the
  profile, so later writes never ask again.
- **Not authorized, headless session** (`pi -p`, JSON, RPC without UI) →
  nothing is authorized. The tool fails with the exact command a human must
  run: `mnexis config allow-namespace <ns>`.

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
