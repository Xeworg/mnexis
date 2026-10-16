//! Criterion benchmarks for FTS5 search latency.
//!
//! Groups
//! -------
//!
//! | Group | Description |
//! |---|---|
//! | `search_10k_cold` | Fresh connection per measurement; 10 000 memories |
//! | `search_10k_warm` | Reused connection; 10 000 memories |
//! | `search_100k_cold` | Fresh connection per measurement; 100 000 memories |
//! | `search_100k_warm` | Reused connection; 100 000 memories |
//!
//! Each benchmark id records the query length variant: `short` or `long`.
//!
//! Corpora are generated deterministically under `target/bench-data` if absent.

use criterion::{Bencher, Criterion, criterion_group, criterion_main};
use std::path::{Path, PathBuf};

use mnexis_core::corpus::{Query, generate_corpus, relevant_queries};
use mnexis_core::retrieval::{NamespaceFilter, create_benchmark_store, insert_memories, search};

/// Path to the pre-generated 10k corpus DB.
fn corpus_10k_path() -> PathBuf {
    PathBuf::from("target/bench-data/10k/mnexis.db")
}

/// Path to the pre-generated 100k corpus DB.
fn corpus_100k_path() -> PathBuf {
    PathBuf::from("target/bench-data/100k/mnexis.db")
}

/// Ensure a corpus DB exists at the given path, generating it if absent.
fn ensure_corpus(path: &Path, n: usize) {
    if path.exists() {
        return;
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let store = create_benchmark_store(path).expect("store");
    let corpus = generate_corpus(n);
    insert_memories(&store.conn, &corpus).expect("insert");
    store
        .conn
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .ok();
}

/// Return a fresh connection for the given corpus.
fn fresh_connection(path: &Path) -> mnexis_core::retrieval::BenchmarkStore {
    create_benchmark_store(path).expect("store")
}

/// Run a search on the given store using the `i`-th relevant query.
/// Returns the number of hits found (not used for timing, just to prevent DCE).
fn run_query(store: &mnexis_core::retrieval::BenchmarkStore, queries: &[Query], i: usize) -> usize {
    let q = &queries[i % queries.len()];
    let hits = search(&store.conn, &q.text, NamespaceFilter::Any, 5).expect("search");
    hits.len()
}

// ── Benchmark groups ──────────────────────────────────────────────────────────

fn search_10k_cold_short(b: &mut Bencher) {
    let path = corpus_10k_path();
    ensure_corpus(&path, 10_000);
    let queries = relevant_queries();
    b.iter(|| {
        let store = fresh_connection(&path);
        run_query(&store, &queries, criterion::black_box(0))
    });
}

fn search_10k_cold_long(b: &mut Bencher) {
    let path = corpus_10k_path();
    ensure_corpus(&path, 10_000);
    let queries = relevant_queries();
    b.iter(|| {
        let store = fresh_connection(&path);
        run_query(&store, &queries, criterion::black_box(1))
    });
}

fn search_10k_warm_short(b: &mut Bencher) {
    let path = corpus_10k_path();
    ensure_corpus(&path, 10_000);
    let store = fresh_connection(&path);
    let queries = relevant_queries();
    let mut counter = 0usize;
    b.iter(|| {
        let i = criterion::black_box(counter);
        run_query(&store, &queries, i);
        counter = counter.wrapping_add(1);
    });
}

fn search_10k_warm_long(b: &mut Bencher) {
    let path = corpus_10k_path();
    ensure_corpus(&path, 10_000);
    let store = fresh_connection(&path);
    let queries = relevant_queries();
    let mut counter = 1usize;
    b.iter(|| {
        let i = criterion::black_box(counter);
        run_query(&store, &queries, i);
        counter = counter.wrapping_add(1);
    });
}

fn search_100k_cold_short(b: &mut Bencher) {
    let path = corpus_100k_path();
    ensure_corpus(&path, 100_000);
    let queries = relevant_queries();
    b.iter(|| {
        let store = fresh_connection(&path);
        run_query(&store, &queries, criterion::black_box(0))
    });
}

fn search_100k_cold_long(b: &mut Bencher) {
    let path = corpus_100k_path();
    ensure_corpus(&path, 100_000);
    let queries = relevant_queries();
    b.iter(|| {
        let store = fresh_connection(&path);
        run_query(&store, &queries, criterion::black_box(1))
    });
}

fn search_100k_warm_short(b: &mut Bencher) {
    let path = corpus_100k_path();
    ensure_corpus(&path, 100_000);
    let store = fresh_connection(&path);
    let queries = relevant_queries();
    let mut counter = 0usize;
    b.iter(|| {
        let i = criterion::black_box(counter);
        run_query(&store, &queries, i);
        counter = counter.wrapping_add(1);
    });
}

fn search_100k_warm_long(b: &mut Bencher) {
    let path = corpus_100k_path();
    ensure_corpus(&path, 100_000);
    let store = fresh_connection(&path);
    let queries = relevant_queries();
    let mut counter = 1usize;
    b.iter(|| {
        let i = criterion::black_box(counter);
        run_query(&store, &queries, i);
        counter = counter.wrapping_add(1);
    });
}

// ── Criterion configuration ───────────────────────────────────────────────────

fn fts5_latency(c: &mut Criterion) {
    c.benchmark_group("fts5_search")
        .sample_size(200)
        .bench_function("search_10k_cold/short", search_10k_cold_short)
        .bench_function("search_10k_cold/long", search_10k_cold_long)
        .bench_function("search_10k_warm/short", search_10k_warm_short)
        .bench_function("search_10k_warm/long", search_10k_warm_long)
        .bench_function("search_100k_cold/short", search_100k_cold_short)
        .bench_function("search_100k_cold/long", search_100k_cold_long)
        .bench_function("search_100k_warm/short", search_100k_warm_short)
        .bench_function("search_100k_warm/long", search_100k_warm_long);
}

criterion_group!(benches, fts5_latency);
criterion_main!(benches);
