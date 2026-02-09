/// Builtin function names used throughout the AST.
///
/// This module contains string constants for all builtin functions available in the mq language,
/// including constructors, accessors, operators, and utility functions.
pub mod builtins {
    pub const ARRAY: &str = "array";
    pub const DICT: &str = "dict";

    pub const GET: &str = "get";
    pub const SET: &str = "set";
    pub const SLICE: &str = "slice";
    pub const ATTR: &str = "attr";
    pub const SET_ATTR: &str = "set_attr";
    pub const LEN: &str = "len";

    pub const EQ: &str = "eq";
    pub const NE: &str = "ne";
    pub const LT: &str = "lt";
    pub const LTE: &str = "lte";
    pub const GT: &str = "gt";
    pub const GTE: &str = "gte";

    pub const ADD: &str = "add";
    pub const SUB: &str = "sub";
    pub const MUL: &str = "mul";
    pub const DIV: &str = "div";
    pub const MOD: &str = "mod";
    pub const FLOOR: &str = "floor";

    pub const NOT: &str = "not";
    pub const NEGATE: &str = "negate";

    pub const RANGE: &str = "range";

    pub const BREAKPOINT: &str = "breakpoint";
    pub const COALESCE: &str = "coalesce";
}

/// Reserved identifiers and special symbols used in the language.
///
/// This module contains string constants for reserved keywords and special symbols
/// that have semantic meaning in the mq language, such as `self` and pattern matching wildcards.
pub mod identifiers {
    pub const SELF: &str = "self";
    pub const PATTERN_MATCH_WILDCARD: &str = "_";
}
