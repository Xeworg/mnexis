# Mnexis E1 integration spike report

**Status:** Partial prototype evidence; not a production-readiness decision.  
**Source:** `PRD-Mnexis.md` v0.4, E1.  
**Environment:** Fedora 44, x86_64, Linux 7.2.9, 20 logical CPUs, 31 GiB RAM; rustc/cargo 1.95.0 (Homebrew installation), `protoc` 3.19.6. `protobuf-compiler` and `protobuf-devel` were installed with dnf by the user. `rustup` is unavailable, so the checked-in toolchain pin was not exercised through rustup.

## Stack decision and compatibility findings

- The package builds with SQLite bundled through `rusqlite 0.40.2`, FTS5, and LanceDB local storage.
- LanceDB **0.37.1** is pinned with default features disabled. The selected Arrow crates are `arrow-array` and `arrow-schema` 58.4.0. Their compatibility with this LanceDB release is confirmed for the exercised schema, create, merge, query, delete, and reopen path by compilation and runtime smoke tests.
- LanceDB 0.38.0, 0.39.0, and 0.40.0 were rejected for this prototype: source inspection found `job.rs` references to `Error::Http` without a `remote` feature gate, while the error variant is gated by `remote`. Enabling `remote` would violate this prototype's local-only feature selection. The 0.37.1 local-only build succeeds; this is a compatibility workaround, not a claim that later versions are unusable with remote enabled.
- `rmcp 3.3.0` is paired with an exact `rmcp-macros 3.3.0` dependency. Cargo otherwise selected macros 3.5.1 through the SDK's semver range; that generated a newer `ServerConfig` API unavailable in rmcp 3.3.0. The matched pair compiles and serves stdio.
- Direct dependency pins relevant to the integration: Tokio 1.53.3, uuid 1.28.0, url 2.5.8, serde_json 1.0.154, thiserror 2.0.21, futures 0.3.34, reqwest 0.12.24, schemars 1.2.3, rusqlite 0.40.2, LanceDB 0.37.1, rmcp/rmcp-macros 3.3.0, Arrow array/schema 58.4.0. `Cargo.lock` records the full resolved graph.

## Checks and observed behavior

| Check | Result | Evidence / boundary |
|---|---|---|
| `cargo fmt --all -- --check` | Pass | Rust formatting check completed successfully. |
| `git diff --check` | Pass | No whitespace errors in the checked source diff. |
| `cargo check --all-targets` | Pass | Completed for the pinned local-only dependency graph. |
| `cargo test --all-targets` | Initial full run failed; focused follow-ups passed | The full run reached the library tests, where both existing SQLite tests failed with `SystemIoFailure` (`disk I/O error`, extended code 778). After moving their tempdirs under `target/`, the two library tests passed. The final focused E0 run passed all 3 schema tests after separating the UUID and date-time rejection cases. The full suite was not rerun, so no full-suite pass is claimed. |
| E0 JSON Schema validation | Pass: 3 focused tests | Both contract schemas pass Draft 2020-12 meta-validation and compile with UUID/date-time format checks enabled; all four scenario fixtures validate against `fixtures/e0/scenario.schema.json`; representative valid and invalid memory/operation instances are checked. The invalid UUID and date-time cases fail independently. `jsonschema` 0.58.6 is a dev dependency with default features disabled. |
| SQLite smoke | Focused follow-up passed; full suite not rerun | After moving both test tempdirs under `target/`, the two library tests passed: FTS5/reopen and independent-connection writes. The earlier full run had failed both with `SystemIoFailure`/extended code 778; its cause remains unconfirmed, and the full suite was not rerun. |
| LanceDB vector lifecycle | Pass | On an isolated profile: create/open table, upsert the same ID twice using merge-insert, verify one row, close/reopen, nearest-vector query returns one row, delete, close/reopen, confirm the deleted ID remains absent. CLI output: `search_rows=1 rows_after_delete=0 reopened=true`. |
| Two concurrent CLI processes | Pass, narrow smoke | Two `mnexis spike` processes used the same ignored `target/concurrent-spike` profile simultaneously; both exited 0 and each reported the vector lifecycle success. This does not establish general LanceDB reader-refresh or crash/concurrency guarantees. |
| MCP stdio | Pass, manual smoke | Sent `initialize`, `notifications/initialized`, and `tools/call` for `ping`. Received a JSON-RPC initialize result and `pong: smoke`; stdout contained only protocol responses. No real MCP client adapter was tested. |
| Ollama `/api/embed` | Pass with `nomic-embed-text`; delayed-response path covered | `qwen3.5:2b` returned HTTP 501 and does not list embedding capability. Pulled `nomic-embed-text`; the first cold isolated spike did not connect before the former 3-second client timeout. A direct API probe then returned HTTP 200 with a nonempty vector (server-reported total duration 1.75 s), and the warmed spike reported 768 dimensions. The client timeout is now 60 seconds. A deterministic local test delayed a valid loopback response by 4 seconds: it failed under the old 3-second timeout (RED) and passed after the change (GREEN). The test proves tolerance of a response slower than 3 seconds, not every real cold-start duration. After the agent's 20-second post-change invocation timed out, the user ran the same isolated spike and supplied output showing SQLite/LanceDB lifecycle success plus `ollama=ok model=nomic-embed-text dimensions=768`; this confirms live post-change integration but not a truly cold model load. Ollama CLI reports 0.40.3. The endpoint validator accepts only HTTP with a literal loopback IP, disables redirects, and rejects hostname/DNS, non-loopback IP, HTTPS, and URL credentials.

