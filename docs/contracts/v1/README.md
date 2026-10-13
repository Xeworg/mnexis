# Mnexis contracts v1

These JSON Schemas define the first wire-format draft from PRD v0.4. They are closed (`additionalProperties: false`) where an operation shape is defined. `v1` is a prototype contract, not a compatibility promise; freeze only after E0 scenario review.

## Scope and authorization

A server process receives an immutable `AccessScope` from trusted configuration, never from a tool argument:

- `read_namespaces`: exact namespace allowlist; `write_namespaces`: exact namespace allowlist.
- `default_namespace`: required and must be in the relevant allowlist.
- `allow_global`: explicit separate permission; global is never implicitly included.
- `allowed_operations`: operation allowlist. Administrative purge/import/configuration are CLI-only in the MVP.
- `all` resolves to the process's allowed namespaces, not the whole profile.

Authorize every read and write route, including IDs, history and cursors; resolve filters and limits before candidate retrieval and revalidate before output. Out-of-scope IDs return `FORBIDDEN` or the non-disclosing `NOT_FOUND` policy selected by the service, without exposing existence or contents. Direct shell access to profile files is outside this boundary.

## State and mutation rules

- `proposed`, `disputed`, `superseded`, `archived`, expired, and deleted memories are excluded from default context. Only `active` and currently valid records are eligible.
- Every update supplies `expected_revision`; a mismatch writes nothing and returns `REVISION_CONFLICT` with current revision.
- A `topic_key` identifies a concept within namespace and never silently overwrites; explicit update is required. Idempotency keys are scoped by actor and namespace; same key/different canonical request is a conflict.
- Supersede is atomic, same-namespace in MVP, revision-checked on both records, and must reject cycles.
- Provenance requires actor, channel, and UTC creation time. `assertion_basis` identifies source, not confidence.

## Context budget

`token_budget` is enforced against the complete serialized response (including provenance and envelope) when `tokenizer_id` is supported. Otherwise callers must explicitly permit estimated mode, which enforces a hard response-byte ceiling and labels the result `estimated`; exact-only callers fail. Select whole items only; never silently truncate or summarize. Return an empty selection when no item fits; return `BUDGET_TOO_SMALL` if even the minimum envelope does not fit.

## Stable error vocabulary

`INVALID_ARGUMENT`, `NOT_FOUND`, `FORBIDDEN`, `REVISION_CONFLICT`, `TOPIC_CONFLICT`, `BUSY`, `INDEX_UNAVAILABLE`, `BUDGET_TOO_SMALL`, `CURSOR_EXPIRED`, `UNSUPPORTED_VERSION`.

Error responses and default logs never contain memory content. Limits from PRD v0.4: memory 16 KiB UTF-8, task/query 8 KiB, 32 tags, 20 default / 100 maximum results, 20 context items, import 50 MiB / 100,000 records, plus a response byte cap to be fixed in E0.

## Scenarios

See `../../../../fixtures/e0/` for representative input/expected-outcome cases. These are contract examples, not proof of a running implementation. Outbox crash/replay and two-process behavior remain E1 experiments.
