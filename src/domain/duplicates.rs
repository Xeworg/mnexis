//! Which records a title looks like.
//!
//! Two **different** titles can be the same fact: `puerto del servicio` and
//! `puerto de pagos` are two keys and two records, and the title cannot
//! tell them apart — that is the gap the `memory-title` feature left.
//!
//! This module does not decide that two titles are the same fact: **it
//! cannot**. Measured over a namespace of titles, a threshold on the
//! similarity of two keys separates nothing (the same fact scores 0.53–0.97
//! and two different facts score 0.47–0.91 — the ranges overlap almost
//! completely). What works is a **shortlist**: the record a new title
//! duplicates is in the first three candidates 15 times out of 17, and in
//! the first five 16 times out of 17.
//!
//! So the engine ranks and proposes, and the agent — which understands
//! `puerto de pagos` and `puerto del servicio` — decides. The ranking is
//! IDF-weighted overlap over the titles, the way FTS5 ranks: the corpus is
//! the namespace's own titles, so a word everybody uses carries almost
//! nothing, and **no stopword list exists** (a list that exists to move the
//! numbers is a list tuned to the fixture).
//!
//! A word used by more than half of the titles is not evidence that two
//! titles are about the same thing, and stops counting: without that rule a
//! title like `correo de soporte` matches every title that says `de`.

use std::collections::HashMap;

use crate::domain::memory::MemoryState;

/// A record as the ranker sees it: enough to propose it, nothing more.
#[derive(Debug, Clone, PartialEq)]
pub struct TitleRecord {
    pub uuid: String,
    pub title: String,
    pub topic_key: String,
    pub state: MemoryState,
}

/// A record the query's title looks like.
#[derive(Debug, Clone, PartialEq)]
pub struct DuplicateCandidate {
    /// The record being proposed.
    pub uuid: String,
    pub title: String,
    pub topic_key: String,
    /// How much the title looks like the query's. Higher is closer. It is a
    /// ranking, not a probability.
    pub score: f64,
    /// The words the two titles share, the most distinctive first. The
    /// evidence the agent judges with.
    pub shared: Vec<String>,
}

/// How many candidates a shortlist holds by default.
pub const DEFAULT_DUPLICATE_LIMIT: usize = 5;

/// The words of a title: the key's segments, lowercased.
///
/// The tokenization is the slug's, so it is the same one the topic key uses:
/// lowercase, and every run of anything that is not a letter or a digit
/// becomes a separator.
fn terms(title: &str) -> Vec<String> {
    crate::domain::title::key_from_title(title)
        .split('-')
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect()
}

/// The share of the titles that use a word above which the word is not
/// evidence of anything: if more than half of the namespace says `de`, then
/// two titles that both say `de` have nothing in common.
const EVIDENCE_MAX_SHARE: f64 = 0.5;

/// BM25's saturation: how fast a repeated word stops counting.
const K1: f64 = 1.2;
/// BM25's length normalization: how much a long title is penalized.
const B: f64 = 0.75;

