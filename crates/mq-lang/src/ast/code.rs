use crate::Program;

use super::node::{AccessTarget, Args, Expr, Literal, Node, Params, Pattern, StringSegment};
use std::fmt::Write;

impl Node {
    /// Converts the AST node back to mq code code.
    ///
    /// This method reconstructs the original code code representation from the AST.
    /// The output is valid, executable mq code that, when parsed, should produce
    /// an equivalent AST structure.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Parse mq code code to AST
    /// let code = "def test(x): x + 1";
    /// // ... parse to AST nodes ...
    /// // Convert AST back to code
    /// let generated = node.to_code();
    /// // generated will be "def test(x): x + 1"
    /// ```
    pub fn to_code(&self) -> String {
        let mut output = String::new();
        self.format_to_code(&mut output, 0);
        output
    }

    fn format_to_code(&self, buf: &mut String, indent: usize) {
        match &*self.expr {
            Expr::Literal(lit) => {
                format_literal(lit, buf);
            }
            Expr::Ident(ident) => {
                write!(buf, "{}", ident).unwrap();
            }
            Expr::Self_ => {
                buf.push_str("self");
            }
            Expr::Nodes => {
                buf.push_str("nodes");
            }
            Expr::Selector(selector) => {
                write!(buf, "{}", selector).unwrap();
            }
            Expr::Break => {
                buf.push_str("break");
            }
            Expr::Continue => {
                buf.push_str("continue");
            }
            Expr::Paren(node) => {
                buf.push('(');
                node.format_to_code(buf, indent);
                buf.push(')');
            }
            Expr::And(left, right) => {
                left.format_to_code(buf, indent);
                buf.push_str(" && ");
                right.format_to_code(buf, indent);
            }
            Expr::Or(left, right) => {
                left.format_to_code(buf, indent);
                buf.push_str(" || ");
                right.format_to_code(buf, indent);
            }
            Expr::Call(func, args) => {
                write!(buf, "{}", func).unwrap();
                buf.push('(');
                format_args(args, buf, indent);
                buf.push(')');
            }
            Expr::CallDynamic(func, args) => {
                func.format_to_code(buf, indent);
                buf.push('(');
                format_args(args, buf, indent);
                buf.push(')');
            }
            Expr::Let(ident, value) => {
                write!(buf, "let {} = ", ident).unwrap();
                value.format_to_code(buf, indent);
            }
            Expr::Var(ident, value) => {
                write!(buf, "var {} = ", ident).unwrap();
                value.format_to_code(buf, indent);
            }
            Expr::Assign(ident, value) => {
                write!(buf, "{} = ", ident).unwrap();
                value.format_to_code(buf, indent);
            }
            Expr::If(branches) => {
                for (i, (cond_opt, body)) in branches.iter().enumerate() {
                    if i == 0 {
                        buf.push_str("if ");
                        if let Some(cond) = cond_opt {
                            buf.push('(');
                            cond.format_to_code(buf, indent);
                            buf.push(')');
                        }
                        buf.push_str(": ");
                        body.format_to_code(buf, indent);
                    } else if let Some(cond) = cond_opt {
                        buf.push_str(" elif (");
                        cond.format_to_code(buf, indent);
                        buf.push_str("): ");
                        body.format_to_code(buf, indent);
                    } else {
                        buf.push_str(" else: ");
                        body.format_to_code(buf, indent);
                    }
                }
            }
            Expr::While(cond, program) => {
                buf.push_str("while (");
                cond.format_to_code(buf, indent);
                buf.push(')');
                if needs_block_syntax(program) {
                    format_program_block(program, buf, indent);
                } else if let Some(stmt) = program.first() {
                    buf.push_str(": ");
                    stmt.format_to_code(buf, indent);
                }
            }
            Expr::Loop(program) => {
                buf.push_str("loop");
                format_program_block(program, buf, indent);
            }
            Expr::Foreach(item, iter, program) => {
                write!(buf, "foreach({}, ", item).unwrap();
                iter.format_to_code(buf, indent);
                buf.push(')');
                if needs_block_syntax(program) {
                    format_program_block(program, buf, indent);
                } else if let Some(stmt) = program.first() {
                    buf.push_str(": ");
                    stmt.format_to_code(buf, indent);
                }
            }
            Expr::Block(program) => {
                format_program_inline(program, buf, indent);
            }
            Expr::Def(name, params, program) => {
                write!(buf, "def {}(", name).unwrap();
                format_params(params, buf);
                buf.push(')');
                if needs_block_syntax(program) {
                    format_program_block(program, buf, indent);
                } else if let Some(stmt) = program.first() {
                    buf.push_str(": ");
                    stmt.format_to_code(buf, indent);
                }
            }
            Expr::Fn(params, program) => {
                buf.push_str("fn(");
                format_params(params, buf);
                buf.push(')');
                if needs_block_syntax(program) {
                    format_program_block(program, buf, indent);
                } else if let Some(stmt) = program.first() {
                    buf.push_str(": ");
                    stmt.format_to_code(buf, indent);
                }
            }
            Expr::Match(value, arms) => {
                buf.push_str("match (");
                value.format_to_code(buf, indent);
                buf.push_str(") do");
                for arm in arms.iter() {
                    buf.push('\n');
                    buf.push_str(&"  ".repeat(indent + 1));
                    buf.push_str("| ");
                    format_pattern(&arm.pattern, buf);
                    if let Some(guard) = &arm.guard {
                        buf.push_str(" if ");
                        guard.format_to_code(buf, indent + 1);
                    }
                    buf.push_str(": ");
                    arm.body.format_to_code(buf, indent + 1);
                }
                buf.push('\n');
                buf.push_str(&"  ".repeat(indent));
                buf.push_str("end");
            }
            Expr::InterpolatedString(segments) => {
                buf.push_str("s\"");
                for segment in segments.iter() {
                    match segment {
                        StringSegment::Text(text) => {
                            buf.push_str(&escape_string(text));
                        }
                        StringSegment::Expr(expr) => {
                            buf.push_str("${");
                            expr.format_to_code(buf, indent);
                            buf.push('}');
                        }
                        StringSegment::Env(name) => {
                            write!(buf, "${{{}}}", name).unwrap();
                        }
                        StringSegment::Self_ => {
                            buf.push_str("${self}");
                        }
                    }
                }
                buf.push('"');
            }
            Expr::Macro(name, params, body) => {
                write!(buf, "macro {}(", name).unwrap();
                format_params(params, buf);
                buf.push_str("): ");
                body.format_to_code(buf, indent);
            }
            Expr::Quote(node) => {
                buf.push_str("quote: ");
                node.format_to_code(buf, indent);
            }
            Expr::Unquote(node) => {
                buf.push_str("unquote(");
                node.format_to_code(buf, indent);
                buf.push(')');
            }
            Expr::Try(try_expr, catch_expr) => {
                buf.push_str("try ");
                try_expr.format_to_code(buf, indent);
                buf.push_str(" catch: ");
                catch_expr.format_to_code(buf, indent);
            }
            Expr::Module(name, program) => {
                write!(buf, "module {}", name).unwrap();
                format_program_block(program, buf, indent);
            }
            Expr::QualifiedAccess(path, target) => {
                for (i, part) in path.iter().enumerate() {
                    if i > 0 {
                        buf.push_str("::");
                    }
                    write!(buf, "{}", part).unwrap();
                }
                match target {
                    AccessTarget::Call(func, args) => {
                        write!(buf, "::{}", func).unwrap();
                        buf.push('(');
                        format_args(args, buf, indent);
                        buf.push(')');
                    }
                    AccessTarget::Ident(ident) => {
                        write!(buf, "::{}", ident).unwrap();
                    }
                }
            }
            Expr::Include(path) => {
                buf.push_str("include ");
                format_literal(path, buf);
            }
            Expr::Import(path) => {
                buf.push_str("import ");
                format_literal(path, buf);
            }
        }
    }
}

