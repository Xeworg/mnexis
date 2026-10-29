# The value filter — the engine answers "which records carry this value?"

**Feature:** `value-filter` · **Branch:** `feat/value-filter` · **Status:** in progress
**Board:** `odd/next.md` item 19 (the ceiling the store repair declares) and item 21 (this feature)
**Evidence base:** `odd/tasks/store-recovery.md` L5 ("Raising the ceiling (an engine filter on
value instead of a walk) is a separate feature, and it is on the board"), the session cut of
2026-10-29 (the engine map and the criterion C1-C5)

The store repair in the shipped extension finds the twin a save must retire by asking the engine
for a **ranked** answer of the value the twin carries (a page of 100, the engine's own
`MAX_LIMIT`); when that answer comes back saturated it walks the namespace page by page, up to
20 pages (`TWIN_SCAN_MAX_PAGES`), and past that it **declares** that the check did not finish.
This feature gives the engine a filter by value, so the repair asks for exactly the records that
carry the value and the walk — and the ceiling it declares — disappear.

## Specs

**S1** — The filter lives on the surface that already gives exhaustive access: **`list`**, which is
paginated and ordered by id. The surface is `mnexis list --value <N>`.

*Why not `search`:* `search` is ranked, top-N and has no paging; paging a ranking by OFFSET can
duplicate or lose rows between pages when the order is not stable, and the answer it gives is a
ranking, not a set. `list` is a set: ordered by id, with a limit and an offset, and every filter it
applies happens **before** its limit.

**S2** — What a value is: **a run of three or more digits, exactly equal to the number asked
for**. The engine matches a record when one of its maximal digit runs equals the value, so:

| the indexed text | carries `8081`? | why |
| --- | --- | --- |
| `the port is 8081` | yes | the run is `8081` |
| `v8081` | yes | the run is `8081`; the letter is not part of it |
| `80810` | no | the run is `80810`, a different value |
| `18081` | no | the run is `18081` |
| `80` | no | a run of two digits is not a value |

The text that is read is the **indexed text** of the record: its content, its title, its topic key
and its keywords — the same four columns the FTS5 index covers
(`src/storage/sqlite.rs:355`, `MEMORIES_FTS_COLUMNS`). The answer is therefore a **superset** of
the records whose content carries the value, which is the safe direction: a candidate the extension
then rejects is a wasted read, never a missed twin.

**S3** — The filter applies **before the limit**, like every other filter of this engine: "los
filtros se aplican antes del límite para que una respuesta filtrada no sea una respuesta corta". A
namespace with 5 000 records and two that carry the value answers with those two, not with the first
two of the page. The `--offset` pages the **matches**, not the rows the engine scanned.

**S4** — The filter is honest. A value nobody carries is an **empty answer** (exit 0), not an
error. A value that is not a run of three or more digits is an **invalid argument** (exit 2), with
a message that says what a value is: a number is not a value, and a two-digit number is not a
value either.

**S5** — The MCP surface mirrors the CLI: `memory_list` gains an optional `value` argument with
the same semantics and the same validation.

**S6** — The extension's repair asks the engine for the records that carry the value and never
walks the namespace. The answer the model reads no longer carries the declaration that the
duplicate check did not finish, and the 2 000-record ceiling is gone from the extension's
contract, from its README and from the board's item 19.

**S7** — The criterion, written **before** the numbers (measured against the real engine, in a
temporary data directory; it needs no docker and no model provider):

- **C1 — exhaustive.** A namespace of N records of which K carry the value answers with exactly
  those K, paged, with no gap and no duplicate.
- **C2 — the ceiling is crossed.** A namespace of more than 2 000 records whose twin sits after
  the page of 100: the repair finds it, retires it, and the answer does **not** carry the
  declaration that the check did not finish.
- **C3 — the cost follows the matches, not the namespace.** A twin in the 3 000th record of a
  5 000-record namespace is found in **at most 3** engine calls (today: 21, and still failing).
- **C4 — no other surface moves.** Every other command's output is byte-for-byte identical to the
  base commit's, for the same input.
- **C5 — the filter is honest.** A value nobody carries answers empty; a non-numeric or too-short
  argument is a clear error.

**S8** — Test-first: every rule of S1-S6 gets a test that fails without it, through the public
interface (the CLI, the MCP tool, the extension's own tool result), never through internal
storage. The three cases the bench produced stay the regression corpus: the twin that must be
retired, the value change that must **not** be retired, and the shared number with no shared
keyword that must not be touched.

**S9** — Docs move with the behaviour: the engine's README, the extension's README and the board
(`odd/next.md`, item 21) describe the filter, the definition of a value, and the fact that the
ceiling is gone.

## Tasks

| id | what | specs | route | commit |
| --- | --- | --- | --- | --- |
| T1 | the value's definition (domain) and its rule | S2, S4 | inline | _pending_ |
| T2 | the engine filter: repository, service, CLI `list --value` | S1, S3, S4 | inline | _pending_ |
| T3 | the MCP mirror: `memory_list` gains `value` | S5 | inline | _pending_ |
| T4 | the extension asks the engine instead of walking | S6 | inline | _pending_ |
| T5 | the corpus: Rust, JS and the parity suite | S8 | inline | _pending_ |
| T6 | the criterion C1-C5, measured | S7 | inline | _pending_ |
| T7 | independent verification of the data effect | S1-S6 | verify | _pending_ |
| T8 | docs: READMEs and the board | S9 | inline | _pending_ |

## Log

**L1** — the human's instruction, verbatim: *"dale"* — answering the question of the session cut:
*"¿Arranco con el filtro por valor?"*. The scope is the one the cut recorded: the feature that
kills the 2 000-record ceiling the store repair declares.

**L2** — the one place this design departs from the cut's wording, and why. The cut said the
filter matches the value "como token entero — el FTS5 ya tokeniza así, así que '8081' NO matchea
'80810' ni 'v8081'". The first half is kept (`80810` does not match `8081`); the second is not, and
the reason is the measured twin rule itself: the extension's `numericValues`
(`extensions/mnexis-memory/index.js:428`) extracts the digit runs of a content with `/\d{3,}/g`, so
it reads `8081` out of `v8081`. A token-exact filter would answer the engine's question with fewer
records than the twin rule asks for, and the repair would **miss a twin** whose value is written
inside a word. Digit runs are the definition the bench measured (R1) and the definition the
extension still applies to decide a twin, so the engine's filter uses the same one: the answer is a
superset of the twin set, and the extension's own signature comparison still decides the twin.
The cost of the choice: the engine's filter is a Rust-side rule, not a SQL `MATCH`, because a
digit run that is not a token cannot be found with FTS5's tokenizer, and a sound SQL formulation of
"maximal digit run" does not exist in SQLite (the LIKE/GLOB boundary trick matches different
occurrences and reports a false positive — `a1212b` with the value `12`). The engine therefore
narrows in SQL with a sound superset predicate (`LIKE '%value%'` over the four indexed columns)
and decides exactly in Rust.

**L3** — what is deliberately not in scope: the `search` surface (a ranked answer is not a set, and
paging it is unsafe — S1), any change to the FTS5 index or to its tokenizer (public behaviour, and
the search's own contract), any change to the extension's twin rule (measured, and it stays), and
the re-measurement of the door (the other open debt, which needs docker and a model provider).
