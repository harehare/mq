pub mod builtins {
    pub const ARRAY: &str = "array";
    pub const DICT: &str = "dict";

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

    pub const IS_REGEX_MATCH: &str = "is_regex_match";

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
