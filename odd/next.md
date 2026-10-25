# The board — what is done, what is open, what comes next

**Last updated:** 2026-10-23. Read this first in a new session, then the
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

Everything is merged. `main` is clean and only `main` remains locally (plus
the archived branches of the features that were measured and discarded, and
`feat/near-duplicate-titles`, archived and **not** merged — see item 1c).

**Released: v0.3.0** (2026-10-23), the identity work. The schema is at
version 7 and there is no downgrade: an old binary refuses to open a newer
store.

## The measurement that runs the board

The comparison with the other engine (in a container, pi, the same model) is
what every decision comes from. The artifacts are in `tmp/bench/` (gitignored
— copy them out before the sandbox is wiped):

- `tmp/bench/report.md` — the comparison and the capability matrix.
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
`tmp/bench/run/score-duplicates.py`.

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

### 5. The prompt/session layer is in, but thin

`session-and-prompts` landed the tables, the tools and the hooks. What is
missing: the **recall** of a session (the agent can save a prompt and search
prompts, but cannot ask "what happened in the session before this one"), and
the **conflict layer** proper (a judgment the agent records, and the verdict
annotated back into the search results).

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
  about "the agent now does X" needs more runs than we did.
- The recall is still not free: the agent reads the memories it finds. A
  memory whose content is long costs context every time it is recalled.
- mnexis still cannot tell that two different titles mean the same fact.
  That is item 1 above.