/// Escapes special characters in strings for mq code code
fn escape_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => result.push_str("\\\\"),
            '"' => result.push_str("\\\""),
            '\n' => result.push_str("\\n"),
            '\t' => result.push_str("\\t"),
            '\r' => result.push_str("\\r"),
            _ => result.push(ch),
        }
    }
    result
}

/// Formats a Literal as mq code code
fn format_literal(literal: &Literal, buf: &mut String) {
    match literal {
        Literal::String(s) => {
            buf.push('"');
            buf.push_str(&escape_string(s));
            buf.push('"');
        }
        Literal::Number(n) => {
            write!(buf, "{}", n).unwrap();
        }
        Literal::Symbol(ident) => {
            write!(buf, ":{}", ident).unwrap();
        }
        Literal::Bool(b) => {
            buf.push_str(if *b { "true" } else { "false" });
        }
        Literal::None => {
            buf.push_str("none");
        }
    }
}

/// Formats function arguments as comma-separated list
fn format_args(args: &Args, buf: &mut String, indent: usize) {
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            buf.push_str(", ");
        }
        arg.format_to_code(buf, indent);
    }
}

/// Formats function parameters as comma-separated list
fn format_params(params: &Params, buf: &mut String) {
    for (i, param) in params.iter().enumerate() {
        if i > 0 {
            buf.push_str(", ");
        }

        write!(buf, "{}", param).unwrap();
    }
}

