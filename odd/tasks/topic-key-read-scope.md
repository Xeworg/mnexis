# The topic-key suggestion asks in the wrong namespace — and the MCP answers in silence

Two surfaces of the same read disagree about which namespace a topic-key
suggestion is about, and both are wrong in the way that hurts: the CLI
refuses a read it could answer, and the MCP answers about a namespace the
caller never asked about.

## Specs

- **S1.** `mnexis suggest-topic-key "<title>"` inside a workspace bound to
  a namespace answers about **that** namespace. The namespace the workspace
  writes into is the one a save would land in, so it is the one whose
  records the suggestion must look at. Today the command answers about the
  profile's `default_namespace` and is then refused by the write guard,
  because the profile's default is not the workspace's namespace:

  ```
  $ mnexis suggest-topic-key "el puerto del servidor"
  error[FORBIDDEN]: forbidden: write: this workspace writes only into namespace 'mnexis'; run `mnexis project bind <namespace> --key git:github.com-xeworg-mnexis --force` to change it
  ```

  The refusal names the namespace the workspace already uses and asks the
  human to rebind the workspace **to the namespace it is already bound to**.
  There is nothing to rebind. The same command with the namespace spelled
  out answers correctly and finds the record:

  ```
  $ mnexis suggest-topic-key "el puerto del servidor" --namespace mnexis
  the key `el-puerto-del-servidor` is already recorded by 2df13a00-92d5-41cd-94fb-07faacf65ae8
  ```

- **S2.** The suggestion is a **read**: it asks which key a title yields and
  who holds it. It writes nothing, so the write isolation of the workspace
  must not refuse it. The read scope of the profile decides, as it decides
  for every other read. A workspace bound to `mnexis` must be able to ask
  about `mnexis` without a `--namespace`.

- **S3.** The MCP tool `memory_suggest_topic_key` answers about the **same**
  namespace the CLI answers about: the workspace's own namespace when the
  process runs inside a bound workspace. Today it answers about the
  profile's `default_namespace` — a namespace the caller never named — and
  it does so **silently**, with a plausible answer:

  ```
  $ mnexis mcp serve   # memory_suggest_topic_key {"title": "el puerto del servidor"}
  {"recorded_by": null, "title": "el puerto del servidor", "topic_key": "el-puerto-del-servidor"}
  ```

  `recorded_by: null` says the key is free. It is not free: the record
  exists in the namespace the workspace writes into. An agent that asks
  before saving — which is what the tool is for — is told to go ahead and
  create a second record for the same fact. That is the whole failure the
  topic key exists to prevent.

- **S4.** The refusal that S1 replaces must not exist any more for a
  namespace the workspace is bound to. A **write** into another namespace
  is still refused with FORBIDDEN and still names the command that changes
  it. The write guard keeps covering save, update, supersede, delete and
  restore; the suggestion is not one of them.

- **S5.** The answer of the suggestion names the namespace it asked about,
  so a human reading it can tell which project was consulted. Today the
  answer never says, and the MCP's answer does not even name the namespace
  it used.

- **S6.** The help of the command and the description of the MCP tool say
  which namespace the suggestion is about. Today both say the profile's
  `default_namespace`, which is the wrong one and is what the code does.

## Tasks

| ID | Spec | Route | Commit |
|---|---|---|---|
| T1 | S1, S2, S4, S6 | the CLI: the suggestion asks in the workspace's namespace and no longer passes the write guard; the help and the answer name it | 5d5c344 |
| T2 | S3, S5 | the MCP: the suggestion asks in the workspace's namespace, and the answer names it | 5d5c344 |
| T3 | S1–S6 | the tests that pin both surfaces, and the docs that describe the contract | e4a2fec, 5d5c344 |
| T4 | S1–S6 | the delivery: the review slice and the PR | PR |

**The slice boundary, and why the work is two units.** The slice
heuristic is about 400 authored lines, and the whole feature is over it
(726). The work therefore lands as two reviewable units rather than one
long one, and the split is not cosmetic: the first unit is the **guard**,
the second is the **behavior**. The guard is what a review must take
alone, because it is the part that can leak.

- `9814e48` — the guard: `topic_for_title`'s `None` branch authorizes the
  read of the scope's default namespace, as the named branch already did.
  Its own regression test, one per surface.
- `5d5c344` — the behavior: the CLI, the MCP, their help and their
  descriptions.

`13bd1cb` is this document, and it carries the reproduction and the review
that found the regression (L7).

## Log

