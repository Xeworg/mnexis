# The skill lean, and the harness that could replace it

**Feature:** `skill-lean` · **Branch:** `feat/skill-lean` · **Status:** shipped — `head` (1 103 B) is the skill now; the fat body is gone (see the Log, L3-L5)
**Board:** `odd/next.md` item 17 (M5)

**Batch 2 closed the `combo` option:** the reduced skill reinforced by the
harness (1 103 B + 678 ch of section) keeps the store perfect (D1 holds,
10/10) but is **not** better than the short skill alone (D3 fails, 10/10
against 10/10) and costs **+244 / +259 tokens per call** against a +200 bar
(D2 fails). It is not adopted. D4's two failures are scorer artifacts,
documented with per-run evidence in the report; they do not touch D2 or D3.

The skill is the biggest thing mnexis makes the agent read (18 184 B, read
in 20 of 20 transcripts). The competitor ships none. Board item 7 already
showed that the **trigger** comes from the skill's *description* (which pi
injects into the system prompt) and that an injected pointer or protocol
buys nothing; what was never measured is whether the **doctrine body** pays
for itself, and whether the extension can carry it with no file at all.

The measurement plan and the criterion live in
`tmp/bench/skill-lean-measurement.md`, written before any arm existed.

## Specs

**S1** — The experiment answers three questions with four arms, one binary,
N = 10, the phases of the previous batches (write, trap, steering):
(a) does the **size** of the body matter (`fat` 18 184 B vs `lean` 4 027 B);
(b) does **any** body matter (`lean` vs `head`, frontmatter plus ~350 ch);
(c) does the **channel** matter (`lean` vs `bare`: the same words as a skill
file read into the transcript, or as a section the extension appends to the
system prompt).

**S2** — `bare` is the arm the human asked for: **no skill installed at
all** — pi's prompt carries no `<available_skills>` entry for mnexis and
there is no body to read — and the guidance reaches the model **only**
through the installed extension, as a `before_agent_start` section written
into `systemPromptOptions.appendSystemPrompt` (never a returned
`systemPrompt`, which forces the whole prompt for the run).

**S3** — The arms differ by the installed assets **only**: one image
(`bench-mem:whole`), one engine, one tool schema. `install-arm-assets <arm>`
writes the skill body (`fat`/`lean`/`head`) or removes the skill directory
and patches the extension (`bare`).

**S4** — The load-bearing measure is the **store**, not only the
transcript: the write phase has to leave five records, all titled, no twins,
in the workspace's namespace. An arm that loses the title rule fills the
store with twins and nothing else would warn.

**S5** — The criterion is written before the numbers and is not edited to
fit one: C1 behavior (9/10 with zero invented values in both measured
phases, and a clean write phase), C2 the lean body saves at least 5 000
median tokens against `fat` in the steering phase, C3 `bare` passes C1 and
is not dearer than `lean`, C4 no arm invents a value or writes outside its
namespace. The cost row is always `input + cacheRead`.

**S6** — The outcome is one of three decisions, pre-registered: the skill
goes and the extension carries the doctrine; the doctrine stays in a file
(ship the lean one if it pays); or the body is load-bearing and the skill
stays, with the number that says so.

**S7** — The documentation follows whatever the numbers say: the skill, the
extension's README, the root README and the board. If `bare` wins, the
skill stops being installed and the extension's section becomes the
contract; if it loses, the board records the cost of the body so the next
cycle does not re-ask.

**S8** — Nothing in the engine moves in this feature: the batch is measured
on the shipped binary, and any product change it earns gets its own branch,
its own spec and its own verification.

## Tasks

| id | what | specs | route | commit |
| --- | --- | --- | --- | --- |
| T1 | the measurement plan and the criterion, before any arm | S5 | inline | — (bench doc, gitignored) |
| T2 | the four arms: fixtures (`fat`, `lean`, `head`, `bare`) and `install-arm-assets` | S1-S3 | inline | — (bench) |
| T3 | verify each arm against the real system prompt before the batch | S1-S3 | inline | — (bench) |
| T4 | run the batch (4 arms × 10 runs × 3 phases) | S1 | inline | — (bench) |
| T5 | score it: the store shape, the two measured phases, the cost | S4-S5 | inline | — (bench) |
| T6 | the report and the decision | S6 | inline | — (bench) |
| T7 | ship whatever the decision says, with its own verification | S7-S8 | inline | — |
| T8 | the doctrine chains: criterion D1-D4, written before the run | S5 | inline | — (bench) |
| T9 | run the doctrine batch (4 arms × 2 chains × 10 runs) | S1, S4 | inline | — (bench) |
| T10 | score it, audit the store per run, append `## The doctrine chains` | S4-S5 | inline | — (bench) |

## Log

**L1** — the human's request, verbatim: *"si haz el experimento pero tambien
quiero que leas esto y hagamos una prueba sin skill usando unicamente
harnesses por medio de una extension de pi https://pi.dev/docs/latest/extensions
lee eso"*.

**L2** — the board's M5, verbatim: *"**M5 — el skill lean, con su propio
experimento.** El skill pesa 18 184 B (290 líneas, 7 secciones) y el agente
lo lee en 20 de 20 transcripts; engram no tiene skill y paga 4 541 input
tokens en la fase de escritura contra nuestros 8 894. Fixture corto YA
existe: `tmp/bench/build-context/short-skill.md` (4 027 B)... Métricas: input
tokens, llamadas a memoria, Y EL STORE (registros por hecho, títulos,
duplicados)".*

**L3** — the batch ran (2026-10-28, 55 min, 4 arms × 10 runs × 3 sessions,
zero empty answers) and is scored in `tmp/bench/skill-lean-report.md`. What
it found, against the criterion written in L1/L2:

