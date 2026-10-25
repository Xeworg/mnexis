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

/// The title a record's own text yields, when it yields one.
///
/// The title is what the fact IS and the content is what it says now. A
/// record that carries no title — the ones that predate the identity work
/// — can be given one from the text it already has: its first sentence,
/// the same thing the agent writes by hand when it saves a fact.
///
/// `None` when the text yields nothing worth naming: an empty content, or
/// a first sentence that is only punctuation. The caller reports the
/// record and leaves it alone — a title is never invented, and a title
/// that names nothing is worse than no title at all.
pub fn derive_title_from_content(content: &str) -> Option<String> {
    // One line: a title with a line break splits the identity of the fact
    // in two, and the content of a legacy record can carry one.
    let flat = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let flat = flat.trim();
    if flat.is_empty() {
        return None;
    }

    // The first sentence ends at the first '.', '!' or '?' that is
    // followed by a space or by the end of the text — so a decimal point
    // ("the port 3.5") does not cut the sentence in two.
    let bytes = flat.as_bytes();
    let mut end = flat.len();
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(byte, b'.' | b'!' | b'?') {
            let next = bytes.get(index + 1);
            if next.is_none_or(|next| next.is_ascii_whitespace()) {
                end = index + 1;
                break;
            }
        }
    }

    // The punctuation that ended the sentence is not part of the name.
    let sentence = flat[..end].trim().trim_end_matches(['.', '!', '?']).trim();
    if sentence.is_empty() {
        return None;
    }

    // The title has a length limit, and the cut lands on a character
    // boundary: a title that ends in half a letter names nothing.
    if sentence.len() > MAX_TITLE_BYTES {
        let mut cut = MAX_TITLE_BYTES;
        while cut > 0 && !sentence.is_char_boundary(cut) {
            cut -= 1;
        }
        let shortened = sentence[..cut].trim();
        if shortened.is_empty() {
            return None;
        }
        return Some(shortened.to_string());
    }

    Some(sentence.to_string())
}

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

    // --- the title a record's own text yields ---

    #[test]
    fn the_first_sentence_names_the_fact() {
        assert_eq!(
            derive_title_from_content("el puerto del servicio es 8081. el cliente usa TLS."),
            Some("el puerto del servicio es 8081".to_string())
        );
    }

    #[test]
    fn a_text_with_no_ending_punctuation_is_its_own_title() {
        assert_eq!(
            derive_title_from_content("el puerto del servicio es 8081"),
            Some("el puerto del servicio es 8081".to_string())
        );
    }

    #[test]
    fn a_decimal_point_does_not_cut_the_sentence() {
        assert_eq!(
            derive_title_from_content("el puerto es 3.5 y el cliente usa TLS"),
            Some("el puerto es 3.5 y el cliente usa TLS".to_string())
        );
    }

    #[test]
    fn the_derived_title_is_one_line() {
        assert_eq!(
            derive_title_from_content("primera linea\nsegunda linea"),
            Some("primera linea segunda linea".to_string())
        );
    }

    #[test]
    fn a_text_that_names_nothing_yields_no_title() {
        assert_eq!(derive_title_from_content(""), None);
        assert_eq!(derive_title_from_content("   \n  "), None);
        assert_eq!(derive_title_from_content("!!!"), None);
    }

    #[test]
    fn a_long_text_is_cut_on_a_character_boundary() {
        let long = "á".repeat(200);
        let title = derive_title_from_content(&long).expect("a title");
        assert!(title.len() <= MAX_TITLE_BYTES, "{}", title.len());
        assert!(title.chars().all(|c| c == 'á'), "no half a character");
    }
}
