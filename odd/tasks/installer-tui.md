# Installer + TUI — mnexis

Two deliverables over main@82f3102 (E0–E4 closed): a cargo-building `install.sh`
with uninstall support, and a full-CRUD ratatui TUI behind `mnexis tui`.

## Specs

- **S1 (installer).** User: "quiero un instalador y desinstalador" — decided scope:
  "Script que compila con cargo".
  - `install.sh` at repo root; default action builds and installs, `./install.sh uninstall`
    removes the installation.
  - Builds with cargo (release profile); detects missing cargo/rustc and fails with a
    clear message pointing to rustup.
  - Installs the `mnexis` binary to `~/.local/bin/mnexis` (created if missing); prints
    PATH guidance when `~/.local/bin` is not on PATH.
  - `uninstall` removes the binary; `uninstall --purge` also removes profile data and
    config (exact paths derived from `src/config/`, XDG-based) behind an explicit
    interactive confirmation. No GitHub Releases / binary-download path in this version.
- **S2 (TUI).** User: "y una tui" — decided scope: "CRUD completo".
  - Entry: `mnexis tui` subcommand (requires an initialised profile; same errors as CLI).
  - Three-pane layout: namespaces / memories / detail.
  - Read surface: browse namespaces, list memories, FTS5 search (`/`), detail pane with
    revision history and supersession chain.
  - Write surface: save form (`n`), update form (`e`) with validation and
    optimistic-locking revision errors surfaced, supersede (`s`), delete (`d`),
    restore (`r`).
  - ratatui + crossterm, exact pinned versions per repo convention; pure UI layer over
    the existing `MemoryService` — no new domain logic in the TUI.
- **S3 (non-goals).** No binary-download install path, no theming/plugin system, no
  context-preview envelope mode (deferred), no Windows/macOS-specific handling yet
  (Linux host per standing instruction).

## Tasks

| ID  | Spec | Route         | Commit |
| --- | ---- | ------------- | ------ |
| T1  | S1   | worker        | 0504f48 feat(installer): cargo-building install.sh with uninstall --purge |
| T2  | S2   | worker        | 293aa55 feat(tui): scaffold mnexis tui three-pane read surface |
| T3  | S2   | worker        | 71498be feat(tui): save and update forms with validation |
| T4  | S2   | worker        | 43b8cea feat(service) + 083235a feat(tui) |
| T5  | S2   | worker        | test(tui): app-state unit tests + docs + help |
| T6  | S1   | verify        | independent verification of installer incl. purge |
| T7  | S2   | verify        | independent verification of TUI CRUD surface |

## Log

- L1 (2026-10-18): "ok quiero un instalador y desinstalador, y una tui"
- L2 (2026-10-18): Product decisions via ask_user_question: installer = "Script que
  compila con cargo" (over "Binarios + curl | sh" and "Híbrido por etapas"); TUI =
  "CRUD completo" (over "Solo lectura v1" and "CRUD + context preview").
- L3 (2026-10-18): T1 evidence: installer harness 62/62 green, suite 212/212 (204
  base + 8 TUI in flight); purge paths derived from src/cli/profile.rs:74-90;
  declined purge exits 1 by design; XDG wording deviation documented in docs/install.md.
- L4 (2026-10-18): T2 evidence: suite 212/212; supersession chain NOT covered —
  MemoryService lacks a links read method (needs application/domain/storage surface);
  detail pane shows an explicit note. Gap folded into T4.
- L5 (2026-10-18): Slice strategy note: dab3903+0504f48+293aa55 total 1782 authored
  lines (two parallel independent units, each with its own commit per the high-risk
  own-commit rule). Parent re-ran scripts/test-installer.sh (62/62) and cargo test (212/212) before
  committing; unused-var warnings (req_a, i) are pre-existing from e803ba9.
- L6 (2026-10-18): T3 evidence at 71498be: suite 224/224; CLI differential vs 293aa55
  identical (shared save_request/update_request builders); slice ~1240 lines kept as
  one green coherent unit (interleaved model/render/tests). Known gaps for T4:
  MemoryService lacks links and tags read methods; storage update_with_history_and_tags
  (memory_repo.rs:490) reports revision_conflict(expected) instead of the row's current
  revision — CLI conflict text misleading, TUI compensates.
- L7 (2026-10-18): T6 independent verification of 0504f48: overall PASS, no blockers.
  Advisories: A1 (harness missed the $HOME refusal branch) and A2 (stale init()
  citations in docs/install.md) fixed at 043fa66, harness now 66 assertions; A3
  (symlink-target note) accepted as documented advisory, no action.
- L8 (2026-10-18): Slice strategy: cumulative 3025 lines in 5 commits; each commit is a
  single coherent unit with recorded rationale. T4 will be committed as TWO slices:
  (a) service/storage layer, (b) TUI flows.
- L9 (2026-10-18): T4 evidence: suite 239/239 (224 + 15 new); conflict fix developed
  test-first (RED: RevisionConflict current_revision: 1 vs expected 2 -> GREEN);
  CLI differential vs 71498be shows exactly ONE intended diff (stale-update conflict
  message now reports the real current revision). Part A ~308 lines (43b8cea),
  Part B ~1105 lines (083235a) — same overshoot precedent as L6, causes recorded:
  three flows + deleted view + ~460 test lines. Restore required two extra service
  reads (list_deleted, get_including_deleted) because list/get hide soft-deleted rows.
- L10 (2026-10-18): Cumulative slice boundary: 4534 authored lines in 8 commits on
  feat/mnexis-installer-tui. Every commit is a single coherent unit; overshoot
  rationales in L5/L6/L9. Remaining slices (T5) must stay <=~500 lines.
- L11 (2026-10-18): Contract gaps found in T4, NOT changed (out of scope, need user
  decision): MemoryService::supersede ignores expected_new_revision and same-namespace/
  cycle rejection (docs/contracts/v1/README.md:22 requires both-revision checks);
  supersede reason is not persisted (needs migration 003 + doctor schema bump).
- L12 (2026-10-18): PAUSED at user request ("cuando el agente termine quedemos ahi")
  after T4. Remaining: T5 (docs/help + app-state unit tests), T7 (independent TUI
  verification), cierre (assess/review + report). Resume from T5.
