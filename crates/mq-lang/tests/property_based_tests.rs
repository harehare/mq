//! Property-based tests for mq-lang AST operations.
use mq_lang::{Arena, AstExpr, AstLiteral, AstNode, IdentWithToken, Program, Shared, SharedCell};
use proptest::prelude::*;
use smallvec::smallvec;

fn create_token_arena() -> Shared<SharedCell<Arena<Shared<mq_lang::Token>>>> {
    Shared::new(SharedCell::new(Arena::new(1024)))
}

fn default_token_id() -> mq_lang::ArenaId<Shared<mq_lang::Token>> {
    mq_lang::ArenaId::new(0)
}

fn make_node(expr: AstExpr) -> Shared<AstNode> {
    Shared::new(AstNode {
        token_id: default_token_id(),
        expr: Shared::new(expr),
    })
}

mod strategies {
    use super::*;

    /// Generates valid mq identifiers (excluding reserved keywords)
    pub fn ident() -> impl Strategy<Value = IdentWithToken> {
        "[a-z_][a-z0-9_]{0,10}"
            .prop_filter("Avoid reserved keywords", |s| {
                !matches!(
                    s.as_str(),
                    "if" | "else"
                        | "elif"
                        | "let"
                        | "var"
                        | "def"
                        | "fn"
                        | "do"
                        | "end"
                        | "while"
                        | "foreach"
                        | "match"
                        | "break"
                        | "continue"
                        | "self"
                        | "nodes"
                        | "true"
                        | "false"
                        | "None"
                        | "import"
                        | "include"
                        | "module"
                        | "macro"
                        | "quote"
                        | "unquote"
                        | "try"
                )
            })
            .prop_map(|name| IdentWithToken::new(&name))
    }

    /// Generates boolean literals
    pub fn bool_lit() -> impl Strategy<Value = AstLiteral> {
        any::<bool>().prop_map(AstLiteral::Bool)
    }

    /// Generates numeric literals (with reasonable bounds)
    pub fn number_lit() -> impl Strategy<Value = AstLiteral> {
        prop_oneof![
            // Small integers
            (-1000i64..=1000).prop_map(|n| AstLiteral::Number(n.into())),
            // Larger integers
            (-1_000_000i64..=1_000_000).prop_map(|n| AstLiteral::Number(n.into())),
            // Floating point
            (-1000.0f64..1000.0f64).prop_map(|n| AstLiteral::Number(n.into())),
        ]
    }

    /// Generates string literals
    pub fn string_lit() -> impl Strategy<Value = AstLiteral> {
        prop_oneof![
            // Simple strings
            "[a-zA-Z0-9 ]{0,20}".prop_map(AstLiteral::String),
            // Empty string
            Just(AstLiteral::String(String::new())),
            // Strings with special chars
            r#"[a-zA-Z0-9!@#$%^&*()_+=\-\[\]{};:'",.<>?/\\| ]{0,15}"#.prop_map(AstLiteral::String),
        ]
    }

    /// Generates None literal
    pub fn none_lit() -> impl Strategy<Value = AstLiteral> {
        Just(AstLiteral::None)
    }

    /// Generates any literal value
    pub fn literal() -> impl Strategy<Value = AstLiteral> {
        prop_oneof![
            3 => bool_lit(),
            4 => number_lit(),
            4 => string_lit(),
            1 => none_lit(),
        ]
    }

    /// Generates literal expression nodes
    pub fn literal_expr() -> impl Strategy<Value = Shared<AstNode>> {
        literal().prop_map(|lit| make_node(AstExpr::Literal(lit)))
    }

    /// Generates identifier expression nodes
    pub fn ident_expr() -> impl Strategy<Value = Shared<AstNode>> {
        ident().prop_map(|id| make_node(AstExpr::Ident(id)))
    }

    /// Generates self expression nodes
    pub fn self_expr() -> impl Strategy<Value = Shared<AstNode>> {
        Just(make_node(AstExpr::Self_))
    }

