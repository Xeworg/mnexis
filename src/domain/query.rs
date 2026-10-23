//! The query contract: what a caller's words mean to the engine.
//!
//! An agent asks in plain language ("puerto base de datos", "staging-eu").
//! FTS5 does not speak that language: a bare multi-word query is an implicit
//! **AND** — every word has to be in the memory, so a paraphrase that drops
//! one word finds nothing — and the query text is parsed as FTS5 syntax, so
//! a hyphen, a colon or a dot in an ordinary word breaks the search.
//!
//! The engine therefore compiles the caller's query into a safe expression
//! before it reaches the index. The caller picks the meaning with a
//! [`MatchMode`]; the default (`any`) is the one an agent wants: every term
//! counts, ranked by relevance.
//!
//! The compiler owns the rule so that every surface (CLI, MCP server, pi
//! extension) behaves the same way.

use std::fmt;

use crate::domain::error::MnexisError;

/// How the terms of a query are combined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MatchMode {
    /// Every term counts: a memory matches if it contains any of them.
    /// This is the default, because it is what "tell me about X" means.
    #[default]
    Any,
    /// Every term has to be present: a memory matches if it contains all.
    All,
    /// The query is one exact phrase, in that order.
    Phrase,
}

impl MatchMode {
    /// The mode's name, as the CLI and the MCP tool spell it.
    pub fn as_str(&self) -> &'static str {
        match self {
            MatchMode::Any => "any",
            MatchMode::All => "all",
            MatchMode::Phrase => "phrase",
        }
    }

    /// Parse a mode name. The accepted set is `any`, `all` and `phrase`.
    pub fn parse(value: &str) -> Result<Self, MnexisError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "any" => Ok(MatchMode::Any),
            "all" => Ok(MatchMode::All),
            "phrase" => Ok(MatchMode::Phrase),
            other => Err(MnexisError::invalid_argument(
                "match",
                format!("unknown match mode '{other}' (expected any, all or phrase)"),
            )),
        }
    }
}

impl fmt::Display for MatchMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The words FTS5 uses as operators. In `any` and `all` they are noise: an
/// agent that writes "puerto OR postgres" means two terms, not an operator.
const OPERATORS: [&str; 4] = ["and", "or", "not", "near"];

/// Compile a caller's query into an FTS5 expression.
///
/// The result is always a quoted expression: a term can never be read as
/// FTS5 syntax, so a hyphen, a colon or a quote inside a word is just a word.
///
/// * `any` (the default) joins the terms with `OR`.
/// * `all` joins the terms with `AND`.
/// * `phrase` keeps the query as one quoted phrase.
///
/// An empty query — or one with no word in it — is an invalid argument, not
/// an empty result: the caller asked something that cannot be searched.
pub fn compile(query: &str, mode: MatchMode) -> Result<String, MnexisError> {
    match mode {
        MatchMode::Phrase => compile_phrase(query),
        MatchMode::Any | MatchMode::All => compile_terms(query, mode),
    }
}

/// The whole query as one quoted phrase.
fn compile_phrase(query: &str) -> Result<String, MnexisError> {
    let collapsed = collapse_whitespace(query);
    if !collapsed.chars().any(|c| c.is_alphanumeric()) {
        return Err(no_terms(query));
    }
    Ok(format!("\"{}\"", collapsed.replace('"', "\"\"")))
}

/// The query as quoted terms joined by the mode's operator.
fn compile_terms(query: &str, mode: MatchMode) -> Result<String, MnexisError> {
    let mut terms: Vec<String> = Vec::new();
    for word in split_words(query) {
        if OPERATORS.contains(&word.as_str()) {
            continue;
        }
        terms.push(word);
    }
    // A query made only of operator words ("or") still means something: keep
    // the words instead of refusing the search.
    if terms.is_empty() {
        terms = split_words(query);
    }
    if terms.is_empty() {
        return Err(no_terms(query));
    }
    let separator = if mode == MatchMode::All {
        " AND "
    } else {
        " OR "
    };
    Ok(terms
        .iter()
        .map(|t| format!("\"{t}\""))
        .collect::<Vec<_>>()
        .join(separator))
}

