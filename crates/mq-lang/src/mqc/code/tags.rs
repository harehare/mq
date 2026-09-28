//! Stable wire IDs for the CODE section. Changing a value requires a format version bump.

/// Parameter wire tags.
pub(super) mod parameter {
    pub(crate) const REQUIRED: u8 = 0;
    pub(crate) const OPTIONAL: u8 = 1;
    pub(crate) const VARIADIC: u8 = 2;
}

/// Capture wire tags.
pub(super) mod capture {
    pub(crate) const LOCAL: u8 = 0;
    pub(crate) const UPVALUE: u8 = 1;
}

/// Constant wire tags.
pub(super) mod constant {
    pub(crate) const NONE: u8 = 0;
    pub(crate) const NUMBER: u8 = 1;
    pub(crate) const BOOLEAN: u8 = 2;
    pub(crate) const STRING: u8 = 3;
    pub(crate) const SYMBOL: u8 = 4;
    pub(crate) const BYTES: u8 = 5;
    pub(crate) const ARRAY: u8 = 6;
    pub(crate) const DICT: u8 = 7;
    pub(crate) const NATIVE_FUNCTION: u8 = 8;
    pub(crate) const COROUTINE_BUILTIN: u8 = 9;
}

/// Coroutine wire tags.
pub(super) mod coroutine {
    pub(crate) const NEXT: u8 = 0;
    pub(crate) const SEND: u8 = 1;
}

/// Binary wire tags.
pub(super) mod binary {
    pub(crate) const ADD: u8 = 0;
    pub(crate) const SUB: u8 = 1;
    pub(crate) const MUL: u8 = 2;
    pub(crate) const DIV: u8 = 3;
    pub(crate) const MOD: u8 = 4;
    pub(crate) const EQ: u8 = 5;
    pub(crate) const NE: u8 = 6;
    pub(crate) const LT: u8 = 7;
    pub(crate) const LE: u8 = 8;
    pub(crate) const GT: u8 = 9;
    pub(crate) const GE: u8 = 10;
}

/// Attribute wire tags.
pub(super) mod attribute {
    pub(crate) const VALUE: u8 = 0;
    pub(crate) const VALUES: u8 = 1;
    pub(crate) const CHILDREN: u8 = 2;
    pub(crate) const LANG: u8 = 3;
    pub(crate) const META: u8 = 4;
    pub(crate) const FENCE: u8 = 5;
    pub(crate) const URL: u8 = 6;
    pub(crate) const ALT: u8 = 7;
    pub(crate) const TITLE: u8 = 8;
    pub(crate) const IDENT: u8 = 9;
    pub(crate) const LABEL: u8 = 10;
    pub(crate) const DEPTH: u8 = 11;
    pub(crate) const LEVEL: u8 = 12;
    pub(crate) const INDEX: u8 = 13;
    pub(crate) const ORDERED: u8 = 14;
    pub(crate) const CHECKED: u8 = 15;
    pub(crate) const COLUMN: u8 = 16;
    pub(crate) const ROW: u8 = 17;
    pub(crate) const ALIGN: u8 = 18;
    pub(crate) const NAME: u8 = 19;
    pub(crate) const KIND: u8 = 20;
    pub(crate) const LINE: u8 = 21;
    pub(crate) const END_LINE: u8 = 22;
}