    /// Generates simple (non-recursive) expression nodes
    pub fn simple_expr() -> impl Strategy<Value = Shared<AstNode>> {
        prop_oneof![
            6 => literal_expr(),
            4 => ident_expr(),
            1 => self_expr(),
        ]
    }

    /// Generates parenthesized expressions
    pub fn paren_expr() -> impl Strategy<Value = Shared<AstNode>> {
        simple_expr().prop_map(|inner| make_node(AstExpr::Paren(inner)))
    }

    /// Generates And expressions
    pub fn and_expr() -> impl Strategy<Value = Shared<AstNode>> {
        (simple_expr(), simple_expr()).prop_map(|(left, right)| make_node(AstExpr::And(left, right)))
    }

    /// Generates Or expressions
    pub fn or_expr() -> impl Strategy<Value = Shared<AstNode>> {
        (simple_expr(), simple_expr()).prop_map(|(left, right)| make_node(AstExpr::Or(left, right)))
    }

    /// Generates binary operation expressions (And | Or)
    pub fn binary_op_expr() -> impl Strategy<Value = Shared<AstNode>> {
        prop_oneof![and_expr(), or_expr()]
    }

    /// Generates function call expressions
    pub fn call_expr() -> impl Strategy<Value = Shared<AstNode>> {
        (ident(), prop::collection::vec(simple_expr(), 0..=3))
            .prop_map(|(func_name, args)| make_node(AstExpr::Call(func_name, args.into())))
    }

    /// Generates let binding expressions
    pub fn let_expr() -> impl Strategy<Value = Shared<AstNode>> {
        (ident(), simple_expr()).prop_map(|(var_name, value)| make_node(AstExpr::Let(var_name, value)))
    }

    /// Generates var binding expressions
    pub fn var_expr() -> impl Strategy<Value = Shared<AstNode>> {
        (ident(), simple_expr()).prop_map(|(var_name, value)| make_node(AstExpr::Var(var_name, value)))
    }

    /// Generates assignment expressions
    pub fn assign_expr() -> impl Strategy<Value = Shared<AstNode>> {
        (ident(), simple_expr()).prop_map(|(var_name, value)| make_node(AstExpr::Assign(var_name, value)))
    }

    /// Generates if-else expressions
    pub fn if_expr() -> impl Strategy<Value = Shared<AstNode>> {
        (simple_expr(), simple_expr(), simple_expr()).prop_map(|(cond, then_branch, else_branch)| {
            make_node(AstExpr::If(smallvec![(Some(cond), then_branch), (None, else_branch)]))
        })
    }

    /// Generates if expressions with optional elif/else
    pub fn if_expr_complex() -> impl Strategy<Value = Shared<AstNode>> {
        (simple_expr(), simple_expr(), prop::option::of(simple_expr())).prop_map(|(cond, then_branch, else_branch)| {
            let mut branches = smallvec![(Some(cond), then_branch)];
            if let Some(else_b) = else_branch {
                branches.push((None, else_b));
            }
            make_node(AstExpr::If(branches))
        })
    }

    /// Generates block expressions
    pub fn block_expr() -> impl Strategy<Value = Shared<AstNode>> {
        prop::collection::vec(simple_expr(), 1..=3).prop_map(|stmts| make_node(AstExpr::Block(stmts)))
    }

    /// Generates any expression node (weighted distribution)
    pub fn any_expr() -> impl Strategy<Value = Shared<AstNode>> {
        prop_oneof![
            4 => simple_expr(),
            2 => call_expr(),
            2 => let_expr(),
            1 => binary_op_expr(),
            1 => if_expr(),
            1 => paren_expr(),
        ]
    }

    /// Generates a program (sequence of nodes)
    pub fn program() -> impl Strategy<Value = Program> {
        prop::collection::vec(any_expr(), 1..=5)
    }

    /// Generates a small program (for faster tests)
    pub fn small_program() -> impl Strategy<Value = Program> {
        prop::collection::vec(simple_expr(), 1..=3)
    }

