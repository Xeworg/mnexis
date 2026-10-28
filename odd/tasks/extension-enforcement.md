# The extension that enforces, not the extension that advises

**Feature:** `extension-enforcement` · **Branch:** `feat/extension-enforcement` · **Status:** measured — all six bars hold; the store half is worth productizing and the context half is not
**Board:** `odd/next.md` item 18 (and item 17, M5)
**Bench criterion:** `tmp/bench/extension-recovery-measurement.md` (written before any run)

**Result in one line:** in `rec`, where the dictated title is asserted as the
user's exact request, the advisory arm (`actual`) left **two live records in
10 of 10 runs** and the enforcing arm (`nueva`) left **one in 10 of 10**, for
**+72 tokens per call**; the post-compaction context recovery answered no
question the advisory arm could not answer and cost **+179 tokens per call**.

M5 measured advisory text and found it worthless: `combo` (short skill + 678 ch
of doctrine in the system prompt) tied `head` on behavior and cost +244/+259
tokens per call. But every arm only *suggested*. The shipped extension already
uses the enforcing hooks — it blocks `mnexis config allow-namespace` from the
model and blocks hand edits of the profile files — so the machinery for
enforcement exists in the product and is used today for security only. This
feature measures whether using it for the doctrine and for recovery buys
anything, and prices it.

## Specs

**S1** — Two arms, one image (`bench-mem:whole`), one binary, one skill:
`fat` (18 184 B, the shipped state) in both. **The only difference between the
arms is the installed extension.** N = 10 runs per chain, fresh profile per run.

**S2** — `actual` is the extension as `mnexis agent install --env pi` installs
it: tools plus `session_start`, `session_compact` (which *stores* the summary)
and two security gates in `tool_call`. It injects nothing into the prompt.

**S3** — `nueva` is that same extension plus two automatic recoveries, both
implemented with the hooks the docs expose, and neither depending on the model
remembering a rule:

1. **Store recovery.** On `mnexis_save`, if the save would leave two live
   records for one fact (a twin: same namespace, same value, different title),
   the extension repairs it without asking the model — by superseding the older
   record or by rewriting the save onto the existing title. On `tool_result`, a
   save that already produced a twin is repaired the same way before the model
   reads the result.
2. **Context recovery.** On `session_compact` the summary is already stored.
   `nueva` additionally re-supplies the project's live facts to the next run
   after a compaction, so the fact survives a context that no longer holds it.

**S4** — The chains: `dup` and `cnf` unchanged from batch 2, so a regression is
visible on the same evidence; plus `rec`, built to create the twin the model is
most likely to create (a dictated title asserted as exact), and `cmp`, which
fills the context, compacts, and then asks for the port with no files.

**S5** — The criterion is `tmp/bench/extension-recovery-measurement.md`,
written before the run, and it is not edited to fit a number. A chain missing
any phase is not data. A batch missing an arm produces no verdict.

**S6** — The recovery is automatic and observable: the variant writes one line
per intervention to a file the runner collects, so "it repaired silently" is a
count, not a claim. An arm whose hook never fires is not the new arm.

**S7** — Nothing in the shipped product moves. The variant lives in the bench,
built by patching an installed copy like `patch_extension_with_section` does.
Productizing it is a separate feature with its own spec, its own branch and its
own verification.

**S8** — Cost is measured on the same row as every other batch:
`sum(input + cacheRead) / sum(model calls)` per arm, because a block or a
rewrite costs zero tokens while an injection costs on every call.

## Tasks

| id | what | specs | route | commit |
| --- | --- | --- | --- | --- |
| T1 | the criterion and the two new chains, before any code | S4-S5 | inline | — (bench doc, gitignored) |
| T2 | the `nueva` variant: store recovery and context recovery in the installed extension | S3, S6-S7 | worker | — (bench) |
| T3 | the two arms installed and verified on their real prompt and their hook | S1-S2, S6 | worker | — (bench) |
| T4 | run the batch (2 arms × 4 chains × 10) | S1 | inline | — (bench) |
| T5 | score it and append `## The enforcement arm` to the skill-lean report | S5-S6, S8 | inline | — (bench) |
| T6 | the board and the decision | S5, S7 | inline | — (docs) |

