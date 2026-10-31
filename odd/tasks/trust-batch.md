# Trust batch — the twin's residual risk, the hang, the silence

Post-#112 batch, in the priority order agreed with the human on 2026-10-31: harden the twin rule
(S1, design decision pending), make a hung engine survivable (S2), stop reporting silence as
emptiness (S3). The trim's behavior bench follows this batch.

## Specs

- **S1** — "Riesgo residual del twin (herencia de G5): mismo tipo + sets numéricos incidentales
  idénticos (un año, un id de ADR) + un keyword compartido todavía emparejan." The hardening
  design is the human's call (see Log); nothing ships until it is answered.
- **S2 (audit A9, verbatim)** — "No timeout and no cancellation anywhere: `signal` is accepted by
  all 14 `execute` signatures and never used; a hung `mnexis` blocks `session_start`, every
  `input` hook and every save, and pi's abort cannot kill the child" (`extensions` lib/cli.js and
  every execute body; line numbers pre-#111, re-locate).
- **S3 (audit A8, verbatim)** — "Three sites report an engine silence as 'there is nothing' (`No
  memories matched/stored/answered`, and `String(result.data)` → `'null'`), and ENOENT maps to
  exit 127 which `EXIT_CODES` does not know → `'exited with code 127 (unknown error)'`; every
  *write* tool throws instead of returning the actionable text."

## Tasks

| ID | Specs | Route | Branch / worktree | Commit |
| --- | --- | --- | --- | --- |
| T1 | S2, S3 | worker, sequential in one worktree | `fix/trust-batch` @ `tmp/wt-trust` | — |
| T2 | S1 | after the design decision (L2) | same branch, same worktree | — |
| T3 | — | the trim's behavior bench (next batch, referenced only) | bench harness | — |

Allowed edit surfaces: T1 `extensions/mnexis-memory/**`, **excluding the twin-rule functions**
(`twinValue`, `sameNumericSignature`, `twinPairs`, `twinRepairLine`, `sharesIdentity`,
`sameKnownType`, `numericValues`) — those are S1 and wait for the decision.

## Log

- **L1 (2026-10-31, user)** — "listo el merge, ahora bien, habian otras cosas pendientes": PR #112
  merged (dfcde51), proceed with the agreed post-feature order — trust batch first, then the trim
  bench; B2/B3 stay parked pending dogfood.
- **L2 (2026-10-31, pending)** — the S1 design question was asked with three options (ambient-value
  frequency / exclude years / accept documented risk). The answer rewrites S1 and opens T2.
