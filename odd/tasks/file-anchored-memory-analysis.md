# File-anchored memory — the analysis: what the idea is, what the engine already has for it, the seven decisions, and the pilot that tests the claim before anything is built

Status: **analysis, nothing built**. This document orders the human's idea
(recorded verbatim in the Log, L1), grounds it in what the engine and the
programme's measurements already say, and separates what can be decided by
analysis from what only a measurement can decide.

## The idea, ordered

The paragraph carries seven distinct parts. Named, in dependency order:

1. **The anchor** — memory attached to *files and what is inside them*:
   functions, classes, interfaces, and the connections between them, read
   out of the source by tree-sitter.
2. **The zone** — those memories live *apart* from the topic-keyed
   memories the engine manages today, in new tables that know about file
   types.
3. **The visibility** — an agent working on a file sees what *another AI,
   in another session*, learned and saved about that file.
4. **The monitor** — a mechanism that keeps checking the files (the idea
   says "como lo hace git") so the memories whose anchor disappeared can be
   retired.
5. **The fast route** — a path so the agent *arrives faster*: less reading
   of the file, less re-deriving of what a prior session already knew.
6. **The justification** — the agent that changes a file must *justify the
   change* it made.
7. **The claim** — a *considerable saving of tokens*.

Parts 1–4 are architecture. Parts 5–6 are behavior. Part 7 is the claim
that decides whether any of it is worth building — and this programme has
learned, three times this cycle, that a plausible claim about agent
behavior is not evidence until it is measured (the checklist, the assembly,
and the opencode benches each found the baseline at the ceiling).

## What the engine already has that this builds on

- **The anchor doctrine, already written.** `src/application/git_state.rs`
  stores the part that does not rot (the branch, the commit, the canonical
  remote) and *derives* the state at read time, "because a stored state
  would rot exactly like a stored branch name." A file anchor is the same
  doctrine one level down: store `(workspace, path, language, entity kind,
  name, body hash)`; derive liveness by re-parsing at read time. Nothing
  rots in a table.
- **Provenance.** Every memory already carries `actor` and `channel`, and
  sessions record the model that ran them. "What an AI saved about this
  file" is a query the store can almost answer today.
- **The door.** `mnexis context` is the budgeted one-call answer. The
  re-comparison measured what happens when the door exists in the engine
  but not in the integration: the agent rebuilt it by hand, 45
  `mnexis_show` calls across ten sessions, 4 111 input tokens against
  engram's 1 851. **The lesson is the feature's biggest risk:** a
  file-memory surface that lives only in the engine will be invisible to
  the agent that needs it. Whatever is built must ship in the pi extension
  (and the opencode plugin) in the same unit.
- **The pointer rule.** The nudge analysis established that a fact about
  how something behaves survives when it carries a citation the harness can
  verify. A code anchor is a *machine-checkable* citation — the entity
  exists, at this path, with this body hash — stronger than any prose
  citation. File memory is the first memory class whose provenance the
  engine can audit without trusting the model.
- **The workspace guard.** File anchors are per-workspace by nature; the
  namespace binding and the read allowlists already decide which workspace
  may write where and which may read what. A file-memory zone inherits
  that fence or it is a hole in it.
- **The program question it answers.** Board item 26 closed the assembly
  bench with: the class still at risk is the facts the model *cannot audit
  by reading a tool* — what a prior session learned is exactly that class
  (it is in no transcript the model can read). File-anchored memory is a
  durable channel for that class, with an anchor the engine can verify.

## The honest tensions

- **Staleness is the killer, and the programme has already met it.** The
  opencode bench's one contradicting record lived in the treatment arm: a
  stale memory is not neutral, it is *worse than none* — the agent trusts
  it and has to unlearn it. Code changes constantly; a memory about old
  code misleads on every recall until it is retired. The whole feature
  stands or falls on the retirement discipline, not on the extraction.
- **Two identity axes in one engine.** The store's doctrine is one fact,
  one record, title-derived `topic_key`, upsert by title. File memory is
  many observations, one anchor. If the two axes mix silently, the upsert
  semantics that #107 and the idempotency hash depend on get muddied. The
  human's instinct — "una zona aparte" — is the right reflex; the decision
  below makes it explicit.
