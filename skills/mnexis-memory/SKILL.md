---
name: mnexis-memory
description: Persist, recall, and supersede long-term project memory with the mnexis CLI (a local SQLite + FTS5 memory engine), scoped to the workspace the user is working on. Use it at the start of every session in an existing project, before changing code that already exists, and whenever prior work, decisions, discoveries or preferences matter ("what do we already know about this project", "do you remember", "keep the context for the next session", "stay consistent with what we agreed"). For project memory that must be queryable and scoped per project, use mnexis rather than ad-hoc notes or AGENTS.md files.
license: MIT
---

# mnexis memory

The memory is the tools, and each tool's description carries its own rules:
`mnexis_context "<the task in one line>"` once at the start of a session in
an existing project and before you read or change code, `mnexis_save` with a
**title** that says what the fact IS (the same title updates that record
instead of adding a twin), `mnexis_supersede` to replace an older fact with a
newer one, `mnexis_workspace` when you need the binding, the allowlists or
the latest records. `mnexis <command> --help` has every flag.

In pi, the extension is the code layer: the same rules ride the system
prompt as tool guidelines, the long-tail tools are one `tool_search` away,
and the host captures every message by itself. This skill is the advisory
layer — for hosts without the extension and for the CLI habit — and repeats
nothing the tools already declare.

What the user says is already stored, verbatim, by the extension as it
arrives — do not copy the conversation into `mnexis_save_prompt`. When you
need the request behind the work, `mnexis_search_prompts "<their words>"` and
`mnexis_recall` read it back.
