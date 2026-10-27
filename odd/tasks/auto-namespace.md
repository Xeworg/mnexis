# auto-namespace — a git workspace provisions its own namespace, once

The human's complaint, from another project: the workspace was a git repo with
a remote, `mnexis status` said `(unbound: the profile allowlist decides)`, and
the saves went into `default` — a shared drawer — so that project never had a
namespace of its own. The ask is that a git workspace get its namespace
automatically. The review (`jd-judge-a`, 2026-10-27) found the proposal touches
three committed contracts, and that the real cause of the complaint is a line
of code, not the absence of provisioning.

## Specs

- S1 (the ask, verbatim 2026-10-27): "mnexis no esta funcionando debemos hacer
  que el namespace se pueda crear automatico cuando exista git que se vincule
  si existe". Scope: a process running inside a git repository — with a remote
  or without one — is a project; a directory that is not in a repository is
  not, and keeps today's behaviour.
- S2 (the candidate name, verbatim answer 2026-10-27): "Último segmento:
  nodexar". The candidate namespace is the last segment of the normalized
  remote, lowercased (`https://github.com/Xeworg/Nodexar.git` → `nodexar`).
  A repository with no remote takes the basename of its root directory. The
  candidate is rejected — with the reason named, never silently — when it is
  the reserved `global`, when it is empty, or when the engine's namespace
  validator refuses it. Collisions are not resolved by the engine: the human
  sees the name they are about to grant and says no.
- S3 (how automatic, verbatim answer 2026-10-27): "Una sola confirmación por
  repo". The agent's extension derives the candidate, asks the human **once
  per workspace**, and the question names exactly four things: the namespace,
  the workspace key, the path, and what the grant allows (read **and** write).
  On confirmation it runs the two existing commands (`project bind` and
  `config allow-namespace`) and continues in that namespace. On refusal it
  writes **nothing** — no binding, no allowlist entry — and the write is
  refused naming the route, the same contract `read-granularity` uses.
- S4 (what does NOT move). `mnexis status` stays read-only: it must not
  provision, and a profile is byte-identical after it runs in an unbound
  repository (`tests/workspace_status.rs`). The engine's own `allow_namespace`
  remains the only way into the write allowlist, and it stays reachable only
  from a human confirmation (the extension's gate keeps refusing a
  model-issued grant, `extensions/mnexis-memory/lib/gate.js`). `global` never
  enters either allowlist through this path. A binding keeps widening the read
  scope exactly as it does today (`allow_bound_read_namespace`) — this feature
  gives the human the question, it does not invent a second permission model.
- S5 (the defect the review found, JD-A-001). A namespace-less save in an
  unbound workspace selects `profile.default_namespace()` **before** the
  workspace guard runs (`src/cli/commands.rs:191`), and a fresh profile allows
  writes into `default` (`src/config/settings.rs:47-49`), so the record lands
  in the shared drawer while status says "unbound". Binding alone does not fix
  it: after the binding the same `default` target is refused as cross-project
  by `WorkspaceGuard` (`src/cli/commands.rs:66-80`). The fix: for a workspace
  inside a repository, the namespace-less save resolves to the **bound**
  namespace and, when there is none, is **refused** with the route named. It
  never falls through to `default`. Outside a repository the allowlist keeps
  deciding, as it does today.
- S6 (the leak the review found, JD-A-007). The extension turns a
  repository-controlled `.mnexis-namespace` marker into a binding
  (`extensions/mnexis-memory/index.js:301-313`), and a binding grants reads —
  so the marker *is* a permission end-to-end, although
  `odd/tasks/read-granularity.md:41-44` and `tests/project_bindings.rs:605`
  say it grants nothing. The marker may name the candidate the human is asked
  about; it may not bind anything by itself.

## Tasks

| ID | Spec | Route | Commit |
|---|---|---|---|
| N1 | S1, S2, S4, S5 | the engine + the CLI: repo-ness and the candidate namespace in the resolver and in `status`, the namespace-less save resolved to the binding or refused with the route named, never `default`; tests first | worker | |
| N2 | S2, S3, S4, S6 | the extension: derive, one confirmation per workspace naming namespace/key/path/grant, bind + allow on yes, nothing on no, the marker names but never binds; tests first | worker | |
| N3 | S1–S6 | the docs and the shipped skill: `docs/install.md`, `docs/projects.md`, `extensions/mnexis-memory/README.md`, the PRD line that contradicts them | inline | |
| N4 | S1–S6 | verification against the real profile in an isolated `MNEXIS_DATA` directory, plus the review | inline + jd-judge | |

## Log

- L1 (2026-10-27, user, verbatim): "tenemos un problema que debemos atacar
  primero necesitamos que el namespace se genere automatico necesito que
  asignes un agente a revision tengo este bug en el otro proyecto" — followed
  by the transcript of the other project's session, and "y mnexis no esta
  funcionando debemos hacer que el namespace se pueda crear automatico cuando
  exista git que se vincule si existe". The transcript records the human's
  confirmation of a plan ("agent runs, with confirmation here"), the harness
  refusing `mnexis project bind` from an agent shell ("Granting a namespace is
  the human's decision"), and the consequent state: no binding, writes into
  `default`, so the project's memories never appeared under a namespace of its
  own.
- L2 (2026-10-27, the review — `jd-judge-a`, read-only, ten findings). Its
  verdict, condensed with the evidence the specs above carry: auto-binding
  alone does not redirect a save (JD-A-001); a binding is also a read grant,
  so automatic binding would widen the closed read scope (JD-A-002); the write
  allowlist is the human's grant and auto-adding to it removes it (JD-A-003);
  identity is only the locally configured remote, so "recognized repository"
  is not "authorized repository" (JD-A-004); free-string namespaces with
  unbind-leaves-memories make a short derived name able to alias an old
  project's records (JD-A-005); provisioning inside `current_workspace` would
  make `status` mutate the profile (JD-A-006); the marker already grants reads
  through the extension (JD-A-007); `global` must be refused by any automatic
  path (JD-A-008); and the shipped docs already contradict each other on
  whether the extension binds on first use (JD-A-009). The ranked option the
  review put first — one explicit confirmation naming namespace, key, path and
  grant — is S3.
- L3 (2026-10-27, the decisions, verbatim answers): "Una sola confirmación por
  repo" (S3) and "Último segmento: nodexar" (S2).