- **The surface costs bytes on every call.** The trim bench priced the
  tools block at 9 014 B → 5 490 B and called it −11.5 % tokens per call.
  A new tool with file-anchor descriptions is paid by every session,
  whether it touches code or not. The surface must be one door, not a
  corridor of tools.
- **"Connections" is not a file parse.** Tree-sitter parses one file into
  its entities; the *connections* (who calls whom, who implements what)
  are a project index — import resolution, cross-file, cross-crate. The
  idea (L2) puts them in scope with the right rule: they are **detected
  from the program's own source** — imports first, declarative and robust
  — and never asserted by the AI. The file-context answer has to list
  them explicitly (D5). Call-level resolution (which of the many `parse`
  calls which) stays phased behind the import graph.
- **Git does not watch.** The idea asks for a monitor "como lo hace git" —
  but git runs no daemon: it fingerprints and diffs *on demand*. A
  filesystem-watcher daemon contradicts the engine's shape (a CLI invoked
  per call, no resident process) and adds state that can drift. The
  git-like model is the comparison, not the watcher.

## The seven decisions

**D1 — The anchor.** What a file memory is attached to.
(a) path only; (b) path + entity name; (c) path + entity kind + name +
body-hash fingerprint.
*Recommendation (c).* The hash is what makes retirement decidable (the
body changed → the memory refers to an older body) and what makes the
citation verifiable. Names alone lie across refactors; paths alone lie
across moves. Renames/moves degrade gracefully to "anchor unresolved",
never to a wrong match.

**D2 — The zone.** New tables vs. a relation over the existing store.
(a) `file_memories` — a dedicated table with its own revisions and state;
(b) `memories` + a `file_anchor` relation (anchor_id → memory row).
*Recommendation (a), the dedicated zone* — the human's instinct. The
access pattern is different (exact anchor lookup, not FTS ranking), the
identity is different (anchor, not title), and the retirement lifecycle is
different (liveness derived from the tree, not from supersede links).
Reuse the *machinery* (revision history, soft-delete, provenance columns)
by sharing the storage patterns, not the table.

**D3 — The monitor.** Daemon vs. on-demand diff.
*Recommendation: on-demand, the git_state doctrine.* At read time (and at
save time), re-parse the file — tree-sitter is local and cheap — and
derive: new entities (unanchored growth), missing entities (orphaned
anchors), changed hashes (memories that refer to an older body). A
`mnexis file scan` subcommand can do it in bulk for the TUI and for
curiosity. No process runs between calls.

**D4 — Retirement.** Delete the unused vs. mark it dead.
*Recommendation: mark dead, never silently delete.* A memory about a
deleted function is still evidence (why it went, what it did), and this
engine's doctrine never destroys evidence quietly — soft-delete with a
reason exists for exactly this. A dead anchor answers "this memory refers
to `parse_keywords`, last seen at hash H, gone now", which is information
the next agent can use, and it is purgeable later through the existing
purge path.

**D5 — The surface.** Where the agent meets it.
Engine: one door — `mnexis file-context <path>` / `memory_file_context`,
budgeted like `context`, answering the anchor's memories + the entity map
+ the liveness verdict in one call. Integration: the pi extension exposes
the door, and the chrome attaches the digest to file reads — the "fast
route" the idea wants is *attaching* prior knowledge to the read the agent
was going to make anyway, not a second call it has to remember to issue.
Every nudge bench this cycle says the same thing: what the agent has to
remember to do, it does not do.

**D6 — The justification.** What "justify the change" means.
(a) a write-gate (no edit without an updated memory) — coercive, noisy,
and unmeasured; (b) the delta-aware nudge: when the session changed a file
that has anchors, the turn-end nudge carries the *derived delta* (entities
added, removed, changed) and asks the agent to record what the change
means — the coverage rule applied to code anchors, reusing the harness the
assembly bench already exercised.
*Recommendation (b).* The delta is computed by the engine (D3), so the
prompt is evidence, not a reminder; and the mechanism is the one the
programme already knows how to measure.

The answer also has to **list the connections explicitly** — the imports
this file makes and (when the project index has run) the files that
import it, each edge named — so "which files does this touch" is read,
not guessed (L2).

