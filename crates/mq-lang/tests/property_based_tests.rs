//! Property-based tests for mq-lang AST operations.
use mq_lang::{
    Arena, AstExpr, AstLiteral, AstNode, DefaultEngine, IdentWithToken, Program, RuntimeValue, Shared, SharedCell,
};
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

fn create_engine() -> DefaultEngine {
    let mut engine = DefaultEngine::default();
    engine.load_builtin_module();
    engine
}

fn eval_code(code: &str) -> Result<Vec<RuntimeValue>, Box<mq_lang::Error>> {
    let mut engine = create_engine();
    let input = mq_lang::null_input();
    engine.eval(code, input.into_iter()).map(|v| v.into_iter().collect())
}

mod strategies {
    use super::*;

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

    pub fn bool_lit() -> impl Strategy<Value = AstLiteral> {
        any::<bool>().prop_map(AstLiteral::Bool)
    }

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

    pub fn none_lit() -> impl Strategy<Value = AstLiteral> {
        Just(AstLiteral::None)
    }

    pub fn literal() -> impl Strategy<Value = AstLiteral> {
        prop_oneof![
            3 => bool_lit(),
            4 => number_lit(),
            4 => string_lit(),
            1 => none_lit(),
        ]
    }

    pub fn literal_expr() -> impl Strategy<Value = Shared<AstNode>> {
        literal().prop_map(|lit| make_node(AstExpr::Literal(lit)))
    }

    pub fn ident_expr() -> impl Strategy<Value = Shared<AstNode>> {
        ident().prop_map(|id| make_node(AstExpr::Ident(id)))
    }

    pub fn self_expr() -> impl Strategy<Value = Shared<AstNode>> {
        Just(make_node(AstExpr::Self_))
    }

    pub fn simple_expr() -> impl Strategy<Value = Shared<AstNode>> {
        prop_oneof![
            6 => literal_expr(),
            4 => ident_expr(),
            1 => self_expr(),
        ]
    }

    pub fn paren_expr() -> impl Strategy<Value = Shared<AstNode>> {
        simple_expr().prop_map(|inner| make_node(AstExpr::Paren(inner)))
    }

    pub fn and_expr() -> impl Strategy<Value = Shared<AstNode>> {
        (simple_expr(), simple_expr()).prop_map(|(left, right)| make_node(AstExpr::And(left, right)))
    }

    pub fn or_expr() -> impl Strategy<Value = Shared<AstNode>> {
        (simple_expr(), simple_expr()).prop_map(|(left, right)| make_node(AstExpr::Or(left, right)))
    }

    pub fn binary_op_expr() -> impl Strategy<Value = Shared<AstNode>> {
        prop_oneof![and_expr(), or_expr()]
    }

    pub fn call_expr() -> impl Strategy<Value = Shared<AstNode>> {
        (ident(), prop::collection::vec(simple_expr(), 0..=3))
            .prop_map(|(func_name, args)| make_node(AstExpr::Call(func_name, args.into())))
    }

    pub fn let_expr() -> impl Strategy<Value = Shared<AstNode>> {
        (ident(), simple_expr()).prop_map(|(var_name, value)| make_node(AstExpr::Let(var_name, value)))
    }

    pub fn var_expr() -> impl Strategy<Value = Shared<AstNode>> {
        (ident(), simple_expr()).prop_map(|(var_name, value)| make_node(AstExpr::Var(var_name, value)))
    }

    pub fn assign_expr() -> impl Strategy<Value = Shared<AstNode>> {
        (ident(), simple_expr()).prop_map(|(var_name, value)| make_node(AstExpr::Assign(var_name, value)))
    }

    pub fn if_expr() -> impl Strategy<Value = Shared<AstNode>> {
        (simple_expr(), simple_expr(), simple_expr()).prop_map(|(cond, then_branch, else_branch)| {
            make_node(AstExpr::If(smallvec![(Some(cond), then_branch), (None, else_branch)]))
        })
    }

