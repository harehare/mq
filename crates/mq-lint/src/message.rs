//! Rule identity and diagnostic message types.
//!
//! Every lint rule is identified by a [`RuleId`] variant and, when it fires,
//! produces a [`LintMessage`] carrying whatever data is needed to render the
//! diagnostic text. Keeping both as enums (rather than free-form strings)
//! means the compiler enforces that every rule has exactly one ID and that
//! every message variant maps to a real rule.

use std::fmt;
use std::str::FromStr;

/// Unique identifier for a lint rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RuleId {
    UnusedVariable,
    UnusedFunction,
    UnusedImport,
    UnreachableCode,
    InfiniteLoop,
    DeprecatedFunctionCall,
    DuplicateMatchArm,
    ShadowVariable,
    MissingElseInExpr,
    AlwaysTrueCondition,
    FunctionTooLong,
    TooManyParams,
    DeeplyNested,
    TooManyMatchArms,
    ComplexInterpolation,
    InefficientSelector,
    MissingDepthGuard,
    SelectorAlwaysEmpty,
    MissingModuleDoc,
    AmbiguousQualifiedAccess,
    PreferLetOverVar,
    PreferPipeStyle,
    PreferCoalesce,
    PreferSpecificHeading,
    RedundantTry,
    NamingConvention,
    BooleanComparison,
    RedundantBooleanLiteral,
}

impl RuleId {
    /// All known rule IDs.
    pub const ALL: &'static [RuleId; 28] = &[
        RuleId::UnusedVariable,
        RuleId::UnusedFunction,
        RuleId::UnusedImport,
        RuleId::UnreachableCode,
        RuleId::InfiniteLoop,
        RuleId::DeprecatedFunctionCall,
        RuleId::DuplicateMatchArm,
        RuleId::ShadowVariable,
        RuleId::MissingElseInExpr,
        RuleId::AlwaysTrueCondition,
        RuleId::FunctionTooLong,
        RuleId::TooManyParams,
        RuleId::DeeplyNested,
        RuleId::TooManyMatchArms,
        RuleId::ComplexInterpolation,
        RuleId::InefficientSelector,
        RuleId::MissingDepthGuard,
        RuleId::SelectorAlwaysEmpty,
        RuleId::MissingModuleDoc,
        RuleId::AmbiguousQualifiedAccess,
        RuleId::PreferLetOverVar,
        RuleId::PreferPipeStyle,
        RuleId::PreferCoalesce,
        RuleId::PreferSpecificHeading,
        RuleId::RedundantTry,
        RuleId::NamingConvention,
        RuleId::BooleanComparison,
        RuleId::RedundantBooleanLiteral,
    ];

    /// The rule's `snake_case` identifier, as used in config and CLI flags.
    pub fn as_str(&self) -> &'static str {
        match self {
            RuleId::UnusedVariable => "unused_variable",
            RuleId::UnusedFunction => "unused_function",
            RuleId::UnusedImport => "unused_import",
            RuleId::UnreachableCode => "unreachable_code",
            RuleId::InfiniteLoop => "infinite_loop",
            RuleId::DuplicateMatchArm => "duplicate_match_arm",
            RuleId::DeprecatedFunctionCall => "deprecated_function_call",
            RuleId::ShadowVariable => "shadow_variable",
            RuleId::MissingElseInExpr => "missing_else_in_expr",
            RuleId::AlwaysTrueCondition => "always_true_condition",
            RuleId::FunctionTooLong => "function_too_long",
            RuleId::TooManyParams => "too_many_params",
            RuleId::DeeplyNested => "deeply_nested",
            RuleId::TooManyMatchArms => "too_many_match_arms",
            RuleId::ComplexInterpolation => "complex_interpolation",
            RuleId::InefficientSelector => "inefficient_selector",
            RuleId::MissingDepthGuard => "missing_depth_guard",
            RuleId::SelectorAlwaysEmpty => "selector_always_empty",
            RuleId::MissingModuleDoc => "missing_module_doc",
            RuleId::AmbiguousQualifiedAccess => "ambiguous_qualified_access",
            RuleId::PreferLetOverVar => "prefer_let_over_var",
            RuleId::PreferPipeStyle => "prefer_pipe_style",
            RuleId::PreferCoalesce => "prefer_coalesce",
            RuleId::PreferSpecificHeading => "prefer_specific_heading",
            RuleId::RedundantTry => "redundant_try",
            RuleId::NamingConvention => "naming_convention",
            RuleId::BooleanComparison => "boolean_comparison",
            RuleId::RedundantBooleanLiteral => "redundant_boolean_literal",
        }
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for RuleId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        RuleId::ALL
            .iter()
            .copied()
            .find(|id| id.as_str() == s)
            .ok_or_else(|| format!("unknown rule id `{s}`"))
    }
}