**D7 — Tree-sitter in the stack.** All dependencies are pinned (`=`) and
the binary is deliberately small. Tree-sitter brings the core crate plus
one grammar crate per language, each with C to build.
*Recommendation: phase it.* v0 needs no parser at all (see the pilot);
v1 takes Rust first (this repo, dogfood), then TS/JS, then Python.

**D8 — The abstraction layer (L2, decided L3).** Tree-sitter behind a
narrow trait, not spread through the engine. The entity vocabulary is
**per language** (L3): the Rust adapter answers `function`, `struct`,
`trait`, `impl` — what the language actually has — and mnexis defines
only the *shape* every adapter must answer, not a forced universal set.
A language with no grammar (or a binary file) has no adapter, and the
file is **out of the zone entirely** (L3): no anchors, no memories. The
door still says so plainly — "unsupported language, no anchors here" —
because an ignored file that answers with silence looks like a file
nobody visited. The layer also fences the dependency: tree-sitter and
its grammar crates stay behind the trait, replaceable per language, and
the pinned-dependency policy applies per grammar.

**D9 — The ignore discipline (L2).** The scan set is exactly git's view
of the worktree: `.gitignore`, `.git/info/exclude`, the global excludes
— a file git ignores, mnexis ignores. Untracked-but-not-ignored files
are in (they are work git can see). Binary and unparseable files are out
of the zone (D8). No second ignore vocabulary in v0: no
`.mnexisignore`, no config lists — one source of truth, git's.

**D10 — Authorship and lifecycle (L3).** Only the AI authors: every
memory in the zone is something an agent chose to save, and the entity
map and the connection structure are always *derived from the source*,
never stored as memories. But the operations that would cost the AI —
detecting that a file or an entity disappeared, and touching the
memories that referred to it — are mnexis's: when the tree says an
anchor is gone, the engine marks the memory dead itself (D3/D4),
because the engine is the one deleting the anchor. The AI writes; the
engine retires.

**D11 — The content model (L4): the why and the what of an entity.** The
zone's canonical memory is the one the agent writes when it *creates*
something: why this function exists, what it is for. The recovery path
the idea wants — look up the file, and what was saved about this function
or class is there — means the door's first-class answer is **intent, not
trivia**. This ties D6 to the delta: when the nudge sees a new entity,
its natural question is "why".

**D12 — Zone search (L4): entity-scoped, never mixed.** The zone does
not grow `mnexis search` results: no FTS on the zone in v1, and if
free-text over the zone ever comes, it is **its own index** — the search
machinery is reused, the *results* are never blended with the topic-keyed
memories. And the search the idea actually asks for — by function,
method or class — needs no FTS at all: it is an anchor lookup on plain
indexes (path, kind, name).

**D13 — Worktrees and clones (L4): one zone per repository.** The anchor
key is `repo_key` (the canonical remote) + repo-relative path, so every
worktree and every clone of the same repository **shares one zone**: a
memory saved from `tmp/wt-bench3` about `src/application/service.rs` is
visible from the main checkout. Branch divergence is what `recorded_hash`
is for: the same relative file on another branch is a different body, and
the door says "written against a different body" instead of lying.

**D14 — Accumulation, newest first (L5).** Memories accumulate per anchor
— "I changed X for this reason" is one more observation, never a
replacement — and the door lists them **newest first** under its budget.
The old memory is never deleted on update (L5): the history of *why* is
the value. Provenance of branch and commit comes from the session's git
anchor (`session_id` → the session already records branch, commit and
remote); a denormalized `branch` label rides on the memory as cheap
provenance that survives session cleanup — a label, never identity
(branches rot; anchor + hash do not).

**D15 — Death now, deletion when main confirms (L5).** A dead memory of
a deleted file is eventually **deleted** — but a file missing from *your*
branch may still live on main, so the sequence is: `dead` when the local
tree says the anchor is gone; **real deletion only when the deletion has
landed in main** (the git-state machinery already answers
merged/not_merged/unknown). No `purge_after` column: the policy is
event-driven, not time-driven.

**D16 — Moves and renames: the hybrid (decided, L6).** An entity that
appears in another file with the **same name and same body hash** is the
same thing moved — migrate the anchor, keep the memories, record a
`path_migrated` event (a rule of ours, exact, no heuristics). A whole
renamed file uses `git diff -M` (heuristic; on failure it falls back).
What cannot be confirmed is not migrated — honest death.

