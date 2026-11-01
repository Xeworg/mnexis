# The answer that names what landed — measured on a host that talks MCP

Objective: measure whether an agent that reads the memory's **type** (and **revision**) in the
MCP `memory_save` answer behaves differently from one that reads the base answer — on a host
that talks MCP directly. Branch `bench/opencode-type` (worktree `tmp/wt-bench4`), based on
`main` at `4d8fb45`. The change under test lives on `feat/mcp-save-type` (`b7beb53` + the
revision unit). Test runner: the podman harness of `bench/checklist-nudge`, reused.

## Specs

- **S1 (the claim, falsifiable)** — the answer names what landed: on an MCP-direct host, an
  agent that reads the record's type and revision in the save answer **writes the type more
  often** and **re-saves less** — the re-save being the path that rewrites a record's type.
  The claim's two halves are measured separately, because they can move in opposite directions.
- **S2 (the arms, one variable)** — same image family, same fixture, same prompt, same model,
  same host. The **only** difference is the content of the save answer:
  - `A base` — the engine at `f93d473`: the answer is `{keywords, row_id, uuid}`.
  - `B names` — the engine of `feat/mcp-save-type`: the answer adds `memory_type` and
    `revision`.
- **S3 (the metrics, and what each one can and cannot say)** —
  - **(i) the axis**: the share of `memory_save` calls that carry an explicit `memory_type`
    argument — the metric the type programme already uses (B1), so the two are comparable.
  - **(ii) the churn**: the number of saves on a `title`/`topic_key` the same session already
    saved — the path that rewrites a record's type.
  - **(iii) the rewrite**: the number of saves that **changed** an existing record's type.
  - **(iv) the honesty**: invented values, and values contradicting the fixture — both must be
    **zero**.
  - **(v) the cost**: billed tokens per run, and the model's call count.
- **S4 (the gates, pre-registered before any run)** — `A`'s rate is read from the same runs,
  never assumed:
  - **T-G1 (the host runs)** — ≥ 10 complete, scored transcripts per arm, none missing, and no
    session that died on a credential or a provider error. Fewer → report, no verdict.
  - **T-G2 (the axis does not pay)** — B's typed-save share **≥ A's**. The answer must not cost
    the axis.
  - **T-G3 (the churn moves)** — B's churn rate **≤ A's**, with the difference read from the
    same runs. This is the mechanism under test; a null here is a legitimate, publishable
    result.
  - **T-G4 (nothing is invented)** — **zero** invented values and **zero** contradicting values
    in both arms.
  - **T-G5 (the cost)** — B's billed tokens per run **≤ 1.05 ×** A's. The answer is a few bytes
    in one reply, not a second call.
  - **T-G6 (the honest limit)** — one model, one host, one fixture, N = 10 per arm. A null
    T-G3 is a statement about *that* model on *that* fixture, not about the answer in general.
- **S5 (the fixture)** — the fixture must make the temptation real: the session must save
  something, then have a reason to **re-save the same title** with a correction (the churn
  path), and the answer's content must be *readable* by the agent — i.e. the host shows the
  answer text to the model, which is true for the MCP path and false for the CLI path on pi.
- **S6 (the pre-flight, before any batch)** — the feasibility of an **opencode** host, because
  the image has never carried one: the binary, the headless invocation, the transcript's shape
  and where the tool calls and their arguments land, the provider configuration in a container,
  and whether the plugin's curated seat runs headless (if it does not, the tool descriptions the
  model sees differ — a confound that must be held equal across arms or declared). **If the host
  cannot be run headless with a scored transcript, the bench reports that and does not spend the
  batch.** The cost of one opencode session, measured, is part of the pre-flight.

## Tasks

