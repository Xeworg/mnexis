# The memory types, the session axis, and the prompt type — what the store actually does

**Analysis:** 2026-10-30 · **Base:** `main` = `e5f429c` · **Status:** an analysis, not a feature
· **Method:** ten independent read-only agents (one per memory type, plus the prompt type,
the supersede semantics, and the session/type organisation), each reading the code and
measuring against the shipped binary on scratch stores; the parent's synthesis

The human's question, verbatim: *"engram te hace una sesion por cada vez que lo inicias,
luego te agrupa las memorias en sesiones, esto es claro las memorias mas nuevas tienen mas
relevancia o eso supongo, luego si vez las memorias tienen una estructura y no son muy
grandes, tambien, nosotros ya tenemos tipos de memorias pero que estructura tienen, estamos
manteniendo preguntas especificas para los tipos de memoria?, entonces esa puede ser nuestra
fuga de contexto son memorias mas grandes, tenemos claro etonces nos hace falta un tipo de
memoria prompt y nos hace falta estructurar correctamente las que tenemos"* — and *"analiza
tambien si el superseed pasa todo el contexto o solo el ultimo"* and *"podriamos organizar
por sesion y por tipo de memoria"*.

**The finding that reframes the question:** the premise is right and the order is wrong. The
store's problem is not that the records are too big — it is that **the type axis does not
select anything** (1 197 of 1 198 `mnexis_save` calls in the corpus declare no type), the
**session axis is dead** (156 sessions, 100 % in `default`; 0 of 156 have a non-empty
window), and **supersede carries nothing** (not the whole context, not the last one —
nothing, and the revision trail stores the *new* content where it should store the old).
Structure a type that nobody writes and the structure is invisible.

---

## 1. The type axis: seven kinds, one behaviour

