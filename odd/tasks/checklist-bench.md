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
- **S3 (the fixture — the thing both earlier benches could not do)** — a session whose work leaves
  **one durable, discoverable fact on the table that is not obviously memorable**: the model does
  **not** save it on its own (it makes **zero** `mnexis_save` calls, so the nudge fires), and it is
  not written anywhere in the repo — but it is there, it is worth saving, and a specific question
  surfaces it where the vague one does not. The prompt asks for the work and never mentions memory.
  **The pilot must show, before the measured batch:** (a) the nudge fires in **≥ 4/10** sessions
  (the load-bearing condition: the nudge's gate is `sessionSaveCalls > 0`, so it is silent in every
  session where the model called save at all — the first fixture revision failed here, firing in
  1 of 20 pilot runs, and the pilot record says so), (b) the fact is discoverable in the work and
  **not** in the repo (a probe that greps the repo for the fact's words and finds nothing), (c) the
  end state is unambiguous and readable from the workspace. If the pilot cannot produce a fixture
  that satisfies all three, the bench reports that and does not spend the batch.
  **The shape that was rejected, and why:** "two facts, one of them skipped" measures the gap the
  **second reader** addresses (a session that saved one thing and missed the other) — but the nudge
  never fires in a session that saved anything, so the checklist's lever cannot be observed there.
  The checklist's lever lives in the sessions where the model saved **nothing**: arm A's vague
  "deserves to survive" answers "nothing deserves memory", arm B's two answerable tests catch the
  fact. That difference is the hypothesis, and it is measurable per session.
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
| T1 | S1–S5 | worker: the criterion doc FIRST, the fixture (with its pilot), the three arms, the runner/scorer reuse, the pre-flight — **run twice**: round 7 with the two-facts fixture (Appendix A), round 8 with the procedure fixture (Appendix B: `verify-gap`, revisions R9→R10, 11 pilot sessions) | `bench/checklist-nudge` @ `tmp/wt-bench3` | — |
| T2 | S1, S4, S5 | parent: verify the criterion preceded the runs, run the measured batch, score, report — **prepared and unspent twice**: the pilot's condition (i) failed in both rounds (2/41, then 1/10), so S3 forbids spending it. Ready for a fixture that clears (i): the runbook is Appendix B.7 | — | — |

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

- **L4 (2026-11-02, T1 result — the bench did NOT run)** — 41 pilot sessions across seven fixture
  revisions; **the measured batch was not spent** (the pilot's load-bearing condition failed: the
  nudge fired in **2 of 41**, 4.9 %, against a required ≥4/10). The criterion was sealed before the
  pilot (sha256 `8dc591a5…`) and the gates did not move; Appendix A of
  `tmp/bench/checklist-measurement.md` is the pilot record, with the seven revisions and the
  mechanism. **The measured mechanism, and it is the deliverable:** (1) the model saves the
  session's salient fact in **40/41** sessions, before the settle boundary, so the nudge's
  `sessionSaveCalls > 0` gate silences it; (2) the other gate is a `write`/`edit` **tool call**,
  not a changed file — **6/41** sessions fixed the repo through `bash` (`chmod`, `sed -i`) and are
  invisible to it. Across the revisions: every one that leaves a fact in the session ends with the
  model saving it; the one that leaves no fact produces no `write`/`edit` call. The three arms
  (`current`, `checklist`, `checklist+files`) and the image (`bench-mem:checklist`) were built and
  pre-flighted (all three saved on their own — the same population problem). **Consequence, recorded
  in the board (commit `12e5e48`): the memory-quality gap lives in the sessions where the model
  already saved (40/41) and the nudge never fires there — so the checklist's lever is the smallest
  of the three proposals and the second reader's is the largest. This reverses the earlier
  ordering, which was made before the measurement.**

- **L5 (2026-11-02, T1 second attempt — the procedure fixture; the batch is NOT spent again)** —
  The board named the one shape not yet tried: a fact that is **not a diagnosis** — a procedure the
  session discovers with no error to explain. It was built and measured: `tmp/bench/fixtures/
  verify-gap/` has **nothing broken** (green `make check` before and after), the prompt asks for one
  config key (`timeout_ms = 400`) plus its bound (`timeout_max_ms = 1000` in `config/bounds.txt`),
  and the marginal fact is a procedure: **a bound declared in `bounds.txt` is inert — the checker
  reads a fixed list of three config keys, so a green check says nothing about the new key.** Two
  revisions were needed: **R9** left the checker announcing the keys it reads, and the model noticed
  its key was missing, probed with an invented key and **saved the fact** (0/1 fires); **R10**
  removed the announcement (image `bench-mem:checklist-r10` `827e58312664`, same engine tarball, one
  line of `ingest-check`) so the fact is discovered by reading the tool. **The pilot: 11 sessions of
  arm `current`** (`pl81` on R9, `pl82`–`pl91` on R10), 104 model calls, 708 740 billed tokens,
  $0.0268. **(i) the nudge fired in 1 of 10 — FAILS** (pl89). **(ii) holds structurally**: the
  fixture states the fact nowhere (`grep` over the fixture exits 1) and probe C reads it from the
  end state in 10/10 runs (the new key at `999999`, above the session's own `1000` bound, still
  exits 0). **(iii) holds 10/10**: check 0, both mutation probes fail, `timeout_ms = 400` present,
  the four bounds intact, exactly two files changed. **The gate, recorded per run because it is a
  confound against L4's pilot:** every session used the image's gate (`write`/`edit` tool calls
  only; main's `4865f51` workspace-state gate is not installed), and it was never the limiting
  factor — the work is two edits, made in **10/10** sessions, and `zero_saves_no_tool_write = 0`, so
  main's gate would have fired in the same 1/10. **The measured mechanism, and it is the
  deliverable:** the model **audits the tool before trusting its green answer** — it read
  `/usr/local/bin/ingest-check` in **10/10** sessions and probed it with a deliberately violating
  value in **5/10** — and then **recorded** what it found in **10/10** (9/10 by the declared rule;
  pl89 states it in other words). So the settle boundary opens in 1/10 and the fact survives 10/10:
  **C-G2's baseline sits at the ceiling and the "≥ 2×" clause is unreachable**, which is a second,
  independent reason not to spend 30 sessions — a spent batch could only return a negative C-G2 that
  says nothing about the rule (C-G6). The criterion did not move: the seal is still
  `8dc591a5…` (`head -c 23114 | sha256sum` re-verified after Appendix B was appended) and no new
  sealed revision was needed; only the fixture's declared fact rule changed, and the scorer prints
  it (`score-checklist.py --rules`). **Consequence: after two rounds and 52 sessions, the fact this
  bench needs — worth saving, needed by a later session, and not inspected by this model — has not
  been found; smaller work makes the tool more, not less, the subject.** Appendix B of
  `tmp/bench/checklist-measurement.md` is the record: the two revisions, the gate per run, the
  per-run table, and T2's runbook (prepared, deliberately unspent).
