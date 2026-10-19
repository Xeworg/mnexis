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
| T1  | S1   | worker        | feat(installer): cargo-building install.sh with uninstall --purge |
| T2  | S2   | worker        | feat(tui): scaffold `mnexis tui` 3-pane read surface |
| T3  | S2   | worker        | feat(tui): save and update forms with validation |
| T4  | S2   | worker        | feat(tui): supersede, delete, restore flows |
| T5  | S2   | worker        | test(tui): app-state unit tests + docs + help |
| T6  | S1   | verify        | independent verification of installer incl. purge |
| T7  | S2   | verify        | independent verification of TUI CRUD surface |

## Log

- L1 (2026-10-18): "ok quiero un instalador y desinstalador, y una tui"
- L2 (2026-10-18): Product decisions via ask_user_question: installer = "Script que
  compila con cargo" (over "Binarios + curl | sh" and "Híbrido por etapas"); TUI =
  "CRUD completo" (over "Solo lectura v1" and "CRUD + context preview").