    pub fn if_expr_complex() -> impl Strategy<Value = Shared<AstNode>> {
        (simple_expr(), simple_expr(), prop::option::of(simple_expr())).prop_map(|(cond, then_branch, else_branch)| {
            let mut branches = smallvec![(Some(cond), then_branch)];
            if let Some(else_b) = else_branch {
                branches.push((None, else_b));
            }
            make_node(AstExpr::If(branches))
        })
    }

    pub fn block_expr() -> impl Strategy<Value = Shared<AstNode>> {
        prop::collection::vec(simple_expr(), 1..=3).prop_map(|stmts| make_node(AstExpr::Block(stmts)))
    }

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

    pub fn program() -> impl Strategy<Value = Program> {
        prop::collection::vec(any_expr(), 1..=5)
    }

    pub fn small_program() -> impl Strategy<Value = Program> {
        prop::collection::vec(simple_expr(), 1..=3)
    }

    pub fn medium_program() -> impl Strategy<Value = Program> {
        prop::collection::vec(any_expr(), 3..=7)
    }
}

mod assertions {
    use super::*;

    pub fn assert_parses(code: &str) -> Result<Program, TestCaseError> {
        let token_arena = create_token_arena();
        mq_lang::parse(code, token_arena).map_err(|e| TestCaseError::fail(format!("Parse failed: {:?}", e)))
    }

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