/// A diagnostic finding, carrying whatever data its rule needs to render a
/// message and help text. Each variant corresponds to exactly one [`RuleId`].
#[derive(Debug, Clone, PartialEq)]
pub enum LintMessage {
    UnusedVariable {
        name: String,
    },
    UnusedFunction {
        name: String,
    },
    UnusedImport {
        name: String,
    },
    UnreachableCode {
        keyword: String,
    },
    InfiniteLoop,
    DuplicateMatchArm {
        pattern: String,
    },
    DeprecatedFunctionCall {
        name: String,
    },
    ShadowVariable {
        name: String,
    },
    MissingElseInExpr,
    AlwaysTrueCondition {
        value: String,
    },
    FunctionTooLong {
        name: String,
        line_count: usize,
        max_lines: usize,
    },
    TooManyParams {
        name: String,
        count: usize,
        max: usize,
    },
    DeeplyNested {
        depth: usize,
        max_depth: usize,
    },
    TooManyMatchArms {
        arm_count: usize,
        max_arms: usize,
    },
    ComplexInterpolation {
        expr_count: usize,
        max_exprs: usize,
    },
    InefficientSelector,
    MissingDepthGuard,
    SelectorAlwaysEmpty {
        first: String,
        second: String,
    },
    MissingModuleDoc {
        name: String,
    },
    AmbiguousQualifiedAccess {
        fn_name: String,
        this_module: String,
        other_module: String,
    },
    PreferLetOverVar {
        name: String,
    },
    PreferPipeStyle {
        outer_name: String,
        inner_name: String,
    },
    PreferCoalesce,
    PreferSpecificHeading,
    RedundantTry,
    NamingConvention {
        name: String,
        suggested: String,
    },
    BooleanComparison {
        op: String,
        bool_val: String,
    },
    RedundantBooleanLiteral {
        then_val: String,
    },
}

impl LintMessage {
    /// The rule that produces this message.
    pub fn rule_id(&self) -> RuleId {
        match self {
            LintMessage::UnusedVariable { .. } => RuleId::UnusedVariable,
            LintMessage::UnusedFunction { .. } => RuleId::UnusedFunction,
            LintMessage::UnusedImport { .. } => RuleId::UnusedImport,
            LintMessage::UnreachableCode { .. } => RuleId::UnreachableCode,
            LintMessage::InfiniteLoop => RuleId::InfiniteLoop,
            LintMessage::DuplicateMatchArm { .. } => RuleId::DuplicateMatchArm,
            LintMessage::DeprecatedFunctionCall { .. } => RuleId::DeprecatedFunctionCall,
            LintMessage::ShadowVariable { .. } => RuleId::ShadowVariable,
            LintMessage::MissingElseInExpr => RuleId::MissingElseInExpr,
            LintMessage::AlwaysTrueCondition { .. } => RuleId::AlwaysTrueCondition,
            LintMessage::FunctionTooLong { .. } => RuleId::FunctionTooLong,
            LintMessage::TooManyParams { .. } => RuleId::TooManyParams,
            LintMessage::DeeplyNested { .. } => RuleId::DeeplyNested,
            LintMessage::TooManyMatchArms { .. } => RuleId::TooManyMatchArms,
            LintMessage::ComplexInterpolation { .. } => RuleId::ComplexInterpolation,
            LintMessage::InefficientSelector => RuleId::InefficientSelector,
            LintMessage::MissingDepthGuard => RuleId::MissingDepthGuard,
            LintMessage::SelectorAlwaysEmpty { .. } => RuleId::SelectorAlwaysEmpty,
            LintMessage::MissingModuleDoc { .. } => RuleId::MissingModuleDoc,
            LintMessage::AmbiguousQualifiedAccess { .. } => RuleId::AmbiguousQualifiedAccess,
            LintMessage::PreferLetOverVar { .. } => RuleId::PreferLetOverVar,
            LintMessage::PreferPipeStyle { .. } => RuleId::PreferPipeStyle,
            LintMessage::PreferCoalesce => RuleId::PreferCoalesce,
            LintMessage::PreferSpecificHeading => RuleId::PreferSpecificHeading,
            LintMessage::RedundantTry => RuleId::RedundantTry,
            LintMessage::NamingConvention { .. } => RuleId::NamingConvention,
            LintMessage::BooleanComparison { .. } => RuleId::BooleanComparison,
            LintMessage::RedundantBooleanLiteral { .. } => RuleId::RedundantBooleanLiteral,
        }
    }

