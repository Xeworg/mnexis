# The opencode MCP polish — curated definitions and the type vocabulary on the other host

The replicability investigation and the probe-backed design (2026-11-01) established what is
viable on opencode: the transcript is host-owned (no per-tool icons, maintainer-confirmed
#54703/#53547), but a V2 plugin CAN rewrite the model-facing definitions of MCP tools —
re-verified at runtime: the patch must live INSIDE the `ctx.tool.transform` callback (a stored
update resets on catalog refresh; before the first `tools/list` it is a no-op), `codemode:false`
is honored (and without it the 17 mnexis tools reach the model NOWHERE), the rewrite is
model-facing, and an uncaught throw inside the transform BREAKS the turn.

## Specs

- **S1 (verbatim, human, 2026-11-01)** — "ok continua con la extension de opencode con iconos,
  ponlo en un worktree". Scope agreed with the design: the icons of the transcript are
  host-blocked; this branch ships what is viable — the definition polish that makes opencode
  saves arrive typed and the memory door visible.
- **S2 (the transform seat)** — V2 only: a new `ctx.tool.transform` seat in `setup(ctx)`
  (`extensions/mnexis-opencode/index.js`, beside the existing `ctx.tool.hook("execute.before")`
  seat, copying its try/catch discipline). The whole transform body is wrapped: a plugin must
  never break a turn. V1 has no equivalent and keeps working unchanged — stated in the README.
- **S3 (the rewrite)** — inside the callback, for every `mnexis_*` tool the editor lists:
  (a) replace `description` with the curated text — one entry per MCP tool id (the engine's tool
  inventory is the source of the id list, `src/mcp/server.rs`), derived from pi's curated block
  (`extensions/mnexis-memory/index.js`) where the tool matches, adapted where MCP-only;
  (b) `mnexis_save`'s curated description carries the **seven type glosses verbatim** — the same
  lines `SETTLE_NUDGE` carries on pi (one definition of the vocabulary, two consumers: pi's
  nudge, opencode's save description);
  (c) skip unknown ids (engine tool renamed) — never throw;
  (d) when the catalog has no `mnexis_*` tools (MCP down), the transform is a no-op.
- **S4 (codemode)** — the same transform sets `options.codemode = false` on every `mnexis_*`
  tool; AND the durable engine-side route ships here: `src/application/opencode_config.rs` writes
  `"codemode": false` when registering the mnexis MCP server, with its unit test updated.
- **S5 (drift discipline)** — the plugin is ONE flat file: the curated texts and the glosses are
  an inline copy. A test pins the copy: the seven gloss lines in the plugin file must equal the
  gloss lines in pi's `SETTLE_NUDGE` (`extensions/mnexis-memory/index.js`) — if either side
  drifts, the test fails. The curated map has exactly one entry per MCP tool id, no empty text.
- **S6 (no TUI panel here)** — the panel (`session.composer.top`) is a separate future branch;
  nothing in this change adds TUI parts, deps, or breaks the flat-file/no-node_modules contract.

## Tasks

| ID | Specs | Route | Branch / worktree | Commit |
| --- | --- | --- | --- | --- |
| T1 | S1–S6 | worker: the transform seat, curated map + glosses, codemode both sides, tests | `ext/opencode-polish` @ `tmp/wt-ocpolish` | — |
| T2 | all | parent: verify (incl. a live-host smoke if cheap), commit, report | — | — |

## Log

- **L1 (2026-11-01, user)** — "ok continua con la extension de opencode con iconos, ponlo en un
  worktree": the go, after seeing the probe-backed design and its second review.
- **L2 (design provenance)** — probe-backed design (verify agent, scratch `/tmp/mnexis-probe-*`):
  P1 in-callback timing, P2 refresh reset, P3 codemode honored (and mnexis invisible under
  Code Mode), P4 model-facing, probe9 uncaught-throw breaks the turn. Second review deltas:
  TYPE_GLOSSES shared once (nudge + opencode description; NEVER pi's tools block — the 5490 B
  pin), one-file inline copy + drift pin, `memory_type` missing from the MCP save answer is a
  separate engine follow-up (board), NOT this branch.
