//! What a **value** is: the number a fact carries, and the rule that decides
//! whether a record carries it.
//!
//! A value is a **run of three or more digits, exactly equal to the number
//! asked for**. The rule is the one the bench measured (R1) and the one the
//! extension's twin rule applies (`extensions/mnexis-memory/index.js`,
//! `numericValues`): the digit runs of a text, in order, each of three or
//! more digits.
//!
//! The run is **maximal**: the digits around it are not part of it. So
//! `8081` is the value of `the port is 8081` and of `v8081`, and it is not
//! the value of `80810` nor of `18081` — those carry `80810` and `18081`.
//!
//! Why runs and not tokens: a token-exact rule (the FTS5 tokenizer's) reads
//! `v8081` as one token and finds no value in it, while the twin rule the
//! bench measured reads `8081` out of it. The engine's filter must be a
//! **superset** of the records the twin rule can name, or the repair would
//! miss a twin; runs are the superset direction.
//!
//! The rule lives here, in the domain layer, because every surface that
//! answers "which records carry this value?" must answer it the same way:
//! the CLI, the MCP server and the extension that asks the engine.

use crate::domain::error::MnexisError;

/// The fewest digits a run must have to be a value.
///
/// A run of one or two digits is a number, not a value: the measured twin
/// signature is a number of three or more digits, and a two-digit run
/// ("80") is far too common to identify a fact.
pub const MIN_VALUE_DIGITS: usize = 3;

/// A value: a run of three or more digits, exactly as the caller wrote it.
///
/// The value keeps the caller's spelling (a leading zero is part of it:
/// `08081` is not `8081`), because a value identifies a fact and the way
/// it was written is part of the record that carries it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Value(String);

impl Value {
    /// The value as the caller wrote it.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Read a caller's argument as a value.
///
/// A value is a run of three or more digits. Anything else is an **invalid
/// argument**, never an empty answer: the caller asked something the engine
/// cannot answer, and an empty list would hide the mistake behind a
/// plausible-looking "no memories".
///
/// The message names the rule, so the caller learns it from the error.
pub fn parse(raw: &str) -> Result<Value, MnexisError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(invalid(raw));
    }
    if !trimmed.chars().all(|c| c.is_ascii_digit()) {
        return Err(invalid(raw));
    }
    if trimmed.len() < MIN_VALUE_DIGITS {
        return Err(invalid(raw));
    }
    Ok(Value(trimmed.to_string()))
}

/// The error for an argument that is not a value.
fn invalid(raw: &str) -> MnexisError {
    MnexisError::invalid_argument(
        "value",
        format!(
            "'{}' is not a value: a value is a run of {MIN_VALUE_DIGITS} or more digits, \
             exactly as written (8081 is a value, 80 and 8a81 are not)",
            raw.trim()
        ),
    )
}

/// True when the text carries the value: one of its **maximal digit runs**
/// is the value, exactly.
///
/// The scan is the same one the extension applies to a content, and it is
/// what makes the engine's answer a superset of the twin set: a value
/// written inside a word (`v8081`) is carried by the text, and a longer run
/// (`80810`) is a different value, not this one.
pub fn text_carries_value(text: &str, value: &str) -> bool {
    let digits = value.as_bytes();
    if digits.is_empty() || !digits.iter().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let bytes = text.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if !bytes[index].is_ascii_digit() {
            index += 1;
            continue;
        }
        // The run starts here: take every digit that follows.
        let start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        let run = &bytes[start..index];
        if run.len() >= MIN_VALUE_DIGITS && run == digits {
            return true;
        }
    }
    false
}

/// True when the record carries the value in its **indexed text**: the
/// content, the title, the topic key and the keywords — the same four
/// columns the FTS5 index covers.
///
/// The answer is a superset of the records whose *content* carries the
/// value, which is the safe direction for a filter that feeds a repair: an
/// extra candidate is a read the caller rejects, a missing one is a twin
/// that stays alive.
pub fn record_carries_value(
    content: &str,
    title: &str,
    topic_key: &str,
    keywords: &[String],
    value: &str,
) -> bool {
    if text_carries_value(content, value)
        || text_carries_value(title, value)
        || text_carries_value(topic_key, value)
    {
        return true;
    }
    keywords
        .iter()
        .any(|keyword| text_carries_value(keyword, value))
}