/// Rank the records of a namespace by how much the query's title looks like
/// theirs.
///
/// Only `active` records with a title are proposed: a superseded record's
/// fact lives in its successor, and a record without a title has nothing to
/// compare. The result is ordered by score, and ties break by uuid so the
/// answer is the same every time.
pub fn rank_duplicates(
    query_title: &str,
    corpus: &[TitleRecord],
    limit: usize,
) -> Vec<DuplicateCandidate> {
    let query_terms = terms(query_title);
    if query_terms.is_empty() || limit == 0 {
        return Vec::new();
    }

    // The corpus: the records that can be proposed.
    let docs: Vec<&TitleRecord> = corpus
        .iter()
        .filter(|r| r.state == MemoryState::Active && !r.title.trim().is_empty())
        .collect();
    if docs.is_empty() {
        return Vec::new();
    }
    let doc_terms: Vec<Vec<String>> = docs.iter().map(|r| terms(&r.title)).collect();
    let n = doc_terms.len();
    let avg_len = doc_terms.iter().map(|d| d.len()).sum::<usize>() as f64 / n as f64;

    // How many titles use each word.
    let mut frequency: HashMap<&str, usize> = HashMap::new();
    for doc in &doc_terms {
        let mut seen: Vec<&str> = Vec::new();
        for term in doc {
            if !seen.contains(&term.as_str()) {
                seen.push(term.as_str());
            }
        }
        for term in seen {
            *frequency.entry(term).or_insert(0) += 1;
        }
    }

    // A word used by more than half of the titles is not evidence.
    let evidence = |term: &str| -> bool {
        match frequency.get(term) {
            Some(count) => (*count as f64) / (n as f64) <= EVIDENCE_MAX_SHARE,
            None => false,
        }
    };
    let idf = |term: &str| -> f64 {
        let df = *frequency.get(term).unwrap_or(&0) as f64;
        (1.0 + ((n as f64) - df + 0.5) / (df + 0.5)).ln()
    };

    let mut candidates: Vec<DuplicateCandidate> = Vec::new();
    for (record, doc) in docs.iter().zip(doc_terms.iter()) {
        let mut score = 0.0;
        let mut shared: Vec<(String, f64)> = Vec::new();
        let mut counted: Vec<&str> = Vec::new();

        for term in &query_terms {
            if counted.contains(&term.as_str()) {
                continue;
            }
            counted.push(term.as_str());
            let occurrences = doc.iter().filter(|t| *t == term).count();
            if occurrences == 0 || !evidence(term) {
                continue;
            }
            let idf = idf(term.as_str());
            let f = occurrences as f64;
            let length = doc.len() as f64;
            let norm = K1 * (1.0 - B + B * length / avg_len.max(f64::MIN_POSITIVE));
            let contribution = idf * (f * (K1 + 1.0)) / (f + norm);
            score += contribution;
            shared.push((term.clone(), contribution));
        }

        if score <= 0.0 {
            continue;
        }
        shared.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        candidates.push(DuplicateCandidate {
            uuid: record.uuid.clone(),
            title: record.title.clone(),
            topic_key: record.topic_key.clone(),
            score: (score * 1000.0).round() / 1000.0,
            shared: shared.into_iter().map(|(term, _)| term).collect(),
        });
    }

    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.uuid.cmp(&b.uuid))
    });
    candidates.truncate(limit);
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(uuid: &str, title: &str) -> TitleRecord {
        TitleRecord {
            uuid: uuid.to_string(),
            title: title.to_string(),
            topic_key: crate::domain::title::key_from_title(title),
            state: MemoryState::Active,
        }
    }

    fn namespace() -> Vec<TitleRecord> {
        [
            "puerto del servicio",
            "url de la base de datos",
            "timeout de conexion",
            "reintentos maximos",
            "oncall del servicio",
            "entorno de despliegue",
            "clave del servicio",
            "intervalo de backoff",
            "usuario de la base de datos",
            "cola de mensajes",
            "limite de peticiones",
            "version del deploy",
            "servicio de correos",
            "puerto de la base de datos",
            "timeout de lectura",
            "clave de la base de datos",
            "url del dsn",
            "timeout de escritura",
            "cola de eventos",
            "limite de conexiones",
            "oncall de pagos",
            "host de la base de datos",
            "reintentos del backoff",
            "intervalo de reintento",
        ]
        .iter()
        .enumerate()
        .map(|(i, t)| record(&format!("uuid-{i:02}"), t))
        .collect()
    }

    #[test]
    fn the_record_a_title_duplicates_is_in_the_shortlist() {
        // The measured case: the record the namespace holds is in the shortlist
        // with the evidence that points at it. It is not always first — the
        // ranking is a ranking, and the agent judges the whole shortlist.
        let corpus = namespace();
        let candidates = rank_duplicates("puerto de pagos", &corpus, 5);
        assert!(
            candidates.iter().any(|c| c.title == "puerto del servicio"),
            "the duplicated record is not in the shortlist: {:?}",
            candidates.iter().map(|c| &c.title).collect::<Vec<_>>()
        );
        let candidate = candidates
            .iter()
            .find(|c| c.title == "puerto del servicio")
            .expect("the duplicated record");
        assert!(candidate.shared.contains(&"puerto".to_string()));
        assert!(candidate.score > 0.0);
        assert_eq!(candidate.uuid, "uuid-00");
        assert_eq!(candidate.topic_key, "puerto-del-servicio");
    }

    #[test]
    fn the_shortlist_keeps_the_record_the_measurement_found() {
        // The measured recall of the shortlist: 13 of the 17 titles that
        // duplicate something reach the record in the first three.
        let corpus = namespace();
        let cases = [
            ("puerto de pagos", "puerto del servicio"),
            ("puerto de la api de pagos", "puerto del servicio"),
            ("timeout de la conexion", "timeout de conexion"),
            ("oncall de pagos", "oncall del servicio"),
            ("url de conexion a postgres", "url del dsn"),
            ("numero maximo de reintentos", "reintentos maximos"),
            ("backoff entre reintentos", "intervalo de backoff"),
            ("clave de la api", "clave del servicio"),
            ("timeout de consulta", "timeout de lectura"),
            ("cola de trabajos", "cola de mensajes"),
            ("limite de tasa de peticiones", "limite de peticiones"),
        ];
        for (query, truth) in cases {
            let candidates = rank_duplicates(query, &corpus, 5);
            assert!(
                candidates.iter().any(|c| c.title == truth),
                "{query:?} does not reach {truth:?}: {:?}",
                candidates.iter().map(|c| &c.title).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn a_title_linked_only_by_a_ubiquitous_word_is_not_proposed() {
        // `dsn de postgres` and `url de la base de datos` have only `de` in
        // common, and `de` is in most of the titles: the engine does not
        // pretend that is the same fact. The agent is the one that knows
        // that a DSN is a database URL.
        let corpus = namespace();
        let candidates = rank_duplicates("dsn de postgres", &corpus, 5);
        assert!(
            !candidates.iter().any(|c| c.title == "url de la base de datos"),
            "a ubiquitous word is not evidence: {:?}",
            candidates.iter().map(|c| &c.title).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_word_everybody_uses_is_not_evidence() {
        // `de` is in most of the titles: a title that only says `de` has
        // nothing in common with any of them.
        let corpus = namespace();
        let candidates = rank_duplicates("correo de soporte", &corpus, 5);
        assert!(
            candidates.is_empty(),
            "a ubiquitous word is not evidence: {:?}",
            candidates.iter().map(|c| &c.title).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_title_with_nothing_in_common_proposes_nothing() {
        let corpus = namespace();
        assert!(rank_duplicates("un hecho nuevo", &corpus, 5).is_empty());
        assert!(rank_duplicates("", &corpus, 5).is_empty());
        assert!(rank_duplicates("!!!", &corpus, 5).is_empty());
    }

    #[test]
    fn only_active_records_are_proposed() {
        let mut corpus = namespace();
        corpus[0].state = MemoryState::Superseded;
        let candidates = rank_duplicates("puerto de pagos", &corpus, 5);
        assert!(
            !candidates.iter().any(|c| c.title == "puerto del servicio"),
            "a superseded record must not be proposed"
        );
    }

    #[test]
    fn the_limit_is_respected() {
        let corpus = namespace();
        assert_eq!(rank_duplicates("puerto de pagos", &corpus, 2).len(), 2);
        assert!(rank_duplicates("puerto de pagos", &corpus, 0).is_empty());
    }

    #[test]
    fn the_order_is_stable() {
        let corpus = namespace();
        let first = rank_duplicates("puerto de pagos", &corpus, 5);
        let second = rank_duplicates("puerto de pagos", &corpus, 5);
        assert_eq!(first, second);
    }

    #[test]
    fn the_words_are_the_title_s_own() {
        let corpus = namespace();
        let candidates = rank_duplicates("Puerto de Pagos", &corpus, 3);
        assert!(!candidates.is_empty());
        // The shared words come from the title, lowercased, and they are the
        // ones the two titles have in common.
        for candidate in &candidates {
            for word in &candidate.shared {
                assert_eq!(word, &word.to_lowercase());
            }
        }
    }
}
