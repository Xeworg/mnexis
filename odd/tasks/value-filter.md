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
walks the namespace. The **2 000-record ceiling over the namespace's records** is gone from the
extension's contract, from its README and from the board's item 19: the engine's answer is
exhaustive, so the repair reads the matches and nothing else.

*Amended after the corpus was written (see L6):* the first wording of this spec said the answer
"no longer carries the declaration that the duplicate check did not finish". That was too strong,
and the worker's own corpus found where: the declaration is still **right** when the check really
did not finish (the engine refused, or more than 2 000 records carry the same value — a bound over
the matches, not over the namespace). What is gone is the ceiling that made the declaration
routine. The declaration stays for a real failure, as the store recovery's own S4 demands.

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
| T1 | the value's definition (domain) and its rule | S2, S4 | inline | `7fb3f6f` |
| T2 | the engine filter: repository, service, CLI `list --value` | S1, S3, S4 | inline | `7fb3f6f` |
| T3 | the MCP mirror: `memory_list` gains `value` | S5 | inline | `7fb3f6f` |
| T4 | the extension asks the engine instead of walking | S6 | inline | `32b16b3` |
| T5 | the corpus: Rust, JS and the parity suite | S8 | inline + worker | `7fb3f6f`, `32b16b3`, `6b9ac36` |
| T6 | the criterion C1-C5, measured | S7 | inline | `tmp/bench/value-filter-measurement.md` |
| T7 | independent verification of the data effect | S1-S6 | verify | L8 |
| T8 | docs: READMEs and the board | S9 | inline | `8100dc6` |

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

**L4** — the slice boundary, recorded as the harness asks. The engine slice was **753 authored
lines in one commit** (`7fb3f6f`) and the extension slice **418** (`32b16b3`), over the ~400 the
workflow aims at, and neither could be split: the repository's method grew a parameter, so the
trait, the storage adapter, the service, the CLI and the MCP mirror travel together (a tree with
half of them does not compile, and a commit that does not compile is not a reviewable slice); and
the extension's slice is the high-risk item (it changes what a save supersedes), which the
workflow asks to travel alone so a native review can take it by itself. The docs are the third
commit.

**L5** — the criterion, measured (S7): `tmp/bench/value-filter-measurement.md`, against the real
engine in a temporary data directory, no docker and no model provider.

- **C1**: a namespace of 2 752 records with three that carry `8081` answers with exactly those
  three — one page, no gap, no duplicate.
- **C2**: the twin sits at row **2 501**, past the 2 000 the old walk stopped at, and the filter
  finds it in the **first page**.
- **C3**: **one** engine call finds the twin; the old path was one ranked search plus up to
  twenty `list` pages, and past 2 000 records it still had not found it.
- **C4**: 18 commands × 2 modes = **36 comparisons, 36 identical** against the base commit
  `9f129c1` (stdout, stderr and exit code, byte for byte).
- **C5**: a value nobody carries answers empty (exit 0); `80`, `abc` and `8a81` are refused with
  exit 2 and the rule in the message.

**L6** — the corpus found an overstatement in S6, and the correction is recorded here rather than
quietly applied. The first wording said the answer "no longer carries the declaration that the
duplicate check did not finish". The declaration is still **right** when the check really did not
finish: the engine refused (the store recovery's S4 demands the declaration), or more than 2 000
records carry the same value — the extension's `TWIN_SCAN_MAX_PAGES` still bounds the pages of
**matches** it reads, and it says so. What the change removes is the ceiling over the namespace's
records, which is what made the declaration routine. S6 is rewritten to say exactly that, and the
extension's README says the same.

**L7** — the corpus (the JavaScript half, delegated and reported): the fake engine mirrors the
value filter (maximal digit runs, the filter before the limit and the offset, the engine's own
`INVALID_ARGUMENT` envelope), the three tests that pinned the ranked search and the namespace walk
were rewritten for the new contract, and the race the corpus reproduces moved from `show` to
`supersede` — the repair reads whole records with `list --full` and never calls `show` any more.
The worker also recorded the RED evidence against a scratch copy of the previous production code
(five failures, exactly the new and rewritten tests), and the GREEN counts: **160 tests, 158 pass,
0 fail, 2 skipped** (the opt-in parity pair), and the parity suite **2/2** against the real binary.

**L8** — the independent verification (T7, `gentle-ai-verify`, read-only, against the **real**
binary and the base commit built in a scratch copy): **every claim HOLDS**, no blocking defect.
Its verdict, per claim: the maximal-digit-run rule over the four indexed columns (a tag is not
indexed text, so `8081` as a tag does not match); the filter before the limit and the offset paging
the matches; the empty answer and the `INVALID_ARGUMENT`; the MCP mirror over raw JSON-RPC; the
repair asking the engine for the value's records (no `search`, no walk) and the ceiling case by
hand (2 500 fillers, the twin at row **2 501**, retired, `recovery.source = "value"`, and **no**
"duplicate check did not finish"); and the twin rule's boundaries (a value change, a shared number
with no shared keyword, another namespace, `archived`, `proposed` and a compaction summary all stay
untouched, while **both** genuine twins are retired). Its failure-mode hunt found no false
negative, no retired non-twin and no false claim. It also ran a wider differential than this
document's C4: **53 command comparisons** against the base (`41` read + `12` write), all
byte-identical in stdout, stderr and exit code; the only difference is `list --help`, which now
names the new flag.

Its advisories, and what I did with them:

1. `TWIN_SCAN_MAX_PAGES` still bounds the pages of **matches** the extension reads (2 000 records
   carrying the *same* value), and S6's first wording overstated that. S6 is amended (L6) and the
   extension's README and the board say the bound honestly.
2. `--value -8081` is refused by clap as an unknown argument instead of with the rule's message.
   It is not a digit run either way; left as it is.
3. `list --value` without `--state` returns every state, which is `list`'s own semantics, and the
   twin rule checks the state anyway.
4. A transient failure of `tests/assets_fidelity.rs` while the README was edited during the run:
   the test embeds the extension's README with `include_str!`, so the binary must be rebuilt after
   a doc change. Green after the rebuild.

The verifier also left three durable probes. The two Rust ones are in
`tests/cli_commands.rs` now (`list_by_value_reads_every_indexed_column_and_the_maximal_run_rule`
and the strengthened refusal), because they pin the contract over the four columns and the rule's
negative cases. Its parity probe (2 500 records through the CLI) was **not** added: it would add
minutes to an opt-in suite to prove what the 121-match parity test already proves about the paging
and what the bench measures about the namespace size.

