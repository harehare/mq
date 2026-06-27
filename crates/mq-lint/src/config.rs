//! Configuration for the mq linter.

use std::collections::HashMap;

use crate::RuleId;

/// Per-rule enable/disable flag.
#[derive(Debug, Clone)]
pub struct RuleConfig {
    pub enabled: bool,
}

impl Default for RuleConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// Thresholds for complexity rules.
#[derive(Debug, Clone)]
pub struct ComplexityThresholds {
    /// Max lines in a function body before `function_too_long` fires.
    pub function_max_lines: usize,
    /// Max number of parameters before `too_many_params` fires.
    pub max_params: usize,
    /// Max nesting depth before `deeply_nested` fires.
    pub max_nesting_depth: usize,
    /// Max number of match arms before `too_many_match_arms` fires.
    pub max_match_arms: usize,
    /// Max interpolated expressions before `complex_interpolation` fires.
    pub max_interpolation_exprs: usize,
}

impl Default for ComplexityThresholds {
    fn default() -> Self {
        Self {
            function_max_lines: 50,
            max_params: 5,
            max_nesting_depth: 4,
            max_match_arms: 15,
            max_interpolation_exprs: 3,
        }
    }
}

/// Top-level linter configuration.
#[derive(Debug, Clone, Default)]
pub struct LintConfig {
    /// Per-rule overrides. Rules not listed here use their default enabled state.
    pub rules: HashMap<RuleId, RuleConfig>,
    pub complexity: ComplexityThresholds,
}

impl LintConfig {
    /// Returns `true` if the given rule should run.
    pub fn is_rule_enabled(&self, rule_id: RuleId) -> bool {
        self.rules.get(&rule_id).map(|r| r.enabled).unwrap_or(true)
    }

    /// Disable a specific rule by ID.
    pub fn disable_rule(&mut self, rule_id: RuleId) {
        self.rules.insert(rule_id, RuleConfig { enabled: false });
    }
}
