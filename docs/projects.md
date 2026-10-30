# Projects: workspace → namespace bindings

A **project** is the user's current focus of work. In mnexis the unit
that holds memories apart is the **namespace**, so a project is a
namespace plus a rule for knowing which namespace belongs to which
workspace.

Two files in the profile data directory carry that state:

| File | Holds |
| --- | --- |
| `mnexis.toml` | the profile settings: `read_namespaces`, `write_namespaces`, `default_namespace`, limits |
| `projects.toml` | the binding registry: workspace key → namespace |

Both are plain text and safe to read. `projects.toml` is written by
`mnexis project`; `mnexis.toml` by `mnexis init` and
`mnexis config allow-namespace`.

## Why a binding, and not just a namespace name

An agent could simply invent a namespace on every call. That breaks the
moment two sessions disagree: the same project's memories land in two
namespaces and no search finds both. The binding makes the choice
**explicit and stable**: the workspace identity is the key, and the
namespace is the value recorded under it.

## Workspace identity (the key)

The key is computed by whoever is writing — normally the agent, from the
working directory. mnexis never guesses it.

| Workspace | Key |
| --- | --- |
| Git repository with a remote | `git:<normalized remote url>` — e.g. `git:github.com-xeworg-mnexis` |
| Git repository without a remote | `path:<absolute repository root>` |
| Plain directory | `path:<absolute working directory>` |

Normalization of a remote: drop the protocol (`https://`, `ssh://`),
drop a trailing `.git`, lowercase the whole URL, and replace `/` and `:`
with `-` — the host's dots survive. A git remote is preferred over a path
because it survives clones, renames and moves between machines.

**The key has one form.** The engine owns the rule, and it applies it when
the key is recorded, when it is looked up and when it is shown: a binding
written with the raw remote (`git:github.com/owner/repo`) is stored as
`git:github.com-owner-repo`, and a binding recorded by an older version
still resolves. The bind command accepts either form, and
`mnexis project list --json` publishes the canonical key of the workspace
it runs in (`current_key`), so an agent never has to compute the key
itself.

The marker (`.mnexis-namespace`, a one-line file) exists for workspaces
that are not repositories: it names the namespace of the workspace, so a
working directory nested inside it still resolves to the same project.
The marker does not change the key, and it never overrides a binding: the
binding for the key wins, and the marker is the fallback. The CLI and the
pi extension resolve it the same way.

## Commands

### Bind a workspace

```sh
mnexis project bind <namespace> --key <key> [--path <dir>]
```

Records the mapping. Binding the same key to the same namespace again
is a no-op. Binding a key that is already bound to **another** namespace
is refused (exit 2) — that would split the project's memories — unless
`--force` is given.

### List the bindings

```sh
mnexis project list
```

Prints `key -> namespace (path)` for every binding.

### Unbind a namespace

```sh
mnexis project unbind <namespace>
```

Removes the binding. The memories are untouched: unbinding changes
where a workspace *points*, not what is stored.

### Authorize a namespace

```sh
mnexis config allow-namespace <namespace>
```

A namespace only accepts writes when it is in the profile's
`write_namespaces`; otherwise every write fails with
`NOT_FOUND namespace:<name>`. This command adds the namespace to both
`read_namespaces` and `write_namespaces` in `mnexis.toml`. The allowlists
only ever grow — nothing is revoked — and the profile must already exist
(`mnexis init`).

Two permissions sit beside the allowlists and are moved with their own
commands:

```sh
mnexis config allow-read-all [--revoke]   # reads across namespaces
mnexis config allow-global [--revoke]     # the reserved 'global' namespace
```

The scope contract (v1) is unchanged: the scope object keeps the same
fields and the same semantics. This command only populates the
allowlists an operator already owns.

## The agent protocol

The `mnexis-memory` skill implements this for AI agents:

1. Compute the key from the working directory (rules above).
2. `mnexis project list` — if the key is bound, use that namespace.
3. Otherwise ask the engine what the workspace would be called:
   `mnexis status --json` publishes `workspace.candidate_namespace` (the
   last segment of the git remote, lowercased, or the name of the
   directory that names the workspace — the repository root, or the working
   directory itself, because a new project before `git init` still has a
   name) and, when it refuses to offer one, the reason in
   `workspace.candidate_refusal`. The pi extension puts that candidate to
   the human as **one question per workspace**, naming the namespace, the
   key, the path and the two permissions a yes grants, and runs
   `mnexis project bind <candidate> --key <key> --path <path>` followed by
   `mnexis config allow-namespace <candidate>` only after an explicit yes.
   A skill working without the extension asks the human and runs the same
   two commands.
4. If writes to that namespace fail with `NOT_FOUND namespace:<name>`,
   **ask the human** and then run `mnexis config allow-namespace`. The
   human owns the permissions; the agent does not grant itself access.
5. Pass `--namespace <name>` on every write for that project.

Recall spans projects: `mnexis search "<query>"` without `--namespace`
searches every namespace, which is what to use when the answer may live
in a different project. The profile decides how far a namespace-less
read reaches: `allow_read_all` (on by default in a new profile) opens it
to every project, and `mnexis config allow-read-all --revoke` scopes it
to `read_namespaces`.

Writes do not span projects: save into this project's namespace and never
update, supersede or delete a memory that lives in another one. The CLI
and the MCP server (`mnexis mcp serve`, the same binary) enforce it for
writes run from a bound workspace — the command fails with `FORBIDDEN`
(exit 4) and names the namespace it accepts — and the pi extension
refuses the same writes before they reach the CLI.

A workspace resolves to a namespace in this order: the binding recorded
for its key, then the nearest `.mnexis-namespace` marker at or above the
working directory, then nothing.

A write without `--namespace` follows the same answer: it lands in the
workspace's own namespace, and inside a git repository that names none it is
refused with the candidate and the two commands that bind and allow it. It
never falls into the profile's default namespace — that fall-through is how
a repository's memories end up pooled in the shared drawer while the
workspace still reports as unbound. Outside a repository the profile's
allowlist keeps deciding, as it always did.

**A namespace is still never invented.** The engine derives a _candidate_ —
the name of the remote's last segment, or the repository directory's name —
and offers it; nothing binds until a human says yes. The `.mnexis-namespace`
marker follows the same rule: it is a file inside the workspace, so it can
name the candidate (and its name wins over the derived one), but it grants
nothing on its own. Reading keeps working for an unbound workspace: the
recall reaches every namespace the profile allows.

## Example

```sh
$ mnexis project bind mnexis --key git:github.com-Xeworg-mnexis \
    --path /home/xeworg/Proyectos/mnexis
Bound git:github.com-Xeworg-mnexis to namespace mnexis in /home/xeworg/.local/share/mnexis/projects.toml

$ mnexis config allow-namespace mnexis
Allowed namespace mnexis for read and write in /home/xeworg/.local/share/mnexis/mnexis.toml

$ mnexis save --json --namespace mnexis "PR #1 merged in main 5635d44"
{"keywords":[],"memory_type":"fact","revision":1,"row_id":1,"title":"","topic_key":"","uuid":"191cc344-9560-46cb-b05b-fc1bbe48b8cb"}

$ mnexis project list
git:github.com-Xeworg-mnexis -> mnexis (/home/xeworg/Proyectos/mnexis)
```

The answer names the record the save landed on, not the request it was given.
`revision` is `1` for a create and higher for an upsert: the same title lands
on the same record, so a second save of the same fact answers with revision
`2` and the uuid it updated instead of a second row. `memory_type` is the type
the engine resolved for the record — which may differ from the one the call
asked for, because a save that names no type keeps the type the record already
has instead of demoting it to `fact`.