**D17 — Saves are voluntary; the justification is on the save (L7).**
Nobody forces the agent to memorize what it does not know — no write
gate, no quota. But the moment it *does* save, the memory must carry its
why: the mechanical evidence (anchor, `recorded_hash`, branch) is
attached by the engine, and the save itself states the reason — the
house already has the pattern (`change_reason` on every history row).
Proposed shape: `memory_file_save` takes a required `reason`, stored with
the memory and shown at the door. *(Schema addition pending the human's
ok.)* This also amends the pilot: P0 is confirmed (L7), and its nudge
asks — it never obliges.

## The proposed schema (migration 011, pending a/b/d below)

Four tables, house conventions (numbered migration, `strftime`
timestamps, CHECK enums, history by revision):

```sql
CREATE TABLE IF NOT EXISTS file_anchors (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    namespace    TEXT NOT NULL,
    repo_key     TEXT NOT NULL DEFAULT '',  -- canonical git remote ('' if no git)
    path         TEXT NOT NULL,             -- repo-relative, '/' separators
    language     TEXT NOT NULL,             -- only languages with an adapter (D8)
    entity_kind  TEXT NOT NULL DEFAULT 'file',  -- per-language vocabulary; 'file' is universal
    entity_name  TEXT NOT NULL DEFAULT '',      -- '' for the file-level anchor
    body_hash    TEXT NOT NULL,             -- sha256 of the entity body at last sighting
    last_seen_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    UNIQUE (namespace, repo_key, path, entity_kind, entity_name)
);
CREATE INDEX IF NOT EXISTS file_anchors_by_path
    ON file_anchors(namespace, repo_key, path);

CREATE TABLE IF NOT EXISTS file_memories (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    anchor_id      INTEGER NOT NULL REFERENCES file_anchors(id),
    namespace      TEXT NOT NULL,           -- denormalized: the fence is never crossed
    content        TEXT NOT NULL,           -- only the AI authors (D10); the why and the what (D11)
    recorded_hash  TEXT NOT NULL,           -- body hash WHEN the AI wrote it: staleness is derived
    revision       INTEGER NOT NULL DEFAULT 1,
    state          TEXT NOT NULL DEFAULT 'active' CHECK (state IN ('active','dead')),
    dead_reason    TEXT,                    -- 'entity_removed' | 'file_removed'
    dead_at        TEXT,
    actor          TEXT NOT NULL,
    channel        TEXT NOT NULL,
    session_id     TEXT,               -- provenance chain: the session holds branch+commit+remote
    branch         TEXT,               -- denormalized provenance label (D14); never identity
    created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);
CREATE INDEX IF NOT EXISTS file_memories_by_anchor
    ON file_memories(anchor_id, state);

CREATE TABLE IF NOT EXISTS file_memory_history (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    file_memory_id  INTEGER NOT NULL REFERENCES file_memories(id) ON DELETE CASCADE,
    revision        INTEGER NOT NULL,
    content         TEXT NOT NULL,
    recorded_hash   TEXT NOT NULL,
    state           TEXT NOT NULL,
    changed_by      TEXT NOT NULL,
    change_reason   TEXT NOT NULL,
    changed_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);

-- The import graph is a CACHE with hash-validated entries, not truth:
-- dropping the table loses time, never knowledge.
CREATE TABLE IF NOT EXISTS file_imports (
    namespace   TEXT NOT NULL,
    repo_key    TEXT NOT NULL DEFAULT '',
    path        TEXT NOT NULL,
    imports     TEXT NOT NULL,   -- resolved in-repo edge; external imports are not stored
    file_hash   TEXT NOT NULL,   -- hash of `path` when the edge was computed
    computed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
    PRIMARY KEY (namespace, repo_key, path, imports)
);
CREATE INDEX IF NOT EXISTS file_imports_reverse ON file_imports(namespace, repo_key, imports);
```

Design notes: anchors are **lazy** (a row exists only where a memory
points; the full entity map is never stored — derived per parse, D10);
**death is persisted though life is derived** (a reverted file must not
resurrect a dead memory as truth); deletion of the dead happens only when
main confirms the file is gone (D15); `file_imports` self-validates by
hash and is recomputable at any time.