| ID | Specs | Route | Branch / worktree | Commit |
| --- | --- | --- | --- | --- |
| T1 | S6, and the parts of S2/S3 that depend on the host | delegated: `gentle-ai-explore` (read-only) — the opencode host's feasibility, with sources | `bench/opencode-type` @ `tmp/wt-bench4` | — |
| T2 | S1–S6 | done: the criterion's seal (`tmp/bench/opencode-type-measurement.md`, `44322ffb…` over 17 339 bytes, revision 2 `76052e83…` over 20 557), the two arms (`bench-oc:base` = `f93d473`, `bench-oc:names` = `feat/mcp-save-type`), the fixture (`fixtures/type-churn`), the runner (`run/opencode-type-bench.sh`) and the scorer (`run/score-opencode-type.py`) | `bench/opencode-type` @ `tmp/wt-bench4` | — |
| T3 | S4 | done: the pilot cleared all five conditions, the batch was spent (N = 10 per arm, 20 sessions), the gates were scored, and the report is `tmp/bench/opencode-type-report.md` | `bench/opencode-type` @ `tmp/wt-bench4` | — |

## Log

- **L1 (2026-11-02, the human, verbatim)** — "pero agrega una mejora?" and then "dale". The
  question was whether naming the type in the MCP answer is an *improvement* or only parity, and
  the answer was: it is parity and measurability — **no bench has measured any behavioural
  effect**, and the rationale inherited from #107 (a model told "Saved" re-saves to tidy the
  duplicate, and the same-title upsert rewrites the record's type) is an argument, not a
  measurement. The human approved measuring it.
- **L2 (2026-11-02, why this host)** — the MCP surface is reached directly by opencode (and,
  later, by codex and claude code); on pi the extension already shows the type, so the answer's
  content is invisible to the model there. The host under test is therefore the one the change
  can reach, and it has **never** been benched: the image carries pi, node and engram, and no
  opencode.
- **L3 (2026-11-02, the honest frame)** — the board's own measured lesson cuts both ways here:
  *more description cannot move what survives* (the enriched nudge, null at 1.14× cost). The
  answer's content is not description in a prompt — it is data in a reply the model can act on
  — but it is equally unmeasured until this bench runs. The falsifier is T-G3.
