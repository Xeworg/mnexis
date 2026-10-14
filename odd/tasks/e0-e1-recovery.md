# Mnexis E0/E1 deferred-follow-up recovery
Source: `PRD-Mnexis.md` v0.4 and the evidence in `docs/e1-spike-report.md`.
Scope: cover every recorded E0/E1 follow-up with Linux-host evidence; cross-platform execution is deferred until the MVP by the user's decision.

## Specs
1. **S1 — New recovery ODD:** “ahora haremos otro odd para corregir el fallo que quedo en el odd fallido”
2. **S2 — Cover incomplete E0/E1 work:** “este nuevo odd debe volver a cubrir lo que no se pudo de e0 y e1”
3. **S3 — Avoid a surprise verification blocker:** “esta vez asegurate que no se pueda bloquear asi”
4. **S4 — Include every recorded follow-up:** “Cubrir todos los follow-ups”
5. **S5 — Defer other platform runs:** “de momento solo trabajemos en linux cuando ya el mvp este listo reproduciremos los test en las otras plataformas”
6. **S6 — Local work-unit commits:** “Sí, autorizo commits locales”

## Tasks
- T1 | S1-S4 | inline: commit the inherited E1 Rust/SQLite/LanceDB/Ollama/MCP spike with the cold-start fix, focused delayed-response test, and updated evidence; request one fresh independent review of this new candidate | `b534ef9` (`feat(mnexis): add local E1 spike and cold-start support`); blocked because the fresh verifier failed before verdict and native assessment was unassessable; do not retry the verifier.
- T2 | S2, S4 | inline Rust Draft 2020-12 meta-schema validation plus a scenario schema for all four E0 fixtures, with explicit UUID/date-time format validation; use pinned `jsonschema` dev dependency with remote/file resolvers disabled; keep the two SQLite test tempdirs under `target/` | 3 schema tests and 2 SQLite tests passed; full suite not rerun; work-unit commit and independent review pending.
- T3 | S2, S4 | inline Linux concurrent-reader refresh and generation-visibility evidence | commit pending.
- T4 | S2, S4 | inline isolated crash/replay evidence for the current SQLite/LanceDB spike; document absent transactional behavior without inventing it | commit pending.
- T5 | S2, S4 | inline LanceDB cleanup, compaction, and retention capability evidence | commit pending.
- T6 | S2, S4 | inline Linux build, clean-build, release-artifact, RSS, disk, and startup measurements | commit pending.
- T7 | S2, S4 | inline 10k/100k exact-vs-ANN benchmark and resource/latency evidence | commit pending.
- T8 | S2, S4 | inline embedding model/fingerprint, tokenizer, and model-license evidence | commit pending.
- T9 | S2, S4 | inline transitive dependency license inventory | commit pending.
- T10 | S2, S4 | inline real MCP-client interoperability smoke | commit pending.
- T11 | S2, S4 | inline review of generation construction, activation, and retirement behavior and evidence | commit pending.

