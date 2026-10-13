# Mnexis E0/E1 prototype plan
Authorized source: `PRD-Mnexis.md` (v0.4). Scope: execute the first implementation block, E0 contracts and E1 technical integration spike.

## Specs
1. **S1 — E0 contracts:** “Cerrar esquemas, scopes, presupuesto y fixture.” Define versioned data contracts, memory states/provenance, authorization scopes, context-budget behavior, stable errors, and reproducible acceptance fixtures strictly from PRD sections 5–10 and 15.
2. **S2 — E0 scenarios:** “Probar manualmente escenarios de decisiones sustituidas, conflicto de revisión, consulta sin respuesta y dos agentes.” Encode these as executable or documented scenarios with expected outcomes and no unrequested product behavior.
3. **S3 — E1 stack validation:** “Crear paquete Rust con biblioteca/binario, toolchain y lockfile.” Build the smallest runnable integration spike for the explicitly proposed Rust/SQLite FTS5/LanceDB/Ollama/rmcp stack; record exact dependency versions and unresolved compatibility.
4. **S4 — E1 local integration:** “Abrir SQLite bundled y verificar FTS5; crear/abrir tabla LanceDB con esquema Arrow compatible.” Exercise local stores, basic vector lifecycle/reopen, one local Ollama embedding, and a minimal rmcp stdio tool only to the extent the available environment/dependencies support it.
5. **S5 — E1 evidence:** “Ejecutar con dos procesos y probar refresco de lectores; medir compilación, RSS, disco y arranque en plataformas objetivo.” Capture reproducible local evidence for feasible checks, clearly label unavailable platform/runtime checks, and do not claim production guarantees.
6. **S6 — E1 decision report:** “informe reproducible del stack y versiones elegidas. Resolver incompatibilidades Arrow y capacidades de cleanup.” Report evidence, constraints, and decisions needed before E2; do not treat unverified dependency behavior as established.

## Tasks
- T1 | S1-S2 | inline | commit: pending
- T2 | S3-S5 | inline | commit: pending
- T3 | S6 | inline | commit: pending

## Log
- L1: “empieza con eso” (refers to the PRD's “Primer bloque de trabajo para el agente de implementación”: empezar con E0 y E1).
- L2: “haz commit cuando consideres necesario solamente debes preguntarme” (authorizes commits when needed; do not ask for commit approval).
- L3: E0 delivered `docs/contracts/v1/` and four scenario fixtures in `fixtures/e0/`. Verification: Python parsed all six JSON documents successfully. `jsonschema` package is unavailable, so meta-schema/instance validation was not run; contracts remain a draft pending executable validation.
