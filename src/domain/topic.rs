//! Which topic is this memory about?
//!
//! The engine cannot ask the agent "is this the same fact as that one?".
//! What it can do is notice that two contents are about the same thing,
//! and say so — with the topic key the namespace already uses, so the
//! agent can update one record instead of writing a second one.
//!
//! The measure is deliberately simple and explainable: the **significant
//! words** of two contents, and how much of the smaller one the larger
//! already contains. Numbers count; the words that carry no topic
//! (`the`, `el`, `of`, `en`) do not.
//!
//! The answer is a **notice**, never a refusal: the save already happened,
//! and a false positive costs a confusing sentence, not a lost fact.
//!
//! ## Why a hand-written stopword list
//!
//! The alternative is to weight the words by how rare they are in the
//! namespace (the inverse document frequency), which needs no list and
//! tunes itself to whatever the project writes about. It was not chosen
//! because every term would cost a document-frequency query over the
//! index, and because a fixed list keeps the measure **predictable**: the
//! same two contents always give the same answer, in any language the
//! list covers.
//!
//! The list is therefore small on purpose: the function words of the two
//! languages a team writes memories in (Spanish and English). It is not a
//! general stopword list, and it must not grow to fit a test: a word that
//! carries a topic (`puerto`, `port`, `deploy`) belongs in the terms, not
//! here.

use std::collections::BTreeSet;

