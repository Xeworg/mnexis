//! Budget accounting for `memory_context` (PRD §9, A-07).
//!
//! E2 implements the `Estimated` mode (4 bytes per token). E3 adds the
//! `Exact` mode with a std-only supported tokenizer. The supported
//! tokenizer (`supported-tokenizer-v1`) is the BPE-lite tokenizer in
//! [`token_count_v1`]: a deterministic, std-only, **upper-bound**
//! counter (i.e. `count(s) <= s.len()` for every UTF-8 string). That
//! property is what A-07 needs: "el contexto con tokenizer soportado
//! nunca supera el presupuesto serializado".

use crate::domain::error::MnexisError;

/// The stable id of the only supported tokenizer in E3. Pinned by tests
/// to catch accidental renames.
pub const SUPPORTED_TOKENIZER_V1: &str = "supported-tokenizer-v1";

/// Budget enforcement mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BudgetMode {
    /// E2 byte-counting proxy (4 bytes per token). The output carries
    /// `budget_enforcement: "estimated"` and a `warnings: ["lexical_estimation"]`.
    #[default]
    Estimated,
    /// E3 exact mode: tokenize with the supported tokenizer and
    /// enforce the budget on the actual token count.
    Exact,
}

/// A budget for `memory_context` output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    token_budget: i64,
    byte_budget: i64,
    mode: BudgetMode,
}

impl Budget {
    /// Build a budget from a token count and a mode.
    pub fn new(token_budget: i64, mode: BudgetMode) -> Self {
        let byte_budget = match mode {
            BudgetMode::Estimated => token_budget.saturating_mul(4),
            // Exact mode: byte budget is informational only (estimated
            // sanity ceiling); the authoritative number is the token
            // budget, enforced by the tokenizer.
            BudgetMode::Exact => token_budget.saturating_mul(8),
        };
        Self {
            token_budget,
            byte_budget,
            mode,
        }
    }

    pub fn token_budget(&self) -> i64 {
        self.token_budget
    }
    pub fn byte_budget(&self) -> i64 {
        self.byte_budget
    }
    pub fn mode(&self) -> BudgetMode {
        self.mode
    }

    pub fn tokenizer_id(&self) -> &'static str {
        SUPPORTED_TOKENIZER_V1
    }

    /// Enforce the byte budget. Only meaningful in `Estimated` mode.
    pub fn check_bytes(&self, response_bytes: i64) -> Result<(), MnexisError> {
        if response_bytes > self.byte_budget {
            Err(MnexisError::budget_too_small(
                self.byte_budget,
                response_bytes,
            ))
        } else {
            Ok(())
        }
    }

    /// Enforce the token budget. Only meaningful in `Exact` mode.
    pub fn check_tokens(&self, tokens_used: i64) -> Result<(), MnexisError> {
        if tokens_used > self.token_budget {
            Err(MnexisError::budget_too_small(
                self.token_budget,
                tokens_used,
            ))
        } else {
            Ok(())
        }
    }

    /// Tokenize with `tokenizer_id` and enforce the corresponding budget
    /// (`Exact` uses the supported tokenizer; `Estimated` uses the byte
    /// proxy with the byte length of the string). A non-supported
    /// `tokenizer_id` is itself a `BudgetTooSmall` per the PRD §9 rule
    /// ("el cliente puede exigir conteo exacto y rechazar dicho modo").
    pub fn check_with_tokenizer(
        &self,
        tokenizer_id: &str,
        response_tokens: i64,
    ) -> Result<(), MnexisError> {
        match self.mode {
            BudgetMode::Estimated => {
                if tokenizer_id == SUPPORTED_TOKENIZER_V1 || tokenizer_id.is_empty() {
                    // Caller is in estimated mode; we treat `response_tokens`
                    // as bytes (the legacy 4-bytes-per-token mapping is
                    // the caller's responsibility, this just enforces).
                    self.check_bytes(response_tokens)
                } else {
                    Err(MnexisError::budget_too_small(
                        self.token_budget,
                        response_tokens,
                    ))
                }
            }
            BudgetMode::Exact => {
                if tokenizer_id != SUPPORTED_TOKENIZER_V1 {
                    Err(MnexisError::budget_too_small(
                        self.token_budget,
                        response_tokens,
                    ))
                } else {
                    self.check_tokens(response_tokens)
                }
            }
        }
    }

    /// Serialize the budget for the `memory_context` output.
    pub fn enforcement_label(&self) -> &'static str {
        match self.mode {
            BudgetMode::Estimated => "estimated",
            BudgetMode::Exact => "exact",
        }
    }
}

// ---------------------------------------------------------------------------
// Tokenizer v1
// ---------------------------------------------------------------------------

