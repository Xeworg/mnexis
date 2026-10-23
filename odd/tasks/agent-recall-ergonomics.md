# agent-recall-ergonomics

Make the recall cheap for the agent that pays for it: a list is a list, a
topic key accepts the separator the agents already use, and the open
question of the injected protocol is answered with a measurement instead of
an opinion.

This is the next block of the list that came out of the comparison with the
other engine (B1, B2 and B3). B3 is not a behavior to ship: it is a
question, and it gets measured.

## Specs

- **S1** — "A list is a list, not a dump". `mnexis list` returns what is
  there — the identity of every memory and a preview of its content — and
  not the whole text of every memory. The complete record stays one call
  away (`mnexis show <uuid>`), and `--full` asks for the old shape
  explicitly. The same projection reaches the agent through the MCP
  `memory_list` tool and the pi extension.
- **S2** — "The `topic_key` accepts the separator the agents already use".
  The accepted charset becomes `[a-z0-9./-]`: the slash is what an agent
  writes when it groups facts (`payments-api/server-config`), and today it
  is rejected with `invalid character: '/'`. The rejection message names
  the whole allowed set, and the CLI help, the MCP schema and the skill
  say the same thing.
- **S3** — "The agent's tools say which call to make". The description of
  the list tool says what the list holds and that `show` holds the rest,
  so the agent does not read twenty memories to learn that one of them
  matters.
- **S4** — The open question is measured, not argued: an injected protocol
  block (the other engine's channel) is run against the same container,
  the same model and the same prompts, and the result decides whether
  mnexis adopts it. The finding is recorded either way.

**Not in scope:** the schema, the store format, the search contract (the
`agent-query-contract` feature) and the permission model.

## Tasks

- **T1** (S1) — The compact list projection on the CLI, with `--full` for
  the complete records. Route: inline. Commit: `feat(list): a list is a
  list, not a dump`.
- **T2** (S2) — The `topic_key` charset accepts the slash, and the three
  places that document it say the same thing. Route: inline. Commit:
  `feat(save): accept the slash in a topic key`.
- **T3** (S1, S3) — The agent-facing surfaces: the MCP `memory_list` tool,
  the pi extension's `mnexis_list`, and the tool descriptions. Route:
  inline. Commit: `feat(agent): the list tool says what it holds`.
- **T4** (S4) — The measurement of the injected protocol block, in the
  container, against the same prompts and the same model. Route: inline.
  Commit: `docs(benchmark): the protocol block, measured`.
- **T5** — Verification: the suites, the acceptance in the container, and
  the PR. Route: inline. Commit: `docs(odd): close agent-recall-ergonomics`.

## Log

- **L1** (2026-10-23) — The comparison with the other engine, run in a
  container with pi and the same model, ended with a list of improvement
  points. B1 came from a measured waste: the recall session listed the
  memories with the full content of every one of them (9.3 KB for five
  memories) when it only needed to know what was there. B2 came from a
  measured rejection: the key an agent writes the way it groups facts
  (`payments-api/server-config`) is refused, while the other engine's
  convention uses exactly that.
- **L2** (2026-10-23) — B3 (an injected protocol block) is **not** a
  behavior to ship on an opinion: the measurements so far show the agent
  already recalls and already saves (2 memory calls in the recall session,
  5 saves in the write session). T4 measures it before anyone adopts it.
- **L3** (2026-10-23) — Branch `feat/agent-recall-ergonomics`, base `main`
  at the merge of PR #22 (`agent-query-contract`).