/// Formats a program (list of statements) in inline syntax using pipes
fn format_program_inline(program: &Program, buf: &mut String, indent: usize) {
    for (i, stmt) in program.iter().enumerate() {
        if i > 0 {
            buf.push_str(" | ");
        }
        stmt.format_to_code(buf, indent);
    }
}

/// Formats a program in block syntax with do...end
fn format_program_block(program: &Program, buf: &mut String, indent: usize) {
    buf.push_str(" do");
    for stmt in program.iter() {
        buf.push('\n');
        buf.push_str(&"  ".repeat(indent + 1));
        stmt.format_to_code(buf, indent + 1);
    }
    buf.push('\n');
    buf.push_str(&"  ".repeat(indent));
    buf.push_str("end");
}

/// Formats a pattern for pattern matching
fn format_pattern(pattern: &Pattern, buf: &mut String) {
    match pattern {
        Pattern::Literal(lit) => {
            format_literal(lit, buf);
        }
        Pattern::Ident(ident) => {
            write!(buf, "{}", ident).unwrap();
        }
        Pattern::Wildcard => {
            buf.push('_');
        }
        Pattern::Array(patterns) => {
            buf.push('[');
            for (i, p) in patterns.iter().enumerate() {
                if i > 0 {
                    buf.push_str(", ");
                }
                format_pattern(p, buf);
            }
            buf.push(']');
        }
        Pattern::ArrayRest(patterns, rest) => {
            buf.push('[');
            for (i, p) in patterns.iter().enumerate() {
                if i > 0 {
                    buf.push_str(", ");
                }
                format_pattern(p, buf);
            }
            if !patterns.is_empty() {
                buf.push_str(", ");
            }
            write!(buf, "..{}", rest).unwrap();
            buf.push(']');
        }
        Pattern::Dict(entries) => {
            buf.push('{');
            for (i, (key, value)) in entries.iter().enumerate() {
                if i > 0 {
                    buf.push_str(", ");
                }
                write!(buf, "{}: ", key).unwrap();
                format_pattern(value, buf);
            }
            buf.push('}');
        }
        Pattern::Type(type_name) => {
            write!(buf, ":{}", type_name).unwrap();
        }
    }
}