## Resource observations

Measured on the above Fedora x86_64 host using the debug binary and a newly created ignored profile under `target/`:

- One `mnexis spike` invocation: **0.15 s elapsed**, **155,364 KiB maximum RSS**.
- `target/debug/mnexis`: **1,542,750,048 bytes**. This is a debug artifact with symbols, not a release binary size.
- The tested profile directory after deleting its probe vector: **72 KiB**; it is only a tiny fixture and says nothing about growth, compaction, or retention.
- The test-profile build in the observed run took **4m52s** while compiling the Lance/DataFusion graph. It was not an isolated clean-build benchmark and should not be used as a release build-time estimate.

- A fresh release build using `CARGO_TARGET_DIR=target/t6-clean` and `TMPDIR=target/t6-tmp` passed. `/usr/bin/time -v` measured 10m59.39 elapsed, 625% CPU, 7,265,268 KiB peak RSS, and no swaps. The target directory used 2.0G; `target/t6-clean/release/mnexis` is 135,546,232 bytes. This is a fresh target-directory build, not a clean-machine result.
- Release CLI `--help` measured 0.00s elapsed and 14,920 KiB peak RSS, with no filesystem I/O or swaps. This measures the help path only, not SQLite/LanceDB/Ollama initialization. After the build, the project filesystem had 118 GB free (76% used).

## Incomplete acceptance and required follow-up

- A warm `nomic-embed-text` call previously returned a 768-dimensional vector. The Ollama request budget is now 60 seconds, and a deterministic local server test delayed a valid response by 4 seconds: it failed at the former 3-second timeout and passed after the change. Full post-change `cargo check --all-targets` and `cargo test --all-targets` passed (4 tests). The agent's post-change invocation (`cargo run --quiet -- spike --data-dir target/ollama-nomic-recovery --model nomic-embed-text`) timed out after 20 seconds and was not retried; the user then ran the same isolated command and reported `sqlite=ok`, `lancedb=ok search_rows=1 rows_after_delete=0 reopened=true`, and `ollama=ok model=nomic-embed-text dimensions=768`. Live post-change integration is confirmed from that user-supplied output, while a truly cold model load remains unmeasured.
- Only Fedora x86_64 was exercised. No other target builds, clean-machine installation, or static-linking claims were tested. The release artifact came from a fresh target directory, which does not establish clean-machine reproducibility or installation behavior.
- Two-process CLI success is a narrow smoke, not the PRD's crash-at-every-boundary gate. A reader holding an old LanceDB handle while another process updates/deletes, process-kill/replay, model-generation changes, index cleanup, compaction, and historical-data purge remain untested.
- ANN versus exact-search quality/performance, embedding model/fingerprint, benchmark corpus, tokenizers, resource budgets, and actual clients remain open spike work.
- Focused E0 validation passes: both contract schemas validate as Draft 2020-12, independent invalid UUID/date-time instances exercise format rejection, representative operation instances exercise constraints, and all four scenario fixtures match a dedicated schema. The earlier full test run failed in two SQLite tests with `SystemIoFailure` (extended code 778). After relocating those two test tempdirs under `target/`, `cargo test --lib` passed both; the final `cargo test --test e0_contracts` passed all 3 tests. The full suite was not rerun, so it is not claimed green. T2 commit `95c5300` exists, but native assessment was unassessable due undeclared untracked files and the one independent-verifier attempt failed before verdict (Node `-122`); it was not retried, so T2 remains unverified.
- The security-sensitive Ollama endpoint guard passed its local unit test, but the required independent `gentle-ai-verify` task was cancelled by the host before returning any verdict. Per harness instructions it was not retried. Therefore S5 is **not independently verified**, and this report does not approve E1 closure or progression to E2.

## Decision before E2

Treat this as a usable local wiring spike only. Before E2, an independent verifier must review the loopback-only guard and the security-sensitive dependency/feature selection, and the remaining local runtime/concurrency gaps above must be explicitly accepted or tested. Keep LanceDB 0.37.1 as the current local-only compile candidate unless a later version is shown to compile with remote features disabled. Do not claim the MVP stack, Ollama integration, production cleanup, or multiprocess guarantees are validated.
