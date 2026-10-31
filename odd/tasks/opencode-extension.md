# The opencode integration, done with quality

The engine already knows opencode (MCP config registration, `src/application/opencode_config.rs`)
and the board already scoped the gap (section 11: the door — `memory_context` — is NOT in the
sixteen MCP tools, so an opencode agent rebuilds the resume answer by hand). This feature
investigates opencode as it is TODAY and ships the integration the research says is quality.

## Specs

- **S1 (verbatim)** — "es importante hacer que la extension de opencode sea de calidad".
- **S2 (verbatim)** — "el agente investigue opencode y lo que hace falta para hacer la extension".
- Deliverables: (a) `extensions/mnexis-opencode/RESEARCH.md` — opencode's current plugin/integration
  model (events, tools, config, distribution), what each mnexis surface needs there, and the
  recommended shape, with sources; (b) the integration itself under `extensions/mnexis-opencode/`,
  built the way the research says, with tests where the host's model allows them.
- Out of scope on this branch: adding `memory_context` to the engine's MCP server (engine work;
  the board owns that order). The research must say whether the extension is blocked on it.

## Tasks

| ID | Specs | Route | Branch / worktree | Commit |
| --- | --- | --- | --- | --- |
| T1 | S1, S2 | worker: investigate, research doc, build | `ext/opencode` @ `tmp/wt-opencode` | — |
| T2 | all | parent: verify, commit, report for the merge decision | — | — |

## Log

- **L1 (2026-10-31, user)** — "tambien, es importante hacer que la extension de opencode sea de
  calidad haz un worktree para cada extension donde el agente investigue opencode y lo que hace
  falta para hacer la extension … hazlo en dos ramas aparte luego vemos si le hacemos merge o no".
- **L2 (prior findings)** — board `odd/next.md` section 11 (the door is not in the MCP surface;
  the multi-host order); the B1-round-2 finding "opencode plugin feasible on hooks+gate";
  `src/application/opencode_config.rs` (the existing MCP-entry registration). Prior finding about
  opencode's TUI: it hardcodes rendering (board ~line 648).