- **L4 (2026-11-02, T1 — the opencode host's feasibility, measured on the sources)** — a
  read-only explorer answered S6 (and the host-dependent parts of S2/S3). **What exists:**
  - **The headless invocation** is `opencode run --standalone --model <provider>/<model>
    --format json --auto "<prompt>"`: `--format json` writes **JSONL on stdout** (one event per
    line), `--auto` is what answers the permission prompts (pi's `--approve` equivalent), and
    the provider and the model travel together in `--model` (`opencode-go/deepseek-v4.1-flash`).
    Sources: the v2 CLI commands page and `packages/opencode/src/cli/cmd/run.ts`.
  - **The transcript is enough to score both metrics**: the run emits one `tool_use` record per
    completed tool call, carrying the tool name (`part.tool`, prefixed `mnexis_`), the
    **arguments** (`part.state.input`, where `memory_type`, `title` and `topic_key` live) and
    **the result** (`part.state.output` — the answer this bench varies), with the session id at
    the top level. Metric (i) = the share of `mnexis_memory_save` calls with an explicit
    `memory_type`; metric (ii) = the saves whose `title`/`topic_key` was already saved in the
    same `sessionID`.
  - **The MCP path is confirmed**: the engine writes the entry itself with `"codemode": false`
    (`src/application/opencode_config.rs:145-155`), and opencode 2's own docs say codemode
    "Defaults to `true`" and that `false` exposes the tools "directly instead of through Code
    Mode" — so the model calls `memory_save` as a direct tool and reads `do_save`'s JSON. The
    host's own `~/.config/opencode/opencode.json` already carries that entry.
  - **The install**: the npm package is **`@opencode/cli`** (2.2.5; the launcher plus a platform
    binary fetched at install time), node 24 in the image is fine, and the install **needs the
    network at build time** — the image chain is otherwise local COPY/RUN, so this is one new
    layer.
  - **The credential**: opencode-go is a **built-in** provider on this host — its key lives in
    opencode's own credential store (`~/.local/share/opencode/opencode.db`, the `credential`
    table, `integration_id = 'opencode-go'`), not in the environment. **The bench must read the
    key from there at run time and must never write it into the repo, the doc or the logs**;
    the bench's own `secrets.env` carries a key for pi, which is not automatically the one
    opencode reads. Whether the built-in provider accepts `OPENCODE_API_KEY` from the
    environment is **not confirmable from the docs** and is a pre-flight item.
  - **The plugin's seat is a confound**: the curated description of `mnexis_memory_save` names
    the seven kinds ("Save it typed — `memory_type` is the one that fits:" + the glosses), while
    the engine's own description names no vocabulary. That primes metric (i) directly. So the
    plugin must be **byte-identical in both arms**, and the bench must record **which**
    description reached the model (if the transform does not fire headless, both arms see the
    engine's text — a weaker but honest baseline, and it must be said so). The seat is a server
    transform, not a UI one, so it should fire headless — **not empirically confirmed**.
  - **What the harness lacks**: the binary in the image, the provider entry, a valid key, the
    extension install for the opencode env (`mnexis agent install --env opencode` plus the
    plugin's own `install.sh` — the engine cannot install the plugin itself,
    `src/application/agents.rs:194`), the transcript format, a scorer over `tool_use` (the
    arg-level scorer of the types bench is **not** in the worktree and must be rewritten), and
    the engine edit that puts `memory_type`/`revision` in the save answer (which is
    `feat/mcp-save-type`: `b7beb53` + `b4d1beb`).
  - **The cost does not transfer**: the pi numbers (~$0.0027 per session, ~$0.00025 per model
    call) are the parent's figures and **no file in the worktree carries them**; opencode sends
    its own system prompt plus 17 MCP tool schemas, may take more steps per session, and has its
    own cache behaviour. opencode reports the bill per step (`step_finish.part.cost`), so the
    session's cost is measurable from the same transcript — **measure it in the pilot, do not
    transfer it**.
- **L5 (2026-11-02, a correction)** — this document lives in the worktree
  (`tmp/wt-bench4/odd/tasks/opencode-type-bench.md`), not in the main checkout; the first
  explorer was pointed at the wrong path and answered from the parent's brief instead. Its
  answer stands on its sources, but the locator was wrong and is fixed here.

- **L6 (2026-10-31, T2+T3 — the bench ran, and the answer bought nothing; the document's other
  entries carry `2026-11-02`, this one carries the clock the artifacts carry)** — the host was never
  benched, and now it is. **What the pilot settled:** opencode `2.2.5` runs headless in the container
  (`opencode run --standalone --model opencode-go/deepseek-v4.1-flash --format json --auto`), the
  bench's own key (`cf78c94d2257`) authenticates `opencode-go` from `OPENCODE_API_KEY` alone — no
  credential store read, nothing written into the repo — the transcript carries the tool calls with
  their arguments and their answers (both metrics scoreable), and **the plugin's curated description
  fires headless in both arms** (`Save it typed — memory_type is the one that fits:` reaches the
  model), so metric (i) starts at the ceiling on this host. **What the batch measured** (N = 10 per
  arm, 20 sessions, `tmp/bench/opencode-type-report.md`): typed share **1.000 vs 1.000** (T-G2 holds,
  trivially — the axis cannot move), churn rate **0.250 vs 0.421** (T-G3 **fails**, the direction is
  against S1's claim), contradicting records **0 vs 1** (T-G4 **fails** — one session left the
  stage-1 value 5000 in the record while its workspace ended at 3000), billed tokens/run **190 522.3
  vs 203 719.9 = 1.069×** (T-G5 **fails**; median 1.004×, and 1.011× without the one 0-save run).
  Provider cost per session: **$0.00** for this model, measured from the transcript (`step_finish`),
  never transferred from pi. **The criterion was sealed before the first bench run** (`44322ffb…`)
  and revised **once**, before the batch, as revision 2 (`76052e83…`): v1's honesty rule fired on the
  fixture's own two-stage design (the stage-1 record legitimately states the value of its stage), so
  metric (iv)'s unit became "the record the session leaves must not contradict the workspace it
  leaves". The gates did not move. A scorer bug found while scoring (the last save *with* a number vs
  the last save, whose claim may be unverifiable) is disclosed with both readings in the measurement
  file's appendix B.3: `contradicting` is 0 vs 1 under the sealed sentence, 0 vs 3 under the literal
  bug. **The reading this bench supports:** on a host that carries the plugin, naming the type and
  revision in the save answer is confirmation of a vocabulary the model already has, not instruction —
  it does not move the axis, it does not lower the churn, and it can coincide with a stale record.
  That is a statement about this model, this host, this fixture and N = 10 (T-G6), and it is the null
  S4 pre-registered as publishable.
