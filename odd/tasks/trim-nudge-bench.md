# The bench arm: does the trim and the nudge change what the agent saves

Two hypotheses from the 2026-10-31 work, both still unmeasured:
- **H1 (trim)** — the tools block cut from 9 014 B to 5 490 B: does the agent behave the same,
  and does the smaller prompt cost nothing in what it manages to remember?
- **H2 (the nudge, S4)** — the settle nudge (a session that changed files and saved nothing gets
  asked once): does the agent actually save more memory because of it?

Both are behavioral claims about the model, so only the bench can settle them.

## Specs

- **S1 (arms)** — three arms, ONE image (the current main engine), the extension file patched per
  arm so the engine is identical and only the extension differs:
  - **A `pretrim`** — `6508e05:extensions/mnexis-memory/index.js` (the 9 014 B block, no S2, no S4).
  - **B `trim`** — `main:extensions/mnexis-memory/index.js` (5 490 B, no S2, no S4).
  - **C `trim+nudge`** — `ext/pi-capture:extensions/mnexis-memory/index.js` (5 490 B + S2 + S4).
- **S2 (fixture)** — a NEW prompt fixture where the S4 signal exists: the agent must WRITE a file
  (so "files changed" is true) and make a decision worth remembering (so a memory is the right
  answer). The current phase0 fixture only asks for saves, so it can never trigger the nudge —
  reusing it would measure nothing about H2.
- **S3 (criterion before numbers)** — the gates and the scoring are written BEFORE any run, in
  `tmp/bench/trim-nudge-measurement.md`, following the round-4 discipline: what counts as "saved a
  memory", what "the same behavior" means, what would falsify each hypothesis.
- **S4 (cost)** — the arms also measure the prompt cost the trim claims to save, from the
  transcript (the bytes the model was actually billed for), because a behavioral win that costs
  more is not a win.
- **S5 (honesty)** — the canonical `estado al cierre` record (arm C only, written by code at
  shutdown) is NOT a model save: it is identified by its canonical title and excluded from the
  save counts, or the arm comparison is rigged in C's favor.

## Tasks

| ID | Specs | Route | Branch / worktree | Commit |
| --- | --- | --- | --- | --- |
| T1 | S1–S5 | worker: image, arms, fixture, criterion, runs, report | `bench/trim-nudge` @ `tmp/wt-bench` | — |
| T2 | all | parent: verify the criterion was written first, the counts, the report | — | — |

Allowed edit surfaces: `tmp/bench/**` only (the harness, the images, the fixture, the criterion
and the report). No product code changes on this branch — the arms are built by patching the
installed extension file, exactly as the types4 arm did.

## Log

- **L1 (2026-10-31, user)** — the agreed post-feature order: trust batch first, then the trim
  bench. S4 (the settle nudge) joined the queue on the human's review of `ext/pi-capture`:
  "aun asi no logramos que el agente evalue si debe guardar memoria" — the nudge is the
  mechanical answer, and it is a hypothesis until the bench says otherwise.
- **L2 (cost note)** — the bench spends model sessions (N runs × 3 arms). The human's standing
  order covers running it; the criterion must exist before the first run.
