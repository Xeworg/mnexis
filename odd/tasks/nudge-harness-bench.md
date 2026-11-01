# The assembly, measured — the bench the nudge's harness analysis asks for

Objective: measure the **assembled** mechanism the three-axis analysis produced — the
`turn_end` seat, the coverage gap and the output-pointer rule — against the shipped nudge and
against no nudge at all. Branch `bench/nudge-harness` (worktree `tmp/wt-bench5`), based on
`main` at `ccce596`. The analysis it measures is `odd/tasks/nudge-harness-analysis.md`
(`6296cbb`). Test runner: the podman harness of `bench/checklist-nudge`, reused.

## Specs

- **S1 (the claim, falsifiable)** — the assembly records the **marginal fact** — the
  affordance fact the session read but never tripped over — in the population where the model
  **already saved** (the 40/41), which is where the shipped nudge is silent by construction
  (`sessionSaveCalls > 0`, `extensions/mnexis-memory/index.js:1929`).
- **S2 (the arms, one variable)** — one image, one fixture, one prompt, one model; the arm is
  the installed extension set:
  - **A** — the shipped settle nudge (`7bbbac6` + `4865f51`).
  - **A′** — **no nudge at all** (the control: the extension without the settle seat).
  - **B** — the assembly.
- **S3 (the mechanism under test)** — three parts, each with the evidence it rests on:
  - **(a) the seat.** `turn_end` of the turn that changed the workspace — not the settle
    boundary. `TurnEndEvent` carries that turn's `toolResults` (pi's
    `types.d.ts:828-835`, verified present in the bench image's pi 1.2.0) and its result type
    is `BoundaryResult` (`:1085`), so the handler may return one entry and `continue: true` for
    one next model request. The gate is the **workspace** comparison already written at
    `index.js:1920-1928` (the git state against the session's baseline) — **not** the
    `sessionSaveCalls > 0` clause, which is precisely what silences the nudge in the 40/41.
    The seat fires **once per session** (the session-scoped one-shot at `index.js:870`, reset at
    `:1746`), because an unconditional continuation can loop
    (`docs/extensions.md:118`).
  - **(b) the payload — the coverage gap.** The values the turn **moved** that no record
    carries: the numeric signature of the changed lines (the measured rule,
    `numericValues`/`sameNumericSignature`, `index.js:515-556`) against the session's own saves
    and the namespace's records that carry those values (`mnexis list --value`). The payload is
    up to three `old→new` pairs, one per line — **not** a file path (the board's named
    degenerate form: "zero saves that are only a change log").
  - **(c) the rule — the output pointer.** A fact about how a tool, command or checker behaves
    is saved only with **the output line that shows it**, quoted into the record; the fragment
    must occur in the turn's `toolResults` (or the transcript's `llmMessages`), so the
    extension can **verify** the citation instead of trusting the prose. The accept set is the
    affordance facts; the reject set is the session's own writing, `project_state` (the
    extension already writes one titled `estado al cierre`, `index.js:404`) and restatements of
    what the store holds.
  - **The shape of the implementation** (a build decision, not a criterion): prefer a
    **standalone extension** loaded next to the shipped one — the mechanism needs the
    workspace's git state (it can snapshot it itself), the session's saves (it can observe the
    `mnexis_save` tool calls), and the turn's tool results (the event carries them) — so the
    arm's diff stays small and reviewable. If the standalone extension cannot see what it
    needs, fall back to a patched copy of `index.js` in the arm's directory, as the previous
    benches did, and say so in the report.
- **S4 (the gates, pre-registered before any run)** — `A′`'s rate is read from the same runs,
  never assumed:
  - **N-G1 (the control has headroom)** — A′ records the marginal fact in **≤ 5/10** sessions.
    The trim bench measured **1/10** for the same facts under the same tools block
    (`tmp/bench/trim-nudge-report.md`, H1-G3: A 5/10, B 1/10). A′ above 5/10 → report, no
    verdict: the fixture cannot show the gap.
  - **N-G2 (the assembly moves what survives)** — B records the marginal fact in **≥ 2× A′**
    **and ≥ 4/10** sessions. **This is the kill**: if B is no better than A′, the "wrong
    moment, wrong author" diagnosis is refuted and the settle boundary was never the problem.
  - **N-G3 (no slop)** — records/run **≤ A+1**; **zero** fabricated facts (human read of every
    save in B); **zero** change-log-only records; the `estado al cierre` record still lands
    once per session.
  - **N-G4 (the cost)** — B's billed cost per run **≤ 1.15 × A′'s**, and the model's call
    count per run is reported next to it (the assembly costs one extra request per firing
    session; the reference is ≈$0.00025 per call, ≈$0.00269 per session, 41 sessions for
    $0.1103 — all derived from the pilot).
  - **N-G5 (nothing regresses)** — the end state holds **10/10** in every arm (`make check`
    exit 0, both mutation probes non-zero, the config byte-identical to the fixture, the
    bounds intact); the tools block stays **≤ 5 500 B**; the opencode drift pin stays green.
  - **N-G6 (the honest limit)** — one model, one fixture, one prompt, N = 10 per arm. A null
    N-G2 is a statement about *that* model on *that* fixture, not about the mechanism in
    general.
