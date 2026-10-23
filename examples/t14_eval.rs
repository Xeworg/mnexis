//! Quality and latency evaluation over a generated corpus.
//!
//! Runs deterministically generated queries over a fresh database and reports:
//! - Recall@5, MRR@5, Precision@5 (relevant query set)
//! - No-answer false-positive rate
//! - Latency p50/p95 in ms (relevant queries)
//! - Cold-first-query latency
//! - Corpus insert time and final DB size on disk
//!
//! Replaces the original spike harness that depended on `src/corpus.rs`
//! (now retired to `docs/historical/corpus.rs.txt`). The corpus is
//! reproduced inline as a deliberately minimal token-arithmetic generator:
//! every active memory `i` embeds the unique token `mnx{i:06}` plus a
//! shared topic word, guaranteeing FTS5 hits are unambiguous per document.
//!
//! ## Usage
//!
//! ```sh
//! cargo run --release --example t14_eval -- --memories 1000 --db target/t14-eval/1k.db
//! ```

use std::{fs, path::Path, time::Instant};

use mnexis_core::application::{MemoryService, SaveRequest, ServiceConfig};
use mnexis_core::domain::idempotency::IdempotencyKey;
use mnexis_core::domain::memory::MemoryState;
use mnexis_core::domain::query::MatchMode;
use mnexis_core::domain::scope::{AccessScope, Operation};
use mnexis_core::retrieval::{DeletedFilter, NamespaceFilter, SearchHit, StateFilter, search};
use mnexis_core::storage::memory_repo::SqliteMemoryRepository;
use mnexis_core::storage::sqlite::open_sqlite;

