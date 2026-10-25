# The board — what is done, what is open, what comes next

**Last updated:** 2026-10-26 (item 6 in main with #50; the install-order trap
closed with #52; the trigger measured — the premise was false, and the road
moved to the door). Read this first in a new session, then the
feature document of whatever you pick.

## Where we are

`main` carries the comparison with the other engine and everything that came
out of it:

| PR | feature | what it changed |
| --- | --- | --- |
| #22 | `agent-query-contract` | the query is compiled (a hyphen is part of a word, every word counts), a bad query is an invalid argument and not `BUSY`, and the workspace key has one canonical form |
| #23 | `canonical-binding-shadow` | a registry written before the canonical rule no longer shadows it, and the CLI's recovery advice works |
| #24 + #26 | `agent-recall-ergonomics` | a list is a list (identity and a preview, `--full` for the rest), and the `topic_key` accepts the slash |
| #25 | `session-and-prompts` | the session the agent runs in and what the user said (migration 004), and the hooks where a compaction lands |
| #27 | `topic-continonomy` | the engine names the topic it already holds — and the measurement showed that was not enough |
| #28 | `memory-title` | the title derives the key and a save with a key the namespace uses **updates** the record (migration 005) |
| #29 | `identity-visibility` | the title travels with every read answer and the index covers it (migration 006); two defects fixed (a save writing into a dead record, the on-demand tools in the wrong namespace) |
| #30 | `agent-keywords` | the words of a fact: the agent's own vocabulary, accumulated and indexed (migration 007) |
| #31 + #34 | `doctor-contract` | the doctor answers with a stable machine contract (check_id, severity, reason_code, evidence, safe_next_step, status/summary), `--check CODE` filters, and `doctor repair --plan/--dry-run/--apply` separates what it fixes from canonical damage; the old `--repair` flag is gone |
| #36 | `operational-commands` | `mnexis stats` (every namespace and the totals), `mnexis timeline <uuid>` (identity, recorded revisions, supersede links both ways), `project list --json` as a typed contract |
| #38 | `read-granularity` | reading another project takes a human-granted permission: `config allow-read-namespace` (read only), a bound workspace may read its own namespace (the marker grants nothing), a refusal names the route, and the reserved `global` namespace cannot enter the allowlists |
| #40 | `read-scope-disclosure` | every read declares the scope it was bounded by (`read_scope`: all \| except_global \| restricted), so "no memories" and "no permission" are different answers; `stats` respects the scope and the pi extension carries the declaration to the agent |
| #42 | `closed-default` | the reads are closed in a new profile (`allow_read_all = false`), measured over ten runs per arm: zero invented values and zero duplicated facts closed, against ten of ten duplicated facts open |
| #46 + #47 + #48 | `session-recall` | the session answers for itself: the recall of a session (`mnexis recall --session <id>` / `--previous`, the MCP `memory_recall`), the working context of a session (migration 008: the branch, the commit and the canonical remote — never the raw URL), and the state of the anchored branch validated **when it is read** (`merged` / `not_merged` / `unknown`, never stored, never a negative) |
| #50 | `extension-recall` | the recall is priced (identity + preview by default, `--full` / `full` for the whole records — one projection for the CLI and the MCP) and the pi extension exposes the three doors it was missing: `mnexis_recall`, `mnexis_workspace`, `mnexis_history` |
| #52 | `install-stale-binary-rebuild` | from a repository, `install.sh skills|extensions` rebuilds a binary older than the tree before delegating (the embedded assets are a snapshot of the compiled tree — the trap that reinstalled stale assets with fresh timestamps, #51); piped mode never rebuilds, and the order of `./install.sh` and `./install.sh skills` stopped mattering |
| #56 | `priced-first-answer` | the first answer a session reads — `mnexis status` and the MCP `memory_workspace` — carries the identity and a preview, not the verbatim content of the last three memories (#53): one projection (`MemoryPreview`), the fatter duplicate deleted, and the narrowing declared where the agent meets it (CLI text, tool description, extension README) |
| #60 + #61 | `priced-first-answer-followups` | the three findings of the blind review of #56: the MCP tool description never said the list was priced, the extension's test fixture still modelled the forbidden shape, and the new renderer printed titles and previews verbatim — a title with a line break split a fact in two |

