# Linux dependency license inventory

**Snapshot:** 2026-10-15 · **Target:** `x86_64-unknown-linux-gnu` · **Lockfile:** current `Cargo.lock` after retiring the vector/Ollama spike from the active package.

## Scope and method

The inventory covers packages reachable from the Mnexis root package in Cargo's resolved Linux graph, including the currently resolved development dependencies. It records the exact `license`/`license_file` metadata supplied by each package; it is an inventory, not legal approval or a release-license decision.

Reproduce the package graph and license expressions with:

```sh
cargo metadata --format-version 1 --locked --offline --filter-platform x86_64-unknown-linux-gnu
```

The resolved graph contains **131 packages**: Mnexis itself plus **130 third-party packages**. Every reachable package has a declared license expression; none report an unknown or license-file-only value. The root package metadata is `UNLICENSED` and still needs a product licensing decision before distribution. This Linux-filtered inventory does not claim coverage for other target platforms; those are deferred until the MVP per project scope.

The active direct dependencies are `clap`, `rmcp`, `rmcp-macros`, `rusqlite`, `serde`, `serde_json`, `schemars`, `thiserror`, and `tokio`; dev dependencies are `jsonschema` and `tempfile`. Vector/database and embedding/network-probe dependencies are no longer direct dependencies. `futures` and `uuid` still occur transitively in the resolved graph and are listed below.

## Declared license expressions by package

Expressions are copied verbatim from Cargo package metadata. Package names are followed by their resolved versions.

| Declared expression | Count | Packages |
| --- | ---: | --- |
| `(MIT OR Apache-2.0) AND Unicode-3.0` | 1 | `unicode-ident 1.0.27` |
| `Apache-2.0` | 3 | `rmcp 3.3.0`; `rmcp-macros 3.3.0`; `unicode-general-category 1.1.0` |
| `Apache-2.0 OR MIT` | 9 | `autocfg 1.5.1`; `bit-set 0.8.0`; `bit-vec 0.8.0`; `equivalent 1.0.3`; `fastrand 2.5.0`; `indexmap 2.14.2`; `pin-project-lite 0.2.17`; `utf8parse 0.2.2`; `uuid 1.28.0` |
| `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | 2 | `linux-raw-sys 0.12.1`; `rustix 1.1.5` |
| `Apache-2.0/MIT` | 1 | `bytecount 0.6.9` |
| `BSD-2-Clause OR Apache-2.0 OR MIT` | 1 | `zerocopy 0.8.63` |
| `MIT` | 31 | `bytes 1.12.1`; `darling 0.24.1`; `darling_core 0.24.1`; `darling_macro 0.24.1`; `data-encoding 2.11.1`; `email_address 0.2.9`; `fancy-regex 0.19.2`; `fluent-uri 0.4.1`; `jsonschema 0.58.6`; `jsonschema-regex 0.58.6`; `jsonschema-value 0.58.6`; `libsqlite3-sys 0.38.2`; `micromap 0.3.0`; `outref 0.5.2`; `referencing 0.58.6`; `rusqlite 0.40.2`; `schemars 1.2.3`; `schemars_derive 1.2.3`; `slab 0.4.12`; `strsim 0.11.1`; `strum 0.28.0`; `strum_macros 0.28.0`; `tokio 1.53.3`; `tokio-macros 2.7.2`; `tokio-util 0.7.20`; `tracing 0.1.44`; `tracing-attributes 0.1.31`; `tracing-core 0.1.36`; `uuid-simd 0.8.0`; `vsimd 0.8.0`; `zmij 1.0.23` |
| `MIT OR Apache-2.0` | 72 | `ahash 0.8.12`; `allocator-api2 0.2.21`; `anstream 0.6.21`; `anstyle 1.0.14`; `anstyle-parse 0.2.7`; `anstyle-query 1.1.5`; `base64 0.23.1`; `bitflags 2.13.2`; `cc 1.8.0`; `cfg-if 1.0.5`; `chrono 0.4.45`; `clap 4.5.53`; `clap_builder 4.5.53`; `clap_derive 4.5.49`; `clap_lex 0.7.7`; `colorchoice 1.0.5`; `dyn-clone 1.0.20`; `errno 0.3.14`; `find-msvc-tools 0.1.14`; `fraction 0.17.0`; `futures 0.3.34`; `futures-channel 0.3.34`; `futures-core 0.3.34`; `futures-executor 0.3.34`; `futures-io 0.3.34`; `futures-macro 0.3.34`; `futures-sink 0.3.34`; `futures-task 0.3.34`; `futures-util 0.3.34`; `getrandom 0.3.4`; `getrandom 0.4.3`; `hashbrown 0.17.1`; `hashlink 0.12.2`; `heck 0.5.0`; `iana-time-zone 0.1.65`; `is_terminal_polyfill 1.70.2`; `itoa 1.0.18`; `libc 0.2.190`; `lock_api 0.4.14`; `num 0.4.3`; `num-bigint 0.4.8`; `num-complex 0.4.6`; `num-integer 0.1.47`; `num-iter 0.1.46`; `num-rational 0.4.2`; `num-traits 0.2.19`; `once_cell 1.21.4`; `parking_lot 0.12.5`; `parking_lot_core 0.9.12`; `pastey 0.2.3`; `percent-encoding 2.3.2`; `pkg-config 0.3.34`; `proc-macro2 1.0.107`; `quote 1.0.47`; `ref-cast 1.0.27`; `ref-cast-impl 1.0.27`; `regex 1.13.1`; `regex-automata 0.4.18`; `regex-syntax 0.8.11`; `scopeguard 1.2.0`; `serde 1.0.228`; `serde_core 1.0.228`; `serde_derive 1.0.228`; `serde_derive_internals 0.30.0`; `serde_json 1.0.154`; `shlex 2.0.1`; `smallvec 1.16.3`; `syn 2.0.119`; `syn 3.0.9`; `tempfile 3.23.0`; `thiserror 2.0.21`; `thiserror-impl 2.0.21` |
| `MIT-0` | 1 | `borrow-or-share 0.2.4` |
| `MIT/Apache-2.0` | 6 | `fallible-iterator 0.3.0`; `fallible-streaming-iterator 0.1.9`; `ident_case 1.0.1`; `num-cmp 0.1.0`; `vcpkg 0.2.15`; `version_check 0.9.5` |
| `UNLICENSED` | 1 | `mnexis 0.1.0` (workspace root, not a third-party dependency) |
| `Unlicense OR MIT` | 2 | `aho-corasick 1.1.5`; `memchr 2.8.3` |
| `Zlib` | 1 | `foldhash 0.2.0` |

## Review notes

- This list reflects package-declared metadata, not a review of each full license text, notices, patent clauses, or redistribution obligations.
- Alternative expressions and non-MIT/Apache identifiers are retained verbatim rather than normalized. Review them against the package's shipped notices as part of release preparation.
- Mnexis is marked `UNLICENSED`; selecting and applying a project license remains an explicit pre-distribution decision.
