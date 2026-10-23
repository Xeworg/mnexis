//! Criterion benchmark for FTS5 search latency on a warm connection.
//!
//! Replaces the original spike benchmark that depended on `src/corpus.rs`
//! (now retired to `docs/historical/corpus.rs.txt`). A single warm-only
//! 10 k benchmark keeps the run bounded and gives a production-aligned
//! signal: one connection, repeated queries over a pre-generated corpus.
//!
//! Run with: `cargo bench --bench fts5_latency`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use criterion::{Bencher, Criterion, criterion_group, criterion_main};

use mnexis_core::application::{MemoryService, SaveRequest, ServiceConfig};
use mnexis_core::domain::idempotency::IdempotencyKey;
use mnexis_core::domain::memory::MemoryState;
use mnexis_core::domain::query::MatchMode;
use mnexis_core::domain::scope::{AccessScope, Operation};
use mnexis_core::retrieval::{DeletedFilter, NamespaceFilter, SearchHit, StateFilter, search};
use mnexis_core::storage::memory_repo::SqliteMemoryRepository;
use mnexis_core::storage::sqlite::open_sqlite;

const N: usize = 10_000;

/// Path to the pre-generated 10k corpus DB.
fn corpus_path() -> PathBuf {
    PathBuf::from("target/bench-data/10k/mnexis.db")
}

/// Generate `N` SaveRequest values matching `examples/t14_eval.rs`.
fn generate_corpus(n: usize) -> Vec<SaveRequest> {
    let namespaces = ["p-a", "p-b", "p-c", "p-d", "p-e"];
    let now = "2026-01-01T00:00:00Z";
    (0..n)
        .map(|i| {
            let state = if i % 20 == 7 {
                MemoryState::Superseded
            } else {
                MemoryState::Active
            };
            SaveRequest {
                uuid: format!("00000000-0000-0000-0000-{i:012}"),
                title: String::new(),
                topic_key: format!("mnx.topic.{i:06}"),
                namespace: namespaces[i % namespaces.len()].to_string(),
                memory_type: "fact".to_string(),
                content: format!("graph mnx{i:06} algorithms for distributed systems"),
                actor: "user".to_string(),
                channel: "cli".to_string(),
                assertion_basis: "user_stated".to_string(),
                valid_from: None,
                valid_until: None,
                tags: vec![format!("mnx-tag-{i:06}")],
                idempotency_key: Some(IdempotencyKey::new(format!("bench-{i}"))),
                state,
                now: now.to_string(),
            }
        })
        .collect()
}

fn permissive_scope() -> AccessScope {
    AccessScope::new(
        vec![
            "p-a".into(),
            "p-b".into(),
            "p-c".into(),
            "p-d".into(),
            "p-e".into(),
        ],
        vec![
            "p-a".into(),
            "p-b".into(),
            "p-c".into(),
            "p-d".into(),
            "p-e".into(),
        ],
        "p-a".into(),
        false,
        vec![Operation::Save, Operation::Search],
    )
}

/// Ensure a corpus DB exists at the given path, generating it if absent.
fn ensure_corpus(path: &Path, n: usize) {
    if path.exists() {
        return;
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    println!("generating {n}-row corpus at {}", path.display());
    let started = Instant::now();
    let conn = open_sqlite(path).expect("open_sqlite");
    let repo = SqliteMemoryRepository::new(conn);
    let mut svc = MemoryService::new(repo, permissive_scope(), ServiceConfig::default());
    for req in generate_corpus(n) {
        svc.save(req).expect("save");
    }
    println!("corpus ready in {:.2}s", started.elapsed().as_secs_f64());
}

fn search_warm_10k(b: &mut Bencher) {
    let path = corpus_path();
    ensure_corpus(&path, N);
    let conn = open_sqlite(&path).expect("open_sqlite");
    let mut counter = 0usize;
    b.iter(|| {
        let i = criterion::black_box(counter);
        counter = counter.wrapping_add(1);
        let text = format!("mnx{:06} graph", i % N);
        let hits: Vec<SearchHit> = search(
            &conn,
            &text,
            MatchMode::Any,
            NamespaceFilter::Any,
            StateFilter::default(),
            DeletedFilter::default(),
            None,
            5,
        )
        .expect("search");
        hits.len()
    });
}

fn fts5_latency(c: &mut Criterion) {
    c.benchmark_group("fts5_search")
        .sample_size(100)
        .bench_function("search_warm_10k", search_warm_10k);
}

criterion_group!(benches, fts5_latency);
criterion_main!(benches);