Everything is merged, item 6 landed with **#50**, and the install-order trap
landed with **#52**. `main` is clean (plus the
archived branches of the
features that were measured and discarded, and `feat/near-duplicate-titles`,
archived and **not** merged — see item 1c). The `session-recall` slices
(`feat/session-recall-command`, `feat/session-recall-anchor`,
`feat/session-recall-state`) and the leftover `feat/session-recall` are
merged and waiting to be deleted.

**Released: v0.3.0** (2026-10-23), the identity work. The schema is at
version 8 (migration 008 added the anchor of a session) and there is no
downgrade: an old binary refuses to open a newer store.

## The measurement that runs the board

The comparison with the other engine (in a container, pi, the same model) is
what every decision comes from. The artifacts are in `tmp/bench/` (gitignored
— copy them out before the sandbox is wiped):

- `tmp/bench/report.md` — the comparison and the capability matrix.
- `tmp/bench/recheck-measurement.md` — the re-comparison: the plan and the
  **criterion written before the numbers**, ten runs per arm.
- `tmp/bench/recheck-report.md` — the re-comparison: the numbers, the verdict
  criterion by criterion, and the two moves that come out of it.
- `tmp/bench/door-open-measurement.md`, `tmp/bench/door-open-report.md` —
  the door-open re-measure: the plan and criterion first, then the numbers
  that falsified the prediction and the cache confound it caught.
- `tmp/bench/conflict-measurement.md` — C2 measured: a conflicts layer would
  change nothing; the topic key is where the cost is.
- `tmp/bench/topic-continuity-measurement.md` — the notice and the refusal,
  with the variance.
- `tmp/bench/memory-title-measurement.md` — the title, and the 3 of 3.
- `tmp/bench/visibility-measurement.md` — the visibility: paired, 11/12
  against main's 7/12.
- `tmp/bench/keywords-measurement.md` — the words: the agent writes them
  3 of 3, and the correction the cycle earns (n=3 is a smoke test).

The harness: `tmp/bench/run/run-conflict.sh` (the conflict experiment),
`tmp/bench/run/run-all.sh` (the write/recall experiment),
`tmp/bench/run/run-duplicates.sh` (the duplicate experiments, four
scenarios), `tmp/bench/run/summarize.py`, `tmp/bench/run/score-conflict.py`,
`tmp/bench/run/score-duplicates.py`, `tmp/bench/run/run-recheck.sh` (the
re-comparison: N runs per arm) and `tmp/bench/run/score-recheck.py` (its
metrics, with the invented-value check).

## What comes next, with the evidence behind each

### 1. The identity of a fact — visible, findable, and with a vocabulary

The engine had the identity of a fact (`topic_key`, unique per namespace,
derived from the title) and the relation that works (`supersedes` +
`was replaced by`). What it lacked was the ability to **show** that
identity and to **find** it. Three features, in this order:

**1a. The visibility** (PR **#29**, merged). The
title travels with what the agent reads (`search`, `list`, `context`, the
MCP answers) and the index covers it (migration 006: `fts5(content, title,
topic_key)`, with the rebuild and the doctor's shape check). Two defects
came with it, both found by the measurement of the previous feature: a save
that wrote into a **dead record** (the fact lost, 0 hits) and the on-demand
tools that answered in the **wrong namespace** (26 refused calls).
Measured, paired, four scenarios: the agent linked the two records in
**11/12** against main's **7/12**, and the fact ended in one current record
in **11/12** against main's **9/12**. In the hard scenario (a busy
namespace, a content that does not repeat the title's words) main left the
fact in two records in **3 of 3** runs and this one in **2 of 3**.

**1b. The words of the fact** (PR **#30**, merged, stacked on #29). A memory had a title (what the fact is) and a content
(what it says now) and **no vocabulary**: the words the same fact is
called in another language, in another team's jargon. The agent says which
words belong to the fact (`--keywords "puerto, port, listen port"`), the
engine stores them (accumulating; `--replace-keywords` prunes) and the
index covers them (migration 007). **The agent writes the words: 3 of 3**
runs. That is the thing the previous cycle could not get: a mechanism the
agent uses without being pushed.

**1c. The scan of near-duplicate titles** (branch
`feat/near-duplicate-titles`, 8 commits, **archived, not merged**, local only). The engine
ranks the namespace's records by how likely they are to be the same fact
and proposes them with the save; the agent decides and links them with the
supersede that already exists. **Measured, and superseded**: the agent acts
on the proposal (12 of 12 runs linked with it, in one scenario with the
command verbatim — the opposite of `topic-continuity`'s notice, which was
read and ignored), but in three of the four scenarios the proposal is
**redundant**, and the engine's similarity **cannot separate** two titles
of one fact from two titles of two facts (0.53–0.97 against 0.47–0.91): a
proposal built on that similarity proposes noise half the time. The
visibility removes the reason the proposal existed, and the words replace
the similarity guess with an exact retrieval — the engine does not have to
judge anything, because the agent already did. Do not merge it without new
evidence.

### 1d. The correction the cycle earns

**n = 3 per arm with one model is a smoke test, not a measurement.** The
same image and the same prompts measured **1/3** and **3/3** on the same
day. Every difference between the arms in this cycle is of that size.

What survives, because it is a mechanism and not a count:

- the agent **acts** on a proposal whose answer carries the command (12/12);
- the agent **writes** the words when the skill and the tool ask (3/3);
- the engine's **similarity cannot separate** two titles of one fact from
  two titles of two facts — a property of the strings.

Everything else needs more runs before it is claimed.

### 2. C3 — `doctor` with a stable JSON envelope — **closed** (#31 + #34, in main)

In the PR table above: the envelope (`check_id`, `severity`, `reason_code`,
`evidence`, `safe_next_step`, top-level `status`/`summary`), the `--check
CODE` filter, and `doctor repair --plan/--dry-run/--apply` with the
repairable / not-repairable split. The old `--repair` flag is gone (no
users yet). The next open candidate is C4, item 3 below.

Five checks today, no contract. The other engine has ten named checks with a
machine envelope (`status`, `summary`, `checks[].check_id`, `severity`,
`reason_code`, `evidence`, `safe_next_step`), a `--check CODE` filter and
`doctor repair --plan|--dry-run|--apply` for seven specific codes. Small, and
it is what an operator needs when something goes wrong.

### 3. C4 — the operational commands — **closed** (#36, in main)

`mnexis stats` (what every namespace holds, and the totals), `mnexis timeline
<uuid>` (one memory's life: identity, the revisions the store records, and
its supersede links in both directions), and `project list --json` as a typed
contract. Read-only, no TUI needed.

### 3b. The closed default — **closed** (#40 + #42, in main)

The operator wants the agents not to see other projects' memories by default.
The granularity is in (#38) and the hazard is closed (#40): a read now
declares whether it was bounded, so "there are no memories" and "you may not
read them" are different answers.

**The measurement** (`odd/tasks/closed-default.md`: the plan and its criterion
written BEFORE the numbers). Two arms — the reads open (today's default) and
the reads closed — same image, same model, same prompts, 10 runs each. Four
metrics: the finding, the mechanism (does the agent ASK instead of guessing),
the honesty (does it say it did not find it), and the cost (does the shared
fact end up duplicated in the agent's own namespace).

| | reads open | reads closed |
| --- | --- | --- |
| found the other project's fact | 10/10 | 0/10 |
| **invented** a value | 0/10 | **0/10** |
| **duplicated the fact in its own namespace** | **10/10** | **0/10** |
| asked (or said it did not find it) | 0/10 | **10/10** |
| the refusal was visible | 7/10 | **10/10** |

**What it says, and it is not what the framing expected:** the duplication
comes from the OPEN default, not from the isolation — an agent that reads
everything copies another project's fact into its own namespace in ten of
ten runs, and the namespaces fill with copies of the same fact. The closed
arm invented nothing, saw the refusal every time, and asked (or said it did
not find it) in ten of ten. The price of closing is explicit incompleteness:
a task that needs another project's facts stops being finishable without
the human. That is a privacy decision, priced.

**The criterion (written before the runs) holds on all three bullets**, so
the default is closed (#42): `allow_read_all` is `false` in a new profile.
Existing profiles keep what they have. The switch is
`mnexis config allow-read-all`.

**The boundary of the isolation, found by the same run.** Six of the first
ten closed-arm runs found a second profile on the filesystem
(`/root/.mnexis-seed/mnexis.toml`) and opened it: the agent listed `/root`,
found a wider allowlist and read the other namespace through it. **The scope
binds the profile the process was given, not the files on the disk — mnexis
is not a sandbox.** The harness was fixed (the seed is deleted right after
it seeds) and the arm was measured again; the limit itself is the
product's, and the skill now says so: a second profile is not a permission
either.

### 4. B3 — the injected protocol block: measured, and **not** adopted

The other engine appends its memory protocol to the system prompt. Measured
here with the same container and the same model: it buys nothing (the agent
already recalls and already saves, because the tool descriptions and the
skill say so). Recorded in `odd/tasks/agent-recall-ergonomics.md` (L4) — do
not rebuild it without new evidence.

### 5. The prompt/session layer is in, and the session now carries its context

`session-and-prompts` landed the tables, the tools and the hooks. The **recall**
of a session and the **provenance with git** are done and **in `main`**
(#46 → #47 → #48, each with its issue: #43, #44, #45). What they added:

- the recall (`mnexis recall --session <id>` / `--previous`) answers what a
  session was: the session, its prompts in order, and the memories of its
  namespace in the session's window — labelled as a **window**, because the
  engine cannot know which session wrote which memory;
- the session records its working context (branch, commit, and the canonical
  remote — never the raw URL, which can carry credentials), and the state of
  that branch is validated **when it is read** (`merged` / `not_merged` /
  `unknown`): never stored, and `unknown` never becomes a negative.

What is still missing: the **conflict layer** proper (a judgment the agent
records, and the verdict annotated back into the search results).

## How to work here

- **Measure before you build.** Every feature of this cycle started as a
  measurement, and two of them (the conflicts layer, the protocol block) were
  dropped by their own measurements. The harness is in `tmp/bench/`.
- **The store is the evidence.** `mnexis list`, `mnexis search --explain` and
  the raw `memories` table say what actually happened; the transcript says
  what the model read.
- **One feature, one branch, one PR.** The name says what changes, never
  another project's name.
- **Commits are work units** of about 400 authored lines, with the evidence
  in the message.

## The open questions, honestly

- With one model and three runs per arm, the numbers move a lot. A claim
  about "the agent now does X" needs more runs than we did. (The re-comparison
  ran **ten** per arm, and the two arms that matter did not waver: 10/10 both.)
- The recall is still not free: the agent reads the memories it finds. A
  memory whose content is long costs context every time it is recalled. **The
  re-comparison measured it**: the agent pays 4 111 input tokens to recover
  five facts, against engram's 1 851 — it reads six records one by one
  because the pi extension does not expose the one-call door.
- mnexis still cannot tell that two different titles mean the same fact.
  That is item 1 above.

### 6. The re-comparison: the recovery is at engram's level, and it costs twice

The first comparison ran when the project was starting. The **re-comparison**
(`tmp/bench/recheck-report.md`) runs **the same experiment** with the mnexis
this cycle produced: same image, same model, same prompts, same arms, ten
runs per arm.

| phase 1 (the recall) | runs at 5/5 | wall ms | input tokens | tool calls |
|---|---|---|---|---|
| **mnexis** | **10 / 10** | 11 404 | 4 111 | 8 |
| **engram** | **10 / 10** | 9 394 | **1 851** | **2** |
| control | 2 / 10 | 12 077 | 1 947 | 4 |

**Quality: at engram's level.** 5/5 in ten runs out of ten, the retrieval
check puts the right record first in all five paraphrased queries, and the
engine invented nothing. The P0 defects of the first comparison (the broken
query contract, the verbatim binding key) are gone, and the store is clean:
six records, one per fact, `rev=1`, no duplicates.

**Cost: twice engram's.** The transcripts say why: engram's store holds **one
consolidated observation** and the agent answers in two calls; mnexis holds
**six records, one per fact** (the identity work), and the agent reads them
one by one — 45 `mnexis_show` calls across the ten sessions.

**And neither arm got a skill and the other did not**: engram ships **no
skill** (its guidance is 9 729 B across 22 tool descriptions), while mnexis
ships a 13 KB skill the agent **reads in 20 of the 20 transcripts**. That
skill is part of the context tax mnexis pays in the write phase (8 894 input
tokens against engram's 4 541) — and it is also why the mnexis arm knows to
save a memory **with a title**, which is what makes the store one record per
fact.

**And the cause is in the integration, not in the engine.** The recall
answers "what does this project know" in one call, and the MCP server
exposes it (`memory_recall`) — but **the pi extension does not** (ten tools,
the recall is not one of them). The agent rebuilds it by hand.

Two moves, both small, and they are the next thing to do:

1. **Expose the recall in the pi extension** (and `memory_workspace` and
   `memory_history`, which are missing there too). The door is built; the
   agent cannot see it.
2. **Price the recall in context**: identity and preview by default, the
   full content on request — the shape `list` got in #24/#26 and
   `memory_search` already has. A recall that answers with 100 full memories
   is not the cheap door the extension would open.

**Both landed and are in `main`** (PR **#50**, issue #49; feature doc:
`odd/tasks/extension-recall.md`): the recall is priced
(`SessionRecall::payload`, one projection for the CLI `--full` and the MCP
`full`; the old default payload is exactly preserved behind the flag — the
differential against main proved it), and the extension exposes the three
doors it was missing: `mnexis_recall` (with `full` through),
`mnexis_workspace` and `mnexis_history`. The skill and the extension
README name them. The differential also proved `list` / `list --full` /
`search` / `status` byte-identical, so the shared projection did not
move.

**The re-measure ran, and it falsified the prediction** (`door-open-report.md`, 10 runs, criterion written before the numbers): the agent **never opened the recall door** (0/10 `mnexis_recall` calls) — its question is project-shaped ("what does this project know"), not session-shaped, and search answers it directly. What it did adopt, first call, is the **other** new door: `mnexis_workspace`, 9/10 runs. The measured wins are real: tool calls halved (8→4), `mnexis_show` down 58% (45→19), quality 10/10, zero invented values, and **total processed context (input+cacheRead) down 26%** (16 858 vs 22 751). And the measurement caught a defect in the metric itself: the re-comparison's input-only token row (4 111 vs 1 851) is **cache-confounded** — a cold provider cache counts the prefix as input, a warm one as cacheRead; every future comparison reports input+cacheRead. The next product question (not decided): the one-call answer the agent wants is project-shaped — whether `mnexis_workspace` should carry the priced identity+preview of everything the project holds, or the skill should teach `mnexis_recall --previous` as the session-resume move, is a separate feature with its own measurement. Artifacts: `tmp/bench/door-open-{measurement,report}.md`, copy in `/home/xeworg/lab/mnexis-bench/door-open/`.

**And the uncomfortable row, measured again**: offered no memory at all, the
agent found a durable channel **twice in ten** — once by writing the
`AGENTS.md` that pi loads for free, once by discovering the mnexis CLI in the
image and wiring it up by hand. The competitor is real but unreliable (2 of
10), and a discoverable engine beats an undiscoverable one.

### 7. The trigger — measured, and the premise was false

The door-open re-measure answered a prompt that **asks** for memory. The
live host opens a session with a **task**, and there the question was
open: what makes the agent reach for the project's memory before it
starts? The competitor's engine does it by injecting a protocol through
pi's `before_agent_start`; mnexis had no such trigger (issue #54).

**The measurement** (`trigger-measurement.md`, criterion written before the
numbers; `trigger-report.md`, the numbers): three arms, ten runs each,
the same image, model, fixture and prompts. The measured phase is a
**task**, the workspace has no files, and the task carries a trap — the
request asks to point the deploy at production, the stored decision says
`staging-eu`.

| arm | trigger fires | keeps the default | median tokens | median tool calls | invented |
| --- | --- | --- | --- | --- | --- |
| **A** shipped | **10 / 10** | **10 / 10** | 44 478 | 9 | 0 |
| **B** protocol block | 9 / 10 | 10 / 10 | 41 055 | 8 | 0 |
| **C** `before_agent_start` hook | 10 / 10 | 10 / 10 | 45 152 | 8 | 0 |

**The premise was false: the trigger already exists, and it is not the
hook.** The shipped arm — nothing added — read the memory **before any
other tool in 10 of 10 runs** and kept the stored decision in 10 of 10.
The reason is in the integration: pi injects a skill's *description* into
the system prompt (not its body), and the mnexis skill's description says
to read the project's memory before changing code. The hook adds a pointer
for a behaviour that already happens (and costs 2 % more context); the
protocol block adds a protocol for a behaviour that already happens.
Criterion (2) — *the hook beats nothing* — fails, so **nothing ships**,
and #54 closed with its numbers. B3's ghost is finally laid down: it was
killed by a measurement that asked the wrong question; this one asked the
live-host question and got the same answer for a better reason.

**The gap the numbers expose is the door, not the trigger.** Every run in
every arm went `mnexis_workspace → bash → list/search → shows`: the
first answer names the memories, and the agent **rebuilds the answer by
hand** (median 3 memory calls, 9 tool calls, on six records). The
budgeted one-call door that would replace the rebuild already exists in
the CLI — `mnexis context "<task>"` (PRD §9), and the skill tells the
agent to run it — but it is **not a pi tool** (#59). The namespace it
searched was the wrong one too (**#57**, closed below). So the road from
here is #59 → measure again, and the pointer question only comes after
those.

Two defects the measurement surfaced along the way: the records that
predate the identity (#29) still answer with an **empty title**, so the
priced answer does not say what each fact *is* (**#58**), and the
honesty check inherited a character-class bug from `score-door.py`
(`/-a` is the range from "/" to "a", which swallows letters — a commit
SHA read as an invented value). Artifacts: `tmp/bench/trigger-{measurement,report}.md`,
copy in `/home/xeworg/lab/mnexis-bench/trigger/`.

### 8. The priced first answer — in main, and what it opened

The first answer a session reads now answers with identity and preview
(PR **#56**, issue #53, merged; the follow-ups **#60** and **#61** closed
what the blind review found). The payload went from 2 033 to 1 220 bytes
on a fixture with one long and one short record, and the CLI gained the
titles it did not have before (the identity patch used to live only in
the MCP path — a drift between the two surfaces that the change closed).

**What it did not settle.** The pricing moves the cost of the first call,
but the agent still *rebuilds* the answer: the trigger measurement showed
every run going `workspace → list/search → shows`. The one-call door that
would replace the rebuild exists in the CLI (`mnexis context "<task>"`,
PRD §9) and the skill tells the agent to run it — and it is not a pi
tool (**#59**).

**The door's namespace, closed (#57).** It answered from *"scope's first
read namespace"* — the order of the allowlist, not the namespace the
agent's workspace writes into — and the envelope did not say which
namespace it searched. In a multi-namespace profile it answered with
**another project's** memories, in silence, and the agent could not tell
that from a good answer. Now the door searches the workspace's own
namespace — the one a save lands in — when the profile may read it, keeps
the allowlist's first entry when the workspace names none, and the
envelope declares the namespace it searched (`searched_namespace`): the
read_scope precedent (#40) applied to the namespace, because a narrowed
answer that stays silent reads as "there are no memories" when the truth
is "it looked in another project". The differential against the base
commit: every other read surface is byte-identical, and `context` moves
only by that field.

**The order from here**: #59 (expose `mnexis context` as a pi tool) →
re-measure against the merged binary → and only then the experiment with a
**steering-shaped** phase (issue #55): the bench asks a knowledge-shaped question ("¿en qué puerto
escucha?"), while the recurring question is steering-shaped ("¿dónde
quedamos?"), which is exactly what the typed-memory idea is for.

Two debts are still open: the records that predate the identity answer
with an **empty title** (**#58**), and the `memory_type` column, which
is written but never read, erased by the next save, and filtered by
nothing (**#55**).
