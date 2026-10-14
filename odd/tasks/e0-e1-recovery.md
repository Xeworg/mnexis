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
- T1 | S1-S4 | inline: commit the inherited E1 Rust/SQLite/LanceDB/Ollama/MCP spike with the cold-start fix, focused delayed-response test, and updated evidence; request one fresh independent review of this new candidate | commit pending; live post-change run supplied by user; fresh review pending.
- T2 | S2, S4 | inline JSON Schema validation for all E0 contracts and fixtures | commit pending.
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