# The checklist bench — does a decision rule widen what survives the judgment?

The enriched bench measured the nudge and found it works (8/8 conversion when it fired) but
that **more description of the same session changes nothing** (the plain and the enriched nudge
were indistinguishable on every gated metric, at +14 % cost). The three explorations
(`odd/next.md`, "Three explorations of the settle nudge") landed on one diagnosis: the nudge fires
**after** the model has already concluded its judgment, so it asks the model to re-open one — and
what it lacks is not more description but **a decision rule the model did not already have**. The
checklist is that rule. This bench measures it.

## Specs

- **S1 (the checklist, verbatim from the exploration)** — the nudge's opening sentence becomes:
  *"This session changed files but saved no memory. Judge it with two tests, and answer both in one
  clause before you act: (1) would a later session's answer or action change because of it? (2)
  will it still be true when that session starts? Both yes: save it now with one mnexis_save.
  Otherwise reply that nothing deserves memory; the diff itself, a green test, a restatement of the
  prompt — none of these pass."* The three survival examples and the seven type glosses stay
  **verbatim** (they are drift-pinned to the opencode plugin's copy).
- **S2 (the arms, one variable)** — same image, engine, fixture, prompt and model; the arm is the
  installed `extensions/mnexis-memory/index.js`:
  - `A current` — `7bbbac6` (main: the enriched nudge, the text that ships today)
  - `B checklist` — `7bbbac6` + the S1 sentence
  - `C checklist+files` — `B` + the nudge naming the files the session wrote ("This session wrote
    X, Y and saved no memory"), the context exploration's proposal, riding the same bench because
    it is cheap to test and cheap to drop.
- **S3 (the fixture — the thing both earlier benches could not do)** — a session whose work yields
  **two distinct candidate facts of different kinds**: a *decision* the model naturally saves (a
  config value moved because an external constraint changed) and a *procedure/knowledge* the model
  naturally skips (something discovered while working that is **not written in the repo** and that a
  later session would otherwise have to rediscover). The prompt asks for the work and never mentions
  memory. **The pilot must show, before the measured batch:** (a) the nudge fires in **≥ 4/10**
  sessions (headroom — the sh-check fixture gave 3-5/10 and the trim fixture 0/10), (b) the
  model's spontaneous saves are **decision-shaped** (so the marginal fact's absence is a judgment
  failure and not a fixture artifact), and (c) the procedure fact is discoverable but not stated
  anywhere in the repo. If the pilot cannot produce a fixture that satisfies all three, the bench
  reports that and does not spend the batch.
- **S4 (the gates, pre-registered before any run)**:
  - **C-G1 (the boundary is reached).** Nudge transcripts ≥ 4/10 in each nudge arm. Fewer → report,
    no verdict.
  - **C-G2 (the rule widens what survives — the hypothesis).** In the fired sessions, the rate at
    which the **procedure/knowledge** fact is recorded is **≥ 2× the current arm's** and **≥ 4/10
    sessions**. The current arm's rate is the baseline, read from the same runs (not assumed).
  - **C-G3 (the rule does not buy noise).** Human read of **every** save in arms B and C: **zero
    fabricated facts** and **≥ 2/3 state something this session established**; records/run ≤ A + 1;
    **zero** saves that are only a change log (a file path with nothing else).
  - **C-G4 (the cost).** Billed tokens/run of B ≤ **1.05 ×** A's (the enriched text's +14 % is the
    bar this must not beat: the checklist replaces a sentence, it does not add a second call).
  - **C-G5 (nothing regresses).** End-state parity 10/10 in every arm; the `estado al cierre` close
    record 10/10; pi's tools block still ≤ 5 500 B; the opencode drift pin green (the seven gloss
    lines unchanged).
  - **C-G6 (the honest limit).** One model, one fixture, one prompt, N = 10 per arm. The bench
    measures *that* model on *that* fixture. A negative C-G2 on one fixture is not a refutation of
    the checklist in general — it is a statement about the fixture's ability to show it.
- **S5 (counting, unchanged)** — a save is a successful `mnexis_save` the MODEL made; refusals
  counted separately; the canonical `estado al cierre` record excluded from every count. The
  busy-retry landed in the extension (`7bbbac6`): a save that found the store locked is tried
  again, so a "busy" refusal in these runs is now evidence about the engine, not about the nudge.

## Tasks

| ID | Specs | Route | Branch / worktree | Commit |
| --- | --- | --- | --- | --- |
| T1 | S1–S5 | worker: the criterion doc FIRST, the fixture (with its pilot), the three arms, the runner/scorer reuse, the pre-flight | `bench/checklist-nudge` @ `tmp/wt-bench3` | — |
| T2 | S1, S4, S5 | parent: verify the criterion preceded the runs, run the measured batch, score, report | — | — |

## Log

- **L1 (2026-11-02, human)** — "dale": the order from the board's analysis — the BUSY retry first
  (done, `7bbbac6`), then the checklist as a bench arm measured before adopted.
- **L2 (evidence carried over)** — the enriched bench (`tmp/bench-enriched/enriched-report.md`):
  the nudge converted 8/8 when it fired; plain vs enriched indistinguishable; typing 100 % in all
  three arms; E-G0's proxy mis-fired (baseline 7/10 against a gate of ≤6/10). The trim bench
  (`tmp/bench/trim-nudge-report.md`): H1 fails on G3 — the fat block pulled 24 save attempts vs the
  trim's 11, and the extra saves were exactly the marginal `knowledge`/`procedure` facts.
- **L3 (the hypothesis under test)** — "a decision rule the model did not already have can move
  what survives; more description cannot" (the exploration's transfer from "LLMs struggle to
  self-correct without external feedback, and performance even degrades"). This bench is the test
  of that transfer, on the one population where text can move anything: the fired sessions.
