# doctor-contract — `mnexis doctor` with a stable machine contract

The doctor is the operator's tool when something goes wrong. Today its
answer is prose: a `name`, an `ok`, and a free-text `detail`. This feature
gives every check a stable identity (`check_id`), a severity, a machine
`reason_code`, evidence, and a `safe_next_step` — plus a `--check CODE`
filter and a `doctor repair` subcommand with `--plan|--dry-run|--apply`.

Branch: `feat/doctor-contract`. Evidence: `tmp/bench/report.md` item 8
(doctor breadth — the capability gap), `odd/next.md` §2 (C3).

## Specs

- **S1** — Stable JSON envelope. Board, verbatim: "The other engine has ten
  named checks with a machine envelope (`status`, `summary`,
  `checks[].check_id`, `severity`, `reason_code`, `evidence`,
  `safe_next_step`), a `--check CODE` filter and `doctor repair
  --plan|--dry-run|--apply` for seven specific codes. Small, and it is what
  an operator needs when something goes wrong." mnexis adopts the envelope
  shape for its own checks (7 today: schema_version, canonical_tables,
  fts_readable, wal_mode, integrity, fts_shape, fts_invariant — the board's
  "five" predates fts_shape and fts_invariant). Top level: `status`
  (`healthy` | `degraded` | `failed`), `summary`, `checks[]`.
- **S2** — Filter, verbatim from the board: "a `--check CODE` filter".
  `mnexis doctor --check fts_shape` runs only that check; an unknown CODE is
  an invalid argument, not a crash.
- **S3** — Repair, verbatim from the board: "`doctor repair
  --plan|--dry-run|--apply` for seven specific codes" (mnexis defines which
  of ITS codes are repairable; it does not copy the other engine's seven).
  `--plan` lists what would be repaired and why (no writes); `--dry-run`
  executes inside a savepoint and rolls back, reporting the counts;
  `--apply` performs the repair. Canonical damage (missing tables, failed
  integrity, newer schema) is reported as not repairable with the reason.
  The old `doctor --repair` flag is removed (operator, 2026-10-23: "no hay
  usuarios aun nadie ha bajado el repo" — clean break allowed).
- **S4** — Exit codes are a contract and do not change (e2-core §8, pinned
  by existing tests): 0 healthy, 6 INDEX_UNAVAILABLE (FTS family), 9
  UNSUPPORTED_VERSION, 1 other infrastructure failure. A filtered run's
  exit code reflects only the checks that ran.

## Tasks

| ID | Spec | Route | Commit |
| --- | --- | --- | --- |
| T1 | S1 | contract types + envelope in `diagnostics/doctor.rs` (severity, reason_code, evidence, safe_next_step per check; `status`/`summary` at top level), tests RED→GREEN for the 7 checks | `339a435` |
| T2 | S2 | `--check CODE` filter in `cli/` (arg, unknown code = invalid argument), tests | `55a32a9` |
| T3 | S3 | `doctor repair` subcommand (`--plan/--dry-run/--apply`), repairable vs canonical classification, replaces `--repair`; tests incl. rollback proof | |
| T4 | S4 + docs | full suite + fmt + clippy, PRD/help/docs updated, board refresh | |

## Log

- **L1** (2026-10-24) — operator picks the next feature from the board:
  "C3: doctor con contrato" (from `odd/next.md` §2, quoted in S1–S3).
- **L2** (2026-10-24) — context for the clean break in S3: operator,
  2026-10-23, "no hay usuarios aun nadie ha bajado el repo por lo visto
  nadie lo ha encontrado aun".
- **L3** (2026-10-24) — slice boundary: T1 landed as one commit (`339a435`, 811
  authored lines — past the ~400 slice) because the contract rewrite touches
  every check construction atomically and splitting it would leave a
  half-old envelope at the boundary. Delivery strategy for the rest: T2
  (docs + board) closes the feature. T2 landed at `55a32a9` (slice total 1009
  lines across two commits — the feature's delivery slice remains T1+T2 as
  one reviewable PR unit of the same concern, the contract; T3+T4 form the
  second).