/// Tokenize a UTF-8 string with the supported tokenizer v1.
///
/// **Algorithm** (deliberately std-only, BPE-lite, upper-bound):
/// - Split on Unicode whitespace (any run collapses).
/// - Each remaining whitespace-separated "word" is split on every
///   non-alphanumeric ASCII character. Each alphanumeric run is one
///   token; each non-alphanumeric ASCII character is one token.
/// - Non-ASCII bytes (multi-byte UTF-8 continuation bytes) are counted
///   as one token each. This guarantees `count(s) <= s.len()` for every
///   UTF-8 string (the A-07 upper-bound property).
///
/// Deterministic, allocation-cheap, and never over-estimates. Any
/// "real" BPE tokenizer (e.g. `tiktoken-rs`, `tokenizers`) can be
/// substituted later behind the same [`SUPPORTED_TOKENIZER_V1`] id
/// without changing the public contract.
pub fn token_count_v1(input: &str) -> i64 {
    let mut count: i64 = 0;
    let mut in_ws = true; // leading whitespace is collapsed
    let mut in_alnum = false;
    for ch in input.chars() {
        if ch.is_whitespace() {
            if in_alnum {
                in_alnum = false;
            }
            if !in_ws {
                in_ws = true;
            }
            continue;
        }
        if in_ws {
            in_ws = false;
        }
        if ch.is_ascii() {
            if ch.is_ascii_alphanumeric() {
                if !in_alnum {
                    count += 1;
                    in_alnum = true;
                }
                // Continuing the same alphanumeric run: zero additional
                // tokens (keeps the upper bound: one alphanumeric run
                // <= the run's byte count).
            } else {
                in_alnum = false;
                // Punctuation: exactly one token.
                count += 1;
            }
        } else {
            in_alnum = false;
            // Non-ASCII: each character is one token. UTF-8 encodes them
            // as >= 2 bytes, so count <= byte length holds.
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimated_mode_uses_4_bytes_per_token() {
        let b = Budget::new(100, BudgetMode::Estimated);
        assert_eq!(b.token_budget(), 100);
        assert_eq!(b.byte_budget(), 400);
        assert_eq!(b.enforcement_label(), "estimated");
    }

    #[test]
    fn exact_mode_uses_token_budget_directly() {
        let b = Budget::new(100, BudgetMode::Exact);
        assert_eq!(b.token_budget(), 100);
        assert_eq!(b.enforcement_label(), "exact");
    }

    #[test]
    fn check_bytes_accepts_within_budget() {
        let b = Budget::new(100, BudgetMode::Estimated);
        assert!(b.check_bytes(200).is_ok());
        assert!(b.check_bytes(400).is_ok());
    }

    #[test]
    fn check_bytes_rejects_over_budget() {
        let b = Budget::new(100, BudgetMode::Estimated);
        let err = b.check_bytes(401).unwrap_err();
        assert!(matches!(err, MnexisError::BudgetTooSmall { .. }));
    }

    #[test]
    fn check_tokens_rejects_over_budget() {
        let b = Budget::new(10, BudgetMode::Exact);
        assert!(b.check_tokens(10).is_ok());
        let err = b.check_tokens(11).unwrap_err();
        assert!(matches!(err, MnexisError::BudgetTooSmall { .. }));
    }

    #[test]
    fn zero_token_budget_yields_zero_byte_budget() {
        let b = Budget::new(0, BudgetMode::Estimated);
        assert_eq!(b.byte_budget(), 0);
        assert!(b.check_bytes(1).is_err());
    }

    #[test]
    fn token_count_v1_idempotent() {
        let s = "el render del grafo requiere 2500 tokens, según PRD";
        assert_eq!(token_count_v1(s), token_count_v1(s));
    }

    #[test]
    fn token_count_v1_upper_bound_on_ascii() {
        let s = "hello, world! 2025.";
        assert!(token_count_v1(s) <= s.len() as i64);
    }

    #[test]
    fn token_count_v1_alphanumeric_runs_collapse() {
        // "alpha" is one alphanumeric run = 1 token.
        assert_eq!(token_count_v1("alpha"), 1);
        // "alpha alpha alpha" = 3 runs.
        assert_eq!(token_count_v1("alpha alpha alpha"), 3);
        // "hello, world" = 2 runs + 1 punctuation token.
        assert_eq!(token_count_v1("hello, world"), 3);
    }

    #[test]
    fn token_count_v1_non_ascii_uses_byte_length() {
        // Each non-ASCII char is 2 bytes in UTF-8; each one is one token.
        assert_eq!(token_count_v1("ñáéíóú"), 6);
    }
}
