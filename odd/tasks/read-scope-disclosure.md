# read-scope-disclosure — a read says whether it was bounded

The hazard the independent verification named, and the reason the closed
default cannot be flipped safely today:

> A read WITHOUT an explicit namespace is filtered by the scope and returns
> fewer rows **with no notice**; a read WITH an explicit namespace is denied.
> Under a closed default an agent searching without a namespace would read
> "no memories" where the truth is "no permission" — and then save the fact
> again. Silence is what turns a permission into a duplicate.

This feature closes the silence. It does **not** flip the default
(`allow_read_all` stays `true` for new profiles): the default is measured
first, separately.

Branch: `feat/read-scope-disclosure`.

## Specs

- **S1** — Every read answer declares the scope it was bounded by, in the
  JSON envelope, as `read_scope` with exactly three values:
  - `all` — the reads are open and the reserved `global` namespace is open
    too (`allow_read_all` and `allow_global`).
  - `except_global` — the reads are open across the namespaces except the
    reserved one (`allow_read_all`, `allow_global` off).
  - `restricted` — the reads cover only what the profile allows
    (`allow_read_all` off): the read allowlist plus the namespace the
    workspace is bound to.
  The value says **whether** the answer may be incomplete. It never carries
  a count and never names a namespace the caller cannot read: the shape of
  the allowlists is not disclosed, and a namespace the profile cannot read
  does not announce itself by existing.
- **S2** — The human-readable text says the same thing in one line, so an
  agent that reads the text (and not the JSON) can tell a bounded answer
  from a complete one.
- **S3** — The declaration covers every read surface: `search` (both the
  normal and the `--explain-hits` answers), `list`, and `context`. The
  field is **additive**: the existing fields, their names and their
  meanings do not change, so a consumer of today's envelopes keeps
  working.
- **S4** — The MCP read tools (`memory_search`, `memory_list`,
  `memory_context`) declare it too, in the JSON they answer. The MCP
  surface gains no tool and loses none.

## Tasks

| ID | Spec | Route | Allowed edit surfaces |
| --- | --- | --- | --- |
| T1 | S1–S4 | the scope mode on the scope type; the envelopes, the text and the MCP tools; tests RED→GREEN | `src/domain/scope.rs`, `src/cli/commands.rs`, `src/cli/output.rs`, `src/mcp/server.rs`, `tests/cli_commands.rs`, `tests/mcp_interop.rs` |

## Log

- **L1** (2026-10-25) — the operator: *"dale"* to the hazard + the measured
  closed default. The hazard first, the measurement second, the flip last.
- **L2** (2026-10-25) — why three values and not a boolean: `all` and
  `except_global` are both "the reads are open", but only the second one
  hides something, and an agent that cannot tell them apart would treat
  "the reserved namespace is hidden" as "the profile is closed".
- **L3** (2026-10-25) — why no count: a count of hidden namespaces is still
  information about namespaces the caller cannot read. The mode is enough
  to make the decision ("maybe something is missing; ask for it by name and
  the refusal names the way to get it").
