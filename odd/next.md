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

Everything is merged. `main` is clean and only `main` remains locally.

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

The harness: `tmp/bench/run/run-conflict.sh` (the conflict experiment),
`tmp/bench/run/run-all.sh` (the write/recall experiment),
`tmp/bench/run/summarize.py`, `tmp/bench/run/score-conflict.py`.

## What comes next, with the evidence behind each

### 1. The scan of near-duplicate titles — the gap the title leaves

Two *different* titles for one fact (`puerto del servicio` and `puerto de
pagos`) are two keys and two records. The other engine proposes relations
**afterwards** (`conflicts scan`) and the agent adjudicates them. mnexis has
the relations (`was replaced by`) and the supersede chain, but no scan.

This is the small version of the conflicts layer that was measured and
discarded in its candidates-and-verdicts form: not "the agent judges every
conflict", but "the engine notices two titles that are probably the same
fact and proposes the link". Measure it with the same harness before
building it: does the agent act on a proposed link?

### 2. C3 — `doctor` with a stable JSON envelope

Five checks today, no contract. The other engine has ten named checks with a
machine envelope (`status`, `summary`, `checks[].check_id`, `severity`,
`reason_code`, `evidence`, `safe_next_step`), a `--check CODE` filter and
`doctor repair --plan|--dry-run|--apply` for seven specific codes. Small, and
it is what an operator needs when something goes wrong.

### 3. C4 — the operational commands

`mnexis stats`, `mnexis timeline <uuid>`, `mnexis projects list`. Inspecting
a namespace without opening the TUI. Small.

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