/// The digit runs of a text, each of three or more digits, in order.
///
/// This is the twin rule's signature, exposed by the engine so both sides
/// of the comparison read the same definition.
pub fn values_in(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut values = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        if !bytes[index].is_ascii_digit() {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        let run = &bytes[start..index];
        if run.len() >= MIN_VALUE_DIGITS {
            values.push(String::from_utf8_lossy(run).into_owned());
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    fn carries(text: &str, value: &str) -> bool {
        text_carries_value(text, value)
    }

    #[test]
    fn a_value_is_a_run_of_three_or_more_digits() {
        assert_eq!(parse("8081").unwrap().as_str(), "8081");
        assert_eq!(parse("  8081  ").unwrap().as_str(), "8081");
        assert_eq!(parse("000").unwrap().as_str(), "000");
    }

    #[test]
    fn a_number_that_is_not_a_value_is_an_invalid_argument() {
        for raw in ["", "   ", "80", "8", "8a81", "port", "8081a", "-8081", "8081.0", "٨٠٨١"] {
            let error = parse(raw).unwrap_err();
            assert!(
                matches!(error, MnexisError::InvalidArgument { .. }),
                "expected an invalid argument for '{raw}'"
            );
            assert_eq!(error.error_code_label(), "INVALID_ARGUMENT");
        }
    }

    #[test]
    fn the_error_names_the_rule() {
        let error = parse("80").unwrap_err();
        let message = error.to_string();
        assert!(message.contains("value"), "{message}");
        assert!(message.contains("8081"), "{message}");
    }

    #[test]
    fn a_text_carries_the_value_it_names() {
        assert!(carries("the port is 8081", "8081"));
        assert!(carries("8081 is the port", "8081"));
        assert!(carries("port 8081 tcp", "8081"));
    }

    #[test]
    fn the_run_is_maximal() {
        // A longer run is a different value.
        assert!(!carries("the port is 80810", "8081"));
        assert!(!carries("18081 is the port", "8081"));
        // A shorter run is not a value at all.
        assert!(!carries("the port is 80", "80"));
    }

    #[test]
    fn a_value_written_inside_a_word_is_carried() {
        // The letters are not part of the run: the twin rule reads the
        // value out of it, so the engine's filter must find it too.
        assert!(carries("v8081", "8081"));
        assert!(carries("http8081", "8081"));
        assert!(carries("8081a", "8081"));
    }

    #[test]
    fn a_text_without_the_value_does_not_carry_it() {
        assert!(!carries("the port is 9090", "8081"));
        assert!(!carries("", "8081"));
        assert!(!carries("no digits here", "8081"));
    }

    #[test]
    fn the_signature_of_a_text_is_its_runs_of_three_or_more_digits() {
        assert_eq!(values_in("the port is 8081"), vec!["8081".to_string()]);
        assert_eq!(
            values_in("8081 and 9090"),
            vec!["8081".to_string(), "9090".to_string()]
        );
        assert_eq!(values_in("80 and 8 are too short"), Vec::<String>::new());
        assert_eq!(values_in("v8081"), vec!["8081".to_string()]);
        assert_eq!(values_in("80810"), vec!["80810".to_string()]);
    }

    #[test]
    fn a_record_carries_the_value_in_any_of_its_indexed_columns() {
        assert!(record_carries_value("the port is 8081", "", "", &[], "8081"));
        assert!(record_carries_value("", "Port 8081", "", &[], "8081"));
        assert!(record_carries_value("", "", "port-8081", &[], "8081"));
        assert!(record_carries_value("", "", "", &["port 8081".to_string()], "8081"));
        assert!(!record_carries_value("the port is 9090", "", "", &[], "8081"));
    }
}
