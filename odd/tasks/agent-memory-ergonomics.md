# agent-memory-ergonomics

Make the memory usable by an agent without being told: the recall has to
happen on its own, the agent has to know where it may write, and the
opencode integration has to work out of the box. This feature applies the
improvements the opencode benchmark produced.

## Specs

- **S1** — Spontaneous recall. The skill's `description` and its "The loop"
  section must say, in the agent's own words, what the benchmark showed is
  missing: "al empezar una sesión en un proyecto existente, buscá una vez" and
  "antes de cambiar código existente, verificá si hay decisiones previas
  registradas". The benchmark's arm C is the failing case: extending a
  project whose memory held exactly the relevant context produced **0 calls**
  to mnexis.
- **S2** — A step zero. `mnexis context "<task>"` is the budgeted context
  pipeline and exists today, but no agent is told to call it. The skill must
  name it as the first call of a session, before the work starts, and the
  command must be discoverable from `mnexis --help`.
- **S3** — The `topic_key` charset must be documented everywhere an agent can
  read it: the CLI `--help`, the skill, and the MCP tool schema. The accepted
  set is `[a-z0-9.-]` (`src/application/service.rs`, `validate_topic_key`).
  The rejection must be actionable: it must name the offending character and
  the allowed set, so an agent that guessed `snake/design` is corrected in one
  step instead of burning three saves.
- **S4** — One call that answers "where may I write, and what is already
  there?". A new `memory_workspace` MCP tool and a new `mnexis status` CLI
  command return the same payload: the bound workspace namespace, the read and
  write allowlists, the profile's default namespace, and the most recent
  memories. The benchmark's arms E and F are the failing cases: nine CLI calls
  spent discovering the interface, and three MCP saves lost to a schema that
  never says which namespace the workspace accepts.
- **S5** — `mnexis agent install --env opencode` must write the MCP server
  entry into opencode's configuration, so the user does not have to write
  `opencode.json` by hand. The rules: create the file when it is missing, merge
  into it when it is strict JSON without touching any other key, refuse with
  instructions when the file is JSONC or invalid, and on uninstall remove only
  the `mcp.mnexis` entry that mnexis added.

## Tasks

- **T1** (S1, S2) — Rewrite the skill's trigger and "The loop" so the recall
  fires on its own; route: inline (parent). Commit: `docs(skill): make the
  recall trigger on its own`.
- **T2** (S3) — Document the `topic_key` charset in the CLI help, the skill
  and the MCP schema; make the rejection actionable. Route: worker.
  Commit: `fix(save): tell the agent which topic keys are valid`.
- **T3** (S4) — New `mnexis status` command and `memory_workspace` MCP tool
  over one application service; update the MCP tool-list test. Route: worker.
  Commit: `feat(status): tell an agent where it may write`.
- **T4** (S5) — The installer writes and removes opencode's MCP entry
  without destroying a foreign configuration. Route: worker (own commit: it
  writes a file the user owns). Commit: `feat(agent): wire opencode to the
  mnexis MCP server`.
- **T5** (S1-S5) — Close the feature: docs, full suite, independent
  verification. Route: inline (parent). Commit: `docs(odd): close
  agent-memory-ergonomics`.

## Log

- **L1** (2026-10-22) — User request, verbatim: "si aplica las mejoras".
  The improvements are the ones reported after the benchmark the user asked
  for: "quiero que hagas una prueba esto sera para ver rendimiento puedes
  hacer una prueba puede ser con opencode en un contenedor, que haga un
  pequeno juego en la terminal y que use la memoria para ver como reacciona
  que podriamos mejorar puedes usar los modelos gratis para esto".
- **L2** (2026-10-22) — The benchmark (harness in `tmp/perf/`, report in
  `tmp/perf/REPORT.md`, 13 runs) produced the P0 list the user approved:
  "1. Que el recall ocurra solo. ... 2. Un ritual de inicio de sesión ... 3.
  Documentar el charset del `topic_key` ... 4. Una tool que responda '¿dónde
  escribo?' ... 5. Que el instalador escriba la config MCP de opencode."
  The P1 engine items were explicitly not applied: `synchronous=NORMAL` was
  measured at 13% on ext4 and 0% on overlayfs, and the recommendation was not
  to change it.
- **L3** (2026-10-22) — Scope decisions taken by the parent, for the record:
  the pi extension keeps its own tool set (the benchmark measured opencode);
  `mnexis status` and `memory_workspace` share one application service so the
  two surfaces cannot drift; the opencode configuration is only written when
  it is strict JSON, because opencode also reads JSONC and mnexis will not
  rewrite a file it cannot parse.
- **L4** (2026-10-22, T1) — The skill now opens with "Start of session (do
  this first)": `mnexis context "<the task in one line>"` is the first call
  of a session, and a new paragraph tells the agent to look for the decisions
  behind the code before changing code that already exists. The frontmatter
  `description` names the start of a session and the "change existing code"
  case as triggers. Evidence: `cargo test --test assets_fidelity` → 2 passed
  (the embedded skill is the repository skill).
