# Supersede contract closure — mnexis

Close the L11 contract gaps of `MemoryService::supersede` against
docs/contracts/v1/README.md, over main@17141ec (installer-tui delivered).

## Specs

- **S1 (contract semantics).** docs/contracts/v1/README.md (State and mutation
  rules) requires verbatim: "Supersede is atomic, same-namespace in MVP,
  revision-checked on both records, and must reject cycles."
  - Atomic: the older state change and the link insert commit together or
    not at all; a failed link insert must not leave a superseded older row.
  - Same-namespace: older and newer must live in the same namespace;
    cross-namespace supersede is rejected (stable `INVALID_ARGUMENT`).
  - Both-revision check: a stale `expected_new_revision` writes nothing and
    returns `REVISION_CONFLICT` carrying the newer row's current revision
    (today only the older side is checked; the TUI mirrors client-side).
  - Cycle rejection: a link older→newer that closes a cycle in the
    supersession graph is rejected (`INVALID_ARGUMENT`), including the
    trivial self-link older==newer.
- **S2 (persisted reason).** L11 (installer-tui log): "supersede reason is
  not persisted (needs migration 003 + doctor schema bump)". The request's
  `reason` must be stored on the link row (migration 003 adds the column),
  returned by the links read, and rendered in the TUI supersession chain.
  Existing link rows keep an empty reason (no backfill).
- **S3 (surfaces).** The service becomes the authoritative gate; CLI and MCP
  wire shapes do not change (they already carry `expected_new_revision` and
  `reason`). The TUI keeps its client-side mirror for fast feedback but the
  service rejects independently. `mnexis doctor` accepts schema version 3.
- **S4 (non-goals).** No backfill of reasons for pre-migration links, no new
  error labels (stable vocabulary only), no contract schema freeze change,
  no cross-namespace supersede support.

## Tasks

| ID  | Spec    | Route   | Commit |
| --- | ------- | ------- | ------ |
| T1  | S1,S2   | worker  | 7d67562 (with T2: one behavior unit) |
| T2  | S1      | worker  | 7d67562 feat(supersede) |
| T3  | S2      | worker  | 7d67562 (doctor v3 travels with the migration) |
| T4  | S2,S3   | worker  | a344e8e feat(tui) |
| T5  | S1-S3   | verify  | PASS at a16a309; probe test 82f939f |

## Log

- L1 (2026-10-19): User "dale" to the proposal "arranco el L11 como feature
  nueva con su documento ODD" (follow-up recorded at installer-tui L16).
  Authority: docs/contracts/v1/README.md mutation rules + installer-tui L11.
- L2 (2026-10-19): RED first: five service-level tests written against the
  public MemoryService (stale expected_new_revision, cross-namespace pair,
  self-link, seeded-cycle closure via raw save_link on a second connection,
  persisted reason). Compile enabler = MemoryLink.reason field. Observed
  RED 5/5, then GREEN after implementation. Suite 249/249 (244 + 5); the
  existing migrations test updated to pin version 3 + the reason column
  (notnull=1 with DEFAULT '', verified against pragma_table_info).
- L3 (2026-10-19): Slice boundary: 7d67562 = the contract unit
  (migration 003 + supersede_atomic tx + service gate + doctor v3 + tests,
  ~330 authored lines, Risk: item 1 stored-data migration — own commit);
  a344e8e = TUI reason rendering + docs (~70). Cumulative 404 authored
  lines in 2 commits. Cycle note: the happy path can never close a cycle
  (superseded is terminal), so the RED setup seeds raw links outside the
  service — the CTE still guards imported/external graphs.
- L4 (2026-10-19): CLI differential vs 17141ec (cold build of the base in a
  tmp/ worktree, per-binary isolated profiles, uuid/timestamps normalized):
  exactly ONE intended behavioral diff — stale --expected-new-revision now
  exits 5 REVISION_CONFLICT (current is 2) writing nothing; happy supersede,
  list, and doctor identical. Migration verified live: NEW on a v2 profile
  auto-migrates (doctor all green, data intact); OLD on the migrated v3
  profile refuses with UNSUPPORTED_VERSION (no-downgrade rule).
- L5 (2026-10-19): T5 independent verification at a16a309: overall PASS, no
  blockers, no advisories. All 8 items PASS with file:line evidence + live
  reproduction through the public binary (atomicity, same-ns, both-revision
  conflicts carrying the disk revision, seeded-cycle CTE walk, migration 003
  + no-backfill read-back, unchanged CLI/MCP wire shapes, TUI mirror kept,
  fmt clean). The verifier's durable probes: #1 (non-active older gate, a real
  pre-existing coverage gap in touched code) ACCEPTED at 82f939f; #2 depth-3
  cycle DECLINED (depth-2 already exercises the recursive step); #3 pre-
  migration empty reason DECLINED (pinned by migrations test + verifier's live
  check); #4 doctor v3 DECLINED (pinned by the three doctor integration
  tests). Verifier scratch dirs cleaned.
- L6 (2026-10-19): Feature closed. T1..T5 done; suite 250/250; CI chain green.
  Slice boundary: 7d67562 (contract unit, Risk: item 1 migration, own commit)
  + a344e8e (TUI reason) + a16a309/82f939f (docs/probe); 500 authored lines
  in 4 commits + doc commits. Merge/push remains the user's call.