    /// Suggested fix text, if the rule has one.
    pub fn help(&self) -> Option<String> {
        match self {
            LintMessage::UnusedVariable { name } | LintMessage::UnusedFunction { name } => {
                Some(format!("if this is intentional, prefix with `_`: `_{name}`"))
            }
            LintMessage::UnusedImport { name } => {
                Some(format!("remove `import \"{name}\"` or use it with `{name}::function`"))
            }
            LintMessage::UnreachableCode { .. } => {
                Some("remove or move this code before the `break`/`continue`".to_string())
            }
            LintMessage::InfiniteLoop => Some("add a `break` expression to exit the loop".to_string()),
            LintMessage::DuplicateMatchArm { .. } => {
                Some("remove or merge this arm with the earlier identical pattern".to_string())
            }
            LintMessage::DeprecatedFunctionCall { name } => Some(format!(
                "deprecated function `{name}`; consider using an alternative or removing the call"
            )),
            LintMessage::ShadowVariable { .. } => Some("consider renaming to avoid confusion".to_string()),
            LintMessage::MissingElseInExpr => {
                Some("add `else: <expr>` to provide a value for the false branch".to_string())
            }
            LintMessage::AlwaysTrueCondition { .. } => {
                Some("replace the `if` with the branch that will always execute".to_string())
            }
            LintMessage::FunctionTooLong { .. } => {
                Some("consider splitting into smaller, focused helper functions".to_string())
            }
            LintMessage::TooManyParams { .. } => {
                Some("consider grouping related parameters or using default arguments".to_string())
            }
            LintMessage::DeeplyNested { .. } => {
                Some("reduce nesting by extracting code into helper functions".to_string())
            }
            LintMessage::TooManyMatchArms { .. } => {
                Some("consider refactoring into helper functions or a lookup table".to_string())
            }
            LintMessage::ComplexInterpolation { .. } => {
                Some("consider extracting parts into intermediate `let` bindings for readability".to_string())
            }
            LintMessage::InefficientSelector => {
                Some("use the specific selector directly (e.g. replace `.. | .h1` with `.h1`)".to_string())
            }
            LintMessage::MissingDepthGuard => Some(
                "consider adding a depth limit, e.g. `.. | select(.depth <= 3)`, \
                 to avoid traversing the entire document"
                    .to_string(),
            ),
            LintMessage::SelectorAlwaysEmpty { .. } => {
                Some("remove one of the selectors, or replace the pipe with a different query".to_string())
            }
            LintMessage::MissingModuleDoc { name } => Some(format!("add a `#` doc comment above `module {name}:`")),
            LintMessage::AmbiguousQualifiedAccess {
                fn_name, this_module, ..
            } => Some(format!(
                "use a fully qualified call (e.g. `{this_module}::{fn_name}()`) to avoid ambiguity"
            )),
            LintMessage::PreferLetOverVar { name } => Some(format!("change `var {name}` to `let {name}`")),
            LintMessage::PreferPipeStyle { outer_name, inner_name } => {
                Some(format!("rewrite as `... | {inner_name}() | {outer_name}()`"))
            }
            LintMessage::PreferCoalesce => Some("rewrite as `<value> ?? <fallback>`".to_string()),
            LintMessage::PreferSpecificHeading => {
                Some("using `.h1`–`.h6` makes the intended heading level explicit".to_string())
            }
            LintMessage::RedundantTry => Some("rewrite as `<expr>?`".to_string()),
            LintMessage::NamingConvention { suggested, .. } => Some(format!("rename to `{suggested}`")),
            LintMessage::BooleanComparison { op, bool_val } => Some(
                match (op.as_str(), bool_val.as_str()) {
                    ("==", "true") => "use the value directly instead of `== true`",
                    ("==", "false") => "use `!` prefix instead of `== false`",
                    ("!=", "true") => "use `!` prefix instead of `!= true`",
                    ("!=", "false") => "use the value directly instead of `!= false`",
                    _ => "simplify this boolean comparison",
                }
                .to_string(),
            ),
            LintMessage::RedundantBooleanLiteral { then_val } => Some(
                if then_val == "true" {
                    "replace `if (cond): true else: false` with just `cond`"
                } else {
                    "replace `if (cond): false else: true` with `not(cond)` or `!(cond)`"
                }
                .to_string(),
            ),
        }
    }
}