/// Selector wire tags.
pub(super) mod selector {
    pub(crate) const BLOCKQUOTE: u8 = 0;
    pub(crate) const FOOTNOTE: u8 = 1;
    pub(crate) const LIST: u8 = 2;
    pub(crate) const TOML: u8 = 3;
    pub(crate) const YAML: u8 = 4;
    pub(crate) const BREAK: u8 = 5;
    pub(crate) const INLINE_CODE: u8 = 6;
    pub(crate) const INLINE_MATH: u8 = 7;
    pub(crate) const DELETE: u8 = 8;
    pub(crate) const EMPHASIS: u8 = 9;
    pub(crate) const FOOTNOTE_REF: u8 = 10;
    pub(crate) const HTML: u8 = 11;
    pub(crate) const IMAGE: u8 = 12;
    pub(crate) const IMAGE_REF: u8 = 13;
    pub(crate) const MDX_JSX_TEXT_ELEMENT: u8 = 14;
    pub(crate) const LINK: u8 = 15;
    pub(crate) const LINK_REF: u8 = 16;
    pub(crate) const WIKI_LINK: u8 = 17;
    pub(crate) const CALLOUT: u8 = 18;
    pub(crate) const EMBED: u8 = 19;
    pub(crate) const STRONG: u8 = 20;
    pub(crate) const CODE: u8 = 21;
    pub(crate) const MATH: u8 = 22;
    pub(crate) const HEADING: u8 = 23;
    pub(crate) const TABLE: u8 = 24;
    pub(crate) const TABLE_ALIGN: u8 = 25;
    pub(crate) const TEXT: u8 = 26;
    pub(crate) const HORIZONTAL_RULE: u8 = 27;
    pub(crate) const DEFINITION: u8 = 28;
    pub(crate) const MDX_FLOW_EXPRESSION: u8 = 29;
    pub(crate) const MDX_TEXT_EXPRESSION: u8 = 30;
    pub(crate) const MDX_JS_ESM: u8 = 31;
    pub(crate) const MDX_JSX_FLOW_ELEMENT: u8 = 32;
    pub(crate) const RECURSIVE: u8 = 33;
    pub(crate) const TASK: u8 = 34;
    pub(crate) const TODO: u8 = 35;
    pub(crate) const DONE: u8 = 36;
    pub(crate) const ATTR: u8 = 37;
    pub(crate) const PROPERTY: u8 = 38;
}

