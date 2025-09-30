/**
 * @file Tree-sitter grammar for mq language
 * @author harehare
 * @license MIT
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

module.exports = grammar({
  name: "mq",

  extras: ($) => [/\s/, $.comment],

  word: ($) => $.identifier,

  conflicts: ($) => [[$._expression, $._primary_expression]],

  rules: {
    source_file: ($) => repeat($._statement),

    _statement: ($) =>
      choice(
        $.def_statement,
        $.let_statement,
        $.if_statement,
        $.foreach_statement,
        $.while_statement,
        $.until_statement,
        $.break_statement,
        $.continue_statement,
        $.include_statement,
        $.block_statement,
        $._expression
      ),

    // Comments
    comment: (_) => token(seq("#", /.*/)),

    // Definition statement
    def_statement: ($) =>
      seq(
        "def",
        field("name", $.identifier),
        optional(field("parameters", $.parameter_list)),
        ":",
        repeat($._statement),
        "end"
      ),

    parameter_list: ($) =>
      seq(
        "(",
        optional(seq($.identifier, repeat(seq(",", $.identifier)))),
        ")"
      ),

    // Let statement
    let_statement: ($) =>
      seq(
        "let",
        field("name", $.identifier),
        "=",
        field("value", $._expression)
      ),

    // Include statement
    include_statement: ($) => seq("include", field("path", $.string)),

    // If statement
    if_statement: ($) =>
      seq(
        "if",
        field("condition", $._expression),
        ":",
        repeat($._statement),
        repeat($.elif_clause),
        optional($.else_clause),
        "end"
      ),

    elif_clause: ($) =>
      seq("elif", field("condition", $._expression), ":", repeat($._statement)),

    else_clause: ($) => seq("else", ":", repeat($._statement)),

    // Loop statements
    foreach_statement: ($) =>
      seq(
        "foreach",
        "(",
        field("variable", $.identifier),
        ",",
        field("iterable", $._expression),
        ")",
        ":",
        repeat($._statement),
        choice("end", ";")
      ),

    while_statement: ($) =>
      seq(
        "while",
        field("condition", $._expression),
        ":",
        repeat($._statement),
        "end"
      ),

    until_statement: ($) =>
      seq(
        "until",
        field("condition", $._expression),
        ":",
        repeat($._statement),
        "end"
      ),

    block_statement: ($) =>
      seq(
        "do",
        repeat($._statement),
        "end"
      ),

    // Control flow
    break_statement: (_) => "break",
    continue_statement: (_) => "continue",

    // Expressions
    _expression: ($) =>
      choice(
        $.pipe_expression,
        $.binary_expression,
        $.unary_expression,
        $.selector_expression,
        $.call_expression,
        $.array,
        $.dict,
        $.group_expression,
        $.function_expression,
        $.interpolated_string,
        $.identifier,
        $.self,
        $.nodes,
        $._literal
      ),

    // Pipe expression (lowest precedence to allow multi-line pipes)
    pipe_expression: ($) =>
      prec.left(
        1,
        seq($._primary_expression, repeat1(seq("|", $._primary_expression)))
      ),

    _primary_expression: ($) =>
      choice(
        $.binary_expression,
        $.unary_expression,
        $.selector_expression,
        $.call_expression,
        $.array,
        $.dict,
        $.group_expression,
        $.function_expression,
        $.interpolated_string,
        $.identifier,
        $.self,
        $._literal
      ),

    // Binary expressions
    binary_expression: ($) =>
      choice(
        prec.left(10, seq($._primary_expression, "+", $._primary_expression)),
        prec.left(10, seq($._primary_expression, "-", $._primary_expression)),
        prec.left(11, seq($._primary_expression, "*", $._primary_expression)),
        prec.left(11, seq($._primary_expression, "/", $._primary_expression)),
        prec.left(11, seq($._primary_expression, "%", $._primary_expression)),
        prec.left(5, seq($._primary_expression, "==", $._primary_expression)),
        prec.left(5, seq($._primary_expression, "!=", $._primary_expression)),
        prec.left(6, seq($._primary_expression, "<", $._primary_expression)),
        prec.left(6, seq($._primary_expression, "<=", $._primary_expression)),
        prec.left(6, seq($._primary_expression, ">", $._primary_expression)),
        prec.left(6, seq($._primary_expression, ">=", $._primary_expression)),
        prec.left(3, seq($._primary_expression, "&&", $._primary_expression)),
        prec.left(2, seq($._primary_expression, "||", $._primary_expression)),
        prec.left(7, seq($._primary_expression, "..", $._primary_expression))
      ),

    // Unary expressions
    unary_expression: ($) =>
      choice(
        prec(12, seq("!", $._primary_expression)),
        prec(12, seq("-", $._primary_expression))
      ),

    // Call expression
    call_expression: ($) =>
      prec(
        13,
        seq(
          field("function", $.identifier),
          field("arguments", $.argument_list)
        )
      ),

    argument_list: ($) =>
      seq(
        "(",
        optional(
          seq($._primary_expression, repeat(seq(",", $._primary_expression)))
        ),
        ")"
      ),

    // Selector expression
    selector_expression: ($) =>
      prec.left(
        15,
        choice(
          // Dot-prefixed selectors like .property, .property.property2, .[], .[][]
          seq(
            ".",
            choice(
              field("property", $.identifier),
              seq("[", optional(field("index", $._primary_expression)), "]")
            ),
            repeat(
              choice(
                seq(".", field("property", $.identifier)),
                seq("[", optional(field("index", $._primary_expression)), "]")
              )
            )
          ),
          // Base expression with selectors
          seq(
            field("base", $._primary_expression),
            repeat1(
              choice(
                seq(".", field("property", $.identifier)),
                seq("[", optional(field("index", $._primary_expression)), "]")
              )
            )
          )
        )
      ),

    // Function expression
    function_expression: ($) =>
      seq(
        "fn",
        optional(field("parameters", $.parameter_list)),
        ":",
        field("body", $._primary_expression),
        ";"
      ),

    // Group expression
    group_expression: ($) => seq("(", $._primary_expression, ")"),

    // Array
    array: ($) =>
      seq(
        "[",
        optional(
          seq(
            $._primary_expression,
            repeat(seq(",", $._primary_expression)),
            optional(",")
          )
        ),
        "]"
      ),

    // Dictionary
    dict: ($) =>
      seq(
        "{",
        optional(
          seq($.dict_entry, repeat(seq(",", $.dict_entry)), optional(","))
        ),
        "}"
      ),

    dict_entry: ($) =>
      seq(
        field("key", choice($.identifier, $.string)),
        ":",
        field("value", $._primary_expression)
      ),

    // Interpolated string
    interpolated_string: ($) =>
      seq('"', repeat(choice($.string_content, $.interpolation)), '"'),

    string_content: (_) => token.immediate(prec(1, /[^"\\$]+/)),

    interpolation: ($) => seq("\\(", $._primary_expression, ")"),

    // Special identifiers
    self: (_) => "self",
    nodes: (_) => "nodes",

    // Literals
    _literal: ($) => choice($.number, $.string, $.boolean, $.none),

    identifier: (_) => /[a-zA-Z_][a-zA-Z0-9_]*/,

    number: (_) => {
      const decimal = /[0-9]+/;
      const float = /[0-9]+\.[0-9]+/;
      return token(choice(float, decimal));
    },

    string: (_) =>
      token(seq('"', repeat(choice(/[^"\\]/, seq("\\", /./))), '"')),

    boolean: (_) => choice("true", "false"),

    none: (_) => "None",
  },
});
