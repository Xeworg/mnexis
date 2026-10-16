//! Budget accounting for `memory_context` (PRD §9).
//!
//! E2 implements the `estimated` mode only: a byte-counting proxy using
//! 4 bytes per token. Exact tokenization is E3.

use crate::domain::error::MnexisError;

/// Budget enforcement mode. E2 supports `Estimated` only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetMode {
    /// Byte-counting proxy (4 bytes per token). The output carries
    /// `budget_enforcement: "estimated"` and a `warnings: ["lexical_estimation"]`.
    Estimated,
}

impl Default for BudgetMode {
    fn default() -> Self {
        BudgetMode::Estimated
    }
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
    /// `token_budget` is the user-supplied input; the effective byte budget
    /// is `token_budget * 4` for `Estimated`.
    pub fn new(token_budget: i64, mode: BudgetMode) -> Self {
        let byte_budget = match mode {
            BudgetMode::Estimated => token_budget.saturating_mul(4),
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

    /// Check if a response of `response_bytes` fits within the byte budget.
    /// Returns `MnexisError::BudgetTooSmall` if it does not.
    pub fn check(&self, response_bytes: i64) -> Result<(), MnexisError> {
        if response_bytes > self.byte_budget {
            Err(MnexisError::budget_too_small(
                self.byte_budget,
                response_bytes,
            ))
        } else {
            Ok(())
        }
    }

    /// Serialize the budget for the `memory_context` output.
    pub fn enforcement_label(&self) -> &'static str {
        match self.mode {
            BudgetMode::Estimated => "estimated",
        }
    }
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
    fn check_accepts_within_budget() {
        let b = Budget::new(100, BudgetMode::Estimated);
        assert!(b.check(200).is_ok());
        assert!(b.check(400).is_ok());
    }

    #[test]
    fn check_rejects_over_budget() {
        let b = Budget::new(100, BudgetMode::Estimated);
        let err = b.check(401).unwrap_err();
        assert!(matches!(err, MnexisError::BudgetTooSmall { .. }));
    }

    #[test]
    fn zero_token_budget_yields_zero_byte_budget() {
        let b = Budget::new(0, BudgetMode::Estimated);
        assert_eq!(b.byte_budget(), 0);
        assert!(b.check(1).is_err());
    }
}