/// Opcode wire tags.
pub(super) mod opcode {
    pub(crate) const CONST: u8 = 1;
    pub(crate) const PUSH_NONE: u8 = 2;
    pub(crate) const GET_LOCAL: u8 = 3;
    pub(crate) const SET_LOCAL: u8 = 4;
    pub(crate) const SET_LOCAL_AND_COPY: u8 = 5;
    pub(crate) const SET_LOCAL_AND_COPY_AND_JUMP: u8 = 6;
    pub(crate) const SET_LOCAL_CONST: u8 = 7;
    pub(crate) const TEE_LOCAL: u8 = 8;
    pub(crate) const COPY_LOCAL: u8 = 9;
    pub(crate) const GET_UPVALUE: u8 = 10;
    pub(crate) const SET_UPVALUE: u8 = 11;
    pub(crate) const MAKE_CLOSURE: u8 = 12;
    pub(crate) const MAKE_STATIC_CLOSURE: u8 = 13;
    pub(crate) const POP: u8 = 14;
    pub(crate) const DUP: u8 = 15;
    pub(crate) const JUMP: u8 = 16;
    pub(crate) const JUMP_IF_FALSE: u8 = 17;
    pub(crate) const ADD: u8 = 18;
    pub(crate) const SUB: u8 = 19;
    pub(crate) const MUL: u8 = 20;
    pub(crate) const DIV: u8 = 21;
    pub(crate) const MOD: u8 = 22;
    pub(crate) const EQ: u8 = 23;
    pub(crate) const NE: u8 = 24;
    pub(crate) const LT: u8 = 25;
    pub(crate) const LE: u8 = 26;
    pub(crate) const GT: u8 = 27;
    pub(crate) const GE: u8 = 28;
    pub(crate) const BINARY_LOCAL_LOCAL: u8 = 29;
    pub(crate) const BINARY_LOCAL_CONST: u8 = 30;
    pub(crate) const BINARY_LOCAL_NUMBER_CONST: u8 = 31;
    pub(crate) const UPDATE_LOCAL_CONST: u8 = 32;
    pub(crate) const UPDATE_LOCAL_NUMBER_CONST: u8 = 33;
    pub(crate) const UPDATE_LOCAL_LOCAL: u8 = 34;
    pub(crate) const JUMP_IF_FALSE_LOCAL_LOCAL: u8 = 35;
    pub(crate) const JUMP_IF_FALSE_LOCAL_CONST: u8 = 36;
    pub(crate) const JUMP_IF_FALSE_LOCAL_NUMBER_CONST: u8 = 37;
    pub(crate) const NEG: u8 = 38;
    pub(crate) const NOT: u8 = 39;
    pub(crate) const ARRAY_NEW: u8 = 40;
    pub(crate) const ARRAY_NEW_WITH_CAPACITY_LOCAL: u8 = 41;
    pub(crate) const ARRAY_PUSH: u8 = 42;
    pub(crate) const ARRAY_SPREAD: u8 = 43;
    pub(crate) const DICT_NEW: u8 = 44;
    pub(crate) const DICT_INSERT: u8 = 45;
    pub(crate) const DICT_SPREAD: u8 = 46;
    pub(crate) const TO_FOREACH_ITERABLE: u8 = 47;
    pub(crate) const ARRAY_LEN: u8 = 48;
    pub(crate) const ARRAY_GET_AT: u8 = 49;
    pub(crate) const ARRAY_LEN_LOCAL: u8 = 50;
    pub(crate) const ARRAY_GET_LOCAL_AT: u8 = 51;
    pub(crate) const FOREACH_NEXT: u8 = 52;
    pub(crate) const FOREACH_COLLECT: u8 = 53;
    pub(crate) const FOREACH_COLLECT_AND_JUMP: u8 = 54;
    pub(crate) const FOREACH_BINARY_LOCAL_NUMBER_CONST_AND_JUMP: u8 = 55;
    pub(crate) const ARRAY_SLICE_FROM: u8 = 56;
    pub(crate) const DICT_GET_LOCAL_OR_FAIL: u8 = 57;
    pub(crate) const TYPE_CHECK: u8 = 58;
    pub(crate) const GET_ENV_VAR: u8 = 59;
    pub(crate) const GET_EXTERNAL_GLOBAL: u8 = 60;
    pub(crate) const INTERP_STRING: u8 = 61;
    pub(crate) const SELECTOR_MATCH: u8 = 62;
    pub(crate) const SELECTOR_MATCH_KIND: u8 = 63;
    pub(crate) const SELECTOR_MATCH_HEADING: u8 = 64;
    pub(crate) const SELECTOR_MATCH_WITH_ARGS: u8 = 65;
    pub(crate) const CALL_BUILTIN_LOCAL: u8 = 66;
    pub(crate) const CALL_BUILTIN: u8 = 67;
    pub(crate) const CALL_STATIC: u8 = 68;
    pub(crate) const CALL_STATIC_EXACT: u8 = 69;
    pub(crate) const CALL_STATIC_EXACT0: u8 = 70;
    pub(crate) const CALL_STATIC_EXACT1: u8 = 71;
    pub(crate) const CALL_STATIC_EXACT2: u8 = 72;
    pub(crate) const CALL_STATIC_IMPLICIT_SELF: u8 = 73;
    pub(crate) const CALL_SELF: u8 = 74;
    pub(crate) const CALL_SELF_EXACT: u8 = 75;
    pub(crate) const CALL_SELF_EXACT0: u8 = 76;
    pub(crate) const CALL_SELF_EXACT1: u8 = 77;
    pub(crate) const CALL_SELF_EXACT2: u8 = 78;
    pub(crate) const CALL_SELF_IMPLICIT_SELF: u8 = 79;
    pub(crate) const CALL_LOCAL: u8 = 80;
    pub(crate) const CALL_UPVALUE: u8 = 81;
    pub(crate) const CALL_UPVALUE_LOCAL: u8 = 82;
    pub(crate) const CALL_VALUE: u8 = 83;
    pub(crate) const MAYBE_AUTO_CALL: u8 = 84;
    pub(crate) const TRY_CATCH: u8 = 85;
    pub(crate) const FLOW_BREAK: u8 = 86;
    pub(crate) const FLOW_CONTINUE: u8 = 87;
    pub(crate) const RAISE_DESTRUCTURING_FAILED: u8 = 88;
    pub(crate) const RETURN_LOCAL: u8 = 89;
    pub(crate) const RETURN_BINARY_LOCAL_LOCAL: u8 = 90;
    pub(crate) const RETURN_BINARY_LOCAL_CONST: u8 = 91;
    pub(crate) const RETURN_BINARY_LOCAL_NUMBER_CONST: u8 = 92;
    pub(crate) const RETURN: u8 = 93;
    pub(crate) const YIELD: u8 = 94;
    pub(crate) const RESUME: u8 = 95;
}