/// The query's words, lowercased, in order.
fn split_words(query: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    for c in query.chars() {
        if c.is_alphanumeric() {
            current.extend(c.to_lowercase());
        } else if !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// The query with its whitespace collapsed to single spaces.
fn collapse_whitespace(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    let mut space = false;
    for c in query.chars() {
        if c.is_whitespace() {
            space = true;
            continue;
        }
        if space && !out.is_empty() {
            out.push(' ');
        }
        space = false;
        out.push(c);
    }
    out
}

/// The error for a query with nothing to search for.
fn no_terms(query: &str) -> MnexisError {
    MnexisError::invalid_argument("query", format!("no searchable term in '{}'", query.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_multi_word_query_is_not_a_phrase() {
        let compiled = compile("puerto base de datos", MatchMode::Any).unwrap();
        assert_eq!(compiled, "\"puerto\" OR \"base\" OR \"de\" OR \"datos\"");
    }

    #[test]
    fn a_hyphenated_word_is_one_term_and_never_syntax() {
        let compiled = compile("staging-eu", MatchMode::Any).unwrap();
        assert_eq!(compiled, "\"staging\" OR \"eu\"");
        assert!(!compiled.contains(':'));
    }

    #[test]
    fn all_mode_joins_the_terms_with_and() {
        let compiled = compile("puerto base de datos", MatchMode::All).unwrap();
        assert_eq!(compiled, "\"puerto\" AND \"base\" AND \"de\" AND \"datos\"");
    }

    #[test]
    fn phrase_mode_keeps_the_query_as_one_phrase() {
        let compiled = compile("puerto base de datos", MatchMode::Phrase).unwrap();
        assert_eq!(compiled, "\"puerto base de datos\"");
    }

    #[test]
    fn operator_words_are_not_terms() {
        let compiled = compile("puerto OR postgres", MatchMode::Any).unwrap();
        assert_eq!(compiled, "\"puerto\" OR \"postgres\"");
    }

    #[test]
    fn a_query_made_only_of_operator_words_still_searches() {
        let compiled = compile("or", MatchMode::Any).unwrap();
        assert_eq!(compiled, "\"or\"");
    }

    #[test]
    fn an_empty_query_is_an_invalid_argument() {
        let error = compile("   ", MatchMode::Any).unwrap_err();
        assert!(matches!(error, MnexisError::InvalidArgument { .. }));
        assert_eq!(error.error_code_label(), "INVALID_ARGUMENT");
    }

    #[test]
    fn a_query_without_words_is_an_invalid_argument() {
        let error = compile("--- ...", MatchMode::Phrase).unwrap_err();
        assert!(matches!(error, MnexisError::InvalidArgument { .. }));
    }

    #[test]
    fn a_quote_inside_the_query_cannot_break_the_expression() {
        let compiled = compile("say \"hi\" now", MatchMode::Phrase).unwrap();
        assert_eq!(compiled, "\"say \"\"hi\"\" now\"");
    }

    #[test]
    fn digits_and_unicode_words_are_terms() {
        let compiled = compile("puerto 8081 ñandú", MatchMode::Any).unwrap();
        assert_eq!(compiled, "\"puerto\" OR \"8081\" OR \"ñandú\"");
    }

    #[test]
    fn the_mode_names_round_trip() {
        for mode in [MatchMode::Any, MatchMode::All, MatchMode::Phrase] {
            assert_eq!(MatchMode::parse(mode.as_str()).unwrap(), mode);
        }
        assert_eq!(MatchMode::parse("ANY").unwrap(), MatchMode::Any);
        assert!(MatchMode::parse("nope").is_err());
        assert_eq!(MatchMode::default(), MatchMode::Any);
    }
}
