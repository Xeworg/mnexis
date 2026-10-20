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
  registry starts with `pi -> $HOME/.pi/skills` and is extensible.
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
- L3 (evidence): pi user skills dir confirmed as `<agent-dir>/skills/`
  (`~/.pi/skills`) from pi docs (configuration.md, skills.md); project
  dir `.pi/skills/` exists but user-level is the install target.
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
  (installer + tests), closing commit (docs + log).