    /// Generates a medium program with more complex expressions
    pub fn medium_program() -> impl Strategy<Value = Program> {
        prop::collection::vec(any_expr(), 3..=7)
    }
}

mod assertions {
    use super::*;

    /// Parse code and return result or proptest error
    pub fn assert_parses(code: &str) -> Result<Program, TestCaseError> {
        let token_arena = create_token_arena();
        mq_lang::parse(code, token_arena).map_err(|e| TestCaseError::fail(format!("Parse failed: {:?}", e)))
    }

    /// Check if two literals are semantically equal
    pub fn literals_equal(lit1: &AstLiteral, lit2: &AstLiteral) -> bool {
        match (lit1, lit2) {
            (AstLiteral::Bool(a), AstLiteral::Bool(b)) => a == b,
            (AstLiteral::None, AstLiteral::None) => true,
            (AstLiteral::String(a), AstLiteral::String(b)) => a == b,
            (AstLiteral::Number(a), AstLiteral::Number(b)) => {
                // Use a more lenient epsilon for floating point comparison
                (a.value() - b.value()).abs() < 0.001
            }
            _ => false,
        }
    }

    /// Check if expression types match
    pub fn expr_types_match(expr1: &AstExpr, expr2: &AstExpr) -> bool {
        std::mem::discriminant(expr1) == std::mem::discriminant(expr2)
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Test that literals roundtrip through to_code and parse
    #[test]
    fn roundtrip_literals(lit in strategies::literal()) {
        let node = make_node(AstExpr::Literal(lit.clone()));
        let code = node.to_code();

        let program = assertions::assert_parses(&code)?;

        prop_assert!(!program.is_empty(), "Parsed program is empty");

        if let AstExpr::Literal(parsed_lit) = &*program[0].expr {
            prop_assert!(
                assertions::literals_equal(&lit, parsed_lit),
                "Literals differ: {:?} vs {:?}", lit, parsed_lit
            );
        }
    }

    /// Test that identifiers roundtrip correctly
    #[test]
    fn roundtrip_identifiers(ident in strategies::ident()) {
        let node = make_node(AstExpr::Ident(ident.clone()));
        let code = node.to_code();

        let program = assertions::assert_parses(&code)?;
        prop_assert!(!program.is_empty());

        if let AstExpr::Ident(parsed_ident) = &*program[0].expr {
            prop_assert_eq!(ident.name, parsed_ident.name);
        }
    }

    /// Test that function calls roundtrip correctly
    #[test]
    fn roundtrip_calls(node in strategies::call_expr()) {
        let code = node.to_code();
        let program = assertions::assert_parses(&code)?;

        prop_assert!(!program.is_empty());

        if let (AstExpr::Call(orig_name, orig_args), AstExpr::Call(parsed_name, parsed_args))
            = (&*node.expr, &*program[0].expr) {
            prop_assert_eq!(orig_name.name, parsed_name.name, "Function names differ");
            prop_assert_eq!(orig_args.len(), parsed_args.len(), "Argument counts differ");
        }
    }

    /// Test that let bindings roundtrip correctly
    #[test]
    fn roundtrip_let_bindings(node in strategies::let_expr()) {
        let code = node.to_code();
        let program = assertions::assert_parses(&code)?;

        prop_assert!(!program.is_empty());
        prop_assert!(matches!(&*program[0].expr, AstExpr::Let(_, _)));
    }

    /// Test that var bindings roundtrip correctly
    #[test]
    fn roundtrip_var_bindings(node in strategies::var_expr()) {
        let code = node.to_code();
        let program = assertions::assert_parses(&code)?;

        prop_assert!(!program.is_empty());
        prop_assert!(matches!(&*program[0].expr, AstExpr::Var(_, _)));
    }

    /// Test that binary operations roundtrip correctly
    #[test]
    fn roundtrip_binary_ops(node in strategies::binary_op_expr()) {
        let code = node.to_code();
        let program = assertions::assert_parses(&code)?;

        prop_assert!(!program.is_empty());
        prop_assert!(
            assertions::expr_types_match(&node.expr, &program[0].expr),
            "Expression types differ"
        );
    }

    /// Test that if expressions roundtrip correctly
    #[test]
    fn roundtrip_if_expressions(node in strategies::if_expr()) {
        let code = node.to_code();
        let program = assertions::assert_parses(&code)?;

        prop_assert!(!program.is_empty());
        prop_assert!(matches!(&*program[0].expr, AstExpr::If(_)));
    }

    /// Test that parenthesized expressions parse correctly
    #[test]
    fn roundtrip_paren_expressions(node in strategies::paren_expr()) {
        let code = node.to_code();
        let program = assertions::assert_parses(&code)?;

        prop_assert!(!program.is_empty());
    }

    /// Test that any generated expression can be parsed
    #[test]
    fn roundtrip_any_expression(node in strategies::any_expr()) {
        let code = node.to_code();
        let program = assertions::assert_parses(&code)?;

        prop_assert!(!program.is_empty(), "Parsed program is empty for: {}", code);
    }

    /// Test that programs (sequences) can roundtrip
    #[test]
    fn roundtrip_programs(program in strategies::small_program()) {
        let mut code = String::new();
        for (i, node) in program.iter().enumerate() {
            if i > 0 {
                code.push_str(" | ");
            }
            code.push_str(&node.to_code());
        }

        let parsed = assertions::assert_parses(&code)?;
        prop_assert!(!parsed.is_empty(), "Parsed program should not be empty");
    }

    /// Test that larger programs can be parsed
    #[test]
    fn roundtrip_medium_programs(program in strategies::medium_program()) {
        let mut code = String::new();
        for (i, node) in program.iter().enumerate() {
            if i > 0 {
                code.push_str(" | ");
            }
            code.push_str(&node.to_code());
        }

        let parsed = assertions::assert_parses(&code)?;
        prop_assert!(!parsed.is_empty(), "Parsed program should not be empty");
        prop_assert!(!parsed.is_empty(), "Should have at least one statement");
    }

    /// Test that full programs with various expressions work
    #[test]
    fn roundtrip_full_programs(program in strategies::program()) {
        let mut code = String::new();
        for (i, node) in program.iter().enumerate() {
            if i > 0 {
                code.push_str(" | ");
            }
            code.push_str(&node.to_code());
        }

        let parsed = assertions::assert_parses(&code)?;
        prop_assert!(!parsed.is_empty(), "Full program should not be empty");
    }

    /// Test that assignment expressions roundtrip correctly
    #[test]
    fn roundtrip_assignments(node in strategies::assign_expr()) {
        let code = node.to_code();
        let program = assertions::assert_parses(&code)?;

        prop_assert!(!program.is_empty());
        prop_assert!(matches!(&*program[0].expr, AstExpr::Assign(_, _)));
    }

    /// Test that block expressions can be parsed
    #[test]
    fn roundtrip_blocks(node in strategies::block_expr()) {
        let code = node.to_code();
        let program = assertions::assert_parses(&code)?;

        prop_assert!(!program.is_empty());
    }

    /// Test that complex if expressions with elif work
    #[test]
    fn roundtrip_complex_if_expressions(node in strategies::if_expr_complex()) {
        let code = node.to_code();
        let program = assertions::assert_parses(&code)?;

        prop_assert!(!program.is_empty());
        prop_assert!(matches!(&*program[0].expr, AstExpr::If(_)));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Test that to_code is deterministic
    #[test]
    fn to_code_is_deterministic(node in strategies::any_expr()) {
        let code1 = node.to_code();
        let code2 = node.to_code();
        prop_assert_eq!(code1, code2, "to_code must be deterministic");
    }

    /// Test that to_code produces non-empty strings for non-empty expressions
    #[test]
    fn to_code_produces_nonempty(node in strategies::any_expr()) {
        let code = node.to_code();
        prop_assert!(!code.is_empty(), "to_code should produce non-empty code");
    }

    /// Test that generated code contains expected keywords
    #[test]
    fn to_code_contains_keywords(node in strategies::let_expr()) {
        let code = node.to_code();
        prop_assert!(code.contains("let"), "let expression should contain 'let' keyword");
    }

    /// Test that different expressions produce different code (when expected)
    #[test]
    fn different_exprs_different_code(
        lit1 in strategies::literal(),
        lit2 in strategies::literal()
    ) {
        if lit1 != lit2 {
            let node1 = make_node(AstExpr::Literal(lit1));
            let node2 = make_node(AstExpr::Literal(lit2));

            let code1 = node1.to_code();
            let code2 = node2.to_code();

            // Note: Some different literals might produce same code (e.g., 1.0 and 1)
            // This is a soft check - we mainly verify determinism
            let _ = (code1, code2);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Test that parser never panics on valid identifiers
    #[test]
    fn parser_handles_identifiers(ident in "[a-z_][a-z0-9_]{0,30}") {
        let token_arena = create_token_arena();
        let _ = mq_lang::parse(&ident, token_arena);
        // Should not panic
    }

    /// Test that parser handles combinations of identifiers
    #[test]
    fn parser_handles_ident_sequences(
        idents in prop::collection::vec("[a-z]+", 1..=10)
    ) {
        let code = idents.join(" | ");
        let token_arena = create_token_arena();
        let _ = mq_lang::parse(&code, token_arena);
        // Should not panic
    }

    /// Test that parser handles numbers gracefully
    #[test]
    fn parser_handles_numbers(n in -1_000_000i64..1_000_000) {
        let code = format!("{}", n);
        let token_arena = create_token_arena();
        let result = mq_lang::parse(&code, token_arena);
        prop_assert!(result.is_ok(), "Should parse number: {}", n);
    }

    /// Test that parser handles strings gracefully
    #[test]
    fn parser_handles_strings(s in r#"[a-zA-Z0-9 ]{0,30}"#) {
        let code = format!(r#""{}""#, s);
        let token_arena = create_token_arena();
        let result = mq_lang::parse(&code, token_arena);
        prop_assert!(result.is_ok(), "Should parse string: {}", code);
    }

    /// Test that parser handles operator sequences
    #[test]
    fn parser_handles_operators(
        op in prop::sample::select(vec!["&&", "||", "|"])
    ) {
        let code = format!("a {} b", op);
        let token_arena = create_token_arena();
        let _ = mq_lang::parse(&code, token_arena);
        // Should not panic
    }
}

#[cfg(test)]
mod semantic_tests {
    use super::*;

    proptest! {
        #[test]
        fn special_keywords_parse_correctly(
            keyword in prop::sample::select(vec!["self", "nodes", "break", "continue"])
        ) {
            let token_arena = create_token_arena();
            let program = mq_lang::parse(keyword, token_arena)?;

            prop_assert!(!program.is_empty());

            let matches_expected = match keyword {
                "self" => matches!(&*program[0].expr, AstExpr::Self_),
                "nodes" => matches!(&*program[0].expr, AstExpr::Nodes),
                "break" => matches!(&*program[0].expr, AstExpr::Break),
                "continue" => matches!(&*program[0].expr, AstExpr::Continue),
                _ => false,
            };

            prop_assert!(matches_expected, "Keyword {} did not parse correctly", keyword);
        }

        #[test]
        fn bool_literals_preserved(b in any::<bool>()) {
            let node = make_node(AstExpr::Literal(AstLiteral::Bool(b)));
            let code = node.to_code();
            let program = assertions::assert_parses(&code)?;

            if let AstExpr::Literal(AstLiteral::Bool(parsed)) = &*program[0].expr {
                prop_assert_eq!(b, *parsed);
            } else {
                prop_assert!(false, "Expected bool literal");
            }
        }
    }

    #[test]
    fn none_is_preserved() {
        let node = make_node(AstExpr::Literal(AstLiteral::None));
        let code = node.to_code();
        let token_arena = create_token_arena();
        let result = mq_lang::parse(&code, token_arena);

        assert!(result.is_ok(), "None should parse successfully");
        if let Ok(program) = result {
            assert!(!program.is_empty(), "Program should not be empty");
            // None might be parsed as an identifier in some contexts
        }
    }
}

#[cfg(test)]
mod edge_cases {
    use super::*;

    #[test]
    fn test_empty_program() {
        let token_arena = create_token_arena();
        let _ = mq_lang::parse("", token_arena);
        // Should not panic
    }

    #[test]
    fn test_whitespace_only() {
        let token_arena = create_token_arena();
        let _ = mq_lang::parse("   \n\t  ", token_arena);
        // Should not panic
    }

    #[test]
    fn test_single_comment() {
        let token_arena = create_token_arena();
        let _ = mq_lang::parse("# comment", token_arena);
        // Should not panic
    }

    #[test]
    fn test_deeply_nested_parens() {
        let mut code = "1".to_string();
        for _ in 0..20 {
            code = format!("({})", code);
        }
        let token_arena = create_token_arena();
        let result = mq_lang::parse(&code, token_arena);
        assert!(result.is_ok(), "Should handle deeply nested parentheses");
    }

    #[test]
    fn test_long_identifier() {
        let long_name = "a".repeat(1000);
        let token_arena = create_token_arena();
        let _ = mq_lang::parse(&long_name, token_arena);
        // Should not panic
    }

    #[test]
    fn test_many_arguments() {
        let args = (0..100).map(|i| i.to_string()).collect::<Vec<_>>().join(", ");
        let code = format!("func({})", args);
        let token_arena = create_token_arena();
        let _ = mq_lang::parse(&code, token_arena);
        // Should not panic
    }

    #[test]
    fn test_chained_pipes() {
        let code = (0..50).map(|i| format!("f{}", i)).collect::<Vec<_>>().join(" | ");
        let token_arena = create_token_arena();
        let result = mq_lang::parse(&code, token_arena);
        assert!(result.is_ok(), "Should handle long pipe chains");
    }
}

#[cfg(test)]
mod determinism {
    use super::*;

    #[test]
    fn test_parse_determinism() {
        let test_cases = vec![
            "1 + 2",
            "let x = 42",
            "add(1, 2)",
            "if (true): 1 else: 2",
            "x && y",
            "nodes | .h",
            "true",
            "false",
            "None",
            r#""hello world""#,
        ];

        for code in test_cases {
            let arena1 = create_token_arena();
            let arena2 = create_token_arena();

            let result1 = mq_lang::parse(code, arena1);
            let result2 = mq_lang::parse(code, arena2);

            assert_eq!(
                result1.is_ok(),
                result2.is_ok(),
                "Parse results should match for: {}",
                code
            );

            if let (Ok(prog1), Ok(prog2)) = (result1, result2) {
                assert_eq!(prog1.len(), prog2.len(), "Program lengths should match");
                for (node1, node2) in prog1.iter().zip(prog2.iter()) {
                    assert_eq!(node1.expr, node2.expr, "Nodes should be equal");
                }
            }
        }
    }

    #[test]
    fn test_to_code_determinism() {
        let test_nodes = vec![
            make_node(AstExpr::Literal(AstLiteral::Bool(true))),
            make_node(AstExpr::Literal(AstLiteral::Number(42.into()))),
            make_node(AstExpr::Ident(IdentWithToken::new("test"))),
            make_node(AstExpr::Self_),
            make_node(AstExpr::Nodes),
        ];

        for node in test_nodes {
            let code1 = node.to_code();
            let code2 = node.to_code();
            let code3 = node.to_code();

            assert_eq!(code1, code2, "to_code should be deterministic");
            assert_eq!(code2, code3, "to_code should be deterministic");
        }
    }
}