- **S5 (the pilot, before the batch — the pilot is not data)** — the pilot must show:
  **(i)** the assembly fires in **≥ 4/10** sessions (the same headroom rule the checklist's
  pilot used); **(ii)** the marginal fact is present in the store at the session's close and is
  **distinguishable** (a declared string, not a human judgement); **(iii)** the end state is
  unambiguous and readable from the workspace; **(iv)** the pointer check is **exercised** — it
  ran at least once and its verdict is in the artifacts. If the pilot cannot show all four,
  report it with the pilot record and **do not spend the batch**.
- **S6 (the counting)** — a save is a successful `mnexis_save` **the model made**; refusals are
  counted separately; the canonical `estado al cierre` record is excluded from every count; a
  record written by the code (not the model) is not a save.
- **S7 (the fixture)** — the trim bench's **`ingest-contract`**: the only fixture with a
  measured marginal-fact gap, whose marginal fact is the **secondary** one ("how to run / what
  `make check` validates") — the class the analysis identified as under-served. The checklist
  fixture is **not** usable: its baseline sat at the ceiling (9–10/10) and its batch was
  correctly unspent.

## Tasks

| ID | Specs | Route | Branch / worktree | Commit |
| --- | --- | --- | --- | --- |
| T1 | S1–S7 | worker: the criterion sealed FIRST, the assembly (the standalone extension), the three arms, the runner and the scorer reuse, the pilot — **done** (L4) | `bench/nudge-harness` @ `tmp/wt-bench5` | `54de3be` |
| T2 | S4 | parent: verify the criterion preceded the runs, read the control, run the measured batch, score, report — **the control was read and N-G1 failed, so the batch was NOT spent** (L5) | `bench/nudge-harness` @ `tmp/wt-bench5` | — |

## Log

- **L1 (2026-11-02, the human, verbatim)** — "falta el test de esto", pointing at the passage
  that says the assembly is **not measured**: the firing rate moves from ~2/41 to ~41/41, the
  cost moves from ~zero to one extra request per changed session, and the constraint moves
  from the firing rate to the judgment. The test is this bench.
- **L2 (2026-11-02, the feasibility, verified by the parent)** — the bench image's pi is
  **1.2.0** and it **does** carry the seat: `TurnEndEvent extends BoundaryState` with
  `turnIndex`, `message`, **`toolResults: ToolResultMessage[]`**, `messageEntryId` and
  `toolResultEntryIds` (`types.d.ts:828-835`), `TurnEndEventResult = BoundaryResult`
  (`:1085`), and `on("turn_end", handler)` (`:1215`). The runner treats both boundary events
  alike (`runner.d.ts:37`). So no image rebuild is needed for the seat itself.
- **L3 (2026-11-02, what this bench is worth)** — it is the measurement the program has been
  circling: it decides whether the nudge's problem is the **moment**, the **state** or the
  **rule** — or none of the three. A null N-G2 kills the assembly as it stands and says the
  diagnosis is wrong; a positive N-G2 is the first measured evidence that anything the
  extension says at the right moment, with the right state, changes what survives.
- **L4 (2026-11-02, T1 complete — the criterion sealed, the pilot's four conditions hold, the batch
  is NOT spent)** — the criterion is `tmp/bench/nudge-measurement.md`, **sealed before the first
  session of any kind**: sha256 `68bea3d7e990924a5e1ebe50f228271d053f9724602d5a81ad4db1e774cbbbc6`
  over its first **22 052 bytes** (re-verified after the appendix; no gate moved). The arms are
  `nudge` (the shipped file, 101 776 B, `f292e425…`), `control` (the same file with three inserted
  lines that neuter the settle seat, 101 958 B, `22662d40…`) and `assembly` (control plus the
  standalone `mnexis-assembly` extension, 17 228 B, `1ac30356…`), pinned in
  `tmp/bench/arms/MANIFEST-nudge.txt` and verified by `run/build-arms.mjs --check`. The assembly is
  a **standalone extension** loaded next to the shipped one (S3's preferred shape): the seat is
  `turn_end` gated on the workspace's git state against a session snapshot, one-shot; the payload is
  up to three `old → new` pairs uncovered by the session's saves or the store's records plus up to
  two output lines, 568 B at this fixture; the rule is the output pointer, and the extension audits
  every save after the seat and logs `verified`/`rejected`/`no_quote`.
  **The pilot (10 sessions of arm B, `pl1..pl10`, after a 3-session pre-flight) holds all four S5
  conditions:** fires **10/10** (needs ≥ 4/10); the marginal fact is in the store at close
  **10/10** by the declared strings; the end state is **10/10** (`check` 0, probes 2/2/2, timeout
  1500, ADR 817–1 988 B, `scripts/check.sh` intact); the pointer check ran **12 times (9 verified,
  1 rejected, 2 no_quote)** and its verdicts are in `out/pl<i>-assembly-assembly.jsonl`. 0 NO ANSWER,
  0 refusals, 1 record/run (close excluded), `estado al cierre` 10/10, 8.2 calls/run, $0.002214/run;
  the pilot cost $0.0221 ($0.0289 with the pre-flight).
  **The risk, declared for T2:** the single pre-flight control session recorded the marginal fact on
  its own (1/1), and the trim bench's 1/10 for the same fact under the same tools block does not
  reproduce today — **N-G1's headroom (control ≤ 5/10) is in doubt**, and its pre-registered answer
  is "report, no verdict: the fixture cannot show the gap". The parent may read the control first
  with `bash tmp/bench/run/nudge-bench.sh pilot 10 control` (pilot tags, never gates) before
  spending the 30 measured sessions; the batch command is
  `bash tmp/bench/run/nudge-bench.sh batch 10`, and the scorer is
  `python3 tmp/bench/run/score-nudge.py tmp/bench/out`.
- **L5 (2026-11-02, T2 — the control was read, N-G1 failed, and the batch was NOT spent)** — T1's
  pilot could not read the control (one pre-flight session, and the marginal fact landed in it,
  1/1), so the parent ran the **control pilot** before the measured batch: ten sessions of arm A′
  under the pilot tags, which are pilot records and never gates. **The control records the
  marginal fact in 9 of 10 sessions** — against the ≤ 5/10 that N-G1 requires. The verdict logic
  sealed in the criterion says: *N-G1 fails → report, no verdict, the fixture cannot show the
  gap*. **The 30 measured sessions were not spent.**
  **What the number means, and what it does not.** It is **not** a refutation of the assembly:
  the fixture cannot show the gap, so there is no verdict either way. What it says is that **the
  premise the analysis rested on no longer holds on this fixture** — the affordance class was
  chosen because it had a sub-ceiling baseline, and today the model records it by itself 9/10.
  The reason is visible in the control's own saves: *"scripts/check.sh (make check) valida que
  quede entre p99 y cap"*, with the bounds, and one save quoting the checker's own output line —
  the checklist bench's measured mechanism (the model audits the tool it is asked to trust:
  read 10/10, probed 5/10, recorded 10/10) running on this fixture too. The trim bench measured
  **1/10** for the same fact under the same tools text, so the baseline moved; two differences
  must not be folded together (the trim's 1/10 was a **human read**, this is a **declared
  string** with the near-miss class printed and never counted; and the fixture here is
  `ingest-api`, the trim bench's own, not the checklist worktree's different `ingest-contract`).
  **The class that remains at risk** is the facts the model **cannot audit by reading a tool** —
  what the environment does, what a neighbour expects, a constraint stated in another session —
  which the pointer rule cannot cite and the coverage rule cannot see. A next bench would have
  to change the class **and** the instrument: that is a program decision, not another fixture
  revision.
  **What the pilot did measure about the mechanism** (reported, not gated): it fires **10/10**,
  the payload computes, the pointer check ran **12 times (9 verified, 1 rejected, 2 no_quote)**,
  the end state holds **10/10** in both arms, the tools block stays **5490 B**, zero refusals,
  zero `busy`, zero change-log records, and the cost is **indistinguishable** ($0.002214 vs
  $0.002232 per run; 8.2 vs 8.1 calls per run) — the extra request did not bill as an extra
  call. **The mechanism works mechanically; this fixture cannot tell whether it changes what
  survives.** The parent's independent checks: the seal recomputes to `68bea3d7…` over its first
  22 052 bytes (re-verified after the appendix was appended), the arms' manifest verifies
  (`nudge` byte-identical to the shipped file, `control` three inserted lines, `assembly` the
  standalone extension), and the scorer's numbers are the ones above.