- L1 (2026-10-27, the user, verbatim): "luego trabajemos en el bug que haz
  encontrado". The bug is the one this document describes: the suggestion
  of a topic key, the read that tells an agent whether the fact it is
  about to record already exists.
- L2 (2026-10-27, the exploration). The reproduction, in a scratch profile
  bound to `mnexis` with a record saved under that namespace:
  `mnexis suggest-topic-key "el puerto del servidor"` exits **4** with
  `forbidden: write: this workspace writes only into namespace 'mnexis';
  run mnexis project bind <namespace> --key git:github.com-xeworg-mnexis
  --force to change it`; with `--namespace mnexis` it exits 0 and answers
  `the key 'el-puerto-del-servidor' is already recorded by
  2df13a00-...`. The MCP tool with the same title answers
  `recorded_by: null` — it consulted the profile's `default_namespace`,
  where the record does not exist.
- L3 (2026-10-27, the exploration). Why the CLI fails: `suggest_topic_key`
  (src/cli/commands.rs) resolves the namespace with
  `namespace.unwrap_or_else(|| profile.default_namespace())` and then calls
  `WorkspaceGuard::authorize(&namespace)` — the **write** guard, added by
  `da9f12a` ("feat(cli): refuse writes that leave the current workspace"),
  which covers save, update, supersede, delete and restore. The suggestion
  is a read and was never meant to be in that set; it inherits the guard
  because it resolves a namespace the same way a save does.
- L4 (2026-10-27, the exploration). Why the MCP fails: `do_suggest_topic_key`
  (src/mcp/server.rs) passes `args.namespace.as_deref()` straight to
  `svc.topic_for_title`, and the service falls back to
  `self.scope.default_namespace` when the namespace is `None`. The MCP
  server already knows the workspace's namespace (`workspace_namespace`) and
  already uses it for the recall (`recall_previous_session`); the suggestion
  does not.
- L5 (2026-10-27, the exploration). The rule the fix follows is the one the
  door already follows: a read without an explicit namespace asks in the
  **workspace's** namespace, the one a save lands in. `read_namespace`
  (src/application/workspace.rs) is the helper that resolves it, and it is
  what `mnexis context` uses. The suggestion must use the same helper and
  not invent a second rule.
- L6 (2026-10-27, the exploration). The pi extension is **not** affected:
  `mnexis_suggest_topic_key` resolves the workspace namespace itself
  (`readNamespace(params.namespace) ?? workspace.namespace`) and passes it
  explicitly, so it never hits the CLI's default. Its tests
  (extensions/mnexis-memory/test/tools.test.js) already pin that. The
  defect lives in the CLI and the MCP, the two surfaces that resolve the
  namespace themselves.
- L7 (2026-10-27, the implementation and the review). The unit came back
  with the rule implemented and its tests green, and the differential run
  was clean for every command but one. **The review found a regression the
  unit's own tests did not see**, in the shape the harness warns about: the
  service's read of a namespace the profile may not read.
  `topic_for_title` (src/application/service.rs) authorizes the read when a
  namespace is named, but its `None` branch took the scope's default
  namespace **without authorizing it**. The CLI had always passed the
  namespace explicitly, so the hole was covered; the new CLI passes `None`
  when the workspace names none, and the hole opened. The MCP already had
  it: it passed `None` whenever the caller named no namespace.
  Measured, with a profile whose `default_namespace` is `secreto` and whose
  read allowlist is `["default"]`, a record saved in `secreto`, and the
  suggestion asked for its title:

  ```
  base CLI: error[NOT_FOUND]: not found: namespace:secreto (run `mnexis config allow-read-namespace secreto` to grant read access)
  new CLI:  the key `puerto-del-servidor` is free in namespace `secreto`   ← the leak
  base MCP: {"recorded_by": null, "topic_key": "puerto-del-servidor"}      ← the same leak, already there
  new MCP:  the same, plus the namespace it read
  ```

  The answer "the key is free" is itself the disclosure: it tells a reader
  that a namespace exists and that the key is not taken in it, and a key
  that IS taken discloses the record's uuid. The fix is at the root, not in
  the callers: the `None` branch of `topic_for_title` authorizes the read of
  the scope's default namespace exactly as the named branch does, so the
  fallback is safe for every caller — the CLI, the MCP, and any future one.
  Two tests pin it, one per surface (a profile whose default namespace is
  unreadable must be refused with NOT_FOUND, exactly as `list --namespace`
  refuses it), and both were observed RED with the service fix reverted and
  GREEN with it.
