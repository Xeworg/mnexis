# The door's answer: priced items and declared coverage

**Feature:** `door-priced-answer` · **Branch:** `feat/door-priced-answer` ·
**Status:** in progress

The re-read metric (board item 12) measured where an agent's context is
wasted: **the door already carried the facts in full and the agent went
looking for them anyway** — `workspace → context → list --full → search`, and
the `list --full` re-delivered 8 of the door's 10 records. Two moves follow
from that measurement, and both are the **door's contract**, not the skill's:
price what the door carries, and declare what it left out.

## Specs

**S1** — The door's answer carries identity and a preview, not the whole
content. From the board (item 12), verbatim:

> Concretely: **M3** (identity + preview + uuid instead of the whole content,
> the handle being `mnexis show <uuid>`)

The item carries what identifies the fact — `uuid`, `title`, `keywords`,
`revision`, `namespace`, `memory_type`, `state`, `topic_key`,
`assertion_basis`, `created_at` — and a **preview** of its content. The whole
content is one call away (`mnexis show <uuid>`), and the caller can ask for it
in the same answer (`--full` on the CLI, `full` on the extension's tool).

**S2** — The answer declares what it did **not** include. From the board
(item 12), verbatim:

> **M4** (the candidates that did not fit, with their sizes)

The declaration names the candidates the door held and left out, with their
size, so the agent's cross-check round has something to look for instead of
rebuilding the answer by hand. A declaration the door cannot back with a
count is a promise; the answer says how many candidates it saw, how many it
carries and how many it left out.

**S3** — The door keeps the **most relevant** facts that fit the budget. This
is the module's own contract (PRD §9 step 6), verbatim from
`src/application/context.rs`:

> 6. Select memories that fit the budget, in deterministic order (bm25 ASC,
>    then created_at ASC).

The implementation sorted by `created_at` alone and dropped the rank, so the
budget filled with the **oldest** facts and the most relevant one could be
left out. A door that declares what it left out must leave out the *least
relevant*, not the newest.

**S4** — The preview is the same rule the other surfaces use
(`src/application/preview.rs`): **160 characters**, the cut marked with `…`,
one physical line (no newline in the answer).

**S5** — The surfaces render what the engine answers: the CLI's text mode
prints the priced items and the declaration, and the pi extension's
`mnexis_context` renders the same. A payload the contract promises and no
surface delivers is a payload that does not exist (the state block's own
defect).

**S6** — The skill and the extension's README say what the door answers
now. A doctrine that describes the old shape is a lie the agent reads.

**S7** — Nothing else moves. Every other read surface stays byte-identical
against the base commit; only the door's two modes change.

## Tasks

| ID | specs | route | commit |
| --- | --- | --- | --- |
| T1 | S1, S4 | inline | `feat(context): the door's items carry a preview, and --full asks for the rest` |
| T2 | S2, S3 | inline | `feat(context): the door declares what it left out, and keeps the most relevant` |
| T3 | S5 | inline | `feat(cli,extension): the door's text mode and the extension render the priced answer` |
| T4 | S6, S7 | inline | `docs(skill): the door answers with previews and declares its coverage` |

## Log

- **L1** — User: "por donde quedamos" → then, offered the board's order
  (M3 the door first, then M4, then the lean skill, then item 11) and
  asked "Merge the PR #91 and start with M3, or do you prefer something
  else?" — User: "si dale".
- **L2** — PR #91 (the board after #89) merged: `29b7505`. `main` before
  this feature: `29b7505`.
- **L3** — The defect found while designing S3: the door sorts by
  `created_at` alone and drops the bm25 rank, so the budget keeps the
  oldest facts and the most relevant one can be left out — the opposite of
  the module's own documented order (S3) and of PRD §9 step 6. Observed on
  the installed binary: a record saved last and ranked #1 by
  `search --explain` came **second** in the door's answer.
- **L4** — S2's "with their sizes": the size declared is the **content's
  length in bytes**, the number that explains why the candidate did not fit
  the budget. The declaration is capped and says how many it left out.
- **L5** — **T1 done** (S1, S4). The item carries `preview` (the same
  160-character rule `preview.rs` uses, the cut marked with `…`) and its
  `content` only when the caller asks for it (`--full` on the CLI). The
  text answer prints the preview, and the uuid — the handle to pull the
  whole record — **only when the preview was cut**, so the cheap case stays
  cheap. RED observed first: the unit test failed with the whole content in
  the item and no `preview` key; the CLI test failed because `--full` did
  not exist. GREEN: 8 context unit tests, 135 CLI integration tests, 16
  tokenizer tests. `cargo clippy --all-targets` clean, `cargo fmt` clean.
  The four tests that pinned the old shape were updated to the new
  contract (`a_context_item_carries_the_title`,
  `a_context_item_carries_the_words`,
  `context_exact_mode_returns_items_and_pins_a_07_invariant`).
- **L6** — **T2 done** (S2, S3). The envelope carries a `coverage` object:
  `candidates` (what the door saw), `carried` (what it carries), `left_out`
  (how many the budget cut) and `left_out_items` (up to 10 of them, with
  `uuid`, `title`, `memory_type` and `bytes` — the size that explains why
  the fact did not fit). The order fix: the rank now travels with the
  memory, and the door sorts **bm25 ASC, then created_at ASC** (PRD §9 step
  6), so the budget keeps the most relevant facts and the declaration names
  the *least* relevant ones.
  RED observed first (the field did not exist: nine compile errors naming
  `coverage` and `COVERAGE_ITEM_LIMIT`). GREEN: 10 context unit tests, the
  whole Rust suite (710 tests), and the new CLI test
  `the_doors_answer_declares_what_it_left_out` (4 facts, a 400-token
  budget: the answer accounts for every candidate and names the ones it
  cut).
  A defect the test caught: when **nothing** fit, the answer was empty and
  had declared only the first candidate — now every candidate is declared,
  because an empty answer still says why.
