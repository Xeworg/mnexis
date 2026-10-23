//! The title of a memory, and the topic key it yields.
//!
//! A memory has two faces: the **content**, which says what the fact is
//! right now, and the **title**, which says what the fact *is*. The title
//! is what stays put when a value changes inside the content, and the topic
//! key derives from it — so the same fact lands on the same record, in any
//! language, with nothing to configure and nothing to guess.
//!
//! This replaced a content-similarity measure (`topic-continuity`): the
//! engine compared contents to guess whether two memories were the same
//! topic, needed a stopword list and a threshold for it, and still got it
//! wrong often enough to cost the agent several refused retries per
//! session. The title is an equality, not a guess.

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

/// The longest a title may be, in bytes.
pub const MAX_TITLE_BYTES: usize = 256;

#[cfg(test)]
mod tests {
    use super::*;

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
}
