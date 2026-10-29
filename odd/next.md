# The board — what is done, what is open, what comes next

**Last updated:** 2026-10-28 (item 17: the lean-skill experiment — the body is
not read where it counts, and the harness pays 755 tokens a call; before
that: item 16: the pricing was measured, lost and
was reverted — the door carries whole facts again, and the door is in the
MCP surface; before that: item 14 landed: the door's answer is priced
and declares its coverage — M3 and M4 of the order the re-read metric
imposes; before that: item 13 landed the budgeted state block, and item 12
landed the re-read metric — the number that was missing; before that: item 9
closed twice — the call budget measured, one complete answer buys the round
back, and the ascension shipped, so the door carries the session state and
the switch retired; then the identity work through #66, the door and its
re-measures, the trigger, the prefix). Read this first in a new session, then
the feature document of whatever you pick.

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
| #62 | `one-line-per-renderer` | the blind review of #61 found two paths where the "one physical line per fact" rule still did not hold: `snippet` folded only the newline (a stored carriage return reached `mnexis list` and painted the fact over) and the timeline's header interpolated the title verbatim — `one_line` is now the single helper for the rule |
| #63 | `context-workspace-namespace` | the one-call door searches the **workspace's** namespace — the one a save lands in — instead of the allowlist's first entry, and the envelope declares the namespace it searched (`searched_namespace`): in a multi-namespace profile it answered with another project's memories, in silence (#57) |

Everything is merged, item 6 landed with **#50**, and the install-order trap
landed with **#52**. `main` is clean (plus the
archived branches of the
features that were measured and discarded, and `feat/near-duplicate-titles`,
archived and **not** merged — see item 1c). The `session-recall` slices
(`feat/session-recall-command`, `feat/session-recall-anchor`,
`feat/session-recall-state`) and the leftover `feat/session-recall` are
merged and waiting to be deleted.

**Released: v0.4.0** (2026-10-26), the door and the call budget: the
one-call door as a pi tool (#64), the doctor's check and repair for the
records without a title (#65, #71, #72), the type of a memory as a closed set
the agent writes (#66), and the session state riding the door's answer by
default — the call-budget experiment's pre-registered ascension (#68, #70).
The schema is at version 8 (migration 008 added the anchor of a session) and
there is no downgrade: an old binary refuses to open a newer store.

**Merged: PR #92** (`feat/door-priced-answer`, 4 commits, ~1 100 authored
lines) — the door's answer is **priced** and declares its **coverage**
(item 14). `main` is now **`70d22a8`**, and the installed binary
(`~/.local/bin/mnexis`, **0.5.0-dev**) and the extension and the skill in
`~/.gentle-shell/agent/` **and** in `~/.pi/agent/` are byte-identical to it
— the installed door answers with the preview and the declaration.

**Merged: PR #89** (`feat/state-block-budget`, issue #88, label
`type:feature`, 8 commits, ~2 100 authored lines) — the state block with a
budget, an index and the live only, and the door that delivers it (item 13).
`main` was **`d844c94`** then, and the independent verifier of
that change was **unavailable** (its own scratch filled the 16 GB tmpfs and
every write from then on failed — the symptom is an exit 101 with an empty
stderr, and the scratch is gone); the parent's own verification is in the
feature document's log (L9).

**The order the re-read metric imposes** (item 12): the door first, then
the skill. The waste is in the door — it already carried the facts in full
and the agent re-listed them anyway — and the agent's cross-check round is
a **coverage** problem, not a trust problem, so the door has to declare
what it did not include. Concretely: **M3** (identity + preview + uuid
instead of the whole content, the handle being `mnexis show <uuid>`) and
**M4** (the candidates that did not fit, with their sizes) — **both landed
in PR #92**, item 14 — then **M5** (the lean skill, with its own
experiment), then item 11 (the door in the MCP surface).

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
priced answer does not say what each fact *is* (**#58**, closed below:
the doctor sees them now and the repair fills them), and the
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

**The door, in the agent's hands (#59).** The tool the agent never walked
through is now declared to the model in every session: `mnexis_context`
takes the task in one line and answers with the memories this project
recorded that matter for it, already trimmed to a token budget. It is the
cheapest way to ask a project-shaped question — "where did we leave off",
"which port does it listen on" — and the answer names the namespace it
searched and the tokens it spent, so the agent can tell what it got. The
tool searches the workspace's own namespace, resolved by the engine and
read from the status answer (asking a question binds nothing), and never
another project's — which is what #57 made possible. It does not replace
`mnexis_search`: the search is the exact query, the context is the
project-shaped question.

