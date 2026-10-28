# The state block, budgeted and decayed — and delivered to the agent

The session-state block rides the door (`mnexis context`) since the
call-budget ascension: the `project_state` and `decision` records of the
namespace **with their whole content**, plus the previous session's summary.
It is the one answer that replaced "where did we leave off" with a single
call. Two things are wrong with it, and the re-read metric (item 12 of the
board) named both: the block's **resident mass never decays** — 4 594 tokens
in this namespace, unbudgeted, growing with every session — and **it does
not reach the agent** in the surfaces the agent actually reads.

## Specs

- S1 (the user's go-ahead, 2026-10-27, verbatim): "arma m1" — the state
  block's package: the mark, the budget, the decay, and the delivery.
- S2 (the user's decision, 2026-10-27, verbatim): "Excluye en silencio" —
  the block carries only the **live** records and says **nothing** about
  what it left out. No `omitted` list, no count of what was dropped.
- S3 (the user's decision, 2026-10-27, verbatim): "La forma + la entrega" —
  the block's shape changes **and** it is delivered: the pi tool renders it,
  and the CLI's text mode prints it. A block nobody reads is not a change.
- S4 (the measured premise). The block's resident cost, with the shipped
  tokenizer (`token_count_v1`) over the `mnexis` namespace's nine
  `project_state` records and two `decision` records: **4 594 tokens**, of
  which the newest record alone is 838. The block is **outside** the door's
  budget by design (the call-budget experiment: it must not move the
  budgeted items' selection), so nothing bounds it. The re-read metric
  (`tmp/bench/reread-report.md`) found the door's answer is re-read in
  **40–47 %** of the steering phase's deliveries, and that the agent's
  enumeration round is a **coverage** problem, not a trust problem.
- S5 (the shape of the block, exact). `SessionState` becomes:

  ```rust
  pub struct SessionState {
      /// The live records that fit the block's budget: whole content,
      /// newest first.
      pub records: Vec<StateRecord>,
      /// The live records that did not fit: identity and title, no
      /// content. The agent sees that they exist and pulls one with
      /// `mnexis show <uuid>`.
      pub index: Vec<StateIndexEntry>,
      /// The budget the resident part may use, in tokens.
      pub budget: i64,
      /// What the resident part costs, in tokens.
      pub resident_tokens: i64,
      /// What the whole block costs (resident + index + session), in
      /// tokens. This is what the agent pays.
      pub tokens: i64,
      /// The session before this one: its identity, when it ended, and its
      /// summary.
      pub session: Option<SessionSummary>,
  }

  pub struct StateRecord {
      pub uuid: String,
      pub memory_type: String,
      pub title: String,
      pub content: String,
      pub created_at: String,
      /// The record's lifecycle state. The block carries only the live
      /// ones, so this is `active` or `disputed`.
      pub state: String,
  }

  pub struct StateIndexEntry {
      pub uuid: String,
      pub memory_type: String,
      pub title: String,
      pub created_at: String,
      pub state: String,
  }
  ```

  Every new field is **additive**: `records` keeps its name, its order and
  its members' names (`StateRecord` only gains `state`), and `session` is
  untouched. No field is removed and no field changes its type. The JSON
  envelope of `mnexis context` therefore keeps every key it had.
- S6 (the policy, exact).
  1. **Live states:** `active` and `disputed`. Excluded: `superseded`,
     `archived`, `proposed` — the history, silently (S2). The state filter
     is in the SQL, never in the service: a filter applied after the query
     would let superseded records crowd the live ones out of the limit.
  2. **The budget:** `SESSION_STATE_TOKEN_BUDGET = 2048` tokens, a
     constant of `src/application/session_state.rs`. It governs the
     **resident part only** — the index is the declaration of what did not
     fit, and the session summary rides with it. The block's budget is its
     own: it must not move the door's budgeted items, which is the
     property the call-budget experiment measured.
  3. **The fill:** the candidates are the live records of the namespace,
     newest first, capped at **25** (`STATE_RECORD_LIMIT`); each candidate
     is added to the resident part, with its whole content, while the
     resident part's token count stays within the budget; the first
     candidate that does not fit, and every one after it, goes to the
     index. The fill is greedy, deterministic and newest-first: the
     recursion's order is the decision, and a decision that depends on the
     budget is not reproducible without one.
  4. **The counts:** `resident_tokens` is the token count of the resident
     part's serialised form; `tokens` is the token count of the whole
     block (the resident part, the index and the session summary). Both
     use `token_count_v1`, the same estimator the door's budget uses.
- S7 (the delivery, exact).
  1. **The CLI's text mode** (`mnexis context "<task>"` without `--json`)
     prints the answer, not a header. The header line and the read-scope
     line stay exactly as they are; after them:

     ```
     selected 2 items (tokens=997, namespace=mnexis)
     1. [<topic_key>] <title> — <content, folded to one physical line>
     session state: 5 records, 1922 of 2048 tokens
     - [<memory_type>] <title> — <content, folded to one physical line>
     indexed: 6 records did not fit the budget (pull one with mnexis show <uuid>)
     - [<memory_type>] <title> — <uuid> (<created_at>)
     session: <id> ended <ended_at> — <summary, folded to one physical line>
     read scope: restricted (this profile may not read every namespace)
     ```

     The folding uses the existing `one_line` helper of
     `src/cli/output.rs` — one physical line per fact, always. A title that
     is empty drops its `— <title>` part; a topic key that is empty drops
     its bracket. The index's lines carry the **full uuid**: `mnexis show`
     takes an exact uuid, so a short one would be a dead end.
     The `indexed:` heading is printed only when the index is not empty,
     and the `session:` line only when there is a previous finished
     session — the same rule the pi tool follows, so the two surfaces
     never drift. A block with nothing in it (no records, no index, no
     session) prints nothing at all: the answer is exactly what the door
     always answered.
  2. **The pi tool** (`mnexis_context` in
     `extensions/mnexis-memory/index.js`) renders the block in its answer
     text, after the items and before the declaration line, in the same
     JSON-per-record style the items already use. Its `details` gain the
     block, so the chrome can draw it.
  3. **The chrome** (`extensions/mnexis-memory/lib/chrome.js`) draws the
     block in the expanded answer: the records with their content, the
     index as the list it is.
  4. **The contract the model reads** — the tool's description, the
     extension's README and the skill — says what the block now is: the
     live state with the content that fits a budget, and the index of what
     did not fit. The promise the description makes today ("the
     `project_state` and `decision` records **with their whole content**")
     becomes the promise it can keep.
- S8 (the criterion, pre-registered).
  1. **The live only:** the block carries no `superseded`, `archived` or
     `proposed` record, and the state filter is in the SQL (S6.1).
  2. **The budget holds:** the resident part's token count is never above
     `SESSION_STATE_TOKEN_BUDGET`, and a record that does not fit is in the
     index, not in the resident part.
  3. **The budget is declared:** the envelope carries `budget`,
     `resident_tokens` and `tokens`, and `tokens` is what the whole block
     costs.
  4. **The door's items do not move:** the block's shape, its budget and
     its policy change **nothing** about the budgeted items: the same
     items, in the same order, with the same `tokens_used`. The block
     stays outside the door's budget.
  5. **The delivery:** the CLI's text mode prints the items and the block;
     the pi tool's answer carries the block; the chrome draws it.
  6. **The differential:** against the base commit, every other surface is
     byte-identical — `status`, `list`, `list --full`, `search`, `recall`,
     `show`, `timeline`, `stats` and `project list --json`. The door's
     envelope changes only by the block's new fields and by the block's
     content, which is what this feature asks for.
  7. **The suite is green:** `cargo test` and the extension's node suite.

## Tasks

- T1 (S5, S6.1, S6.2, S6.3, S6.4, S8.1, S8.2, S8.3) — the block's shape and
  policy in the service: the `SessionState` / `StateRecord` /
  `StateIndexEntry` types, the live-state filter in the SQL
  (`src/domain/repository.rs`, `src/storage/memory_repo.rs`), the
  budgeted fill, the declared counts. Route: worker. Commit: work unit 1.
- T2 (S8.1-S8.3, S8.7) — the tests for T1: `tests/session_state.rs` grows
  the contract (the live only, the budget, the declared counts, the items
  that do not move), written before the change and failing against it.
  Route: worker. Commit: work unit 1.
- T3 (S7.1, S8.4, S8.5, S8.7) — the CLI's text mode: the door's text
  answer carries the items and the block. Route: worker. Commit: work
  unit 2.
- T4 (S7.2, S7.3, S7.4, S8.5, S8.7) — the pi tool and the chrome: the
  block in the answer, in the details, in the drawing, and the contract
  the model reads (description, README, skill). Route: worker. Commit:
  work unit 2.
- T5 (S8.6) — the differential against the base commit, run in a scratch
  worktree, and the report of what moved and what did not. Route: inline.
  Commit: work unit 2.

## Log

- L1 (2026-10-27, the user's request, verbatim): "arma m1".
- L2 (2026-10-27): the two decisions that shape the feature were asked and
  answered before any code was written: **"Excluye en silencio"** (S2) and
  **"La forma + la entrega"** (S3). The finding that forced the second
  question: the shipped pi extension renders **only the items** — the block
  exists in the CLI's JSON envelope and in the service, and reaches no
  agent through the tool. The branch experiment's arm B rendered it, so the
  ascension was measured on a branch and the shipped door does not carry
  the block.
- L3 (2026-10-27): the block's measured cost, with the shipped tokenizer
  over this namespace's live state: **4 594 tokens** (the newest record
  alone 838). The budget of S6.2 (2 048) was chosen so the last session's
  state — typically two to five records — fits whole, and the index keeps
  the identity of everything older.
- L4 (2026-10-27): the extension's worker finished first and raised two
  questions the spec had not settled; the parent settled both and amended
  S7.1 (the amendment is the only change to a spec, and it closes a gap
  the spec itself had): **the `session:` line is printed** (the block
  carries the previous session's summary and both surfaces have to say so —
  dropping it would break the promise the description makes), and **the
  `indexed:` heading appears only when the index is not empty** (the pi
  tool's rule, adopted by the CLI so the two never drift).
- L5 (2026-10-27): T3 (the CLI's text mode) implemented inline by the
  parent, test-first: the RED evidence is the header-only answer
  (`selected 1 items (tokens=161, namespace=default)` plus the scope
  line), and the GREEN evidence is the answer carrying the item and the
  block (`tests/cli_commands.rs`, two tests). The rendering helpers live in
  `src/cli/output.rs` next to the other renderers, and the command's text
  is now `context_answer(&response, read_scope)`.
- L6 (2026-10-27): the slice boundary, recorded as the harness asks. The
  branch carries **five** commits and ~1 950 authored lines, and the
  feature does not split further along them: the service (the block's
  shape, the policy, the repository's state filter and the contract suite)
  is one unit because the repository's method rename breaks its caller —
  the tree does not compile in the middle of it — and the extension's tool
  and its drawing are two units because they are two surfaces with two
  suites. The boundary that was crossed is the one the board already
  named once ("neither splits without breaking the thing it pins"), and
  the unit that follows is the door's own.
- L7 (2026-10-27): the outcome. **The criterion (S8) holds on all six
  points.** The live only (S8.1): the block carries `active` and
  `disputed`; the superseded record of the differential's fixture is gone,
  and the filter is in the SQL. The budget holds (S8.2): the fixture's
  resident part is 1 388 of 2 048 tokens, and the record that did not fit
  is in the index. The budget is declared (S8.3): `budget`,
  `resident_tokens` and `tokens` are on the wire. The door's items do not
  move (S8.4): the same items, the same order, the same `tokens_used`. The
  delivery exists (S8.5): the CLI's text mode prints the items and the
  block, the tool's answer carries it, the chrome draws it. The
  differential (S8.6): **twelve surfaces byte-identical** against
  `bee8287`, only the two door modes moved. The suite (S8.7): 710 cargo
  tests and 133 node tests green, clippy clean.
  Two definitions were settled by the parent after the work: the
  `tokens` field counts the block's **whole serialised form** with its own
  value counted as zero (what the agent pays, recomputable from the
  answer it declares) — not the sum of the three named parts; and the
  `indexed:` heading appears only when the index is not empty, the pi
  tool's rule, adopted by the CLI so the two surfaces never drift.
- L8 (2026-10-27): **what the block costs now, measured.** The fixture's
  block: 4 live records, 1 388 resident tokens of 2 048, 1 427 for the
  whole block. The namespace that motivated it held 4 594 tokens of
  unbudgeted state before this change; with the same budget it would hold
  the newest records whole and index the rest, and the index names every
  one of them with the uuid that pulls it. The resident mass now has a
  ceiling, and the ceiling is declared.
- L9 (2026-10-27): the review. The native assessment (`assess`, escalating
  item 3 because the shared SQL helper changed) returned **high risk** and
  a plan with two parts: the writer self-verifies and **an independent
  verifier runs**. The independent verifier **failed on infrastructure**
  — the pi process died with a stream write error, not with a finding — so
  it is reported as **unavailable** and was not relaunched. The parent's
  own verification took its place, question by question:
  1. **The shared helper.** `filter_sql`'s `state: Option<&String>`
     became `states: &[String]`, and every caller was checked:
     `list_in` and `list_in_typed` pass the one state they were given
     (`state IN (?)` with one element is `state = ?`), `list_deleted`,
     `list_recent` and `count_in` pass an empty set, which is the "no
     filter" they had before. No query changed its semantics.
  2. **The filter and the limit.** The state filter is in the WHERE
     clause (`src/storage/memory_repo.rs`, `filter_sql`), and the LIMIT
     is applied by the query that wraps it — a superseded record cannot
     crowd a live one out.
  3. **The fill.** A candidate that does not fit is popped back out of
     the resident part and `full` is set, so the resident part can never
     end up over the budget, and everything after the first one that did
     not fit goes to the index — the newest-first rule holds, and a
     smaller, older record is never kept in place of a newer one.
  4. **The counts.** `resident_tokens` counts the resident part's
     serialised form and `tokens` the whole block with its own value
     counted as zero, so `tokens` is never below `resident_tokens` and
     the declared budget is the one the fill uses.
  5. **The items.** The block is attached after the response is built, so
     the items, their order and `tokens_used` cannot move — and the
     differential says they did not.
  6. **The empty paths.** A namespace the profile may not read yields the
     empty block, a store that cannot answer yields an empty candidate
     list (`.unwrap_or_default()`), and the session that cannot be read
     yields no session — the block is additive and never takes a command
     down.
  With the differential (twelve surfaces byte-identical), the suite (710
  cargo, 133 node) and clippy clean, the criterion holds. **Risk: item 3**
  — the shared SQL helper changed — and the verification above is what
  answers it.