    pub fn expr_types_match(expr1: &AstExpr, expr2: &AstExpr) -> bool {
        std::mem::discriminant(expr1) == std::mem::discriminant(expr2)
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

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

    #[test]
    fn roundtrip_calls(node in strategies::call_expr()) {
        let code1 = node.to_code();
        let program = assertions::assert_parses(&code1)?;

        prop_assert!(!program.is_empty());

        let code2 = program[0].to_code();
        prop_assert_eq!(code1, code2, "Code roundtrip failed for call expression");
    }

    #[test]
    fn roundtrip_let_bindings(node in strategies::let_expr()) {
        let code1 = node.to_code();
        let program = assertions::assert_parses(&code1)?;

        prop_assert!(!program.is_empty());
        prop_assert!(matches!(&*program[0].expr, AstExpr::Let(_, _)));

        let code2 = program[0].to_code();
        prop_assert_eq!(code1, code2, "Code roundtrip failed for let expression");
    }

    #[test]
    fn roundtrip_var_bindings(node in strategies::var_expr()) {
        let code1 = node.to_code();
        let program = assertions::assert_parses(&code1)?;

        prop_assert!(!program.is_empty());
        prop_assert!(matches!(&*program[0].expr, AstExpr::Var(_, _)));

        let code2 = program[0].to_code();
        prop_assert_eq!(code1, code2, "Code roundtrip failed for var expression");
    }

    #[test]
    fn roundtrip_binary_ops(node in strategies::binary_op_expr()) {
        let code1 = node.to_code();
        let program = assertions::assert_parses(&code1)?;

        prop_assert!(!program.is_empty());
        prop_assert!(
            assertions::expr_types_match(&node.expr, &program[0].expr),
            "Expression types differ"
        );

        let code2 = program[0].to_code();
        prop_assert_eq!(code1, code2, "Code roundtrip failed for binary operation");
    }

    #[test]
    fn roundtrip_if_expressions(node in strategies::if_expr()) {
        let code1 = node.to_code();
        let program = assertions::assert_parses(&code1)?;

        prop_assert!(!program.is_empty());
        prop_assert!(matches!(&*program[0].expr, AstExpr::If(_)));

        let code2 = program[0].to_code();
        prop_assert_eq!(code1, code2, "Code roundtrip failed for if expression");
    }

    #[test]
    fn roundtrip_paren_expressions(node in strategies::paren_expr()) {
        let code1 = node.to_code();
        let program = assertions::assert_parses(&code1)?;

        prop_assert!(!program.is_empty());

        let code2 = program[0].to_code();
        prop_assert_eq!(code1, code2, "Code roundtrip failed for paren expression");
    }

    #[test]
    fn roundtrip_any_expression(node in strategies::any_expr()) {
        let code1 = node.to_code();
        let program = assertions::assert_parses(&code1)?;

        prop_assert!(!program.is_empty(), "Parsed program is empty for: {}", code1);

        let code2 = program[0].to_code();
        prop_assert_eq!(&code1, &code2, "Code roundtrip failed for: {}", code1);
    }

    #[test]
    fn roundtrip_programs(program in strategies::small_program()) {
        let mut code1 = String::new();
        for (i, node) in program.iter().enumerate() {
            if i > 0 {
                code1.push_str(" | ");
            }
            code1.push_str(&node.to_code());
        }

        let parsed = assertions::assert_parses(&code1)?;
        prop_assert!(!parsed.is_empty(), "Parsed program should not be empty");
        prop_assert_eq!(program.len(), parsed.len(), "Program length mismatch");

        let mut code2 = String::new();
        for (i, node) in parsed.iter().enumerate() {
            if i > 0 {
                code2.push_str(" | ");
            }
            code2.push_str(&node.to_code());
        }

        prop_assert_eq!(code1, code2, "Code roundtrip failed for small program");
    }

    #[test]
    fn roundtrip_medium_programs(program in strategies::medium_program()) {
        let mut code1 = String::new();
        for (i, node) in program.iter().enumerate() {
            if i > 0 {
                code1.push_str(" | ");
            }
            code1.push_str(&node.to_code());
        }

        let parsed = assertions::assert_parses(&code1)?;
        prop_assert!(!parsed.is_empty(), "Parsed program should not be empty");
        prop_assert_eq!(program.len(), parsed.len(), "Program length mismatch");

        let mut code2 = String::new();
        for (i, node) in parsed.iter().enumerate() {
            if i > 0 {
                code2.push_str(" | ");
            }
            code2.push_str(&node.to_code());
        }

        prop_assert_eq!(code1, code2, "Code roundtrip failed for medium program");
    }

    #[test]
    fn roundtrip_full_programs(program in strategies::program()) {
        let mut code1 = String::new();
        for (i, node) in program.iter().enumerate() {
            if i > 0 {
                code1.push_str(" | ");
            }
            code1.push_str(&node.to_code());
        }

        let parsed = assertions::assert_parses(&code1)?;
        prop_assert!(!parsed.is_empty(), "Full program should not be empty");
        prop_assert_eq!(program.len(), parsed.len(), "Program length mismatch");

        let mut code2 = String::new();
        for (i, node) in parsed.iter().enumerate() {
            if i > 0 {
                code2.push_str(" | ");
            }
            code2.push_str(&node.to_code());
        }

        prop_assert_eq!(code1, code2, "Code roundtrip failed for full program");
    }

    #[test]
    fn roundtrip_assignments(node in strategies::assign_expr()) {
        let code1 = node.to_code();
        let program = assertions::assert_parses(&code1)?;

        prop_assert!(!program.is_empty());
        prop_assert!(matches!(&*program[0].expr, AstExpr::Assign(_, _)));

        let code2 = program[0].to_code();
        prop_assert_eq!(code1, code2, "Code roundtrip failed for assignment expression");
    }

    #[test]
    fn roundtrip_blocks(node in strategies::block_expr()) {
        let code1 = node.to_code();
        let program = assertions::assert_parses(&code1)?;

        prop_assert!(!program.is_empty());

        let code2 = program[0].to_code();
        prop_assert_eq!(code1, code2, "Code roundtrip failed for block expression");
    }

    #[test]
    fn roundtrip_complex_if_expressions(node in strategies::if_expr_complex()) {
        let code1 = node.to_code();
        let program = assertions::assert_parses(&code1)?;

        prop_assert!(!program.is_empty());
        prop_assert!(matches!(&*program[0].expr, AstExpr::If(_)));

        let code2 = program[0].to_code();
        prop_assert_eq!(code1, code2, "Code roundtrip failed for complex if expression");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn to_code_is_deterministic(node in strategies::any_expr()) {
        let code1 = node.to_code();
        let code2 = node.to_code();
        prop_assert_eq!(code1, code2, "to_code must be deterministic");
    }

    #[test]
    fn to_code_produces_nonempty(node in strategies::any_expr()) {
        let code = node.to_code();
        prop_assert!(!code.is_empty(), "to_code should produce non-empty code");
    }

    #[test]
    fn to_code_contains_keywords(node in strategies::let_expr()) {
        let code = node.to_code();
        prop_assert!(code.contains("let"), "let expression should contain 'let' keyword");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn parser_handles_identifiers(ident in "[a-z_][a-z0-9_]{0,30}") {
        let token_arena = create_token_arena();
        let _ = mq_lang::parse(&ident, token_arena);
        // Should not panic
    }

    #[test]
    fn parser_handles_ident_sequences(
        idents in prop::collection::vec("[a-z]+", 1..=10)
    ) {
        let code = idents.join(" | ");
        let token_arena = create_token_arena();
        let _ = mq_lang::parse(&code, token_arena);
        // Should not panic
    }

    #[test]
    fn parser_handles_numbers(n in -1_000_000i64..1_000_000) {
        let code = format!("{}", n);
        let token_arena = create_token_arena();
        let result = mq_lang::parse(&code, token_arena);
        prop_assert!(result.is_ok(), "Should parse number: {}", n);
    }

    #[test]
    fn parser_handles_strings(s in r#"[a-zA-Z0-9 ]{0,30}"#) {
        let code = format!(r#""{}""#, s);
        let token_arena = create_token_arena();
        let result = mq_lang::parse(&code, token_arena);
        prop_assert!(result.is_ok(), "Should parse string: {}", code);
    }

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

// Property-based tests for eval functionality
proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn eval_is_deterministic(lit in prop_oneof![
        strategies::number_lit(),
        strategies::string_lit(),
    ]) {
        let node = make_node(AstExpr::Literal(lit));
        let code = node.to_code();
        // Use let binding to ensure output with null input
        let full_code = format!("let x = {} | x", code);

        let result1 = eval_code(&full_code)?;
        let result2 = eval_code(&full_code)?;

        prop_assert_eq!(result1, result2, "eval must be deterministic for: {}", full_code);
    }

    #[test]
    fn eval_number_literals(n in -1000i64..1000) {
        // Use let binding to ensure output with null input
        let code = format!("let x = {} | x", n);
        let result = eval_code(&code)?;

        prop_assert_eq!(result.len(), 1, "Should produce exactly one value");
        if let RuntimeValue::Number(num) = &result[0] {
            prop_assert_eq!(num.value() as i64, n, "Number value should match");
        } else {
            return Err(TestCaseError::fail(format!("Expected number, got {:?}", result[0])));
        }
    }

    #[test]
    fn eval_bool_literals(b in any::<bool>()) {
        // Boolean literals need to be used in context to produce output
        let bool_str = if b { "true" } else { "false" };
        let code = format!("let x = {} | if (eq(x, {})): 1 else: 0", bool_str, bool_str);
        let result = eval_code(&code)?;

        prop_assert_eq!(result.len(), 1, "Should produce exactly one value");
        if let RuntimeValue::Number(num) = &result[0] {
            // Since we're comparing x == bool_str, it should always be 1
            prop_assert_eq!(num.value() as i64, 1, "Boolean equality check should succeed");
        } else {
            return Err(TestCaseError::fail(format!("Expected number, got {:?}", result[0])));
        }
    }

    #[test]
    fn eval_string_literals(s in r#"[a-zA-Z0-9 ]{0,20}"#) {
        // Use let binding to ensure output with null input
        let code = format!(r#"let x = "{}" | x"#, s);
        let result = eval_code(&code)?;

        prop_assert_eq!(result.len(), 1, "Should produce exactly one value");
        if let RuntimeValue::String(val) = &result[0] {
            prop_assert_eq!(val, &s, "String value should match");
        } else {
            return Err(TestCaseError::fail(format!("Expected string, got {:?}", result[0])));
        }
    }

    #[test]
    fn eval_none_literal(_unit in Just(())) {
        // Note: None literal evaluates to empty result with null input in mq
        let result = eval_code("None");
        prop_assert!(result.is_ok(), "None should parse and evaluate successfully");
    }

    #[test]
    fn eval_and_operation(a in any::<bool>(), b in any::<bool>()) {
        let code = format!("{} && {}", a, b);
        let result = eval_code(&code)?;

        prop_assert_eq!(result.len(), 1, "Should produce exactly one value");
        if let RuntimeValue::Boolean(val) = result[0] {
            prop_assert_eq!(val, a && b, "AND operation should match Rust's &&");
        } else {
            return Err(TestCaseError::fail(format!("Expected boolean, got {:?}", result[0])));
        }
    }

    #[test]
    fn eval_or_operation(a in any::<bool>(), b in any::<bool>()) {
        let code = format!("{} || {}", a, b);
        let result = eval_code(&code)?;

        prop_assert_eq!(result.len(), 1, "Should produce exactly one value");
        if let RuntimeValue::Boolean(val) = result[0] {
            prop_assert_eq!(val, a || b, "OR operation should match Rust's ||");
        } else {
            return Err(TestCaseError::fail(format!("Expected boolean, got {:?}", result[0])));
        }
    }

    #[test]
    fn eval_simple_if_true(then_val in 0i64..100) {
        // Use (1 > 0) as a true condition
        let code = format!("let x = 1 | if (x > 0): {} else: 999", then_val);
        let result = eval_code(&code)?;

        prop_assert_eq!(result.len(), 1, "Should produce exactly one value");
        if let RuntimeValue::Number(result_num) = &result[0] {
            prop_assert_eq!(result_num.value() as i64, then_val, "Should evaluate to then branch");
        } else {
            return Err(TestCaseError::fail(format!("Expected number, got {:?}", result[0])));
        }
    }

    #[test]
    fn eval_simple_if_false(else_val in 0i64..100) {
        // Use (1 < 0) as a false condition
        let code = format!("let x = 1 | if (x < 0): 999 else: {}", else_val);
        let result = eval_code(&code)?;

        prop_assert_eq!(result.len(), 1, "Should produce exactly one value");
        if let RuntimeValue::Number(result_num) = &result[0] {
            prop_assert_eq!(result_num.value() as i64, else_val, "Should evaluate to else branch");
        } else {
            return Err(TestCaseError::fail(format!("Expected number, got {:?}", result[0])));
        }
    }

    #[test]
    fn eval_let_binding(var_name in strategies::ident(), val in 0i64..100) {
        let code = format!("let {} = {} | {}", var_name.name, val, var_name.name);
        let result = eval_code(&code)?;

        prop_assert_eq!(result.len(), 1, "Should produce exactly one value");
        if let RuntimeValue::Number(num) = &result[0] {
            prop_assert_eq!(num.value() as i64, val, "let binding should preserve value");
        } else {
            return Err(TestCaseError::fail(format!("Expected number, got {:?}", result[0])));
        }
    }

    #[test]
    fn eval_paren_expression(n in -100i64..100) {
        // Use let binding to ensure output with null input
        let code = format!("let x = ({}) | x", n);
        let result = eval_code(&code)?;

        prop_assert_eq!(result.len(), 1, "Should produce exactly one value");
        if let RuntimeValue::Number(num) = &result[0] {
            prop_assert_eq!(num.value() as i64, n, "Parenthesized expression should evaluate correctly");
        } else {
            return Err(TestCaseError::fail(format!("Expected number, got {:?}", result[0])));
        }
    }
}
