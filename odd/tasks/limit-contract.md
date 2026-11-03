# The limits are declared and actionable

The engine enforces byte and count caps; until now the caps the caller can
hit were the ones it could not see. The answer named a count and a number
("exceeds max count: 40 > 32") and stopped there, so an agent that hit one
re-tried the same way and failed the same way. The fix is two-sided: the
answer names the limit, whose limit it is and the way out, and the schema
the model reads declares the cap before the caller hits it.

## Specs

- **S1** — "si ya me paso con otro agente que se preguntaba por que no lo dejaba guardar arreglalo"
  (the user's words, 2026-11-03). The answer to a cap tells the caller what
  the limit is, whose limit it is, and what to do next; a bare count is not
  an answer.
- **S2** — The keywords' cap counts the **union**: the words the record
  already carries plus the ones the request adds. When the union crosses the
  cap, the answer says how the count is made ("the record already carried 2,
  and you are adding 31") and names the escape (`replace_keywords`).
- **S3** — A save that asks for more than the cap on its own (no record
  underneath it) says "Send at most 32 words"; a list that **replaces** the
  record's words says that it has to fit on its own.
- **S4** — The content, the topic key and the query name the limit in
  effect, say it is the profile's, and say what to do ("Shorten the query",
  "Trim it, or split it into several records"). The tags and a single
  keyword word name their cap too.
- **S5** — The schema the model reads declares the caps it cannot guess:
  `memory_save.content` (16 KiB, `max_content_bytes`), `memory_save.topic_key`
  (128 bytes, `max_topic_key_bytes`), `memory_save.keywords` (48 bytes per
  word), `memory_search.query` and `memory_search_prompts.query` (8 KiB,
  `max_query_bytes`). The numbers are the engine's defaults and the setting
  that moves them.
- **S6** — The schema never declares a cap the engine does not enforce: the
  door's `task` is not bounded by the query's cap, and the schema says
  nothing about it.
- **S7** — The pi extension's `mnexis_save` declares `replace_keywords` and
  passes it to the engine, so the escape the engine's answer names exists on
  that surface too. Its `keywords` description names the 32-word cap.
- **S8** — Nothing else changes: the caps are the same caps, the refusals
  are the same refusals, and a save that fits behaves exactly as before.

## Tasks

- **T1** (S1, S2, S3, S4) — the engine's answers name the limit and the way
  out — inline, `src/application/service.rs`.
- **T2** (S5, S6) — the MCP schema declares the caps — inline,
  `src/mcp/tools.rs`.
- **T3** (S5) — the CLI's help declares the same caps — inline,
  `src/cli/mod.rs`.
- **T4** (S7) — the pi extension's save declares and sends
  `replace_keywords` — inline, `extensions/mnexis-memory/{index.ts,lib/cli.ts}`.
- **T5** (S1-S8) — the tests that hold the contract — inline,
  `tests/{application_service,cli_commands,mcp_interop}.rs` and
  `extensions/mnexis-memory/test/tools.test.js`.
- **T6** (S8) — the differential against the previous binary — inline, a
  scratch script (`/tmp/mnexis-diff/run.sh`), never committed.

## Log

- **L1** — the user's request, verbatim: "me habias dicho algo que querias
  mejorar del mcp que no dice el limite" / "si ya me paso con otro agente
  que se preguntaba por que no lo deja guardar arreglalo" (2026-11-03). The
  earlier turn: "por donde quedamos" — the state was reconstructed from
  memory and git before this feature started.
- **L2** — The gap was named in the programme's own records before it was
  fixed: `program.pending` (mnexis, rev 21) says the cap is not a contract
  hole — `replace_keywords` exists and the cap counts the union — and that
  what failed was the **path of the error**: `validate_keywords` answered
  "exceeds max count: 40 > 32" without saying how many words the record
  already carried nor naming the escape, and the agent that did not know the
  escape re-tried with fewer words and failed again (twice in one session,
  2026-11-03).
- **L3** — What the limit says where the caller can see it before hitting it:
  the engine enforced `max_content_bytes` (16 KiB), `max_query_bytes`
  (8 KiB), `max_topic_key_bytes` (128) and `max_tags` (32), and the schema
  declared only the two that were not the ones that bite. The content cap is
  the one that bites: it is what a save of a log hits.
- **L4** — RED first. Seven service-level tests, one CLI test and one MCP
  schema test, all failing on the old messages before a line of the
  implementation moved.
- **L5** — The engine's answers now read: "exceeds max count: 40 > 32. The
  count is the union: the record already carried 2, and you are adding 31.
  Send replace_keywords to prune the record's words, or send fewer new
  words." and "exceeds max bytes: 20000 > 16384 (the profile's limit). Trim
  it, or split it into several records."
- **L6** — **The differential caught a false claim of my own**: the schema
  declared an 8 KiB cap on the door's `task`, and the door does not enforce
  one — it reads the index directly (`service.rs:722`, `repo.search_in_typed`),
  bypassing `validate_query`. The declaration was withdrawn, and the schema
  test now says the door must not claim a cap the engine does not enforce.
  This is the reason the differential runs against the previous binary and
  not against the expectation.
- **L7** — The pi extension was the surface where the escape did not exist:
  it declared `keywords` ("They accumulate across saves") and had no
  `replace_keywords` at all, so an agent that hit the union cap there was
  told to use a flag the tool did not have. The flag exists now
  (`--replace-keywords`; the CLI's flag is kebab-case, not
  `--replace_keywords`).
- **L8** — Suites: cargo green (every binary), the MCP interop 43, the
  service 67, the CLI 184; the pi extension 320 pass / 2 fail (the two
  path-dependent failures that predate this work, proven by running the same
  tree from another path); the opencode plugin 80 pass / 0 fail / 1 skip.
- **L9** — Not committed: the tree carries the change and the human decides
  the branch, the PR and the merge.
- **L10** — The delivery, branch `fix/limit-contract` over main `3975363`: the
  slice boundary is **one contract, three surfaces**, and the authored total is
  ~540 lines, past the ~400-line review unit. It stays one PR because the
  three slices are the same contract and a reader cannot judge any of them
  alone: `549373c` (the engine's answers and the tests that hold them),
  `3dc7b94` (the schema and the help that declare the caps), and the last
  slice (the pi extension's escape, `S7`). A PR that carried only the first
  would leave the schema claiming a cap and the extension missing the flag.
- **L11** — Delivered: PR #123 (`fix/limit-contract`, three commits) and PR
  #124 (`style/rustfmt-drift`, the pre-existing `cargo fmt` drift from
  #119-#121, on its own branch so review stays focused). The merge of both is
  the human's decision. Meanwhile the host was repaired with the human's
  explicit OK: the release built from this branch was installed at
  `~/.local/bin/mnexis` and `mnexis agent install --env pi` rewrote
  `~/.gentle-shell/agent/extensions/mnexis-memory` (both files compared
  byte-identical to the tree). Verified against the **installed** binary on a
  scratch profile: the content cap says "the profile's limit. Trim it, or
  split it into several records.", and the union case says "the record
  already carried 2, and you are adding 31. Send replace_keywords to prune
  the record's words" — and `--replace-keywords` on that same record answered
  `revision 2` with the new words. The opaque error is gone from the host.