Decided on the schema: (a) accumulate per anchor, newest first, branch as
provenance label (D14); (b) dead → delete only when the deletion lands in
main, no `purge_after` (D15). Still open: (d) renames — the hybrid
proposal of D16 awaits the human's confirmation.

## The engine architecture (proposed L6, two points decided L8)

```
domain/          + file_anchor.rs        the anchor type, the parsed entity, the liveness verdict
application/     + adapters/             the D8 trait (LanguageAdapter) + registry + rust_adapter
                 + file_zone.rs          the zone service: context (the door), save, scan, delta
                 + ignore.rs             the D9 scan set: git's own view
storage/         + file_zone_repo.rs     the four tables' queries + migration 011
mcp/tools.rs     + memory_file_context   the door
                 + memory_file_save      the zone save
cli/             + mnexis file …         context | save | scan | list
extensions/      mnexis-memory           expose the door, digest on file-read chrome,
                                          delta wired into the turn-end nudge (D6)
```

New dependencies, all pinned: `tree-sitter`, `tree-sitter-rust` (v1),
`sha2` (a persisted, cross-version-stable body hash; std's DefaultHasher
is not). No daemon, no notify, no `ignore` crate.

**Decided (L8): two thin MCP tools.** Not for bytes (two short
descriptions cost the same as one long one) — for the house doctrine
that a field the tool does not read is an error: a single action-tool
would carry conditional fields no schema can express honestly.

**Decided (L8): the first scan is complete but gradual.** The project
gets fully scanned over time, in priority order — what the agent touches
first (the door primes touched files), the rest fills in while the
software runs. No all-or-nothing first barrier, and no reverse edge held
hostage to a manual command.

## The claim, and the pilot that tests it before anything is built

The claim: *considerable token savings*, because the agent (1) does not
re-read and re-derive what a prior session learned, (2) does not rebuild
the recall by hand (the 45-call lesson), (3) localizes faster.

The costs the claim has to beat: the surface bytes on every call (trim
bench), the save-time discipline, and the staleness tax — a misleading
memory costs the read *plus* the unlearning.

**P0 — the pilot, no infrastructure.** Hand-write file memories for two
or three hot files of this very repository (`src/application/service.rs`,
`src/cli/mod.rs`, `src/mcp/tools.rs` — files every session re-derives),
stored as ordinary memories with a conventional anchor line in the
content. Two arms, same model, same tasks: a code task that depends on
prior knowledge of those files, with and without the file memories
pre-loaded in the context. Gates pre-registered before the first run:
tokens-to-first-correct-edit, calls, and correctness. **Falsifier: the
arm with file memories does not save a meaningful fraction of tokens, or
degrades correctness.** If the falsifier fires, the idea dies here,
having cost an afternoon and no schema. If it holds, it justifies D1–D7
in order — and the pilot's hand-written anchors become v1's test
fixtures.

## Phasing

- **P0** — the pilot above. Measurement only. Decides everything.
- **P1** — the zone: tables + migration, the anchor model (D1/D2), the
  abstraction layer (D8) with the Rust adapter, the ignore discipline
  (D9), the import-graph connections derived from source, the door
  (`file-context`, D5 — with the connection list) in engine + pi
  extension, dead-marking (D3/D4).
- **P2** — the justification flow (D6): the delta-aware turn-end nudge,
  measured with the harness the assembly bench used.
- **P3** — call-level connections (name resolution across files), more
  grammars. Only if P1/P2 earned them.

## The decisions that are the human's

1. **P0 first?** The recommendation is yes — it is the only cheap test of
   the claim, and everything else is weeks.
2. **The zone (D2)** — confirm the dedicated tables over a relation.
3. **The monitor (D3)** — confirm on-demand over a daemon.
4. **Retirement (D4)** — confirm dead-mark over deletion.
5. **v1 languages (D7)** — Rust first?
6. **The degradation (D8)** — *decided (L3): out of the zone, visibly.*
7. **The connections' v0 scope** — *decided (L3): the import graph.*
8. **Authorship (D10)** — *decided (L3): the AI writes, the engine
   retires.*

## Log

