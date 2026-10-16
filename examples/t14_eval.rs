//! Quality and latency evaluation over the T14 generated corpus.
//!
//! Runs deterministically generated queries over a fresh database and reports:
//! - Recall@5, MRR@5, Precision@5 (relevant query set)
//! - No-answer false-positive rate
//! - Latency p50/p95 in ms (relevant queries)
//! - Cold-first-query latency
//! - Corpus insert time and final DB size on disk
//! - Content length statistics
//!
//! ## Usage
//!
//! ```sh
//! cargo run --release --example t14_eval -- --memories 1000 --db target/t14-eval/1k.db
//! ```
//!
//! ## Methodology
//!
//! Corpus and queries are derived by pure index arithmetic (no RNG).
//! The first query is issued on a freshly opened connection to capture
//! a "cold" latency measurement (OS page cache warm from corpus generation).

use std::{fs, path::Path, time::Instant};

use mnexis_core::corpus::{generate_corpus, no_answer_queries, relevant_queries};
use mnexis_core::retrieval::{NamespaceFilter, create_benchmark_store, insert_memories, search};

/// Print a summary line.
fn summary_line(label: &str, value: f64) {
    println!("{}: {:.4}", label, value);
}
fn summary_line_int(label: &str, value: usize) {
    println!("{}: {}", label, value);
}
fn summary_line_str(label: &str, value: &str) {
    println!("{}: {}", label, value);
}

/// Compute the p-th percentile of a sorted slice.
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (p / 100.0 * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// Run the evaluation.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut n_memories: Option<usize> = None;
    let mut db_path: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--memories" => {
                n_memories = Some(args.next().unwrap().parse().unwrap());
            }
            "--db" => {
                db_path = Some(args.next().unwrap());
            }
            _ => {}
        }
    }

    let n_memories = n_memories.unwrap_or(1000);
    let db_path = db_path.unwrap_or_else(|| "target/t14-eval/default.db".to_owned());

    summary_line_str("=== MNEXIS T14 EVALUATION ===", "");
    summary_line_str("db_path", &db_path);
    summary_line_int("memories", n_memories);

    // ── 1. Prepare the database ────────────────────────────────────────────────
    let db_path = Path::new(&db_path);
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if db_path.exists() {
        fs::remove_file(db_path)?;
        // Also remove WAL/SHM
        let _ = fs::remove_file(format!("{}-wal", db_path.display()));
        let _ = fs::remove_file(format!("{}-shm", db_path.display()));
    }

    let store = create_benchmark_store(db_path)?;

    // ── 2. Generate corpus ────────────────────────────────────────────────────
    let corpus = generate_corpus(n_memories);

    // Content length stats
    let lengths: Vec<usize> = corpus.iter().map(|m| m.content.chars().count()).collect();
    let mean_len = lengths.iter().sum::<usize>() as f64 / lengths.len() as f64;
    let min_len = *lengths.iter().min().unwrap();
    let max_len = *lengths.iter().max().unwrap();

    // ── 3. Insert corpus and time it ─────────────────────────────────────────
    let insert_start = Instant::now();
    insert_memories(&store.conn, &corpus)?;
    // Checkpoint WAL so DB bytes reflects indexed data
    store
        .conn
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    let insert_secs = insert_start.elapsed().as_secs_f64();

    // ── 4. Measure DB size on disk (including WAL) ─────────────────────────────
    let db_bytes = fs::metadata(db_path)?.len()
        + fs::metadata(format!("{}-wal", db_path.display()))
            .map(|m| m.len())
            .unwrap_or(0)
        + fs::metadata(format!("{}-shm", db_path.display()))
            .map(|m| m.len())
            .unwrap_or(0);

    // ── 5. Prepare query sets ─────────────────────────────────────────────────
    let rel_queries = relevant_queries();
    let no_ans_queries = no_answer_queries();

    // ── 6. Cold-first-query latency ───────────────────────────────────────────
    // Open a fresh connection to measure the first query latency
    let cold_store = create_benchmark_store(db_path)?;
    let cold_start = Instant::now();
    let _cold_hits = search(
        &cold_store.conn,
        &rel_queries[0].text,
        NamespaceFilter::Any,
        5,
    )?;
    let cold_ms = cold_start.elapsed().as_secs_f64() * 1000.0;

    // ── 7. Run relevant queries and collect latency ───────────────────────────
    let mut recall_at_5_count = 0usize;
    let mut mrr_sum = 0.0f64;
    let mut precision_hits = 0usize;
    let mut relevant_latencies: Vec<f64> = Vec::with_capacity(rel_queries.len());

    for q in &rel_queries {
        let start = Instant::now();
        let hits = search(&store.conn, &q.text, NamespaceFilter::Any, 5)?;
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        relevant_latencies.push(elapsed_ms);

        // Quality metrics
        let top5_ids: Vec<i64> = hits.iter().map(|h| h.id).collect();
        let relevant_set: std::collections::HashSet<i64> = q.relevant.iter().copied().collect();

        // Recall@5: is the correct doc in top-5?
        let correct_in_top5 = top5_ids.iter().any(|id| relevant_set.contains(id));
        if correct_in_top5 {
            recall_at_5_count += 1;
        }

        // MRR@5: reciprocal rank (1-based position)
        let rank = top5_ids
            .iter()
            .position(|id| relevant_set.contains(id))
            .map(|pos| 1.0 / (pos as f64 + 1.0))
            .unwrap_or(0.0);
        mrr_sum += rank;

        // Precision@5: how many of top-5 are relevant?
        let relevant_in_top5 = top5_ids
            .iter()
            .filter(|id| relevant_set.contains(id))
            .count();
        precision_hits += relevant_in_top5;
    }

    let n_rel = rel_queries.len();
    let recall_at_5 = recall_at_5_count as f64 / n_rel as f64;
    let mrr_at_5 = mrr_sum / n_rel as f64;
    let precision_at_5 = precision_hits as f64 / (n_rel * 5) as f64;

    // Latency percentiles
    relevant_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50_ms = percentile(&relevant_latencies, 50.0);
    let p95_ms = percentile(&relevant_latencies, 95.0);

    // ── 8. Run no-answer queries ───────────────────────────────────────────────
    let mut no_answer_fp = 0usize;
    for q in &no_ans_queries {
        let hits = search(&store.conn, &q.text, NamespaceFilter::Any, 5)?;
        if !hits.is_empty() {
            no_answer_fp += 1;
        }
    }
    let no_answer_fp_rate = no_answer_fp as f64 / no_ans_queries.len() as f64;

    // ── 9. Print summary ─────────────────────────────────────────────────────
    println!();
    summary_line("recall_at_5", recall_at_5);
    summary_line("mrr_at_5", mrr_at_5);
    summary_line("precision_at_5", precision_at_5);
    summary_line("no_answer_fp_rate", no_answer_fp_rate);
    println!();
    summary_line("latency_p50_ms", p50_ms);
    summary_line("latency_p95_ms", p95_ms);
    summary_line("cold_first_query_ms", cold_ms);
    println!();
    summary_line("insert_seconds", insert_secs);
    summary_line("db_bytes", db_bytes as f64);
    println!();
    summary_line("content_length_mean", mean_len);
    summary_line_int("content_length_min", min_len);
    summary_line_int("content_length_max", max_len);
    summary_line_int("relevant_queries", n_rel);
    summary_line_int("no_answer_queries", no_ans_queries.len());

    Ok(())
}