- **the body is not read** — 7 of 30 write sessions and **1 of 60 measured
  (trap + steering) sessions**; `fat` read it 0 times in 30. The board's
  "20 de 20" was the *description* pi injects, not the body;
- **the behavior is indistinguishable** — C1 holds for all four arms (trap
  10/10, 3/3 state facts 10/10, clean write 10/10, 0 invented), including
  `head` (350-character body) and `bare` (no skill at all);
- **C2 holds as a number (14 007 tokens) and fails as a mechanism** — the
  gap is 5.0 vs 6.0 model calls, and no skill was read in any steering
  session;
- **C3 fails** — `bare` forces +755 tokens on every model call (prefix
  6 024 vs 5 269) and buys nothing; the file it replaces is free when
  nobody reads it;
- **C4 holds** after one scorer correction (a superseded record is history,
  not a live twin; the two runs that duplicated a fact repaired it
  themselves with `mnexis_supersede`).

**T4, T5 and T6 are done; T7 waits on the human's decision**, which is the
one thing the criterion did not foresee: it asked "ship `lean` if C2
holds", and C2 holds for a reason that is not the body.

**L4** — the second batch ran (2026-10-28, 80 of 80 chains, zero empty
answers) and is scored in the report's `## The doctrine chains`. Against
D1-D4, written in `tmp/bench/doctrine-measurement.md` before any run:

- **D1 HOLDS** — `combo` leaves one live record in the `dup` chain 10/10
  and answers the current value 10/10.
- **D2 FAILS** — `combo` costs **+244 (dup) and +259 (cnf) tokens per call**
  over `head`, against a pre-registered bar of +200. The section's own
  arithmetic was ~170, so it also shifts the call count (median 12.0 vs
  11.0 in `dup`).
- **D3 FAILS, and it could not have held** — `head` already scored 10/10 on
  the same chain, so the reinforcement could at best tie. P1 called this
  before the run: the title rule is carried by the tool description.
- **D4 reads FAILS with both failures verified as measurement artifacts**,
  reported and not corrected (the criterion is not edited to fit numbers):
  the 15 "live stale records" are all the *same* active row recalling the
  old value in its own sentence, and the 1 "invented" value is a
  hypothetical ("como 8080 o similar") written while explaining what could
  not be confirmed. Real twins on `cnf`: 0. Real stale answers: 0. With the
  prose excluded, `cnf`'s one-record column is 10/10 in all four arms.
- **The two store-side findings worth keeping:** `fat` is the only arm that
  ended with two live rows for the port (`ddup-fat9`, 1/10 — and it marked
  the second as an unverified user claim), and `bare` is the only arm that
  wrote a wrong value, saving 9090 in 2 of 10 `dup` runs where no file says
  so — 9090 being the doctrine section's own example number.
- **`head` is equal or better than `fat` on every measure and worse on
  none**: `dup` 10/10 against 9/10, `cnf` equal at 10/10 once prose is not
  counted as a record, and cheaper per call (5 829 / 6 005 against
  6 202 / 7 101).

**The human's idea is answered:** *"reducir la skill y reforzarla con
harness"* is **not adopted**, on two independent grounds (D3: no gain over
the short skill; D2: dearer than its own arithmetic). T7 still waits on the
ship decision, which is the human's — and the two candidates that survive
the measurement are keeping `fat` or shipping `head`, justified by
maintenance and by not being worse, never by tokens.

**L5** — the ship decision, the human's, verbatim: *"si actualiza la skill"*.
It answers the question L4 left standing — keep `fat` or ship `head` — and it
lands **on `head`**, whose bytes were already measured (`head` 1 103 B, `fat`
18 184 B).

**What ships is the measured artifact, byte for byte.** `skills/mnexis-memory/SKILL.md`
is now a copy of `tmp/bench/build-context/skill-head.md` — the arm that ran in
the batch — with nothing edited, so the shipped skill is exactly what scored
10/10 on `dup`, equal on `cnf` and cheaper per call (5 829 / 6 005 against
6 202 / 7 101). The frontmatter is unchanged, so the **description** pi
injects into the system prompt — the only part of the file the model ever saw
— is byte-identical to the one the measurement ran with: the trigger does not
move.

**Why `head` and not `fat`, in one place.** The body was never read (`fat`
0 of 30 write sessions, 1 of 60 measured); the behaviour is the same
everywhere (C1, C4 hold for all four arms, and `head` was the only arm that
beat `fat` on `dup`); the rules live in the tools, not in the file (the
title rule rides `mnexis_save`'s description — P1 registered it before the
run — and the CLI declares its own setup in its own error); and the store
half the fat body was teaching is now repaired by the shipped extension
(PR #99, item 19), while `fat` was the only arm that ended with two live rows for
the port (1/10).

**What is lost, stated plainly.** The fat body carried the operational
recipes — `mnexis init`, the binding trap, the doctor — and no agent read
them in 672 transcripts. The one honest caveat: a body that nobody reads is
free *today*, and a future host that reads it would lose those recipes. The
tools and the CLI's own errors carry the rules they need.

**The verification of the ship (T7).** The file is byte-identical to the
measured arm (`diff` against `tmp/bench/build-context/skill-head.md`); the
frontmatter is byte-identical to the previous version's, so the trigger is
untouched; the binary is rebuilt before the install (the skill ships inside
it, `src/assets.rs:33`), and the installed copy is byte-identical to the
tree in **both** agent directories. What is **not** re-measured is stated
plainly: the shipped combination is `head` plus the extension's store
repair, and the repair was measured against `fat` (item 18) — the two
changes make the store safer, never weaker, but the combination itself was
not run as an arm.
