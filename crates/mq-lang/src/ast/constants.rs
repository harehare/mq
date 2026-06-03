pub mod builtins {
    pub const ARRAY: &str = "array";
    pub const DICT: &str = "dict";

    pub const CONVERT: &str = "convert";
    pub const GET: &str = "get";
    pub const SET: &str = "set";
    pub const SLICE: &str = "slice";
    pub const SHIFT_LEFT: &str = "shift_left";
    pub const SHIFT_RIGHT: &str = "shift_right";
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
    pub const CEIL: &str = "ceil";
    pub const ROUND: &str = "round";
    pub const ABS: &str = "abs";
    pub const TRUNC: &str = "trunc";

    pub const TO_STRING: &str = "to_string";
    pub const TO_NUMBER: &str = "to_number";

    pub const TRIM: &str = "trim";
    pub const LTRIM: &str = "ltrim";
    pub const RTRIM: &str = "rtrim";
    pub const UPCASE: &str = "upcase";
    pub const DOWNCASE: &str = "downcase";

    pub const STARTS_WITH: &str = "starts_with";
    pub const ENDS_WITH: &str = "ends_with";
    pub const INDEX: &str = "index";
    pub const RINDEX: &str = "rindex";

    pub const REPLACE: &str = "replace";

    pub const IS_REGEX_MATCH: &str = "is_regex_match";
    pub const IS_NOT_REGEX_MATCH: &str = "is_not_regex_match";

    pub const NOT: &str = "not";
    pub const NEGATE: &str = "negate";

    pub const RANGE: &str = "range";

    pub const BREAKPOINT: &str = "breakpoint";
    pub const COALESCE: &str = "coalesce";
}

pub mod identifiers {
    pub const SELF: &str = "self";
    pub const PATTERN_MATCH_WILDCARD: &str = "_";
}
