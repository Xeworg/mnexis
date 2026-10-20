# project-bindings — workspace → namespace binding

Binds a working "project" (the user's current focus of work) to a mnexis
namespace so an agent's memories land in the right bucket and stay stable
across sessions. Two CLI surfaces (profile authorization + binding
registry) plus the agent protocol in the `mnexis-memory` skill. Inline
route; Rust TDD.

## Specs

- S1 (user, verbatim): "creo que sera mejor que el agente decida el nombre de
  acuerdo al espacio de trabajo, tambien que si este detecta un repositorio
  git vincule el repositorio cuando exista me parece la opcion mas optima" —
  the agent names the namespace from the workspace; when the workspace is a
  git repository, the binding is tied to that repository.
- S2 (user decision): binding store is the profile — a `projects.toml` next
  to the profile toml.
- S3 (user decision): authorizing a new namespace is a new mnexis command,
  not hand-edited TOML.
- S4 (user decision): without git, the workspace identity is found by walking
  up to a marker.
- S5 (user, verbatim): "veo necesario hacer una skill para algunos entornos
  de ia" — the `mnexis-memory` skill teaches the agent the resolution
  protocol (the skill already exists from the agent-skills feature; this
  feature extends it).

## Commands

- `mnexis config allow-namespace <ns>` — add `<ns>` to the profile's
  `read_namespaces` and `write_namespaces` (idempotent; never shrinks an
  allowlist; prints what changed).
- `mnexis project bind <ns> --key <key> [--path <path>]` — record/update a
  binding in `~/.local/share/mnexis/projects.toml`.
- `mnexis project list` — list the recorded bindings.
- `mnexis project unbind <ns>` — remove a binding (memories are untouched).

## Resolution protocol (agent side, skill `mnexis-memory`)

1. Identity: inside a git repo → `git:<normalized remote origin url>`, else
   `path:<repo root>`; without git → walk up from the cwd to a marker
   (`.mnexis-namespace`) and use that root, else `path:<cwd>`.
2. Lookup: `mnexis project list` → if the key is bound, use its namespace.
3. Miss: the agent picks a readable name from the workspace and binds it
   (`mnexis project bind`).
4. Authorization: the namespace must be in the profile allowlist. The
   agent asks the human first and runs `mnexis config allow-namespace <ns>`
   only after an explicit ok — the human owns permissions (PRD §7).

## Risk

- Risk: item 2 (security/permissions) — `mnexis config allow-namespace` grants a
  namespace write permission in the profile allowlist. The command only grows
  the allowlists, never shrinks them; the agent protocol requires an explicit
  human ok before running it; the scope contract v1 is unchanged.

## Tasks

| ID | Spec | Route | Commit |
|----|------|-------|--------|
| T1 | S1-S5 | inline: feature doc + mirror | (this doc) |
| T2 | S3 | inline: RED tests for `config allow-namespace` | 80d2c09 |
| T3 | S3 | inline: implement `config allow-namespace` | 80d2c09 |
| T4 | S2,S4 | inline: RED tests for `project bind/list/unbind` | f276979 |
| T5 | S2,S4 | inline: implement `project` subcommand + registry | f276979 |
| T6 | S1,S5 | inline: extend the `mnexis-memory` skill protocol | 0f36d78 |
| T7 | S1-S5 | inline: docs (`docs/projects.md`, README) | 0f36d78 |
| T8 | S1-S5 | inline: close, evidence, commits | (closing commit) |

## Log

- L1 (user, verbatim): "veo algo importante y es como se considera un
  proyecto, debe trabajarse la carpeta como proyecto para el usuario, en
  este caso creo que serian los namespaces es correcto?"
- L2 (user, verbatim): "es que un proyecto puede tratarse desde un git y no
  un directorio ahi estaria el problema principal por que digamos que un
  proyecto puede desarollarse sin git, aun que no es lo ideal, puede pasar
  que el usuario no sabe o solo le interesa la memoria, entonces creo que lo
  mas optimo es que el agente nombre el namespace de acuerdo al proyecto que
  el usuario esta trabajando analiza esto con un agente, tal vez se le ocurre
  algo mejor no le des mi idea que desarrolle la propia expon el problema"
- L3 (decision): explore subagent delivered three designs (agent
  convention / cwd marker / global registry); user chose the agent-decided
  name with git-repository binding, and settled the three open questions:
  profile `projects.toml`, a new `mnexis config allow-namespace` command,
  and marker walk-up for non-git workspaces.
- L4 (evidence): mnexis has no `namespaces` table — namespaces are free
  strings, so "creating" one is a scope-allowlist change
  (`NOT_FOUND namespace:<ns>` when absent). `search` without `--namespace`
  does not check scope and already crosses all namespaces (global recall
  works today); the gate is write-side only. `allow_global` enables the
  literal namespace "global", not a wildcard. Namespace validation only
  rejects the empty string.
- L5 (delivery): stacked on `feat/mnexis-agent-skills` (the skill file this
  feature extends lives there); the agent-skills branch is not merged yet,
  so delivery order is agent-skills first, then this branch.
- L6 (evidence): test-first held. RED observed before implementing:
  `config_allow_namespace` 5/5 failing (subcommand absent),
  `project_bindings` 9/9 failing. GREEN after the implementation: 5/5 and
  9/9, plus 8 new unit tests in `src/config/projects.rs`; full suite
  273/273 green; `cargo fmt --check` clean; clippy warning count
  unchanged (36 before, 36 after — zero new warnings from this feature).
- L7 (verification): the independent verifier (required by the escalated
  item 2) refused to launch twice — it did not detect the risk
  declaration in the change — and was NOT retried a third time. It is
  reported as unavailable; the same checklist was executed inline
  instead, with these observations: (1) `allow-namespace` only grows the
  allowlists (`read_namespaces`/`write_namespaces`), leaves
  `default_namespace`, `allow_global`, `existence_disclosure` and the
  limits untouched, is idempotent, refuses an empty namespace and one
  with a line break (exit 2), requires an existing profile (exit 3,
  creating no file), and writes atomically; a namespace outside the
  allowlist still fails with exit 3 while the authorized one writes
  (exit 0). (2) `git diff main -- src/domain/ docs/contracts/` is empty:
  the v1 scope contract and `can_read`/`can_write` are untouched, and
  `allow_global` still enables only the literal namespace "global".
  (3) Rebinding a key to another namespace is refused without `--force`;
  `unbind` leaves the database byte-identical (md5) and the memory
  still searchable; a malformed registry fails with exit 2 and is not
  reset; bindings round-trip across separate processes. (4) The new
  tests assert exit codes, file contents and real writes — no test
  pins an implementation detail.
- L8 (skill): the `mnexis-memory` skill now carries the resolution
  protocol (identity -> `project list` -> bind -> authorize with an
  explicit human ok -> `--namespace` on writes). The whole flow was
  smoke-tested end to end against an isolated profile in a temp
  directory inside the repo's `tmp/` (bind, allow, save, list, scoped
  search, global search); the real user profile was never touched.
- L9 (delivery slice): this branch is stacked on
  `feat/mnexis-agent-skills`, so the authored-line counter from
  `origin/main` counts that feature's commits too (its ~500 lines).
  This feature's own work units are the three commits above, each
  reviewable on its own: `80d2c09` (permission command + tests),
  `f276979` (binding registry + CLI + tests), `0f36d78` (skill
  protocol + docs). Delivery order: merge `feat/mnexis-agent-skills`
  first, then this branch (it only adds on top of the skill file).