**The order from here**: re-measure against the merged binary (the
trigger batch, with the criterion written before the numbers) → and only
then the experiment with a **steering-shaped** phase (issue #55): the
bench asks a knowledge-shaped question ("¿en qué puerto
escucha?"), while the recurring question is steering-shaped ("¿dónde
quedamos?"), which is exactly what the typed-memory idea is for.

Two debts are still open: the **re-measurement** of the door, which needs
the bench environment (docker + the model provider) and cannot run in
the sandbox that wrote it, and the **steering-shaped experiment** that
the re-measurement has to precede (issue #55): whether a typed priority
earns its place, measured with the criterion written before the numbers.

**The type axis, honest (#55, part one).** `memory_type` was half-built:
a free-text column that travelled in every read shape and was filtered
by nothing, erased by the next save with no error and no repair path.
It is now a **closed set of seven kinds** — `knowledge`, `decision`,
`preference`, `procedure`, `project_state`, `episode` and the generic
`fact` — that the **agent writes** (an attribute the agent writes is
used; one the engine guesses proposes noise), that never enters the
identity (the topic key stays title-only), that a save which says nothing
about preserves, and that `--type` filters the reads by, exactly, in the
SQL and before the limit. The vocabulary was the human's call: the seven
kinds, `fact` included, so not a single stored row had to be rewritten.

What is NOT settled: whether a typed priority ever earns its place. That
is the steering experiment, and it needs the bench environment.

**The identity debt, closed (#58).** The records that predate the
identity work carried no title at all, so every priced answer named
nothing — a uuid, a namespace and a preview, with no word saying what
the fact *is*. Nothing repaired it and no check saw it: the doctor had
seven checks and none looked at the title shape.

The doctor has an eighth now. `title_shape` counts the records without
a title and names them, and it is a **warning**: a record without a
title is read and answered, it is just not named, so the store still
serves and the run does not fail on it. The status follows the
severity, not the count of green checks: a warning is a gap, not a
broken store.

The repair (`doctor repair --plan|--dry-run|--apply`) fills the missing
titles from each record's **own text** — its first sentence, the same
thing the agent writes by hand — and, when the record holds no key at
all, the key that title yields, exactly as the save path derives it. A record whose text yields nothing
is reported, not invented, and a key another record already holds is not
written: merging two facts is the agent's decision, not the doctor's.
The fact does not change, so the revision does not move and the history
keeps what it says: only the missing identity is filled.

### 9. The call budget — one complete answer buys the round back

The prefix experiment (`tmp/bench/prefix-cost-report.md`) priced the tool
schema and lost: the saving per call was real and the bill still went **up**,
because the lean arms needed two extra calls — each model call bills the
fixed prefix (~5 046 tokens) **plus the whole accumulated transcript**. The
lever is the number of calls, so the next experiment put a **session-state
block** (the namespace's `project_state`/`decision` records with full
content, plus the last session summary) on a surface the agent already
opens, and asked whether one complete answer stops the fan-out
(`odd/tasks/call-budget.md`, criterion committed before any arm,
`84eb092` on `bench/call-budget`).

Three arms × two phases × N=10, same model, same fixture: A the shipped
surfaces, B the block on the door (`mnexis context`), C the block on
`status`/workspace — the block behind `MNEXIS_BENCH_SESSION_STATE`, read
once and injected, the shipped behavior untouched with the variable
absent. The batch survived its own interruption (the memory guard killed
it mid-run; the arm resumed with the same image and seeds, nothing
re-rolled) and the protocol is recorded pre-numbers
(`tmp/bench/calls-measurement.md`).

| phase S ("¿por dónde quedamos?") | A | B door | C workspace |
| --- | --- | --- | --- |
| median model calls | 6.0 | **5.0** | **5.0** |
| median input+cacheRead | 48 771 | **37 121** (−24 %) | 41 752 (−14 %) |
| 3/3 state facts stated | 9/10 | **10/10** | **10/10** |
| invented / trap broken | 0 / 0 | 0 / 0 | 0 / 0 |

And in phase K (the trap) only B also saved (−0.5 calls, −20 % tokens);
C's K did not move. The prefix was 5 046 in every arm: the block is
payload, not schema — every token saved came from **fewer calls**.

**Which round disappeared** (rule 6, `calls-rounds-dump.txt`): the rebuild
rounds. A's tail is `(workspace,context) | (list,recall) |
(show,show,show,search_prompts) | (search,list) | (history,show) | answer`;
B's floor is `(workspace,context) | (list,recall) | answer`. The agent
cross-checks one answer before trusting it — the `list+recall` round
survives in every run of every arm — so the fan-out was a payload problem,
not a trust problem.

**The verdict, honestly:** rules 1–5 hold for both arms, with rule 3 (the
prize is a round) **exactly at the boundary** — the ceiling is this batch's
A − 1 = 5.0 and both arms sit on it; the doc's "≤ 4.0" parenthetical was a
stale projection (A re-measured 6.0 with the same image). The direction is
consistent everywhere (calls, rounds, tokens, B's K too) but the margin is
one median call at N=10.

**The decision (S6, pre-registered): the block ascends on the door, only
there.** B dominates C on every measured axis, and the agent's own runs
pick the surface: it opens `(workspace,context)` in round 1 every time and
answers from the door 9/10 — C's block on workspace never made workspace
the answer surface (8/10 still door), so workspace keeps its priced shape
(#56). Artifacts: `tmp/bench/calls-{measurement,report}.md`, copy in
`/home/xeworg/lab/mnexis-bench/call-budget/`.

**And the ascension shipped.** The block is now what `mnexis context` —
and therefore the pi tool `mnexis_context` — answers by default: it is
attached by `MemoryService::context`, so every caller inherits it, outside
the budgeted envelope. The bench switch and the `status` carrier retired
with the experiment (setting the old variable changes nothing, pinned by a
test), and `status` keeps its priced shape. The contract the model reads
says so: the tool description, the extension README and the skill name the
state block and its price (nothing — it rides outside the budget). The
differential against the pre-ascension binary: `status`, `list`,
`list --full`, `search`, `recall` and the budgeted `context` envelope are
byte-identical; the door's new key is the whole behaviour change.

### 10. The chrome — the drawing the human reads, free

The other engine's tools look alive: a brain, a tick, a line per call. That
drawing is not the engine and not the CLI — it is the **extension's own
renderer**, and it is free there: pi draws the call and the result, and the
model reads the plain text the CLI answered.

**Measured, with the engine's own tokenizer** (`token_count_v1`, the one the
door's budget uses), for one memory: the JSON the extension hands the model
today costs **236 tokens**; the human's plain block costs **110**; a drawn
panel with the type's icon and no table border costs **81**; the same panel
with ANSI colour codes costs **117** (pi's bash tool does not strip escape
sequences, so the codes land in the agent's context); a panel drawn with
markdown table borders costs **374**, because every box character is a token.

**So the drawing is not portable, only the shape is.** The CLI's stdout is
the machine channel of every agent — this extension, opencode, and the
codex and claude code integrations still to come — and a drawing there is
paid by every agent, in tokens, for every memory it reads. What ships is
therefore:

- **pi only**: `extensions/mnexis-memory/lib/chrome.js` draws the call line,
  the panel that names a memory (its kind, its state, its revision, its
  namespace) and the footer status. The tool's own answer is untouched: the
  text the model reads is exactly what the CLI answered, and the drawing
  reads `details` to draw what the human reads.
- **every host**: the shape, not the decoration. The kind of the fact next
  to its name, one physical line per memory in a list, the identity of a
  record before its text — and, in the open items, the priced answer the
  model reads instead of the fat JSON.

**The renderer's contract, the way pi calls it.** pi hands the renderer
`{content, details}` and `{expanded, isPartial}` — **never the error flag**:
that lives in the render context, the fourth argument (`context.isError`).
A renderer that looks for the error in the result or in the options draws a
tick on a failed call. The review after the merge found exactly that, and
the fix reads the context.

**The hosts, measured, not assumed.** pi is the only host with a renderer
API (`renderCall`/`renderResult`/`renderShell`). opencode's TUI hardcodes
the rendering per tool id, so a plugin cannot restyle an MCP tool (issues
#21018, #33302); its web UI can (`ToolRegistry.register()`). claude code
renders MCP results as plain text and has an open request for structured
display formats (#30271) — and its own issue #15718 measures an
ANSI-formatted MCP answer at ~7k tokens of escape codes against ~6k of real
content. codex renders MCP results as plain text too (#1265 explores rich
widgets). The drawing is therefore a pi feature, and the shape is the
portable contract.

**The slices.** `07f442a` the chrome and its wiring, `fd468db` the pins that
hold it, and this commit the board. The module is one file and the suite
another: neither splits without breaking the thing it pins, so the
~400-line slice gives way to the unit — the drawing, then the promise.

### 11. The door is not in the MCP surface — the next thing for opencode

The MCP server the binary ships has sixteen tools, and **`memory_context` is
not one of them**. It has `memory_workspace` (the priced identity and
preview) and `memory_recall`, but not the one-call door, and therefore not
the session state that now rides it.

So in opencode — and in the codex and claude code integrations still to
come — the agent has to **rebuild the answer by hand**, which is exactly
what #59 fixed for pi, and it never receives the state block that made one
complete answer buy the round back (item 9). The order for the multi-host
goal is: expose the door in the MCP, then add the environments to the
installer (`AgentEnvironment` knows `pi` and `opencode` today), each one a
skill plus its host's MCP entry.

**And the question the chrome opens — measured, and the answer is no.** The
board's rule 6 says the agent cross-checks an answer before it trusts it, and
the `list+recall` round survives in every run of every arm. The shape
experiment ran 3 arms × 2 phases × N=10 on the same model, the same images and
the same seeds, with the criterion pre-registered in
`odd/tasks/chrome-shape.md` S4 before any arm existed: today's fat JSON, the
same items plus the session-state block, and the same content rendered in the
chrome's shape. **The verification round survived 10/10 in all three arms.**
P1 failed in the direction that matters — the shape did not remove it — and
P5 failed too: the block alone did not. Rules 2–5 held everywhere: the state
facts 10/10 with nothing invented, the trap `staging-eu` kept 10/10, the write
path 20/20, and both new arms answered *cheaper* than today's answer (36.2k
and 37.3k input+cacheRead tokens against 45.6k) while still paying the round.
The round is not a presentation problem: reading the sequences, it is the
agent enumerating (`list`, often in the door's own round) and then recalling —
a protocol, not a doubt about whether the answer was complete.

So **nothing ascends**, and the next question is the door's contract: the
extension's `memory_context` description promises the model the session state
(`PLUS the session state — the project_state and decision records with their
whole content and the previous session's summary`) and **no client renders
it**. Either the description stops promising it, or the block ships as a
promise kept rather than as a measured win — and the docs would have to say
exactly that. After that, item 11 (the door in the MCP surface) is what the
multi-host goal needs.

### 12. The re-read metric — the number that was missing

The bench measured calls, tokens, quality, invented values and the write
path. It never counted the **redundancy**: the facts the agent is handed
twice. The competitor's metric set carries *re-fetch* as a first-class
number, and the reading of `billion-context` (2026-10-27) showed why it
matters: re-fetch is the interaction cost of every failure mode the
compression work targets.

**The metric** (`odd/tasks/reread-metric.md`, S4/S5; the scorer
`tmp/bench/run/score-reread.py`, the report `tmp/bench/reread-report.md`):
a *delivery* is a memory in a tool result; a delivery whose fact was
already delivered in the same run is **redundant**; a call whose every
delivery was redundant is a **wasted call**. The definition and the
criterion were written before any transcript was read; the criterion holds
on all five points, with three hand-checks and nine self-test cases.

**The baseline** (six batches of earlier runs, no model call: 620
transcripts already on disk).

| | deliveries | redundant | rate | median redundant bytes/run |
|---|---|---|---|---|
| shape-a, steering | 186 | 74 | **39.8 %** | 5 469 |
| shape-b, steering | 212 | 100 | **47.2 %** | 6 418 |
| shape-c, steering | 214 | 99 | **46.3 %** | 3 821 |
| mnexis, the old recall phase | 161 | 5 | 3.1 % | 1 327 |
| **engram**, the same phase | 36 | 12 | **33.3 %** | 110 |

**What it says, and it changes the order of the work.**

- **The redundancy is a recall phenomenon.** The write phase has **zero**
  redundant deliveries in every arm; the trap phase runs at 22–36 %; the
  "where did we leave off" phase — the question that makes the agent
  enumerate — runs at **40–47 %**.
- **The door and the enumeration are the same machine.** `workspace →
  context → list --full → search`, and the `list --full` re-delivered the
  facts the door had already answered in full: 8 of its 10 records were
  repeats. The verification round the shape experiment measured surviving
  10/10 is priced here: **a median of 5.5 KB of pure repetition per run**,
  and a wasted call in 6 of 10 runs.
- **The `show` fan-out is mostly a no-op.** The preview is 160 chars
  (`src/application/preview.rs`) and the facts are 70–181 chars, so for
  most of them the preview *is* the whole fact: in the old batch, **3 of 5
  `show` calls added nothing**.
- **It is not our disease alone.** The competitor's door answers truncated
  and its agent then enumerated: **33.3 %**, as high as ours.
- **The cross-check round is a coverage problem, not a trust problem.** The
  door already carried the facts in full and the agent re-fetched them
  anyway — so the door should **declare what it did not include**, and the
  round that looks for the missing facts has something to look for.

**So the plan moves:** the door's answer (what it carries, what it declares
as missing, the state block) comes **before** the skill — the doctrine is
worth its own experiment, but the measured waste is in the door.

### 13. The state block, budgeted and delivered — closed (feature `state-block-budget`)

The block that rides the door was the one answer that replaced "where did
we leave off" with a single call, and it had two defects: its **resident
mass never decayed** (every live state record with its whole content,
unbudgeted and outside the door's budget — **4 594 tokens** in this
namespace, growing every session) and it **carried the history as if it
were current** (a superseded record looked exactly like a live one, and
the block had no field that said otherwise). And it did not reach the
agent at all: the pi extension rendered only the items, so the promise in
the tool's description was a promise the tool could not keep.

**What shipped** (feature doc: `odd/tasks/state-block-budget.md`; branch
`feat/state-block-budget`).

- **The live only:** the block carries `active` and `disputed`, and the
  filter is **in the SQL** — a filter applied after the query would let
  superseded records crowd the live ones out of the limit.
- **The budget:** the resident part (the records with their whole content)
  fits **2 048 tokens**; every live record that does not fit rides the
  **index** with its identity and its title, so the agent sees that the
  state exists and pulls one with `mnexis show <uuid>`. The fill is greedy,
  deterministic and newest-first.
- **The declaration:** the block carries `budget`, `resident_tokens` and
  `tokens` — what it costs, in the door's own tokenizer.
- **The delivery:** the pi tool renders the block between the items and
  the declaration, the chrome draws it, and **the CLI's text mode prints
  the answer** — the items with their content, the block, the index. That
  last one closed a hole the skill had been describing for a while: the
  text mode printed a header and nothing else, so every agent that reaches
  the CLI instead of the tool got no answer at all.

**The differential** (base `bee8287`, `tmp/diff-state-block.sh`): twelve
surfaces **byte-identical** — `status`, `list`, `list --full`, `search`,
`recall`, `sessions`, `stats`, `doctor`, `project list --json`,
`timeline`, `show`, `suggest-topic-key` — and only the two door modes
moved. The door's **items did not move**: the same items, the same order,
the same `tokens_used`; the block is outside the door's budget, and the
fixture's block holds 4 live records in 1 388 of 2 048 tokens (1 427 for
the whole block).

**What it does not settle:** the block's budget is a constant, not a
measurement — the number of records that fit depends on how long a
project's state records are, and the first project with long records will
show the index working (or not). The typed priority that would decide
*which* state is worth the content is still the unmeasured experiment
(#55), and the index is the honest fallback until it is measured.

### 14. The door's answer declares its coverage — closed (features `door-priced-answer`, `door-whole-content`)

The re-read metric (item 12) measured where an agent's context is wasted:
**the door already carried the facts in full and the agent re-listed them
anyway** — `workspace → context → list --full → search`, and the `list
--full` re-delivered 8 of the door's 10 records (39.8 %–47.2 % redundant
deliveries in the steering phase). The waste looked like it was in the
**door's payload**, and the board's M3 and M4 were the door's contract.

**What shipped and stayed** (feature docs: `odd/tasks/door-priced-answer.md`
and `odd/tasks/door-whole-content.md`; PRs **#92** and **#96**):

- **The coverage is declared (M4):** the envelope carries a `coverage`
  object — `candidates` (what the door saw), `carried` (what it carries),
  `left_out` (how many the budget cut) and `left_out_items` (up to ten,
  with their uuid, title, kind and the **size** that explains why they did
  not fit). The CLI's text mode prints it, the pi tool renders it, the
  chrome draws it. An engine that predates it sends nothing, and the
  extension invents nothing.
- **The door keeps the most relevant facts that fit.** The pipeline dropped
  the bm25 rank and sorted by `created_at` alone, so the budget filled with
  the **oldest** facts and the most relevant one could be left out — the
  opposite of PRD §9 step 6 and of the module's own documented order. The
  rank now travels with the memory (bm25 ASC, then `created_at` ASC), so
  what the door leaves out is the *least* relevant — and the declaration
  names the ones that **just missed the cut** first (verified on the
  binary).
- **The item carries the whole fact** — the pricing that briefly replaced it
  was measured, and it lost (items 15 and 16). The door never cuts a fact:
  the budget decides **which** facts fit, not how much of a fact you see.
- **The doctrine follows:** the skill, the extension's README and the root
  README say what the door answers now.

### 15. The priced door, measured — the verdict that reverted it

The plan and the criterion were written **before** the numbers
(`tmp/bench/reread-priced-measurement.md`); the report is
`tmp/bench/reread-priced-report.md`. The arm: `priced` — the merged tree
(`main` = `70d22a8`, PR #92), the same container family, the same fixture,
the same prompts, the same model, N = 10. The baseline: the **shape-b**
arm of the shape experiment — the fat door with the state block. The metric:
`score-reread.py`, unchanged (it still reproduces the baseline exactly).

| steering phase | the fat door | the priced door | |
| --- | --- | --- | --- |
| **re-read rate** | 47.2 % | **31.6 %** | ↓ |
| median redundant bytes | 6 418 | 6 484 | ≈ |
| wasted calls | 8 | 26 | ↑ |
| **median memory calls** | 4.5 | **9.0** | ↑↑ |
| **median tokens** | 37 290 | **50 464** | **+35 %** |
| the `mnexis_show` fan-out | 2/10 runs | **7/10 runs** | ↑ |
| ≥2 state facts stated | 10/10 | 10/10 | = |
| invented values | 0 | 0 | = |
| the trap kept | 10/10 | 10/10 | = |

**The verdict is negative, and the mechanism is not noise.** The preview cut
five of the fixture's six knowledge facts (105–323 characters, median 160 —
the preview is 160), and the agent fetched each one it needed: one
`mnexis_show` per fact, where the fat door's enumeration was **one**
`list --full`. **The redundancy the re-read metric counted as waste was not
waste**: reading N facts costs **one** call, not N, and every extra model
call re-bills the prefix plus the whole accumulated transcript — the
regression over the ten paired runs gives **+10 574 tokens per extra model
call**. The re-read *rate* also fell partly by definition (the metric only
counts a redundancy when the fact was delivered **in full**, and a preview
does not arm it), while the redundant **bytes** did not move.

The decision was the human's, and it was the board's recommendation:
**revert the pricing, keep the coverage** — with the door free to try the
pricing again later, if it can be shown to pay.

### 16. The revert, measured — and the door in the MCP surface

**The revert** (feature doc: `odd/tasks/door-whole-content.md`; PR **#96**):
the item carries the whole fact again, `mnexis context --full` and the
`preview` field are gone from the code, and the coverage declaration and
the relevance order stay. The re-measurement (the criterion written before
the batch, the arm `whole`, N = 10) holds **all four points**: the steering
phase's median tokens are **38 902** (the fat arm's 37 290, the bar 40 000),
the `mnexis_show` fan-out is back to **1 of 10** runs, the facts are stated
in 10/10 with nothing invented and the trap kept in 10/10. Run by run, the
whole door is cheaper than the priced one in **8 of 10** steering runs. The
differential leaves twelve surfaces byte-identical.

**The honest caveat:** in the **trigger** phase the whole door looks dearer
in 7 of 10 runs against the priced door, inside a spread that overlaps
every arm — and the direction makes sense: **the door's answer rides in the
transcript of every later call**, so a bigger answer is a tax on the rest
of the session. The door's answer has **two prices that pull in opposite
directions** — what it **carries** (the facts the agent would otherwise
fetch one by one) and what it **weighs** (every later call pays for it).
The pricing bought the second and paid the first.

**The door in the MCP surface** (feature doc: `odd/tasks/mcp-door.md`; PR
**#94**): the server shipped sixteen tools and `memory_context` was not
one of them, so the agent in opencode — and in the codex and claude code
integrations still to come — rebuilt the answer by hand, exactly what PR
#59 fixed for pi. The tool calls the same `MemoryService::context` the CLI
calls and serialises the typed envelope as it is; the differential leaves
the other six read tools' answers **byte-identical**. Item 11 is closed.

**What comes next.** The question the measurement sharpened: **which facts
are worth the answer's weight** — the typed priority that would decide it
(#55) is still the unmeasured experiment, and now it has a concrete form:
the door should carry the facts **when the task is about the facts**, and
carry less when it is not. And **M5**: the lean skill, with its own
experiment.

### 17. The lean skill — measured: the body is not read, and the harness costs more than the file

**M5, closed with its numbers** (`odd/tasks/skill-lean.md`; the criterion in
`tmp/bench/skill-lean-measurement.md`, written before any run; the numbers
in `tmp/bench/skill-lean-report.md`). Four arms, one image
(`bench-mem:whole`, mnexis 0.5.0-dev, pi 1.2.0), N = 10, three sessions per
run, nothing different between them except what is installed in
`~/.pi/agent` — verified on each arm's **real system prompt** before the
batch:

| arm | skill file | body | trigger | doctrine channel |
| --- | --- | --- | --- | --- |
| `fat` | 18 184 B | 17 447 ch | description | `read` → transcript |
| `lean` | 4 027 B | 3 362 ch | description | `read` → transcript |
| `head` | 1 103 B | ~350 ch | description | none |
| `bare` | **none** | — | extension section | extension section (`before_agent_start`, 3 794 ch) |

**The board's own claim was wrong, and the artifacts say so.** "The agent
reads the skill in 20 of 20 transcripts" is not true: what appears in every
prompt is the *description* pi injects (name, description, location), never
the body. Over the 672 transcripts this bench kept, **110 (16 %)** contain a
`read` of a SKILL.md, and in the recent batches 1-3 of 21 per arm. In this
batch the body was read in **7 of 30 write sessions and 1 of 60 measured
(trap + steering) sessions**; `fat`, with the most to read, read it **0
times in 30**.

**Behavior is indistinguishable, everywhere.** All four arms, 10 runs each:
the trap kept 10/10, the three typed state facts stated 10/10, the write
phase clean 10/10 (5 live records, all titled, no live twin), zero invented
values. `head` (350 characters of body) equals `fat` (17 447). Two runs
duplicated a fact and repaired it with `mnexis_supersede` **without reading
the rule**.

**Only the channel moves the price.** Per model call, `sum(tokens)/sum(calls)`:

| arm | write | trap | steering | prefix |
| --- | --- | --- | --- | --- |
| `fat` | 7 136 | 7 843 | 8 620 | 5 269 |
| `lean` | 7 984 | 7 460 | 8 147 | 5 269 |
| `head` | 7 110 | 7 358 | 7 990 | 5 269 |
| `bare` | 8 054 | 8 886 | **9 433** | **6 024** |

**C1 (behavior) holds for all four. C2 (the lean body saves ≥ 5 000 tokens)
holds on its number — 14 007 — and its mechanism is refuted: no skill was
read in any of the 40 steering sessions, and the gap is 5.0 against 6.0
model calls.** C3 (the harness beats the file) **fails**: `bare` forces
+755 tokens on every call and buys zero behavior; a file nobody reads is
free. C4 holds (zero invented, zero untitled, zero wrong namespace, zero
live twins).

**The open decision (the human's).** The pre-registered rule says "ship
`lean` if C2 holds". C2 holds, but its number is the call count, not the
body — shipping on it would be the same mistake the priced door made. The
honest options: keep `fat` and record that the body's bytes buy nothing
(the remaining argument is maintenance, not context); shrink the skill to
`head` because 350 characters passed everything; or measure the read rate
in the live host before deciding. **Nothing ships until the human picks.**

**The second batch (the human's idea), finished and scored.** *"Reducir la
skill y reforzarla con harness"* became a fourth arm, `combo` (1 103 B skill
+ a 678-character section with the four rules), measured on the chains where
the doctrine decides rather than on the phases where every arm scored
10/10: `dup` (the same fact saved under a dictated title: one record or two
twins?) and `cnf` (the value changed 8081 → 9090 with a prompt that hints at
nothing). The criterion is `tmp/bench/doctrine-measurement.md` (D1-D4,
written before the run). **80 of 80 chains, zero empty answers:**

| criterion | result | the number |
| --- | --- | --- |
| D1 the reinforcement pays | **HOLDS** | `combo` one live record 10/10, answer 8081 10/10 |
| D2 the insurance is priced | **FAILS** | +244 (dup) and +259 (cnf) tokens per call against `head`, bar was +200 |
| D3 no better than `head`, no ship | **FAILS** | 10/10 against 10/10 — `head` was already perfect |
| D4 nothing regresses | **FAILS, both hits are artifacts** | the 15 "live stale records" are all the *same* active row recalling the old value in its own sentence; the 1 "invented" is a hypothetical ("como 8080 o similar") |

**The reinforcement is not adopted**, on two independent grounds: `head`
with 1 103 B already reached 10/10 in the same chain, so the section could
at best tie (P1, registered before the run, called exactly this), and the
section costs more per call than its own ~170 tokens predict. D4 is
reported against the criterion as written, not corrected to fit: with the
prose mentions excluded, `cnf`'s one-record column is **10/10 in all four
arms** and the real stale count is 0, so the artifacts cannot flatter an
arm — and D2 and D3 fail on their own numbers either way.

Two store-side findings survive the batch. `fat` is the only arm that ended
with two live rows for the port (`ddup-fat9`, 1/10 — and it marked the
second one as an unverified user claim rather than overwriting the file's
value). `bare` is the only arm that wrote a wrong value: 2 of 10 `dup` runs
saved 9090 where no file says so, and 9090 is the number the doctrine
section itself uses as its example. `head` is equal or better than `fat` on
every measure and worse on none: `dup` 10/10 against 9/10, `cnf` equal at
10/10, and cheaper per call (5 829 / 6 005 against 6 202 / 7 101).

**What is still the human's:** the ship. The two candidates the measurement
leaves standing are keeping `fat` or shipping `head` — justified by
maintenance and by not being worse, never by tokens.

**And the direction the human declared at the cut:** the next build is a
**pi extension**, not more skill text — *"he visto que es mejor hacer una
extension de pi directamente da mas control"*. This batch sharpens it: the
harness channel is where the control is, but it is **not** free — a bare
harness pays +755 tokens on every call and, in the doctrine chains, wrote a
wrong value twice where the description alone never did. An extension earns
its place by carrying something a `read` cannot reach, not by restating the
file.

**One correction to that number, checked against `pi` 1.3.0's
`docs/extensions.md`:** both harness arms were injected through
`systemPromptOptions.appendSystemPrompt` (`patch_extension_with_section` in
`tmp/bench/run/run-all.sh`), which is the system prompt itself — a permanent
prefix, and a changed prefix is a provider cache miss. The docs recommend
`sections` / `promptGuidelines`, which pi diffs into **one patch message**
while keeping the cached prefix. So the +755 and the +244/+259 are the price
of **that injection point**, not of extensions: the cheaper path was never
measured. And the extension mnexis ships today
(`extensions/mnexis-memory/index.js`) has no `before_agent_start` at all — it
registers tools and a `session_start` handler — so the arms measured a hook
the product does not have. Neither correction revives the combo: D3 failed
on behavior, and `head` needed no section to reach 10/10.

### 18. The enforcement arm — the control does buy something, and it is the store

**Feature `extension-enforcement`** (`odd/tasks/extension-enforcement.md`; the
criterion in `tmp/bench/extension-recovery-measurement.md`, written before the
run; the numbers in `tmp/bench/skill-lean-report.md`, `## The enforcement
arm`). Two arms, one image, one binary and **the same skill (`fat`) in both**:
`actual` is the extension `mnexis agent install --env pi` ships today, `nueva`
is that same extension plus store recovery and post-compaction context
recovery. N = 10 per arm per chain, 80 chains, zero empty answers, zero
invalid runs. The two recoveries ride one arm by the human's decision, so this
section — not the design — separates them.

| criterion | result | the number |
| --- | --- | --- |
| R0 the arms are what they claim | **HOLDS** | `actual` wrote 0 intervention lines |
| R1 the enforcement repairs the twin | **HOLDS** | `rec`: `nueva` one live record 10/10, `actual` **0/10** |
| R2 no regression | **HOLDS** | `dup` 10/10 vs 10/10, `cnf` 5/10 vs 4/10, answers 10/10 everywhere |
| R3 the fact survives a compaction | **HOLDS** | `cmp`: 8081 answered 10/10, 10/10 runs compacted |
| R4 enforcement is cheap | **HOLDS** | dup +35, cnf −2 069, rec +72, cmp +179 tokens per call (bar +200) |
| R5 the hook is real | **HOLDS** | `rec` 10 interventions, `cmp` 10, `dup` not exercised, `cnf` not applicable |

**Three findings worth keeping.** (1) The store recovery is real: in `rec`,
where the promulgated title is asserted as the user's exact request, the
advising arm left **two live rows in all ten runs** and the enforcing arm left
**one in all ten**, for **+72 tokens per call**. (2) That also corrects batch
2: the doctrine does not break on an unfamiliar title (10/10 in `head` there),
it breaks on an **imposed** one. (3) The context recovery bought nothing: in
`cmp` both arms answered 8081 10/10 — `actual` by searching the store as the
product intends — while the injection saved no calls (median 16.0 against
15.5) and cost **+179 tokens per call**.

**What is still the human's:** whether to productize the store half, which is
bench-only today, and the two M5 decisions that item 17 left open (ship `fat`
or `head`; whether to correct the doctrine scorer's two artifacts). The
context half is not worth shipping on this evidence.

### 19. Store recovery productized — and the ceiling it declares

**Feature `store-recovery`** (`odd/tasks/store-recovery.md`, Log L1-L5). The
enforcement arm's winning half is now in the shipped extension
(`extensions/mnexis-memory/index.js`): a save that would leave two live records
for one fact supersedes **every** qualifying twin onto the saved record, so the
fact ends with one live record. The context half stays out: it answered nothing
and cost +179 tokens per call (item 18).

**Two rounds of independent verification shaped it, and both found real
defects.** The first refused the change because the candidate page was the 50
*oldest* rows — the repair was silently inert in any project with more than 50
records, reproduced at 64 records with five live twins and zero repairs. The
fix searches by the value the twin must carry and escalates to an exhaustive
walk when the ranked answer saturates. The second round confirmed B2-B4 closed
(only genuine twins are retired; every twin, not one; no test claims a history
row the engine never writes) and left one edge open: the walk stops at 2 000
records. That is **declared, not hidden** — the answer the model reads says the
duplicate check did not finish and that a second live record may remain.

**Known limits, on purpose:** the 2 000-record ceiling (an engine filter on
value would remove the walk entirely — a separate feature); the repair is not
transactional, so a concurrent save racing the same fact can leave a twin for
the next one; a twin whose fact carries no number is out of scope by the
measured rule.

**Evidence:** `node --test "extensions/mnexis-memory/test/*.test.js"` → 158
tests, 156 pass, 2 skipped (the parity suite is opt-in); with the parity binary,
`MNEXIS_PARITY_BIN=$HOME/.local/bin/mnexis node --test
"extensions/mnexis-memory/test/parity.recovery.test.js"` → 2/2 against mnexis
0.5.0-dev in a temporary data directory.