`src/domain/memory_type.rs:35-43`. The type is validated on write
(`resolve_memory_type`, `:63`), filtered in SQL **before the limit**
(`memory_repo.rs:304-311`, `fts5.rs:162-167`), re-checked in memory in the door
(`context.rs:401-404`), and **that is all**. It is never ranked (the tier was measured and
retired, board item 20: +35 % for a 10/10 the shipped engine already gave), never repaired
(the doctor's eight checks never read it, `doctor.rs:86-95`), and it has no `CHECK`
constraint on the column (`sqlite.rs:56`) — so `mnexis import` can write a type outside the
set (`io.rs:112,166` copies it verbatim, `service.rs:806-807` never validates it), and such a
record matches no filter and no state block.

**The axis does not select.** For one and the same fact — "the default deploy environment is
staging-eu" — across 149 writes: `fact` 108 (72 %), `decision` 21 (14 %), `knowledge` 16
(11 %). Across 1 316 transcripts, **1 197 of 1 198 `mnexis_save` calls declare no type**. The
`fact` bucket *is* the untyped writes: 757 of 985 unique corpus records, 73 % of the corpus's
content bytes. Hand-classifying the 37 live `fact` records against the set's own
definitions: knowledge 20, episode 10, project_state 5, decision 1, procedure 1, **fact 0**.

**Per type, what the engine does beyond the shared behaviour:**

| type | read specially by | behaviour beyond the label | live records (real store) | median content |
| --- | --- | --- | --- | --- |
| `knowledge` | nothing | — | 15 | 1 472 B (max 3 213) |
| `decision` | the state block (`session_state.rs:45`) | the only type with a "does it still stand" answer (`state`) | 5 | 1 152 B |
| `preference` | nothing | — | 1 | 384 B |
| `procedure` | nothing | the string appears **once** in the whole Rust tree (`memory_type.rs:39`) | **0** | — |
| `project_state` | the state block | the block's query and its 2 048-token budget | 35 (28 live) | 1 930 B (max 5 981) |
| `episode` | nothing (no engine path reads it) | a label | 3 | 1 507 B (max 1 900) |
| `fact` | nothing | the **default**: a save with no declared intent | 37 | 784 B (max 3 484) |

**What each type cannot answer today** (the agents' answers, condensed):

- `knowledge` — **Q3: "is it still true?"** The columns `valid_from`/`valid_until` exist
  (`sqlite.rs:64-65`) and **no read path enforces them**: `filter_sql` has no validity
  clause, the door's check reads `valid_from` only if a caller sets `valid_window`, and
  **nobody does** (`cli/commands.rs:1427-1430`). Staleness is handled instead by the
  same-title upsert: identity is the title, so 8081→9090 is one record at revision 2.
  `knowledge` needs its validity **wired**, not a new column.
- `decision` — **Q3 (what did we reject), Q4 (when it was decided, as distinct from when it
  was written), Q5 (who decided)**. The reason is usually present in prose (6 of 7
  agent-written decisions) but there is no field for it; the alternatives are prose too
  (1 of 7); the date is `created_at` (the write time); the decider is not recorded —
  `actor` is the *surface* ("cli", "mcp") and `assertion_basis` defaults to `user_stated`
  for agent writes, so **an agent's own inference saved through pi is filed as "the human
  said it"** (`cli/commands.rs:217-219`; the extension never sends `--basis`).
- `preference` — **"who may write one, and is it still in force?"** The two columns that
  could answer it (`actor`, `assertion_basis`) are identical on the one live pair: the
  board's "architecture direction" record was typed `decision` while its own keywords say
  *"preferencia"*. The shape does not hold the difference; only the writer's choice does.
- `procedure` — **Q1 (how do I do X)** is not addressable: one free-text blob, no step
  identity, so "step 3" cannot be fetched alone and the whole record must be re-read and
  re-paid (the re-read rate is 44.7 %). Nothing makes a record a procedure rather than a
  fact about a process: the type is a promise nothing keeps. There is no procedure-shaped
  data in the project today (0 live; the bench's one record is 71 B and is a *fact* about
  the process).
- `project_state` — **"where were we three sessions ago?"** `memories` has no `session_id`;
  the recall is a *time window*, not a link (`session_recall.rs:11-17`); and the block
  carries only the previous session's summary, of the same namespace — which is never
  populated (§3). Also: **the state before the last update is lost** — the history row
  carries the *new* content, so `mnexis show <uuid> --revision 1` → NOT_FOUND (measured).
- `episode` — **"what happened in the last session?" and "what happened when we broke X?"**
  Neither is answerable: no link, and the session machinery answers them **without** the
  type (4 of 154 sessions have a summary; the extension never writes one; `user_prompts`
  holds 1 row store-wide). The agent itself cannot place an episode: *"Maybe worth
  recording the episode: repo checkout empty… **That's project state**"* (a bench
  transcript).
- `fact` — **there is no question only `fact` can answer.** It answers "what is true?"
  with nothing else to say, and it is the absence of a type, not a type. 16 of the 37 live
  facts have no title; 24 exceed 512 B; the 10 largest carry 55 % of the bucket's bytes,
  and they are session-close narratives and feature retrospectives.

## 2. Where the context leak actually is

The human's hypothesis — "son memorias mas grandes" — is **right about the symptom and
wrong about the cause**:

| where the bytes go (the trigger phase, the merged arm) | bytes | share |
| --- | --- | --- |
| the door's answer, whole | 3 530 | 100 % |
| ...of which the records' **content** | 825 | **23 %** |
| ...of which the **envelope** (uuid, title, keywords, timestamps, topic key, per item) | ~2 705 | **77 %** |

And in the real store, the **session-state block costs more than the facts it accompanies**:
`mnexis context` in the `mnexis` namespace = 3 068 tokens for the block against 1 101 for
the items — **2.8×**. 28 live state records: **3 resident** (1 199 of the 2 048 tokens),
22 indexed (identity only), and **3 dropped in silence** by the block's 25-candidate cap
(`session_state.rs:56,64`) — absent from the records, from the index, and from any
declaration. The binding limit is the candidate cap, not the token budget.

So the leak is three leaks, and only the third is "memories too big":
1. **the envelope** — 77 % of the answer's bytes are the record's own metadata;
2. **the block** — 2.8× the items, with a silent drop at the cap;
3. **the records** — median 784-1 934 B, max 5 981, against a global cap of 16 384 B
   (`service.rs:46`), with no per-type bound.

The agents' proposed **512 B per type** is a **contract, not a measurement**: it is derived
from the door's 4 096-byte budget. And it is a **store-wide** change, not type-scoped:
**76 of 98 live records already exceed it**.

## 3. The session axis: dead, and cheap to revive

**Measured on the real store (read-only):** 156 sessions, **100 % in the `default`
namespace** (81 mnexis directories, 73 Nodexar). Memories: 83 `mnexis`, 12 `nodexar`, **1
`default`**. The cause is one line: `start_session` → `target_namespace(None)` →
`scope.default_namespace` (`service.rs:993-999`), while `mnexis save` **does** resolve the
workspace's namespace (`cli/commands.rs:82-114`). The extension calls `sessionStart` with
no namespace (`index.js:1280`) although `lib/cli.js:186` accepts one.

**Consequence, measured:** `mnexis recall --previous` from the workspace answers
`NOT_FOUND`; from an unbound cwd it answers with the last session of *whichever project
last ran*, with an empty window; **0 of 156 sessions have a non-empty window**; and the
door's `session_state.session` is **null** for every namespace. The two axes do not merely
fail to join — **the join is dead**: every session in the store recalls "none".

**A per-session organisation of the state is not free either:** 22 sessions hold state, so
a snapshot per session would add 22 rows to 28 (+79 %) to the very pool the block has to
choose from — and the block already drops 3 of 28 silently.

## 4. The prompt type: the table exists, nothing writes to it

`user_prompts` (migration 004) has the session link, the namespace, the FTS index and the
triggers (`sqlite.rs:210-238`); the engine has `save_prompt`/`search_prompts` (CLI, MCP and
a pi tool); the recall carries the prompts **whole** (`session_recall.rs:44,52-72`,
`RECALL_LIMIT=100`). And yet `mnexis_search_prompts` answers **"No prompts stored yet."**:
the extension registers no hook that sees the user's message (only `session_start`,
`session_shutdown`, `session_compact`, `tool_call`), and the shipped skill never mentions
the word "prompt", so the model is never told the tool exists.