- **L1 (2026-11-02, verbatim):** "he estado pensando que podria hacer una
  caracteristica que permita crear memoria por archivos, usar treesitter
  para hacer una lectura de conexiones, funicones, classes, interfaces,
  etc de archivos que se pueda guardar memoria en una zona aparte de las
  memorias que ya manejamos donde le agente pueda ver las memorias que una
  ia guardo respecto a ese archivo, tendriamos que crear nuevas tablas,
  las cuales soporten los tipos que tienen los archivos, este cambio
  tendria que tener tambien un monitor que constantemente este verificando
  los cambios en los archivos como lo hace git, para poder eliminar las
  memorias que ya no se ocupan, para poder hacer una ruta para que el
  agente llegue mas rapido, para hacer que el agente deba justificar el
  cambio que hizo, tendriamos un ahorro considerable de tokens, ordena la
  idea y armemos un documento para analizarla"
- **L2 (2026-11-02, verbatim):** "antes de empezar ocupamos tener algo
  claro no ocupamos todas las funciones del tree sitter, debemos crear una
  capa de abtraccion para poder solo traer lo que necesitamos ya que
  podemos toparnos con distintos lenguajes y algunos talvez o tengan
  soporte, debemos ignorar lo mismo que git, las conexiones entre archivos
  deben detectarse desde el programa y debe ser claro cuales son cuando la
  ia consulte el archivo con menxis agrega eso preguntame todo lo que
  pienses que falta de alcarar tambien"
- **L3 (2026-11-02, the questionnaire, answers verbatim):** sin grammar →
  "Se ignora el archivo"; conexiones v1 → "Grafo de imports"; quién
  escribe → "solo debe guardar la ia pero la detencion de funciones y
  toda la estructura de conexiones y las funciones que le costaran a la
  ia como eliminar un archivo las tendremos que hacer desde menexis ahi
  si tendra que tocar memorias ya que las esta eliminando"; entidades →
  "Por lenguaje".
- **L4 (2026-11-02, verbatim):** "lo que se me ocurre ejemplo que el
  agente explique por que creo esa funcion, metodo, que hace la idea es
  que la ia tenga su contexto mas facil luego, con solo buscar el archivo
  que se guardo de esta funcion ahi, o de esta clase, entiendes y fts
  talvez no sea bueno que aun permita la busqueda podria aumentar
  consideablemente los resultados de busqueda al menos no mezclaria las
  memorias de la ia con las memorias sobre los archivos podriamos
  reutilizar el motor de busqueda pero siempre debe ser una busqueda por
  funcion o metodo o clase ya me tiendes y si los worktrees es una de las
  dudas que tengo como se podria manejar"
- **L5 (2026-11-02, verbatim):** "eso hay que evaluar que tiene mas
  utilidad si acumular memorias dando prioridad a la ultima ejemplo
  cambie x por tal razon y adjuntamos la rama que ya tenemos la opcion de
  hacer esto, no creo que valga la pena solo borrar la memoria vieja y
  ya, las memorias muertas si ya son de un archivo eliminado seria mejor
  eliminar, pero hasta que esto ya llegue al main podria ser, y el ultimo
  punto ocupo que me lo expliques explicame d"
- **L6 (2026-11-02):** the human takes the hybrid (D16) and asks for the
  engine architecture; proposed: domain/file_anchor, application/adapters
  (the D8 trait + rust_adapter), application/file_zone (context/save/scan/
  delta), application/ignore (git's own view), storage/file_zone_repo +
  migration 011, two thin MCP tools, `mnexis file` CLI, the pi extension
  exposing the door and wiring the delta into the turn-end nudge; deps:
  tree-sitter + tree-sitter-rust + sha2, all pinned. Open: the reverse
  index strategy and one-vs-two MCP tools.
- **L7 (2026-11-02, verbatim):** "si se debe hacer un primer analisis pero
  este no tendremos que obligar al agente a agregar los memorias de algo
  que no sabe, pero cuando lo haga lo tendra que justificar, luego el mcp
  que beneficio tendria tener dos explicame" — P0 confirmed; saves are
  voluntary, justification rides on the save (D17); the two-tools question
  is being explained before it is decided.
- **L8 (2026-11-02, verbatim):** "dos finas pero el primer escan si
  debera hacerse completo puede ser gradual mientras funciona el softaware,
  dando prioridad a lo que el agente toca, por ahora recuerda esto y
  quedemos aca seguiremos la proxima" — two thin tools decided; the first
  scan complete-but-gradual, agent-touched files first; the human closes
  the session here and resumes next time.