fn summary_line(label: &str, value: f64) {
    println!("{label}: {value:.4}");
}
fn summary_line_int(label: &str, value: usize) {
    println!("{label}: {value}");
}
fn summary_line_str(label: &str, value: &str) {
    println!("{label}: {value}");
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (p / 100.0 * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// Generate `n` SaveRequest values. Each one embeds `mnx{i:06}` plus a
/// shared topic word, so FTS5 bm25 picks the unique token above the topic
/// word — relevant queries hit exactly one document.
fn generate_corpus(n: usize) -> Vec<SaveRequest> {
    let namespaces = ["p-a", "p-b", "p-c", "p-d", "p-e"];
    let now = "2026-01-01T00:00:00Z";
    (0..n)
        .map(|i| {
            // 5% superseded; the rest are active (deleted is exercised by the
            // service path in production scenarios; the corpus here stays
            // simple to keep the latency profile clean).
            let state = if i % 20 == 7 {
                MemoryState::Superseded
            } else {
                MemoryState::Active
            };
            SaveRequest {
                uuid: format!("00000000-0000-0000-0000-{i:012}"),
                title: String::new(),
                keywords: Vec::new(),
                replace_keywords: false,
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
                idempotency_key: Some(IdempotencyKey::new(format!("eval-{i}"))),
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
        vec![Operation::Save, Operation::Get, Operation::Search],
    )
}

fn map_err<E: std::fmt::Display>(ctx: &str, e: E) -> Box<dyn std::error::Error> {
    Box::new(std::io::Error::other(format!("{ctx}: {e}")))
}

fn run_evaluation(n_memories: usize, db_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    summary_line_str("=== MNEXIS T14 EVALUATION ===", "");
    summary_line_str("db_path", &db_path.display().to_string());
    summary_line_int("memories", n_memories);

    // ── 1. Prepare the database ────────────────────────────────────────────────
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if db_path.exists() {
        fs::remove_file(db_path)?;
        let _ = fs::remove_file(format!("{}-wal", db_path.display()));
        let _ = fs::remove_file(format!("{}-shm", db_path.display()));
    }

    // ── 2. Generate and insert the corpus via the application service ─────────
    let corpus = generate_corpus(n_memories);
    let lengths: Vec<usize> = corpus.iter().map(|m| m.content.chars().count()).collect();
    let mean_len = lengths.iter().sum::<usize>() as f64 / lengths.len() as f64;
    let min_len = *lengths.iter().min().unwrap();
    let max_len = *lengths.iter().max().unwrap();

    let insert_start = Instant::now();
    {
        let conn = open_sqlite(db_path).map_err(|e| map_err("open_sqlite (insert)", e))?;
        let repo = SqliteMemoryRepository::new(conn);
        let mut svc = MemoryService::new(repo, permissive_scope(), ServiceConfig::default());
        for req in corpus {
            svc.save(req).map_err(|e| map_err("save", e))?;
        }
    }
    let insert_secs = insert_start.elapsed().as_secs_f64();

    // ── 3. WAL checkpoint + DB size on disk ────────────────────────────────────
    let conn = open_sqlite(db_path).map_err(|e| map_err("open_sqlite (checkpoint)", e))?;
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    let db_bytes = fs::metadata(db_path)?.len()
        + fs::metadata(format!("{}-wal", db_path.display()))
            .map(|m| m.len())
            .unwrap_or(0)
        + fs::metadata(format!("{}-shm", db_path.display()))
            .map(|m| m.len())
            .unwrap_or(0);
    drop(conn);

    // ── 4. Relevant query set (one per active memory) ──────────────────────────
    let rel_queries: Vec<(String, String)> = (0..n_memories)
        .filter(|i| i % 20 != 7) // not superseded
        .map(|i| {
            (
                format!("mnx{i:06} graph"),
                format!("00000000-0000-0000-0000-{i:012}"),
            )
        })
        .collect();

    // ── 5. Cold first-query latency (fresh connection) ─────────────────────────
    let cold_ms = {
        let cold_conn = open_sqlite(db_path).map_err(|e| map_err("open_sqlite (cold)", e))?;
        let cold_start = Instant::now();
        let _cold_hits: Vec<SearchHit> = search(
            &cold_conn,
            &rel_queries[0].0,
            MatchMode::Any,
            NamespaceFilter::Any,
            StateFilter::default(),
            DeletedFilter::default(),
            None,
            5,
        )
        .map_err(|e| map_err("search cold", e))?;
        cold_start.elapsed().as_secs_f64() * 1000.0
    };

    // ── 6. Warm relevant-query pass ──────────────────────────────────────────
    let warm_conn = open_sqlite(db_path).map_err(|e| map_err("open_sqlite (warm)", e))?;
    let mut recall_at_5_count = 0usize;
    let mut mrr_sum = 0.0f64;
    let mut precision_hits = 0usize;
    let mut relevant_latencies: Vec<f64> = Vec::with_capacity(rel_queries.len());

    for (text, target_uuid) in &rel_queries {
        let start = Instant::now();
        let hits: Vec<SearchHit> = search(
            &warm_conn,
            text,
            MatchMode::Any,
            NamespaceFilter::Any,
            StateFilter::default(),
            DeletedFilter::default(),
            None,
            5,
        )
        .map_err(|e| map_err("search warm", e))?;
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        relevant_latencies.push(elapsed_ms);

        let correct_in_top5 = hits.iter().any(|h| h.uuid == *target_uuid);
        if correct_in_top5 {
            recall_at_5_count += 1;
            precision_hits += 1;
        }
        let pos_score = hits
            .iter()
            .position(|h| h.uuid == *target_uuid)
            .map(|p| 1.0 / (p as f64 + 1.0))
            .unwrap_or(0.0);
        mrr_sum += pos_score;
    }
    drop(warm_conn);

    let n_rel = rel_queries.len();
    let recall_at_5 = recall_at_5_count as f64 / n_rel as f64;
    let mrr_at_5 = mrr_sum / n_rel as f64;
    let precision_at_5 = precision_hits as f64 / (n_rel * 5) as f64;

    relevant_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50_ms = percentile(&relevant_latencies, 50.0);
    let p95_ms = percentile(&relevant_latencies, 95.0);

    // ── 7. No-answer query pass ──────────────────────────────────────────────
    let no_answer_pairs: Vec<String> = (0..50)
        .map(|k| {
            let i = (k * 1009) % n_memories;
            let j = ((k * 7919 + 13) % n_memories) as i64;
            format!("mnx{i:06} mnx{j:06}")
        })
        .collect();

    let warm2 = open_sqlite(db_path).map_err(|e| map_err("open_sqlite (noans)", e))?;
    let mut no_answer_fp = 0usize;
    for text in &no_answer_pairs {
        let hits: Vec<SearchHit> = search(
            &warm2,
            text,
            MatchMode::Any,
            NamespaceFilter::Any,
            StateFilter::default(),
            DeletedFilter::default(),
            None,
            5,
        )
        .map_err(|e| map_err("search noans", e))?;
        if !hits.is_empty() {
            no_answer_fp += 1;
        }
    }
    drop(warm2);
    let no_answer_fp_rate = no_answer_fp as f64 / no_answer_pairs.len() as f64;

    // ── 8. Print summary ─────────────────────────────────────────────────────
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
    summary_line_int("no_answer_queries", no_answer_pairs.len());

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut n_memories: Option<usize> = None;
    let mut db_path: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--memories" => {
                n_memories = Some(
                    args.next()
                        .ok_or("missing value for --memories")?
                        .parse()
                        .map_err(|e| format!("invalid --memories: {e}"))?,
                );
            }
            "--db" => {
                db_path = Some(args.next().ok_or("missing value for --db")?);
            }
            _ => {}
        }
    }

    let n_memories = n_memories.unwrap_or(1000);
    let db_path = db_path.unwrap_or_else(|| "target/t14-eval/default.db".to_owned());
    let db_path = Path::new(&db_path).to_path_buf();

    run_evaluation(n_memories, &db_path)
}
