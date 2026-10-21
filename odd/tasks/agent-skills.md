# agent-skills — skills home + per-environment install/uninstall

Ships an agent-skills home (`skills/`) with the first skill (`mnexis-memory`),
wired into `install.sh` so users can select the target AI environment (pi
first) when installing skills and uninstall them later. Inline route;
bash-installer test-first.

## Specs

- S1: "crea una carpeta de skills, donde guardaremos las skills que se le
  instalaran a los agentes" — a repository `skills/` directory where each
  subdirectory is one installable skill in Agent Skills format (`SKILL.md`
  + supporting files).
- S2: "veo necesario hacer una skill para algunos entornos de ia empecemos
  con pi" — the first skill (`mnexis-memory`) teaches agents the mnexis
  memory loop (recall/search/save/supersede via CLI, MCP alternative);
  `pi` is the first selectable environment.
- S3: "creemos una opcion que al instalar el usuario pueda seleccionar el
  entorno a instalar" — `./install.sh skills` offers interactive
  environment selection; `./install.sh skills --env <name>` is the
  non-interactive form. Re-running overwrites cleanly. The environment
  registry starts with `pi -> <agent-dir>/skills` (default
  `$HOME/.pi/agent/skills`) and is extensible.
- S4: "y tambie pueda desinstalar" — `./install.sh uninstall --skills`
  removes the mnexis-installed skill directories for the selected
  environment (interactive select or `--env <name>`), never touching
  skill directories mnexis does not own.

## Tasks

| ID | Spec | Route | Commit |
|----|------|-------|--------|
| T1 | S1-S4 | inline: feature doc + mirror | (this doc) |
| T2 | S1,S2 | inline: `skills/mnexis-memory/SKILL.md` | 61acc7d |
| T3 | S3,S4 | inline: RED tests in `tests/installer/` | 8215175 |
| T4 | S3 | inline: `skills` subcommand in `install.sh` | 8215175 |
| T5 | S4 | inline: `uninstall --skills` in `install.sh` | 8215175 |
| T6 | S3,S4 | inline: GREEN + full installer suite | 8215175 |
| T7 | S1-S4 | inline: docs + log + work-unit commits | (closing commit) |

## Log

- L1 (user, verbatim): "no solo era una prueba, de momento solo queria
  probar veo necesario hacer una skill para algunos entornos de ia
  empecemos con pi, crea una carpeta de skills, donde guardaremos las
  skills que se le instalaran a los agentes y creemos una opcion que al
  instalar el usuario pueda seleccionar el entorno a instalar y tambie
  pueda desinstalar"
- L2 (decision): dedicated `skills` subcommand + `uninstall --skills`
  flag (default `install`/`uninstall` behavior unchanged; purge semantics
  untouched — skills are not profile data). Ownership rule for uninstall:
  only directories whose names match skills shipped in the repo `skills/`
  are removed.
- L3 (evidence, corrected by L6): pi user skills dir is
  `<agent-dir>/skills/` — `$HOME/.pi/agent/skills` by default, and
  `$PI_CODING_AGENT_DIR/skills` when that variable is set. `~/.pi/skills/`
  is NOT a user skills location for pi: it is the *project*-level
  `.pi/skills/` equivalent, and pi never advertises skills installed
  there. The original reading of the pi docs conflated both.
- L4 (evidence): test-first held. RED observed (05: 13, 06: 10, 07: 8
  failing assertions against the pre-change script). GREEN: 44/44 new
  assertions; full `scripts/test-installer.sh` suite green (01-07).
  Two implementation bugs caught by the cycle: glob basename with
  trailing slash returned empty names; `select_env` leaked the menu into
  its captured stdout (menu now goes to stderr). Differential against
  the base-commit script: `--help`, `uninstall --bogus`,
  `uninstall --purge` keep exit codes and semantics (only the usage text
  documents the new subcommand — requested); new inputs behave as
  specced. shellcheck: not installed on this machine — reported
  unavailable; `bash -n` plus the suite cover the surface. Cargo suite
  untouched (no Rust change). Commits: 61acc7d (skill), 8215175
  (installer + tests), 0721ed6 (docs + log).
- L5 (review): native assess over main..HEAD returned nativeRisk high
  (process/shell signals on install.sh), reviewDue true. Ordinary START
  (lenses risk/resilience/reliability) was declined at the consent gate
  by the user — candidate-scoped decline, no lineage created, nothing
  frozen. Native review therefore did not run for this candidate.
- L6 (user, verbatim): "si la skill se usa unicamente cuando se le pide al
  agente puede ser tambien por falta de un harness ya que pi viene vacio
  complemetamente, igual intentemos reforzar y luego probaremos con opencode"
- L7 (defect found by live test): the installer put pi skills in
  `~/.pi/skills/`, which pi never scans. Plain pi (no harness) therefore
  did not advertise `mnexis-memory` in its system prompt — the transcript
  showed 0 occurrences of the skill name, and the agent fell back to
  `AGENTS.md`. Empirical proof in the test container: with the skill under
  `/root/.pi/agent/skills/`, pi emits it in `<available_skills>` with name,
  description and location; under `/root/.pi/skills/` it emits nothing.
  Fix: `pi` resolves to `${PI_CODING_AGENT_DIR:-$HOME/.pi/agent}/skills`.
- L8 (decision): the gate in `skills/mnexis-memory/SKILL.md` was hardened
  (explicit confirmation required, a request to "set up memory" is not
  consent, headless sessions must stop instead of self-authorizing, and
  hand-editing `mnexis.toml`/`projects.toml` is forbidden). The workspace
  key rule was also made unambiguous (replace `/` and `:`, keep the host's
  dots) because the doc contradicted its own example.
- L9 (scope): `opencode` added to the installer registry
  (`opencode -> ~/.config/opencode/skills`, per the opencode skills
  docs), with installer test `08_skills_opencode_test.sh`.
- L10 (evidence): live test with plain pi (no harness) and plain opencode,
  both inside an isolated podman container (`node:22-trixie-slim`, pi 1.2.0,
  opencode 2.x, mnexis 0.1.0), workspace AegisPI, isolated profile.
  Both environments now discover the skill on their own (pi advertises it in
  `<available_skills>`; opencode lists it via `opencode debug skill`), bind
  the workspace with the canonical key `git:github.com-xeworg-aegispi`, stop
  before `mnexis config allow-namespace` asking for explicit confirmation,
  and — after the operator grants it — save one memory with
  `--topic-key`/`--tags` and recover it with `mnexis search` + `mnexis show`.
  Routing note: opencode competes with its own built-in `customize-opencode`
  skill, so the skill description must state the memory intent explicitly
  ("use mnexis rather than ad-hoc notes or AGENTS.md files").
- L11 (defect found by live test): `mnexis project bind` does not require a
  profile, so an agent could bind first and leave the profile uninitialized;
  the later `mnexis config allow-namespace` then fails with `NOT_FOUND`.
  The skill now requires `mnexis init` before any other command.
- L12 (defect found by live test): FTS5 token semantics — a multi-word query
  is a phrase, so queries must use terms that appear in the stored content.
