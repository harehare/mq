//! CST parser behavior and AST/CST agreement.
#![cfg(feature = "cst")]

use mq_syntax::ast::node::{Expr, Node};
use mq_syntax::{Arena, CstNode, CstNodeKind, Lexer, LexerOptions, Shared, TOP_LEVEL_MODULE_ID, Token, parse_recovery};

#[test]
fn test_parse_recovery_success() {
    let (cst_nodes, errors) = parse_recovery("add(1, 2)");

    assert!(!errors.has_errors());
    assert!(!cst_nodes.is_empty());
}

#[test]
fn test_parse_recovery_with_errors() {
    let (cst_nodes, errors) = parse_recovery("add(1,");

    assert!(errors.has_errors());
    assert!(cst_nodes[0].has_error());
}

proptest::proptest! {
    #[test]
    fn parse_recovery_never_panics(code in "\\PC{0,64}") {
        let _ = parse_recovery(&code);
    }
}

#[test]
fn test_parse_recovery_with_error_lexer() {
    let (cst_nodes, errors) = parse_recovery("add(1, \"");

    assert!(errors.has_errors());
    assert!(cst_nodes[0].has_error());
}

#[test]
fn test_parse_recovery_with_invalid_unicode_escape() {
    let (nodes, errors) = parse_recovery("\"\\u{ZZZZ}\"");
    assert!(nodes.iter().any(|node| node.has_error()));
    assert!(errors.has_errors());
}

/// Binary operators grouped by precedence, lowest first, excluding assignments.
const BINARY_OP_LEVELS: &[&[&str]] = &[
    &["||"],
    &["??"],
    &["&&"],
    &["==", "!=", ">", ">=", "<", "<=", "=~", "!~"],
    &["+", "-", ">>", "<<"],
    &["*", "/", "%"],
    &["@"],
    &[".."],
];

fn cst_shape(node: &CstNode) -> String {
    match &node.kind {
        CstNodeKind::BinaryOp { lhs, rhs, .. } => format!(
            "({} {} {})",
            cst_shape(lhs),
            node.token.as_ref().unwrap(),
            cst_shape(rhs)
        ),
        _ => node.token.as_ref().map(|t| t.to_string()).unwrap_or_default(),
    }
}

fn ast_shape(node: &Node, arena: &Arena<Token>) -> String {
    let op = || arena[node.token_id].to_string();
    let fold = |operands: &[Shared<Node>]| {
        let mut shape = ast_shape(&operands[0], arena);
        for operand in &operands[1..] {
            shape = format!("({} {} {})", shape, op(), ast_shape(operand, arena));
        }
        shape
    };
    match &node.expr {
        Expr::BinaryOp(_, lhs, rhs) => fold(&[Shared::clone(lhs), Shared::clone(rhs)]),
        Expr::And(operands) | Expr::Or(operands) => fold(operands),
        Expr::Call(_, args) if args.len() == 2 && arena[node.token_id].kind.binary_op_precedence().is_some() => {
            fold(args)
        }
        Expr::Ident(ident) => ident.name.to_string(),
        _ => "?".to_string(),
    }
}

#[test]
fn test_binary_op_precedence_matches_between_ast_and_cst() {
    let ops: Vec<(usize, &str)> = BINARY_OP_LEVELS
        .iter()
        .enumerate()
        .flat_map(|(level, ops)| ops.iter().map(move |op| (level, *op)))
        .collect();

    for &(lx, x) in &ops {
        for &(ly, y) in &ops {
            let code = format!("a {x} b {y} c");
            let expected = if lx < ly {
                format!("(a {x} (b {y} c))")
            } else {
                format!("((a {x} b) {y} c)")
            };

            let (cst_nodes, errors) = parse_recovery(&code);
            assert!(!errors.has_errors(), "CST errors for {code:?}");
            assert_eq!(cst_shape(&cst_nodes[0]), expected, "CST shape for {code:?}");

            let tokens = Lexer::new(LexerOptions::default())
                .tokenize(&code, TOP_LEVEL_MODULE_ID)
                .unwrap_or_else(|e| panic!("{code:?}: {e:?}"));
            let mut arena = Arena::new(16);
            let program = mq_syntax::ast::parser::Parser::new(tokens.iter(), &mut arena, TOP_LEVEL_MODULE_ID)
                .parse()
                .unwrap_or_else(|e| panic!("{code:?}: {e:?}"));
            assert_eq!(ast_shape(&program[0], &arena), expected, "AST shape for {code:?}");
        }
    }
}

#[rstest::rstest]
#[case::assign("=")]
#[case::plus_assign("+=")]
#[case::minus_assign("-=")]
#[case::mul_assign("*=")]
#[case::div_assign("/=")]
#[case::mod_assign("%=")]
#[case::floor_div_assign("//=")]
fn test_cst_assignment_has_lowest_precedence(#[case] op: &str) {
    let code = format!("x {op} a || b && c");
    let (nodes, errors) = parse_recovery(&code);
    assert!(!errors.has_errors(), "CST errors for {code:?}");

    let CstNodeKind::Assign { lhs, rhs, .. } = &nodes[0].kind else {
        panic!("expected an assignment for {code:?}");
    };
    assert_eq!(cst_shape(lhs), "x");
    assert_eq!(cst_shape(rhs), "(a || (b && c))");
}

#[test]
fn test_cst_pipe_assignment_has_lowest_precedence() {
    let code = "x.url |= a || b && c";
    let (nodes, errors) = parse_recovery(code);
    assert!(!errors.has_errors(), "CST errors for {code:?}");

    let CstNodeKind::Assign { rhs, .. } = &nodes[0].kind else {
        panic!("expected an assignment for {code:?}");
    };
    assert_eq!(cst_shape(rhs), "(a || (b && c))");
}
