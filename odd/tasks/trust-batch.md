# Trust batch — the twin's residual risk, the hang, the silence

Post-#112 batch, in the priority order agreed with the human on 2026-10-31: harden the twin rule
(S1, design decision pending), make a hung engine survivable (S2), stop reporting silence as
emptiness (S3). The trim's behavior bench follows this batch.

## Specs

- **S1 (DECIDED, L2)** — "Valor ambiente ≠ identidad": un valor que muchos registros vivos del
  namespace llevan (un año, un id repetido) no es evidencia de identidad y no puede emparejar
  gemelos; solo un valor RARO empareja. Implementación: el repair ya consulta `list --value` por
  cada valor distinto — esa misma respuesta cuenta los portadores vivos en el namespace; si el
  valor tiene más portadores que el par bajo evaluación (tercero incluido), es ambiente y el par
  se rechaza. El umbral exacto lo propone la implementación con su razonón y los tests lo pinean.
  Aceptación sobre el corpus G5 (`tmp/bench/out/s3-types4-phase0.jsonl`): la cadena `{0007, 2026}`
  NO empareja (ambos valores ambiente en ese namespace); un twin verdadero (mismo puerto en dos
  registros, namespace chico) SIGUE emparejando.
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
| T1 | S2, S3 | worker, sequential in one worktree | `fix/trust-batch` @ `tmp/wt-trust` (task mvvy25xq-6-xkfw) | `c2afcd4` |
| T2 | S1 | DECIDED (L2); shipped after T1 — same worktree, same files | same branch, same worktree | `4374f17` |
| T3 | — | the trim's behavior bench (next batch, referenced only) | bench harness | — |

Allowed edit surfaces: T1 `extensions/mnexis-memory/**`, **excluding the twin-rule functions**
(`twinValue`, `sameNumericSignature`, `twinPairs`, `twinRepairLine`, `sharesIdentity`,
`sameKnownType`, `numericValues`) — those are S1 and wait for the decision.

## Log

- **L1 (2026-10-31, user)** — "listo el merge, ahora bien, habian otras cosas pendientes": PR #112
  merged (dfcde51), proceed with the agreed post-feature order — trust batch first, then the trim
  bench; B2/B3 stay parked pending dogfood.
- **L2 (2026-10-31, decision)** — the S1 design question was answered by the human:
  **"Valor ambiente ≠ identidad"** (ambient value is not identity) over exclude-years and
  accept-documented-risk. S1 rewritten above; T2 opened queued behind T1 (same worktree, the twin
  functions were T1's forbidden zone until it landed).
- **L5 (2026-10-31, T1 result, parent-verified)** — A9 + A8 shipped as ONE commit (`c2afcd4`):
  the worker stated the two-commit split needed hunk surgery (`git add -p`) in adjacent regions,
  so one honest commit is more reviewable than a broken split. A9: detached spawn (own process
  group), SIGTERM → 200 ms → SIGKILL on the group, the accepted `signal` bound in all 14 execute
  bodies via `client.withSignal`; budgets READ 10 s / WRITE 30 s (`MNEXIS_TIMEOUT_MS` overrides).
  A8: engine silence is a failed call with a next step — never "No memories…", never "null";
  ENOENT/127 classified and in EXIT_CODES; writes return the reason instead of throwing. One
  deliberate semantic change beyond the audit lines: a failing `mnexis project list` no longer
  means "unbound workspace" (the write fails with that text) — the old behaviour was a lie about
  a bound workspace. Suite 200 tests / 198 pass / 2 parity skips (same skips as base). Parent
  re-ran the timeout (4/4) and silence (11/11) tests. Tools prose 5 490 B and the twin-rule
  functions verified untouched. T2 (S1 twin) is now unblocked.
- **L6 (2026-10-31, T2 result, parent-verified)** — S1 shipped as `4374f17`. **Threshold:
  `carriers > twins`** (the cluster size): the carriers of each value are counted from the SAME
  `list --value` answers the repair already issues — zero extra engine calls — counting live
  records of the namespace that carry the value, excluding the record the save just wrote. A
  value one more live record also carries never pairs; two or three records of ONE fact still
  converge to one (the strict alternative, `carriers > 1`, was measured and breaks the shipped
  "every live twin is retired" case — that measurement is why the threshold is the cluster).
  RED: the ambient-year corpus and the G5 `{0007, 2026}` chain both superseded a live record
  before the change; GREEN: neither pairs, with controls proving the refusal is about rarity.
  PRESERVE: a true twin (8081 carried only by the pair) still supersedes exactly as before. The
  worker also ran a differential: the base implementation with the new corpus fails only the two
  new rules — every pre-existing scenario is byte-identical. Suite 203/201/2 skips; tools prose
  untouched. **Two honest limits carried into the board:** (1) a true twin stops pairing when any
  live record outside the fact carries any value of its signature — a save mentioning a year no
  longer pairs in a busy namespace (that IS the decision, applied literally); (2) a frequency rule
  needs a third live witness — in a namespace of only two records a year looks rare and pairs, so
  the residual G5 risk survives in stores too small to show the year is ambient (the rejected
  "exclude years" option would have closed it). Ambient refusals are silent, consistent with the
  other narrow rules.