impl fmt::Display for LintMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LintMessage::UnusedVariable { name } => write!(f, "unused variable `{name}`"),
            LintMessage::UnusedFunction { name } => write!(f, "unused function `{name}`"),
            LintMessage::UnusedImport { name } => write!(f, "imported module `{name}` is never used"),
            LintMessage::UnreachableCode { keyword } => write!(f, "unreachable code after `{keyword}`"),
            LintMessage::InfiniteLoop => write!(f, "loop without `break` may run forever"),
            LintMessage::DuplicateMatchArm { pattern } => write!(f, "duplicate match arm pattern `{pattern}`"),
            LintMessage::DeprecatedFunctionCall { name } => {
                write!(f, "call to deprecated function `{name}`")
            }
            LintMessage::ShadowVariable { name } => {
                write!(f, "variable `{name}` shadows a variable in an outer scope")
            }
            LintMessage::MissingElseInExpr => {
                write!(
                    f,
                    "`if` expression is missing an `else` branch (evaluates to `none` on false)"
                )
            }
            LintMessage::AlwaysTrueCondition { value } => {
                write!(f, "condition is always `{value}` — this branch is never/always taken")
            }
            LintMessage::FunctionTooLong {
                name,
                line_count,
                max_lines,
            } => {
                write!(f, "function `{name}` is {line_count} lines long (limit: {max_lines})")
            }
            LintMessage::TooManyParams { name, count, max } => {
                write!(f, "function `{name}` has {count} parameters (limit: {max})")
            }
            LintMessage::DeeplyNested { depth, max_depth } => {
                write!(f, "nesting depth {depth} exceeds the limit of {max_depth}")
            }
            LintMessage::TooManyMatchArms { arm_count, max_arms } => {
                write!(f, "match expression has {arm_count} arms (limit: {max_arms})")
            }
            LintMessage::ComplexInterpolation { expr_count, max_exprs } => {
                write!(
                    f,
                    "interpolated string has {expr_count} interpolated expressions (limit: {max_exprs})"
                )
            }
            LintMessage::InefficientSelector => write!(f, "inefficient selector: `..` followed by a specific selector"),
            LintMessage::MissingDepthGuard => write!(f, "`..` (recursive selector) used without a depth guard"),
            LintMessage::SelectorAlwaysEmpty { first, second } => {
                write!(f, "`{first} | {second}` can never match: a node can't be both")
            }
            LintMessage::MissingModuleDoc { name } => write!(f, "module `{name}` has no documentation comment"),
            LintMessage::AmbiguousQualifiedAccess {
                fn_name, other_module, ..
            } => {
                write!(f, "function `{fn_name}` is also defined in module `{other_module}`")
            }
            LintMessage::PreferLetOverVar { name } => {
                write!(f, "`{name}` is never reassigned; prefer `let` over `var`")
            }
            LintMessage::PreferPipeStyle { outer_name, inner_name } => {
                write!(
                    f,
                    "nested call `{outer_name}({inner_name}(...))` reads better as a pipe"
                )
            }
            LintMessage::PreferCoalesce => {
                write!(
                    f,
                    "`if`/`else` null-check can be simplified using the `??` coalesce operator"
                )
            }
            LintMessage::PreferSpecificHeading => {
                write!(f, "prefer a specific heading level selector (`.h1`–`.h6`) over `.h`")
            }
            LintMessage::RedundantTry => {
                write!(
                    f,
                    "`try: ... catch: none` is equivalent to the `?` error-suppression operator"
                )
            }
            LintMessage::NamingConvention { name, .. } => write!(f, "`{name}` should be written in snake_case"),
            LintMessage::BooleanComparison { bool_val, .. } => {
                write!(f, "unnecessary comparison with boolean literal `{bool_val}`")
            }
            LintMessage::RedundantBooleanLiteral { .. } => {
                write!(
                    f,
                    "redundant boolean literal in `if`/`else` — condition already is the result"
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_id_round_trips_through_str() {
        for id in RuleId::ALL {
            assert_eq!(id.as_str().parse::<RuleId>().unwrap(), *id);
        }
    }

    #[test]
    fn rule_id_from_str_rejects_unknown() {
        assert!("not_a_real_rule".parse::<RuleId>().is_err());
    }

    #[test]
    fn message_rule_id_matches_intent() {
        let msg = LintMessage::UnusedVariable { name: "x".to_string() };
        assert_eq!(msg.rule_id(), RuleId::UnusedVariable);
        assert_eq!(msg.to_string(), "unused variable `x`");
        assert!(msg.help().unwrap().contains("_x"));
    }
}
