# canonical-binding-shadow

Fix what the independent verification of `agent-query-contract` found in the
canonical workspace key: a registry written before the rule could keep a
stale entry that shadows the canonical one, and the recovery the CLI
advised did not work.

## Specs

- **S1** — "One workspace, one binding". A binding recorded with any
  spelling of the key (`git:github.com/owner/repo`, the full URL, the
  normalized form) is one entry. Binding the same workspace again updates
  that entry **in place**: the registry never holds two entries for one
  workspace, and the entry it holds is the canonical one.
- **S2** — "The stale entry cannot shadow the canonical one". The lookup
  compares the canonical forms on both sides, so a registry written before
  the rule resolves to the namespace it always resolved to — and a bind
  over it replaces the stale key instead of appending a second entry that
  the resolver would never reach.
- **S3** — "The advice the CLI gives has to work". The `FORBIDDEN` message
  a write receives when the workspace resolves to another namespace tells
  the human to bind the workspace again with `--force`; after this change
  that command reaches the binding it names instead of stopping at the
  stale copy.
- **S4** — "The shipped extension must not describe a behaviour that no
  longer exists". `extensions/mnexis-memory/README.md` is embedded in the
  binary and installed into every user's agent directory: it described the
  removed auto-bind, so it is corrected.

**Not in scope:** the search contract, the store format, the schema and the
permission model.

## Tasks

- **T1** (S1, S2) — The key rule moves to the domain layer
  (`src/domain/workspace.rs`), so the registry that stores keys and the
  resolver that reads them share one implementation, and the registry
  compares canonically. Route: inline. Commit: `fix(projects): one
  workspace, one binding`.
- **T2** (S1, S2, S3) — The tests that pin it: a legacy raw binding
  resolves, and binding over it updates it in place without duplicating
  it. The test is hermetic: it runs in a workspace with a known remote, so
  it does not depend on the checkout it runs in. Route: inline. Commit:
  `test(projects): the legacy binding is updated in place`.
- **T3** (S4, and the rest of the verification) — The shipped README, the
  dead code the change left behind, the FTS5 error mapping that had to be
  pinned by a test instead of by an input that no longer exists, the help
  text, and the durable probes the verification left behind. Route: inline.
  Commit: `docs(extension): the shipped README must match the behaviour`.

## Log

- **L1** (2026-10-23) — The independent verification of PR #22 (against
  the released 0.2.0 binary) reproduced both defects and confirmed the
  contract impact, but found three things the change did not get right:
  - **HIGH**: a registry written before the rule keeps its stale raw entry,
    the canonical bind appends a second one, and the resolver takes the
    first canonical match — so the stale entry wins forever. The `FORBIDDEN`
    message then advises a rebind with `--force` that cannot work. This is
    the default state of an existing user's registry.
  - **MEDIUM**: the shipped extension README still described the removed
    auto-bind, and it is installed into every user's agent directory.
  - **MEDIUM**: the new legacy-binding test did not pin the defect (it
    passed on 0.2.0 too) and it depended on the remote of the checkout it
    ran in.
- **L2** (2026-10-23) — Two low findings are accepted as trade-offs and
  not changed here: `any` + stopwords can return the whole store (the best
  match still ranks first, and `--match all` is the precise mode), and the
  operator words (`and`, `or`, `not`, `near`) can no longer be searched as
  words (they are terms in `--match phrase`). Both are documented in the
  feature document of the change they belong to.
- **L3** (2026-10-23) — Branch `fix/canonical-binding-shadow`, base `main`
  at the merge of PR #22.