> **Correction (2026-10-30, after A2).** The line above is true of the extension's
> *registrations*, not of pi's API: pi **does** expose an `input` hook — "Fired when user
> input is received, before agent processing", emitted *before* skill/template expansion
> (`dist/core/extensions/types.d.ts:909-919`, `dist/core/agent-session.js:1622`). It is the
> only hook that carries the human's text before the expansion. The missing piece was the
> **registration**, not the capability.

The human's premise about recency is **false in this engine**: the search is bm25 only
(`fts5.rs:170`), the door's order is bm25 ASC with the tie-break **`created_at` ASC — the
oldest first** (`context.rs:414-425`). Newest-first exists in *lists* and in the block's
fill, which is an enumeration, not a rank.

Should `prompt` be a type? The agents' verdict: **no** — as a type it inherits the identity
machinery (title→topic key, revision, state, supersede, the doctor's missing-title repair)
that a prompt has no use for, and a 16 KB prompt on the door is a per-session tax. It
belongs as the row it already is, with **a writer that does not need the model**.

## 5. Supersede: the answer to the human's question

**It carries nothing — not the whole context, not the last one.**

`supersede` copies **no** content, title, topic key or revision into the new record
(`service.rs:564-608`; `memory_repo.rs:788-874`): it flips the old row to `superseded`,
bumps **its** revision, and writes one `memory_links` row with the reason. The new record
keeps its own title, its own topic key and revision 1.

And the trail is empty or wrong:
- **on the supersede path**: no `memory_history` row is written at all (the transaction's
  only INSERT is the link). Measured: `history <uuid>` → 0 rows.
- **on the update path** (the same title re-saved): the history row carries the **new**
  content, so **revision 1 exists nowhere** — `show <uuid> --revision 1` → NOT_FOUND
  (measured), and the old content is gone from the database.
- **the design doc contradicts the code**: `odd/tasks/e2-core.md:163-165` says every write
  appends a history row capturing the **previous** state; two test names assert the
  opposite of what they check (`tests/application_service.rs:766-789` is called
  "the_previous_content_stays_in_the_history" and asserts the *new* content).
- **the trail pins the rows it describes**: `memory_history.memory_uuid REFERENCES
  memories(uuid)` has no `ON DELETE CASCADE` (`sqlite.rs:86`), while `purge` deletes
  `memories WHERE deleted = 1`. Measured: a soft-deleted record with history rows cannot be
  purged — `BUSY: FOREIGN KEY constraint failed`, exit 10.

**The cost of "the new record carries the whole context"**: the door re-pays the trail on
every read — a 400 B record grows the answer from 742 B to 1 126 B (+384 B, +95 tokens per
400 carried bytes), and at revision 5 that is 5× the content in every answer, against a
door median of 1 542 tokens. The type axis was retired precisely because only *removing*
content helps.

**The cheap fix, the evidence supports it**: write the OLD record's pre-supersede state
into `memory_history` on the supersede path (one INSERT, no migration — every column
exists), and fix the FK pin so `purge` works. It costs **bytes, not tokens**: the trail is
not in FTS, and the door carries the active rows only.

## 6. The sequence the evidence supports

**Phase 0 — no model, no migration, no bench; every point is measurable on a scratch store
with the shipped binary.** These four are the cheap ones, and two of them fix things the
human asked about by name:

| # | change | why first | criterion (written before the numbers) |
| --- | --- | --- | --- |
| **A1** | **the session goes to the workspace's namespace** (`mnexis session` resolves it like `save` does; the extension passes it — `lib/cli.js:186` already accepts it) | the session axis is dead (0 of 156 windows non-empty); the fix is ~10 lines | `recall --previous` from a bound workspace returns **its** finished session, the window holds the memories saved in it, and the door's `session_state.session` is non-null. **Today: 0/3.** |
| **A2** | **a capture of the user's message that does not need the model** (the `session_compact` hook is the precedent) | `user_prompts` is empty; the human's verbatim decisions survive only paraphrased | 10 fixture prompts → 10 rows, byte-identical, each with the session's id; 0 model calls; with the capture off, the row count does not move |
| **A3** | **the supersede writes the OLD content into the trail**, and the FK stops pinning the rows it describes | the revision trail is empty on supersede and inverted on update; `purge` is BROKEN on any record with history | `show <old> --revision 1` returns the old content byte-identical; `history <old>` ≥ 1 row; `show <new>` byte-identical before/after; the store grows ≤ old content + 150 B; **the door's answer byte-identical**; `purge` succeeds |
| **A4** | **a warn-level doctor check `type_shape`** | the doctor is the only surface that can answer "how much of the store is unclassified?" | it counts the live `fact` records and their byte share, names them, and the run stays `healthy` (a warning never fails the store) |

**Phase 1 — with the model, and only after Phase 0:**

- **B1 — make the agent write the type.** The measurement that decides whether the whole
  "structure the types" programme is admissible at all: *the same fact keeps the same type
  in ≥ 8/10 runs* (today ≤ 5/10), *and the other facts are not typed the same way*
  (≥ 8/10), *and the median content of a typed record is ≤ 256 B*. **If the type is not
  written, no per-type rule is admissible** — the structure would only be visible through
  free-text search, which already answers the steering question 10/10 for a median
  29 493 tokens.
- **B2 — the per-type cap**, only if B1 holds: 512 B of content per type, with the
  doctor's check and a repair plan for the 76 records that already exceed it. Note the
  cap is a **store-wide** change, not a type-scoped patch.
- **B3 — the door organised by type** (the type selects what the answer carries), only if
  B1 holds — with the block's silent drop fixed first (the 3 records the cap discards).

**Phase 2 — the prompt type as a type**, only after A2: with the prompts actually captured,
the question "does the door carry them, and how are they budgeted?" has an answer. The
recall's unbudgeted 100 × 16 KB is a pricing hole that exists today.

## 7. What the evidence does NOT settle

- **That a structured record serves the agent.** Nothing in the corpus shows an agent
  asking *why* a decision was made. The steering phase already answers "¿por dónde
  quedamos?" **10/10** with a 70 B, reason-free decision. The case for restructuring rests
  on the human's premise, not on a measured deficit in the bench.
- **That a bigger `procedure` is better.** The store holds **zero** procedures; the only
  one that exists is 71 B and is not a procedure. The 2 048 B an agent proposed is derived
  from the door's budget — a contract, not a measurement.
- **That a per-session organisation helps.** The measurement that would show it needs A1
  first (today the recall answers "none" for every session in the store).
- **What Engram's session model actually is.** This repository has **no measurement of
  Engram's session grouping**; what it measured is Engram's retrieval cost (1 851 input
  tokens and 2 calls against mnexis's 4 111 and 8) and the fact that Engram's store holds
  one consolidated observation where mnexis holds six records read one by one. The
  human's premise about Engram's sessions is, in this repository, **unverified**.

---

## 8. The order (the parent's call, from the evidence above)

**A1 → A2 → A3 → A4.** The order is not by size but by **dependency and blast radius**:

1. **A1 first** — it is the cheapest (≈10 lines, no migration, no bytes) and it revives a
   whole dead axis. It is also a **precondition for A2**: if the session keeps living in
   `default`, the captured prompt would carry a foreign `session_id` and a foreign
   namespace — the prompt would be captured and still unreachable.
2. **A2 second** — it depends on A1 (same namespace, same session id) and it is the
   capture of the human's verbatim words, which is the store's emptiest promise.
3. **A3 third** — it changes what the trail *means*, which moves `mnexis history`,
   `mnexis show --revision`, the TUI panel and the MCP `memory_history`: four pinned
   consumers, a differential and the most review of the four. It also fixes two real
   defects (the lost revision 1 and the `purge` that fails with `BUSY`).
4. **A4 last** — it is a read-only instrument for a criterion that does not exist yet. It
   belongs with the Phase-1 feature that writes that criterion, not before it.

**A1's criterion (written before the numbers, deterministic, no model):** from a bound
workspace, `mnexis recall --previous` returns **its** finished session, its window is
**non-empty** and holds the memories saved in it, and the door's `session_state.session`
is non-null for that namespace; a session started from an unbound cwd still lands in the
profile's default namespace; every other surface answers byte-identically against the base
commit `e5f429c`. **The measurement today: 0 of 3.**