## Log
- L1: “ahora haremos otro odd para corregir el fallo que quedo en el odd fallido”
- L2: “este nuevo odd debe volver a cubrir lo que no se pudo de e0 y e1”
- L3: “esta vez asegurate que no se pueda bloquear asi”
- L4: The user selected “Cubrir todos los follow-ups”.
- L5: “de momento solo trabajemos en linux cuando ya el mvp este listo reproduciremos los test en las otras plataformas” — no CI workflow or non-Linux runner work in this ODD.
- L6: “Sí, autorizo commits locales” — work-unit commits are authorized; push and PR are not.
- L7: The previous `gentle-ai-verify` attempt was cancelled without verdict. T1 changes the candidate, so one fresh review is allowed; do not replay the old cancelled request. Schedule it at T1 closure, and continue unrelated work units if that review is unavailable. Never claim T1 verified or the full ODD closed without the required verdict. This reduces a surprise global stop but cannot guarantee reviewer availability.
- L8: A cold-start timeout test draft exists in `src/main.rs` from the first narrow attempt. It has not been run or implemented; T2 owns it.
- L9: Existing evidence, unresolved boundaries, and acceptance references are in `docs/e1-spike-report.md` and the read-only gap map from `gentle-ai-explore`.
- L10: T1 test-first evidence: `cargo test --bin mnexis ollama_probe_allows_a_slow_cold_start_response` failed with `reqwest::Error { kind: Request, source: TimedOut }` after 3.02 s under the former timeout; after setting the request timeout to 60 s, the same test passed with a valid response delayed by 4 s (4.02 s). `cargo fmt --all -- --check`, `git diff --check`, `cargo check --all-targets`, and `cargo test --all-targets` passed; the suite has four tests.
- L11: The agent's post-change live spike command timed out after 20 seconds with no output and was not retried or moved to background. The user then ran `cargo run --quiet -- spike --data-dir target/ollama-nomic-recovery --model nomic-embed-text` and supplied `sqlite=ok`, `lancedb=ok search_rows=1 rows_after_delete=0 reopened=true`, and `ollama=ok model=nomic-embed-text dimensions=768`. This resolves the live integration evidence; truly cold model startup remains unmeasured.
- L12: T1 implementation commit: `b534ef9` (`feat(mnexis): add local E1 spike and cold-start support`).
- L13: The one fresh `gentle-ai-verify` call for commit `b534ef9` failed before `agent_settled` with Node `Unknown system error -122`; it produced no verdict and was not retried. Native `gentle_review assess` returned `unassessable` because untracked files were undeclared; its plan calls for independent verification. T1 remains blocked, but T2 is independent and can proceed.
- L14: T2 exploration found `memory.schema.json` and `operation.schema.json` are Draft 2020-12 contracts, while the four E0 fixtures are scenario documents, not instances of those contracts. Use Rust `jsonschema` 0.58.6 as a dev dependency, explicitly enable format checks for `uuid`/`date-time`, validate both contract schemas, and add a scenario schema for fixture shape. The crate supports Draft 2020-12 and requires Rust 1.85+, below this project's 1.95 toolchain; its license is MIT OR Apache-2.0.
- L16: Dependency-source review found `jsonschema` 0.58.6 enables HTTP and filesystem reference resolvers by default. T2 sets `default-features = false` because these bundled local schemas need no external `$ref` resolution; format validation remains explicitly enabled.
- L17: `cargo test --test e0_contracts` passed all 3 focused tests: the two contract schemas validate and compile as Draft 2020-12, representative valid/invalid memory and operation instances behave as expected, and all four E0 scenario fixtures validate against the new scenario schema.
- L18: The first combined check command exited 1 at `cargo fmt --all -- --check` because one assertion needed multiline formatting; later check stages were not reached. Ran `cargo fmt --all` to apply that formatting.
- L19: The subsequent full check command passed formatting, diff check, and `cargo check --all-targets`, but `cargo test --all-targets` exited 101: the two existing SQLite tests failed with `SystemIoFailure`/`disk I/O error` (extended code 778). T2's three focused schema tests passed separately. `df -h .` showed 119 GB free on the project mount and `du -sh target` showed 16 GB. No test was retried.
- L21: With the user's read-only metadata authorization, `TMPDIR=/tmp` resolved to a 16 GB tmpfs with 13 GB used, 3.1 GB free (81%), and 6% inode use. This does not establish the cause; no tests were rerun during the diagnostic.
- L22: The user authorized “Sí, ajuste y prueba focal”: change the two SQLite test tempdirs to `target/` and run only those two SQLite tests once. Do not rerun the full suite or read files from `/tmp`.
- L23: Changed both SQLite unit tests to use `tempfile::tempdir_in("target")`. The authorized command `cargo fmt --all -- --check && git diff --check && cargo test --lib` passed formatting, diff check, and both tests (`2 passed; 0 failed`; 0.11 s). The earlier full suite was not rerun.
- L24: Self-review found that the invalid memory instance set both UUID and date-time invalid simultaneously, so a single rejection did not independently prove either format constraint. Updated the test to clone a valid memory instance and invalidate one field at a time. `cargo fmt --all -- --check && git diff --check && cargo test --test e0_contracts` passed all 3 tests; the full suite was not rerun.
- L20: The user authorized “Sí, solo metadatos” for read-only checks of `TMPDIR`/system temp mount capacity. No file contents or tests may be read/re-run.
- L15: T2 RED observed via `cargo test --test e0_contracts`: compilation fails because `jsonschema` is not a dependency and `fixtures/e0/scenario.schema.json` does not exist. This is the expected pre-implementation failure for the new validator.