/// Determines if a program needs block syntax (do...end) or can use inline syntax
fn needs_block_syntax(program: &Program) -> bool {
    program.len() > 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Ident, IdentWithToken, Shared, arena::ArenaId, ast::node::MatchArm, number::Number};
    use rstest::rstest;
    use smallvec::smallvec;

    // Helper function to create a Node from an Expr
    fn create_node(expr: Expr) -> Node {
        Node {
            token_id: ArenaId::new(0),
            expr: Shared::new(expr),
        }
    }

    #[rstest]
    #[case::string(Literal::String("hello".to_string()), r#""hello""#)]
    #[case::string_with_quote(Literal::String(r#"hello"world"#.to_string()), r#""hello\"world""#)]
    #[case::string_with_backslash(Literal::String(r"hello\world".to_string()), r#""hello\\world""#)]
    #[case::string_with_newline(Literal::String("hello\nworld".to_string()), r#""hello\nworld""#)]
    #[case::string_with_tab(Literal::String("hello\tworld".to_string()), r#""hello\tworld""#)]
    #[case::number_int(Literal::Number(Number::new(42.0)), "42")]
    #[case::number_float(Literal::Number(Number::new(42.5)), "42.5")]
    #[case::symbol(Literal::Symbol(Ident::new("test")), ":test")]
    #[case::bool_true(Literal::Bool(true), "true")]
    #[case::bool_false(Literal::Bool(false), "false")]
    #[case::none(Literal::None, "none")]
    fn test_to_code_literals(#[case] literal: Literal, #[case] expected: &str) {
        let node = create_node(Expr::Literal(literal));
        assert_eq!(node.to_code(), expected);
    }

    #[rstest]
    #[case::ident(Expr::Ident(IdentWithToken::new("foo")), "foo")]
    #[case::self_(Expr::Self_, "self")]
    #[case::nodes(Expr::Nodes, "nodes")]
    #[case::break_(Expr::Break, "break")]
    #[case::continue_(Expr::Continue, "continue")]
    fn test_to_code_simple_expressions(#[case] expr: Expr, #[case] expected: &str) {
        let node = create_node(expr);
        assert_eq!(node.to_code(), expected);
    }

    #[rstest]
    #[case::simple(Expr::Literal(Literal::Number(Number::new(42.0))), "(42)")]
    #[case::ident(Expr::Ident(IdentWithToken::new("x")), "(x)")]
    fn test_to_code_paren(#[case] inner_expr: Expr, #[case] expected: &str) {
        let inner_node = Shared::new(create_node(inner_expr));
        let node = create_node(Expr::Paren(inner_node));
        assert_eq!(node.to_code(), expected);
    }

    #[rstest]
    #[case::and(
        Expr::And(
            Shared::new(create_node(Expr::Ident(IdentWithToken::new("x")))),
            Shared::new(create_node(Expr::Ident(IdentWithToken::new("y"))))
        ),
        "x && y"
    )]
    #[case::or(
        Expr::Or(
            Shared::new(create_node(Expr::Ident(IdentWithToken::new("a")))),
            Shared::new(create_node(Expr::Ident(IdentWithToken::new("b"))))
        ),
        "a || b"
    )]
    fn test_to_code_operators(#[case] expr: Expr, #[case] expected: &str) {
        let node = create_node(expr);
        assert_eq!(node.to_code(), expected);
    }

    #[rstest]
    #[case::no_args(
        Expr::Call(IdentWithToken::new("test"), smallvec![]),
        "test()"
    )]
    #[case::one_arg(
        Expr::Call(
            IdentWithToken::new("add"),
            smallvec![Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(1.0)))))]
        ),
        "add(1)"
    )]
    #[case::two_args(
        Expr::Call(
            IdentWithToken::new("add"),
            smallvec![
                Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(1.0))))),
                Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(2.0)))))
            ]
        ),
        "add(1, 2)"
    )]
    fn test_to_code_call(#[case] expr: Expr, #[case] expected: &str) {
        let node = create_node(expr);
        assert_eq!(node.to_code(), expected);
    }

    #[rstest]
    #[case::simple(
        Expr::CallDynamic(
            Shared::new(create_node(Expr::Ident(IdentWithToken::new("func")))),
            smallvec![Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(42.0)))))]
        ),
        "func(42)"
    )]
    fn test_to_code_call_dynamic(#[case] expr: Expr, #[case] expected: &str) {
        let node = create_node(expr);
        assert_eq!(node.to_code(), expected);
    }

    #[rstest]
    #[case::let_simple(
        Expr::Let(
            IdentWithToken::new("x"),
            Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(42.0)))))
        ),
        "let x = 42"
    )]
    #[case::var_simple(
        Expr::Var(
            IdentWithToken::new("y"),
            Shared::new(create_node(Expr::Literal(Literal::String("hello".to_string()))))
        ),
        r#"var y = "hello""#
    )]
    #[case::assign_simple(
        Expr::Assign(
            IdentWithToken::new("z"),
            Shared::new(create_node(Expr::Ident(IdentWithToken::new("value"))))
        ),
        "z = value"
    )]
    fn test_to_code_variables(#[case] expr: Expr, #[case] expected: &str) {
        let node = create_node(expr);
        assert_eq!(node.to_code(), expected);
    }

    #[rstest]
    #[case::if_simple(
        Expr::If(smallvec![
            (
                Some(Shared::new(create_node(Expr::Ident(IdentWithToken::new("x"))))),
                Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(1.0)))))
            )
        ]),
        "if (x): 1"
    )]
    #[case::if_else(
        Expr::If(smallvec![
            (
                Some(Shared::new(create_node(Expr::Ident(IdentWithToken::new("x"))))),
                Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(1.0)))))
            ),
            (
                None,
                Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(2.0)))))
            )
        ]),
        "if (x): 1 else: 2"
    )]
    #[case::if_elif_else(
        Expr::If(smallvec![
            (
                Some(Shared::new(create_node(Expr::Ident(IdentWithToken::new("x"))))),
                Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(1.0)))))
            ),
            (
                Some(Shared::new(create_node(Expr::Ident(IdentWithToken::new("y"))))),
                Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(2.0)))))
            ),
            (
                None,
                Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(3.0)))))
            )
        ]),
        "if (x): 1 elif (y): 2 else: 3"
    )]
    fn test_to_code_if(#[case] expr: Expr, #[case] expected: &str) {
        let node = create_node(expr);
        assert_eq!(node.to_code(), expected);
    }

    #[rstest]
    #[case::while_inline(
        Expr::While(
            Shared::new(create_node(Expr::Ident(IdentWithToken::new("x")))),
            vec![Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(1.0)))))]
        ),
        "while (x): 1"
    )]
    #[case::while_block(
        Expr::While(
            Shared::new(create_node(Expr::Ident(IdentWithToken::new("x")))),
            vec![
                Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(1.0))))),
                Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(2.0)))))
            ]
        ),
        "while (x) do\n  1\n  2\nend"
    )]
    fn test_to_code_while(#[case] expr: Expr, #[case] expected: &str) {
        let node = create_node(expr);
        assert_eq!(node.to_code(), expected);
    }

    #[rstest]
    #[case::loop_single(
        Expr::Loop(vec![Shared::new(create_node(Expr::Break))]),
        "loop do\n  break\nend"
    )]
    fn test_to_code_loop(#[case] expr: Expr, #[case] expected: &str) {
        let node = create_node(expr);
        assert_eq!(node.to_code(), expected);
    }

    #[rstest]
    #[case::foreach_inline(
        Expr::Foreach(
            IdentWithToken::new("item"),
            Shared::new(create_node(Expr::Ident(IdentWithToken::new("items")))),
            vec![Shared::new(create_node(Expr::Ident(IdentWithToken::new("item"))))]
        ),
        "foreach(item, items): item"
    )]
    #[case::foreach_block(
        Expr::Foreach(
            IdentWithToken::new("x"),
            Shared::new(create_node(Expr::Ident(IdentWithToken::new("arr")))),
            vec![
                Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(1.0))))),
                Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(2.0)))))
            ]
        ),
        "foreach(x, arr) do\n  1\n  2\nend"
    )]
    fn test_to_code_foreach(#[case] expr: Expr, #[case] expected: &str) {
        let node = create_node(expr);
        assert_eq!(node.to_code(), expected);
    }

    #[rstest]
    #[case::single(
        Expr::Block(vec![Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(1.0)))))]),
        "1"
    )]
    #[case::multiple(
        Expr::Block(vec![
            Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(1.0))))),
            Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(2.0))))),
            Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(3.0)))))
        ]),
        "1 | 2 | 3"
    )]
    fn test_to_code_block(#[case] expr: Expr, #[case] expected: &str) {
        let node = create_node(expr);
        assert_eq!(node.to_code(), expected);
    }

    #[rstest]
    #[case::no_params_inline(
        Expr::Def(
            IdentWithToken::new("test"),
            smallvec![],
            vec![Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(42.0)))))]
        ),
        "def test(): 42"
    )]
    #[case::with_params_inline(
        Expr::Def(
            IdentWithToken::new("add"),
            smallvec![
                Shared::new(create_node(Expr::Ident(IdentWithToken::new("x")))),
                Shared::new(create_node(Expr::Ident(IdentWithToken::new("y"))))
            ],
            vec![Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(1.0)))))]
        ),
        "def add(x, y): 1"
    )]
    #[case::block(
        Expr::Def(
            IdentWithToken::new("test"),
            smallvec![Shared::new(create_node(Expr::Ident(IdentWithToken::new("x"))))],
            vec![
                Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(1.0))))),
                Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(2.0)))))
            ]
        ),
        "def test(x) do\n  1\n  2\nend"
    )]
    fn test_to_code_def(#[case] expr: Expr, #[case] expected: &str) {
        let node = create_node(expr);
        assert_eq!(node.to_code(), expected);
    }

    #[rstest]
    #[case::no_params_inline(
        Expr::Fn(
            smallvec![],
            vec![Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(42.0)))))]
        ),
        "fn(): 42"
    )]
    #[case::with_params_inline(
        Expr::Fn(
            smallvec![
                Shared::new(create_node(Expr::Ident(IdentWithToken::new("x")))),
                Shared::new(create_node(Expr::Ident(IdentWithToken::new("y"))))
            ],
            vec![Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(1.0)))))]
        ),
        "fn(x, y): 1"
    )]
    fn test_to_code_fn(#[case] expr: Expr, #[case] expected: &str) {
        let node = create_node(expr);
        assert_eq!(node.to_code(), expected);
    }

    #[rstest]
    #[case::simple(
        Expr::Match(
            Shared::new(create_node(Expr::Ident(IdentWithToken::new("x")))),
            smallvec![
                MatchArm {
                    pattern: Pattern::Literal(Literal::Number(Number::new(1.0))),
                    guard: None,
                    body: Shared::new(create_node(Expr::Literal(Literal::String("one".to_string()))))
                },
                MatchArm {
                    pattern: Pattern::Wildcard,
                    guard: None,
                    body: Shared::new(create_node(Expr::Literal(Literal::String("other".to_string()))))
                }
            ]
        ),
        "match (x) do\n  | 1: \"one\"\n  | _: \"other\"\nend"
    )]
    fn test_to_code_match(#[case] expr: Expr, #[case] expected: &str) {
        let node = create_node(expr);
        assert_eq!(node.to_code(), expected);
    }

    #[rstest]
    #[case::text_only(
        Expr::InterpolatedString(vec![StringSegment::Text("hello".to_string())]),
        r#"s"hello""#
    )]
    #[case::with_expr(
        Expr::InterpolatedString(vec![
            StringSegment::Text("Hello ".to_string()),
            StringSegment::Expr(Shared::new(create_node(Expr::Ident(IdentWithToken::new("name"))))),
            StringSegment::Text("!".to_string())
        ]),
        r#"s"Hello ${name}!""#
    )]
    fn test_to_code_interpolated_string(#[case] expr: Expr, #[case] expected: &str) {
        let node = create_node(expr);
        assert_eq!(node.to_code(), expected);
    }

    #[rstest]
    #[case::simple(
        Expr::Macro(
            IdentWithToken::new("double"),
            smallvec![Shared::new(create_node(Expr::Ident(IdentWithToken::new("x"))))],
            Shared::new(create_node(Expr::Ident(IdentWithToken::new("x"))))
        ),
        "macro double(x): x"
    )]
    fn test_to_code_macro(#[case] expr: Expr, #[case] expected: &str) {
        let node = create_node(expr);
        assert_eq!(node.to_code(), expected);
    }

    #[rstest]
    #[case::quote(
        Expr::Quote(Shared::new(create_node(Expr::Ident(IdentWithToken::new("x"))))),
        "quote: x"
    )]
    #[case::unquote(
        Expr::Unquote(Shared::new(create_node(Expr::Ident(IdentWithToken::new("x"))))),
        "unquote(x)"
    )]
    fn test_to_code_quote_unquote(#[case] expr: Expr, #[case] expected: &str) {
        let node = create_node(expr);
        assert_eq!(node.to_code(), expected);
    }

    #[rstest]
    #[case::simple(
        Expr::Try(
            Shared::new(create_node(Expr::Ident(IdentWithToken::new("risky")))),
            Shared::new(create_node(Expr::Literal(Literal::String("error".to_string()))))
        ),
        r#"try risky catch: "error""#
    )]
    fn test_to_code_try(#[case] expr: Expr, #[case] expected: &str) {
        let node = create_node(expr);
        assert_eq!(node.to_code(), expected);
    }

    #[rstest]
    #[case::simple(
        Expr::Module(
            IdentWithToken::new("math"),
            vec![Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(1.0)))))]
        ),
        "module math do\n  1\nend"
    )]
    fn test_to_code_module(#[case] expr: Expr, #[case] expected: &str) {
        let node = create_node(expr);
        assert_eq!(node.to_code(), expected);
    }

    #[rstest]
    #[case::ident(
        Expr::QualifiedAccess(
            vec![IdentWithToken::new("module"), IdentWithToken::new("submodule")],
            AccessTarget::Ident(IdentWithToken::new("func"))
        ),
        "module::submodule::func"
    )]
    #[case::call(
        Expr::QualifiedAccess(
            vec![IdentWithToken::new("module")],
            AccessTarget::Call(
                IdentWithToken::new("func"),
                smallvec![Shared::new(create_node(Expr::Literal(Literal::Number(Number::new(1.0)))))]
            )
        ),
        "module::func(1)"
    )]
    fn test_to_code_qualified_access(#[case] expr: Expr, #[case] expected: &str) {
        let node = create_node(expr);
        assert_eq!(node.to_code(), expected);
    }

    #[rstest]
    #[case::include(
        Expr::Include(Literal::String("file.mq".to_string())),
        r#"include "file.mq""#
    )]
    #[case::import(
        Expr::Import(Literal::String("module.mq".to_string())),
        r#"import "module.mq""#
    )]
    fn test_to_code_include_import(#[case] expr: Expr, #[case] expected: &str) {
        let node = create_node(expr);
        assert_eq!(node.to_code(), expected);
    }

    // Complex expression tests
    #[rstest]
    #[case::nested_call(
        Expr::Call(
            IdentWithToken::new("map"),
            smallvec![
                Shared::new(create_node(Expr::Call(
                    IdentWithToken::new("filter"),
                    smallvec![Shared::new(create_node(Expr::Ident(IdentWithToken::new("items"))))]
                )))
            ]
        ),
        "map(filter(items))"
    )]
    fn test_to_code_complex(#[case] expr: Expr, #[case] expected: &str) {
        let node = create_node(expr);
        assert_eq!(node.to_code(), expected);
    }
}
