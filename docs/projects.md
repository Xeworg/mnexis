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
| Directory with a `.mnexis-namespace` marker | `path:<absolute directory holding the marker>` |
| Plain directory | `path:<absolute working directory>` |

Normalization of a remote: drop the protocol (`https://`, `ssh://`),
drop a trailing `.git`, lowercase the host, and replace `/`, `:` and `.`
with `-`. A git remote is preferred over a path because it survives
clones, renames and moves between machines.

The marker (`.mnexis-namespace`, a one-line file) exists for workspaces
that are not repositories: it marks the project root so a working
directory nested inside it still resolves to the same project.

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

The scope contract (v1) is unchanged: the scope object keeps the same
fields and the same semantics. This command only populates the
allowlists an operator already owns.

## The agent protocol

The `mnexis-memory` skill implements this for AI agents:

1. Compute the key from the working directory (rules above).
2. `mnexis project list` — if the key is bound, use that namespace.
3. Otherwise pick a short, lowercase name and `mnexis project bind` it.
4. If writes to that namespace fail with `NOT_FOUND namespace:<name>`,
   **ask the human** and then run `mnexis config allow-namespace`. The
   human owns the permissions; the agent does not grant itself access.
5. Pass `--namespace <name>` on every write for that project.

Recall spans projects: `mnexis search "<query>"` without `--namespace`
searches every namespace, which is what to use when the answer may live
in a different project.

## Example

```sh
$ mnexis project bind mnexis --key git:github.com-Xeworg-mnexis \
    --path /home/xeworg/Proyectos/mnexis
Bound git:github.com-Xeworg-mnexis to namespace mnexis in /home/xeworg/.local/share/mnexis/projects.toml

$ mnexis config allow-namespace mnexis
Allowed namespace mnexis for read and write in /home/xeworg/.local/share/mnexis/mnexis.toml

$ mnexis save --namespace mnexis "PR #1 merged in main 5635d44"
{"row_id":1,"uuid":"191cc344-9560-46cb-b05b-fc1bbe48b8cb"}

$ mnexis project list
git:github.com-Xeworg-mnexis -> mnexis (/home/xeworg/Proyectos/mnexis)
```