/// The topic key a title yields.
///
/// The same title always gives the same key, in any language: lowercase
/// everything, turn every run of anything that is not a letter or a digit
/// into a single `-`, and trim the dashes at both ends. A title that has no
/// letter and no digit (`!!!`) yields no key at all — the caller decides
/// what to do with a memory that has no topic.
///
/// This is the whole mechanism behind "one fact, one record": the key is a
/// function of the title, and the save updates the record that already holds
/// the key. No similarity, no thresholds, no language to configure.
pub fn key_from_title(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    for c in title.trim().chars() {
        let lowered: String = c.to_lowercase().collect();
        let is_word = lowered.chars().all(|l| l.is_alphabetic() || l.is_numeric());
        if is_word && !lowered.chars().all(char::is_whitespace) {
            out.push_str(&lowered);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// Words that carry no topic. Spanish and English, because a project's
/// memories are written in whichever language the team speaks.
const STOPWORDS: &[&str] = &[
    // Spanish
    "el", "la", "los", "las", "un", "una", "unos", "unas", "de", "del", "al", "a", "en", "con",
    "por", "para", "que", "se", "su", "sus", "lo", "es", "son", "y", "o", "no", "si", "ya", "este",
    "esta", "esto", "ese", "esa", "como", "más", "pero", "sin", "sobre", "entre", "cuando",
    "donde", "hay", "ser", "está", "están", "the", "and", "for", "with", "that", "this", "these",
    "those", "from", "into", "onto", "but", "not", "was", "were", "are", "its", "it", "in", "on",
    "at", "to", "of", "as", "by", "is", "be", "or", "if", "so", "than", "then",
];

/// The significant words of a content: the alphanumeric runs, lowercased,
/// without the words that carry no topic.
pub fn significant_terms(content: &str) -> BTreeSet<String> {
    let mut terms = BTreeSet::new();
    let mut current = String::new();
    for c in content.chars() {
        if c.is_alphanumeric() {
            current.extend(c.to_lowercase());
        } else if !current.is_empty() {
            push_term(&mut terms, &current);
            current.clear();
        }
    }
    if !current.is_empty() {
        push_term(&mut terms, &current);
    }
    terms
}

/// Add a term unless it is a word that carries no topic.
fn push_term(terms: &mut BTreeSet<String>, term: &str) {
    if term.len() < 2 {
        return;
    }
    if STOPWORDS.contains(&term) {
        return;
    }
    terms.insert(term.to_string());
}

/// How much of the smaller content the larger one already contains, from 0
/// to 1.
///
/// 1.0 means the smaller one's every significant word is in the larger one:
/// the two contents are about the same thing. 0.0 means they share
/// nothing.
pub fn containment(smaller: &BTreeSet<String>, larger: &BTreeSet<String>) -> f64 {
    if smaller.is_empty() || larger.is_empty() {
        return 0.0;
    }
    let shared = smaller.intersection(larger).count() as f64;
    shared / smaller.len() as f64
}

/// The overlap between two contents, from 0 to 1.
pub fn overlap(a: &str, b: &str) -> f64 {
    let first = significant_terms(a);
    let second = significant_terms(b);
    let (smaller, larger) = if first.len() <= second.len() {
        (&first, &second)
    } else {
        (&second, &first)
    };
    containment(smaller, larger)
}

/// The number of significant words two contents share.
pub fn shared_terms(a: &str, b: &str) -> usize {
    let first = significant_terms(a);
    let second = significant_terms(b);
    first.intersection(&second).count()
}

/// How many significant words two contents must share before the engine
/// says they are about the same thing.
///
/// Two words are not enough on their own: "the port is 8081" and "the port
/// is 9090" share a single topic word, and a memory about the port and a
/// memory about the host share two. Three is where the signal starts to
/// mean something.
pub const MIN_SHARED_TERMS: usize = 3;

/// How much of the smaller content the larger one must contain.
pub const MIN_CONTAINMENT: f64 = 0.75;

/// True when two contents are about the same thing.
///
/// The measure is conservative on purpose: a notice that fires on every
/// pair of memories is noise, and the agent has to read it.
pub fn is_same_topic(a: &str, b: &str) -> bool {
    let first = significant_terms(a);
    let second = significant_terms(b);
    if first.is_empty() || second.is_empty() {
        return false;
    }
    let shared = first.intersection(&second).count();
    if shared < MIN_SHARED_TERMS {
        return false;
    }
    let (smaller, larger) = if first.len() <= second.len() {
        (&first, &second)
    } else {
        (&second, &first)
    };
    containment(smaller, larger) >= MIN_CONTAINMENT
}

/// The first words of a content, with an ellipsis when it was cut.
pub fn preview(content: &str, chars: usize) -> String {
    let mut out: String = content.chars().take(chars).collect();
    if content.chars().count() > chars {
        out.push('…');
    }
    out.replace('\n', " ")
}

/// A record that already holds this topic.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TopicSuggestion {
    /// The record's uuid.
    pub uuid: String,
    /// The topic key the namespace already uses for it.
    pub topic_key: String,
    /// The record's state (`active`, `superseded`, …).
    pub state: String,
    /// The first words of the record, so the agent can recognise it.
    pub preview: String,
    /// How much of the content the record already contains (0 to 1).
    pub overlap: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &str = "El servicio payments-api escucha en el puerto TCP 8081, definido como \
                        `port` en la seccion `[server]` de `config.toml` (con 4 workers).";
    const NEW: &str = "El servicio payments-api escucha en el puerto TCP 9090, definido como \
                        `port` en la seccion `[server]` de `config.toml` (con 4 workers).";

    // --- the key a title yields ---

    #[test]
    fn the_same_title_always_gives_the_same_key() {
        assert_eq!(key_from_title("puerto del servicio"), "puerto-del-servicio");
        assert_eq!(key_from_title("Puerto del Servicio"), "puerto-del-servicio");
        assert_eq!(
            key_from_title("  puerto   del servicio  "),
            "puerto-del-servicio"
        );
        assert_eq!(
            key_from_title("puerto del servicio!"),
            "puerto-del-servicio"
        );
    }

    #[test]
    fn a_title_with_a_version_or_a_symbol_slugs_cleanly() {
        assert_eq!(key_from_title("puerto 8081"), "puerto-8081");
        assert_eq!(
            key_from_title("payments-api: puerto"),
            "payments-api-puerto"
        );
        assert_eq!(
            key_from_title("retry (max 5 / backoff)"),
            "retry-max-5-backoff"
        );
    }

    #[test]
    fn a_title_in_another_language_still_yields_a_key() {
        // The key has to work for the language the team writes in.
        assert_eq!(key_from_title("サービスのポート"), "サービスのポート");
        assert_eq!(key_from_title("Θύρα υπηρεσίας"), "θύρα-υπηρεσίας");
        assert_eq!(key_from_title("Порт сервиса"), "порт-сервиса");
    }

    #[test]
    fn a_title_without_a_letter_or_a_digit_yields_no_key() {
        assert_eq!(key_from_title("!!!"), "");
        assert_eq!(key_from_title("   "), "");
        assert_eq!(key_from_title("---"), "");
    }

    #[test]
    fn the_key_never_opens_or_closes_with_a_dash() {
        for title in ["-puerto-", "¿puerto?", "...puerto...", "puerto — servico"] {
            let key = key_from_title(title);
            assert!(!key.starts_with('-'), "{title} -> {key}");
            assert!(!key.ends_with('-'), "{title} -> {key}");
        }
    }

    #[test]
    fn the_key_a_title_yields_is_a_valid_topic_key() {
        // The charset the service validates, kept in sync on purpose.
        for title in [
            "puerto del servicio",
            "サービスのポート",
            "Порт сервиса",
            "v1.2 del deploy",
        ] {
            let key = key_from_title(title);
            for c in key.chars() {
                assert!(
                    (c.is_alphabetic() && !c.is_uppercase())
                        || c.is_numeric()
                        || matches!(c, '.' | '-' | '/'),
                    "{title} -> {key} has an invalid character {c:?}"
                );
            }
        }
    }

    #[test]
    fn the_words_that_carry_no_topic_are_not_terms() {
        let terms = significant_terms("el puerto de la pasarela");
        assert!(terms.contains("puerto"));
        assert!(terms.contains("pasarela"));
        assert!(!terms.contains("el"));
        assert!(!terms.contains("de"));
        assert!(!terms.contains("la"));
    }

    #[test]
    fn a_number_is_a_term() {
        let terms = significant_terms("el puerto es 8081");
        assert!(terms.contains("8081"), "the value is part of the topic");
    }

    #[test]
    fn a_changed_value_is_the_same_topic() {
        assert!(is_same_topic(OLD, NEW), "only the port changed");
    }

    #[test]
    fn two_different_facts_are_not_the_same_topic() {
        let other = "El servidor de metricas escucha en el puerto TCP 9090, con 8 workers.";
        assert!(
            !is_same_topic(OLD, other),
            "a different service is a different topic"
        );
    }

    #[test]
    fn a_short_pair_needs_the_floor_of_shared_words() {
        // "el puerto es 8081" and "el puerto es 9090" share one topic word.
        assert!(!is_same_topic("el puerto es 8081", "el puerto es 9090"));
        // Three shared words are enough.
        assert!(is_same_topic(
            "el puerto de la pasarela de pagos es 8081",
            "el puerto de la pasarela de pagos es 9090"
        ));
    }

    #[test]
    fn an_empty_content_is_never_a_topic() {
        assert!(!is_same_topic("", "el puerto es 8081"));
        assert!(!is_same_topic("el puerto es 8081", "   "));
    }

    #[test]
    fn the_overlap_is_a_number_between_zero_and_one() {
        assert_eq!(overlap(OLD, OLD), 1.0);
        assert_eq!(overlap("el puerto es 8081", "el host es mnexis"), 0.0);
        assert!(overlap(OLD, NEW) > 0.9);
    }

    #[test]
    fn the_shared_words_are_counted() {
        assert!(shared_terms(OLD, NEW) >= 10);
        assert_eq!(shared_terms("el puerto es 8081", "el host es mnexis"), 0);
    }
}
