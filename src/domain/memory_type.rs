//! The type of a memory: what kind of thing the fact is.
//!
//! The type is an attribute the **agent writes**. The keywords lesson is
//! the evidence: an attribute the agent sets is used (3 of 3 runs), while
//! an attribute the engine guesses proposes noise. So the type never
//! enters the identity — the topic key stays title-only — and the engine
//! never infers one: it validates what it is given and keeps what is
//! already there.
//!
//! The set is small and closed on purpose. A free-text type is a type
//! nobody filters: every writer invents its own word and the filter never
//! matches. Seven values cover what a project remembers:
//!
//! - `knowledge` — a fact about the world that does not change with the
//!   project (`the service listens on 8081`);
//! - `decision` — a choice that was made, with its reason
//!   (`we deploy to staging-eu`);
//! - `preference` — a stated taste or constraint (`never log the payload`);
//! - `procedure` — how something is done here (`the installer rebuilds a
//!   stale binary first`);
//! - `project_state` — where the project is right now (`the door is open,
//!   the bench needs docker`);
//! - `episode` — what happened, once (`the capture was blocked at collect`);
//! - `fact` — the generic one: a fact with no stronger kind. It is the
//!   default every record inherits, and the value the records that predate
//!   the type axis already carry, so no stored row has to be rewritten.
//!
//! Certainty is **not** a type: it already has two axes — the
//! `assertion_basis` (who said it) and the state (`disputed`) — and a
//! third one would duplicate them.

use crate::domain::error::MnexisError;

/// Every memory type, in the order the filter and the docs list them.
pub const MEMORY_TYPES: [&str; 7] = [
    "knowledge",
    "decision",
    "preference",
    "procedure",
    "project_state",
    "episode",
    "fact",
];

/// The type a record has when nothing says otherwise: a fact with no
/// stronger kind. It is also what every record that predates the type
/// axis carries, so the axis can be made honest without rewriting a
/// single stored row.
pub const DEFAULT_MEMORY_TYPE: &str = "fact";

/// The steering tier of a type: the order the door's ranking consults
/// before anything else (arm B of the steering experiment — see
/// `tmp/bench/steering-measurement.md`). Lower sorts first: the state
/// of the project answers "where are we", the decision says what was
/// chosen, and the episode is history. The tier is a fixed order over
/// the closed set — never a guess about the question's words. Unknown
/// values rank with the generic `fact`.
pub fn steering_tier(value: &str) -> u8 {
    match value {
        "project_state" => 0,
        "decision" => 1,
        "preference" => 2,
        "procedure" => 3,
        "knowledge" => 4,
        "episode" => 6,
        // `fact`, and anything a validated write can never produce.
        _ => 5,
    }
}

/// True when the value is one of the types.
pub fn is_memory_type(value: &str) -> bool {
    MEMORY_TYPES.contains(&value)
}

/// The type a write resolves to, or the error that says why not.
///
/// An empty request means "not specified", not "the default": a re-save
/// that says nothing about the type must not erase the one the record
/// already carries. So the type a record ends up with is the one the
/// request named, or the one it already has, or the default when it has
/// neither.
pub fn resolve_memory_type(
    requested: Option<&str>,
    current: Option<&str>,
) -> Result<String, MnexisError> {
    match requested.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => {
            validate_memory_type(value)?;
            Ok(value.to_string())
        }
        None => Ok(current
            .map(|value| value.to_string())
            .unwrap_or_else(|| DEFAULT_MEMORY_TYPE.to_string())),
    }
}

/// The set is closed: a type nobody can filter is a type nobody uses, so
/// a value outside the set is an invalid argument and not a new kind.
pub fn validate_memory_type(value: &str) -> Result<(), MnexisError> {
    if is_memory_type(value) {
        return Ok(());
    }
    Err(MnexisError::invalid_argument(
        "type",
        format!(
            "unknown memory type '{value}'; valid types: {}",
            MEMORY_TYPES.join(", ")
        ),
    ))
}

/// The kinds a filter names, parsed from a comma-separated list.
///
/// The list is the caller's: `decision,preference`. Every name is checked
/// against the closed set, so a filter nobody can use is refused instead
/// of silently matching nothing. An empty list filters nothing.
pub fn parse_memory_types(value: &str) -> Result<Vec<String>, MnexisError> {
    let mut types = Vec::new();
    for name in value.split(',') {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        validate_memory_type(name)?;
        types.push(name.to_string());
    }
    Ok(types)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_set_is_small_and_closed() {
        assert_eq!(MEMORY_TYPES.len(), 7);
        for value in MEMORY_TYPES {
            assert!(is_memory_type(value), "{value} is in the set");
        }
        assert!(!is_memory_type("nota"));
        assert!(!is_memory_type(""));
        assert!(!is_memory_type("Decision"));
    }

    #[test]
    fn the_default_is_a_type_of_the_set() {
        assert!(is_memory_type(DEFAULT_MEMORY_TYPE));
        assert_eq!(DEFAULT_MEMORY_TYPE, "fact");
    }

    #[test]
    fn a_type_outside_the_set_is_an_invalid_argument() {
        let error = validate_memory_type("nota").expect_err("not a type");
        assert!(
            error.to_string().contains("unknown memory type 'nota'"),
            "the error names the value: {error}"
        );
        assert!(
            error.to_string().contains("knowledge"),
            "and the valid set: {error}"
        );
    }

    #[test]
    fn a_named_type_is_kept_as_it_is() {
        assert_eq!(
            resolve_memory_type(Some("decision"), None).expect("a type"),
            "decision"
        );
    }

    #[test]
    fn a_request_that_names_none_gets_the_default() {
        assert_eq!(
            resolve_memory_type(None, None).expect("the default"),
            "fact"
        );
        assert_eq!(
            resolve_memory_type(Some("  "), None).expect("the default"),
            "fact"
        );
    }

    #[test]
    fn a_request_that_names_none_keeps_the_type_the_record_already_has() {
        assert_eq!(
            resolve_memory_type(None, Some("decision")).expect("the type it has"),
            "decision"
        );
    }

    #[test]
    fn a_request_with_a_wrong_type_is_refused() {
        assert!(resolve_memory_type(Some("nota"), None).is_err());
    }

    #[test]
    fn a_filter_names_several_kinds() {
        assert_eq!(
            parse_memory_types("decision, preference").expect("the filter"),
            vec!["decision".to_string(), "preference".to_string()]
        );
    }

    #[test]
    fn an_empty_filter_filters_nothing() {
        assert!(parse_memory_types("").expect("no filter").is_empty());
        assert!(parse_memory_types(" , ").expect("no filter").is_empty());
    }

    #[test]
    fn a_filter_with_a_wrong_kind_is_refused() {
        assert!(parse_memory_types("decision,nota").is_err());
    }
}