- **L5** (2026-10-22, T4) — The installer writes opencode's MCP entry.
  New module `src/application/opencode_config.rs`: it creates
  `~/.config/opencode/opencode.json` when it is missing, inserts the entry
  textually when the file has no `mcp` key (the user's bytes survive), merges
  it as JSON when the file already has an `mcp` key, and refuses to touch a
  JSONC or invalid file (the outcome carries the snippet to add by hand).
  Uninstalling removes the entry and restores the user's bytes. The command
  is part of the opencode integration, so it is written and removed by
  `mnexis agent install|uninstall --env opencode` whatever `--only` says.
  Evidence: 9 unit tests in the new module, 33 assertions in the new
  `tests/installer/15_opencode_mcp_config_test.sh`, and the live check in a
  container: `mnexis agent install --env opencode` wrote the configuration
  and `opencode mcp list` reported "✓ mnexis connected
  /usr/local/bin/mnexis mcp serve".
- **L6** (2026-10-22) — Delivery note: T4 is a high-risk unit (it writes a
  file the user owns), so it got its own commit even though it is larger than
  the ~400-line slice. The insert/remove is textual on purpose: the bytes the
  user wrote are never rewritten, only the block mnexis wrote is added or
  taken away.
- **L7** (2026-10-22, T2) — The `topic_key` rejection now names the allowed
  set: "invalid character: '/' (allowed: lowercase letters, digits, '.' and
  '-')". The same charset is in the CLI help (one line, so `save -h` shows
  it too) and in the `memory_save` MCP schema. Evidence: the worker's RED
  runs (four focused tests failing before the change) and GREEN after it, a
  differential against the base commit (valid saves byte-identical, the
  invalid one differs only in the message), and the whole suite green.
- **L8** (2026-10-22, T3) — `mnexis status` and the new `memory_workspace`
  MCP tool render ONE payload, built by
  `src/application/workspace_status.rs`: the workspace key and its bound
  namespace, the read and write allowlists, the memory count and the most
  recent memories (newest first). The storage layer grew `list_recent` and
  `count_in` (one shared `filter_sql` so the All/AllExcept/Only filter exists
  once) and the service grew `recent` and `count`. The workspace namespace is
  reported even when the scope cannot read it, and the recent list then falls
  back to the readable scope. Evidence: 5 integration tests, the worker's
  differential against the base commit (byte-identical output for init, save,
  list, show, history, config show, project list, export and doctor), the
  full suite green (18 targets) and clippy at the 36-warning baseline.
- **L9** (2026-10-22) — The MCP server now exposes 10 tools. The new
  `memory_workspace` carries the same payload as `mnexis status` and its
  description tells the agent to call it first, before saving anything. The
  tool-list tests in `tests/mcp_interop.rs` and `tests/mcp_harness.rs` were
  updated to the tenth tool, and a new test asserts the newest memory comes
  first.
- **L10** (2026-10-22) — The independent verification (gentle-ai-verify) of
  the opencode-configuration unit found three data-loss defects and six
  lesser findings, and it said plainly that the change could lose user data.
  All of them are fixed, in their own commit:
  - B1: the uninstall removed an `mcp.mnexis` entry the USER had written.
    Now the entry is removed only when it is exactly the entry mnexis would
    write today; a hand-written or edited entry is left byte-identical.
  - B2: a configuration whose root is not an object (an array, a string) was
    rewritten into a broken file or erased. Now it is refused.
  - B3: a non-object `mcp` member was silently replaced. Now it is refused.
  - A1: a symlinked configuration was replaced by a regular file. Now it is
    refused, and the link stays a link.
  - A2: the file mode was not preserved and a read-only file was
    overwritten anyway. Now the mode is preserved and a file that is not
    writable by its owner is refused.
  - A3/A4: the merge path re-serialised the user's document (a 30-digit
    number became a float, a duplicate key lost its first value, the file
    was reformatted). The re-serialising path is GONE: a small
    string-aware JSON scanner locates the member, and the file is only ever
    changed by adding or removing ONE textual block. 24 unit tests cover
    the scanner and every refusal, including the round trip that proves a
    30-digit number and a duplicate key survive untouched.
  - A5: `XDG_CONFIG_HOME` is now honoured.
  - A6: the help of `mnexis agent install|uninstall` says that the host
    configuration is part of the opencode integration and is not narrowed
    by `--only`.
  - A7: a refusal is not an error (the command exits 0 and reports
    `host config refused: …` with the snippet to add by hand); the docs say
    so.
  - A8: a failure of the host configuration no longer reports "could not
    write the agent assets".
  - A9: the README's "16 command groups" was stale; it says 25 now.
  The installer test suite also scrubs `XDG_CONFIG_HOME` now, so it can
  never write into the caller's real configuration directory.
- **L11** (2026-10-22) — The live re-test with opencode (the same arm that
  made ZERO calls to mnexis before the feature) is in L12: the skill alone
  did not make the recall spontaneous, and the finding is recorded there
  with its consequence.
- **L12** (2026-10-22, the live re-test) — The benchmark's failing arm was
  replayed with the feature in place, same prompt, same model, same
  workspace, same three memories: "extend the game", with no mention of
  memory anywhere in the prompt.
  - Before the feature: **0 calls** to mnexis (the agent never looked).
  - With the new skill alone (CLI path): still **0 calls**. The skill has
    to be opened on purpose, and this model does not open it on its own.
  - With the MCP server wired (the installer does it now) and the tool
    descriptions rewritten to say WHEN to use them: **2 calls, on its own**:
    `memory_workspace` first (the namespace `snake`, the allowlists and
    the recent memories), then `memory_update` at the end, with the reason
    and the revision, because the behaviour it had just changed was now
    described in the stored memory.
  The finding is the important one: **the channel decides**. A skill is a
  document the model has to decide to open; an MCP tool is in the tool list
  at every step, so its description is what the model reads. The rewritten
  descriptions ("Call this FIRST, at the start of every session and before
  you change code that already exists: the decisions behind the code live
  here, not in the files") are what turned the recall on. That is why the
  feature ships the MCP wiring and the descriptions, and not only the skill.
