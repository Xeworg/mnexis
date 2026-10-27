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
  or without one — is a project. A directory that is not in a repository is a
  project too, and the human's question made that explicit (L7): the
  candidate is offered for it, while the CLI's refusal stays where the silent
  fall-through was measured, in a repository, and the profile's default
  namespace keeps deciding outside one.
- S2 (the candidate name, verbatim answer 2026-10-27): "Último segmento:
  nodexar". The candidate namespace is the last segment of the normalized
  remote, lowercased (`https://github.com/Xeworg/Nodexar.git` → `nodexar`).
  With no remote — a repository before its first push, or a directory that is
  not a repository at all — it is the name of the directory that names the
  workspace: the repository root, or the working directory itself. The
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
| N1 | S1, S2, S4, S5 | the engine + the CLI: repo-ness and the candidate namespace in the resolver and in `status`, the namespace-less save resolved to the binding or refused with the route named, never `default`; tests first | inline (the writer refused the unit; see L4) | `531941c` |
| N2 | S2, S3, S4, S6 | the extension: derive, one confirmation per workspace naming namespace/key/path/grant, bind + allow on yes, nothing on no, the marker names but never binds; tests first | worker | `eba6d3e` |
| N3 | S1–S6 | the docs and the shipped skill: `docs/install.md`, `docs/projects.md`, `extensions/mnexis-memory/README.md`, `docs/PRD-Mnexis.md`, `skills/mnexis-memory/SKILL.md` | inline | (this commit) |
| N4 | S1–S6 | verification against a real profile in an isolated `MNEXIS_DATA` directory | inline | (no commit: evidence in L6) |

**Delivery slices** (the review workload, recorded before delivery): the
branch carries two review slices of about 600 authored lines each — `N1`
(the Rust engine, the CLI and their tests) and `N2` (the extension and its
tests) — plus the docs slice `N3`. Chained as two pull requests, `N1` first;
the user decides when they open.

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
- L4 (2026-10-27, routing). The delegate lane refused N1 twice: the writer
  runs only for a task of an existing feature document named in the
  `Spec: odd/tasks/<feature>.md, T<n>` form, and then reported that the
  unit was inline work. N1 was written inline (tests first: RED observed by
  stashing `src/` and running the new tests against the base commit — the
  refusal test saw exit 0 and a record pooled in `default`, the bound-save
  test failed, `status` carried no candidate — then GREEN). The context
  backstop fired during N2, which is when the writer accepted the unit.
- L5 (2026-10-27, the measured cost of S5, and the decision). The strict
  refusal broke 74 tests in `tests/cli_commands.rs`: they save without
  `--namespace` from the test binary's working directory, which is inside
  the mnexis repository, against a profile with no binding. The human's
  verbatim answer: "Estricto: rechazar y actualizar 74 tests". The suite now
  drives the binary from a directory outside every repository unless a test
  chooses one (`no_repository()`), so those tests say what they mean, and
  `tests/read_granularity_advisories.rs` passes `--namespace default` on the
  incidental save it uses as setup. One suite remains as the regression
  guard for the new rule: `tests/workspace_status.rs`.
- L6 (2026-10-27, verification). `cargo test`: every suite green (284 lib
  tests, 131 in `cli_commands`, 12 in `workspace_status`, all the rest);
  `node --test extensions/mnexis-memory/test/*.test.js`: 123/123. End to
  end, in an isolated `MNEXIS_DATA_DIR` and a scratch repository with the
  remote `https://github.com/Xeworg/Nodexar.git`: `status --json` answered
  `key git:github.com-xeworg-nodexar`, `namespace null`,
  `candidate_namespace nodexar`; a namespace-less save was refused with the
  exact `project bind nodexar --key … --path …` and
  `config allow-namespace nodexar` commands; after those two commands the
  save landed in `nodexar` with `default` left at 0 records; and in a
  directory outside every repository the same save still landed in
  `default`, unchanged. The extension's one-confirmation path is covered by
  its own suite against a fake engine (a real TUI confirmation is the one
  step this record does not observe live).
- L7 (2026-10-27, the human's question, verbatim): "solo tengo una duda y si no
  hay repo que pasara cuando haga un proyecto nuevo y no haya iniciado el repo
  eso se ha contemplado". Measured answer, in a directory with no `git` at
  all: `status` reported `candidate_namespace: null` and a namespace-less save
  still landed in `default` — the same silent pooling S5 removed from
  repositories, one case over. `git init` without a remote was already
  covered (the candidate is the directory's name). The decision, verbatim:
  "Candidato siempre: el nombre de la carpeta" — the candidate is derived from
  the directory that names the workspace whether or not it is a repository, so
  a project before `git init` gets the same one question; the CLI's refusal
  stays inside repositories, where the silent fall-through was measured, and
  the profile's default namespace keeps deciding outside one.
- L8 (2026-10-27, the delivery, recorded because the record has to match the
  repository). PR #79 (engine) merged into `main`, and PR #80 merged next —
  into its own base, `feat/auto-namespace-engine`, not into `main`, so the
  extension slice and the docs were left on a branch while the engine reached
  `main`. The repair is `feat/auto-namespace-delivery` off `main` carrying the
  two extension commits and the folder-candidate commit (this one), with the
  conflict in this document resolved by keeping both logs. The lesson is
  mechanical: in a stack, the child PR has to be retargeted to the default
  branch before it is merged, because merging it into its parent delivers it
  to the parent and not to the repository.