All of T1-T6 are done. T6 delivered the measurement and `odd/next.md` item 18;
the product decisions it names are the human's and are not taken here.

## Log

**L1** — the human's request, verbatim: *"haz una prueba del actual vs una
nueva version que use la skill y la extension de pi y ten en cuenta que el
recovery si lees el documento se puede automatizar ya que nos da control sobre
cada llamada que se hace a la ia"*.

**L2** — asked which recovery, the human chose one arm carrying both:
*"Las dos en un solo brazo"*. That makes attribution coarser and the run
cheaper; the report must say so rather than attribute a win to one mechanism.

**L3** — why this experiment follows from M5 and not from taste. The extension
canonical `docs/extensions.md` (pi 1.3.0) gives control at every model call:
`tool_call` can block or mutate the input, `tool_result` can modify what the
model reads, `before_agent_start` can patch the prompt, `agent_before_settle`
can refuse to let a run end. M5's batch 2 priced only the advisory use
(`appendSystemPrompt`) — +755 tokens per call in batch 1, +244/+259 in batch 2
— and never measured enforcement, which costs no context at all. The task the
batch could not discriminate was the one where the model must get the store
right under pressure; the only two failures it did produce were exactly that
kind (`fat` 1/10 with two live rows, `bare` 2/10 with a wrong value).

**L4** — the batch ran (2026-10-28, 80 of 80 chains, zero empty answers, zero
invalid runs, `cmp` compacted 10/10 in both arms) and all six bars hold:

- **R0** — `actual` wrote 0 intervention lines across all 40 of its runs; the
  arms are distinguishable by evidence, not by name.
- **R1** — `rec`: `nueva` one live record for 8081 in **10/10**, `actual`
  **0/10** (20 live rows over 10 runs, the twin every time). Ten interventions,
  ten repairs, the superseded row kept as history each time.
- **R2** — no regression: `dup` 10/10 against 10/10, `cnf` 5/10 against 4/10,
  and both arms answer the current value 10/10 in both chains.
- **R3** — `cmp`: 8081 answered 10/10 by `nueva`, **and by `actual` too**, which
  reached the same answer by searching the store (`mnexis_context`,
  `mnexis_show`, `mnexis_search`). R3 holds as a bar and not as a difference.
- **R4** — +35 (dup), −2 069 (cnf), +72 (rec), +179 (cmp) tokens per call
  against a bar of +200. The smoke's +712 was single-run noise.
- **R5** — fired where it could: 10 store interventions in `rec`, 10 context
  re-supplies in `cmp`, 1 legitimate repair in `cnf` (two live rows sharing the
  same title and the same 8081 under two topic keys). `dup` was **not
  exercised** — `actual` never left a twin there — and `cnf` is **not
  applicable** by the criterion's own definition.

**What this changes beyond its own bars.** (1) The store half is the only
mechanism in four batches that changed the store at all, and it is cheap
(+72). (2) Batch 2's "the doctrine survives a dictated title" was task-specific
and is corrected: it survives an unfamiliar title and breaks under an
**imposed** one, ten times out of ten. (3) The context half is not worth
shipping on this evidence: it bought no answer and paid +179 per call. (4)
`invented()` produced a third false positive — 2311 is the real line count of
the compaction fixture, quoted while remarking on a summary record, not a claim
about the port.

**Still the human's:** productizing the store recovery (bench-only today, S7 —
it needs its own spec, branch and verification, and the twin rule is a numeric
signature, not a proof of identity), and the two M5 decisions item 17 left open
(ship `fat` or `head`; whether to correct the doctrine scorer's artifacts).
