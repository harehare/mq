use std::borrow::Cow;
use std::fmt::{self, Display};

use smallvec::SmallVec;
use smol_str::SmolStr;

use crate::{Module, Range, Token};
use crate::{Shared, TokenKind};

type Comment = (Range, String);

/// A flat, source-ordered list of child nodes for a variable-arity construct
/// (e.g. call arguments), including any structural tokens (parens, commas)
/// interleaved with them so lossless formatting stays possible.
pub type ArgList = SmallVec<[Shared<Node>; 2]>;
/// `elif` clauses attached to an `if`.
pub type Elifs = SmallVec<[Shared<Node>; 2]>;

#[derive(Debug, Clone, PartialEq)]
pub enum Trivia {
    Whitespace(Shared<Token>),
    NewLine,
    Tab(Shared<Token>),
    Comment(Shared<Token>),
}

impl Display for Trivia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Trivia::Whitespace(token) => write!(f, "{}", token),
            Trivia::NewLine => writeln!(f),
            Trivia::Tab(token) => write!(f, "{}", token),
            Trivia::Comment(token) => write!(f, "{}", token),
        }
    }
}

impl Trivia {
    pub fn is_whitespace(&self) -> bool {
        matches!(self, Trivia::Whitespace(_))
    }

    pub fn is_new_line(&self) -> bool {
        matches!(self, Trivia::NewLine)
    }

    pub fn is_tab(&self) -> bool {
        matches!(self, Trivia::Tab(_))
    }

    pub fn is_comment(&self) -> bool {
        matches!(self, Trivia::Comment(_))
    }

    pub fn comment(&self) -> Option<&str> {
        match self {
            Trivia::Comment(token) => match &token.kind {
                TokenKind::Comment(comment) => Some(comment.as_str()),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn range(&self) -> Range {
        match self {
            Trivia::Comment(token) => token.range,
            Trivia::Whitespace(token) => token.range,
            Trivia::Tab(token) => token.range,
            _ => Range::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Node {
    pub kind: NodeKind,
    pub token: Option<Shared<Token>>,
    pub leading_trivia: Vec<Trivia>,
    pub trailing_trivia: Vec<Trivia>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum NodeKind {
    Array {
        items: ArgList,
    },
    As {
        expr: Shared<Node>,
        name: Shared<Node>,
    },
    /// `lhs = rhs` or a compound assignment (`+=`, `-=`, ...); `op` identifies which.
    Assign {
        op: BinaryOp,
        lhs: Shared<Node>,
        rhs: Shared<Node>,
    },
    BinaryOp {
        op: BinaryOp,
        lhs: Shared<Node>,
        rhs: Shared<Node>,
    },
    /// A `do...end` block, an implicit pipeline (`foo(a | b)`), or a selector
    /// descendant-chain grouping; all share the same flat `program` shape.
    Block {
        program: ArgList,
    },
    /// `break` or `break: value`.
    Break {
        colon: Option<Shared<Node>>,
        value: Option<Shared<Node>>,
    },
    /// A function call (`ident(args)`) or a bracket/slice access desugared to the
    /// same shape (`target[index]` -> args holds `[`, index expr(s), `]`).
    /// `args` is the flat, token-interleaved list `parse_args`/`parse_bracket_access`
    /// produce, in source order.
    Call {
        args: ArgList,
    },
    /// A call whose callee is an arbitrary expression, e.g. `(f)(x)` or `a[0](x)`.
    CallDynamic {
        callee: Shared<Node>,
        args: ArgList,
    },
    Continue,
    Def {
        name: Shared<Node>,
        params: Option<ArgList>,
        colon_or_do: Option<Shared<Node>>,
        program: ArgList,
    },
    Dict {
        entries: ArgList,
    },
    DictEntry {
        key: Shared<Node>,
        colon: Shared<Node>,
        value: Shared<Node>,
    },
    Do,
    End,
    Elif {
        args: ArgList,
        colon: Option<Shared<Node>>,
        then_branch: Shared<Node>,
    },
    Else {
        colon: Option<Shared<Node>>,
        then_branch: Shared<Node>,
    },
    Env,
    #[default]
    Eof,
    Fn {
        params: ArgList,
        colon_or_do: Option<Shared<Node>>,
        program: ArgList,
    },
    Foreach {
        args: ArgList,
        colon_or_do: Option<Shared<Node>>,
        program: ArgList,
    },
    Group {
        lparen: Shared<Node>,
        expr: Shared<Node>,
        rparen: Shared<Node>,
    },
    /// An identifier; `attr` is a trailing attribute selector (`x.attr`).
    Ident {
        attr: Option<Shared<Node>>,
    },
    If {
        args: ArgList,
        colon: Option<Shared<Node>>,
        then_branch: Shared<Node>,
        elifs: Elifs,
        else_branch: Option<Shared<Node>>,
    },
    /// `import "path"` or `import "path" as alias`.
    Import {
        path: Shared<Node>,
        as_token: Option<Shared<Node>>,
        alias: Option<Shared<Node>>,
    },
    Include {
        path: Shared<Node>,
    },
    InterpolatedString,
    Let {
        lhs: Shared<Node>,
        eq_token: Shared<Node>,
        rhs: Shared<Node>,
    },
    Loop {
        colon_or_do: Option<Shared<Node>>,
        program: ArgList,
    },
    Var {
        lhs: Shared<Node>,
        eq_token: Shared<Node>,
        rhs: Shared<Node>,
    },
    Literal,
    Match {
        args: ArgList,
        colon_or_do: Option<Shared<Node>>,
        arms: ArgList,
        end_token: Option<Shared<Node>>,
    },
    /// `| pattern [if (guard)]: body`. `guard_args` is the flat, token-interleaved
    /// `(...)` list `parse_args` produces (present iff `if_token` is).
    MatchArm {
        pipe: Shared<Node>,
        pattern: Shared<Node>,
        if_token: Option<Shared<Node>>,
        guard_args: Option<ArgList>,
        colon: Shared<Node>,
        body: Shared<Node>,
    },
    Module {
        name: Shared<Node>,
        colon_or_do: Option<Shared<Node>>,
        program: ArgList,
    },
    Nodes,
    /// `p1 || p2 || ...`; `items` interleaves the patterns with their `||` tokens.
    OrPattern {
        items: ArgList,
    },
    /// A `def`/`fn`/`catch` parameter; `token` is its name.
    Param {
        asterisk: Option<Shared<Node>>,
        eq_token: Option<Shared<Node>>,
        default: Option<Shared<Node>>,
    },
    /// Wildcard/literal/ident/type/array/dict pattern (one tag, several grammars,
    /// like `Call`). Leaf patterns carry no items; array/dict patterns hold the
    /// flat, token-interleaved `[...]`/`{...}` list.
    Pattern {
        items: ArgList,
    },
    /// `a::b::c` or `a::b(args)`; `items` interleaves idents, `::` tokens and any call args.
    QualifiedAccess {
        items: ArgList,
    },
    /// A selector. `items` holds bracket tokens for `.[n]` or a trailing attribute selector.
    Selector {
        items: ArgList,
    },
    /// A `:name` symbol literal.
    Symbol {
        colon: Shared<Node>,
        name: Shared<Node>,
    },
    SelectorCall {
        args: ArgList,
    },
    Self_ {
        attr: Option<Shared<Node>>,
    },
    SelfAttr,
    Spread {
        operand: Shared<Node>,
    },
    Token,
    Try {
        colon_or_do: Option<Shared<Node>>,
        body: Shared<Node>,
        catch: Option<Shared<Node>>,
    },
    /// `params` is the optional `(e)` error binder.
    Catch {
        params: Option<ArgList>,
        colon_or_do: Option<Shared<Node>>,
        body: Shared<Node>,
    },
    Unless {
        args: ArgList,
        colon: Option<Shared<Node>>,
        then_branch: Shared<Node>,
    },
    Until {
        args: ArgList,
        colon_or_do: Option<Shared<Node>>,
        program: ArgList,
    },
    UnaryOp {
        op: UnaryOp,
        operand: Shared<Node>,
    },
    While {
        args: ArgList,
        colon_or_do: Option<Shared<Node>>,
        program: ArgList,
    },
    Yield {
        colon: Option<Shared<Node>>,
        value: Option<Shared<Node>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    And,
    Convert,
    Assign,
    Coalesce,
    Division,
    DivisionEqual,
    DoubleDivisionEqual,
    PipeEqual,
    Equal,
    Gt,
    Gte,
    Lt,
    Lte,
    Minus,
    MinusEqual,
    Modulo,
    ModuloEqual,
    Multiplication,
    MultiplicationEqual,
    NotEqual,
    Or,
    Plus,
    PlusEqual,
    RangeOp,
    RegexMatch,
    NotRegexMatch,
    LeftShift,
    RightShift,
}

impl BinaryOp {
    pub fn is_assignment(&self) -> bool {
        matches!(
            self,
            BinaryOp::Assign
                | BinaryOp::PlusEqual
                | BinaryOp::MinusEqual
                | BinaryOp::MultiplicationEqual
                | BinaryOp::DivisionEqual
                | BinaryOp::ModuloEqual
                | BinaryOp::DoubleDivisionEqual
                | BinaryOp::PipeEqual
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Not,
    Negate,
}

impl Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            self.token
                .as_ref()
                .map(|token| token.kind.to_string())
                .unwrap_or_default()
        )
    }
}

impl Node {
    pub fn new_pipe(with_new_line: bool) -> Self {
        Node {
            kind: NodeKind::Token,
            token: Some(Shared::new(Token {
                kind: TokenKind::Pipe,
                range: Range::default(),
                module_id: Module::TOP_LEVEL_MODULE_ID,
            })),
            leading_trivia: if with_new_line { vec![Trivia::NewLine] } else { vec![] },
            trailing_trivia: vec![Trivia::Whitespace(Shared::new(Token {
                kind: TokenKind::Whitespace(1),
                range: Range::default(),
                module_id: Module::TOP_LEVEL_MODULE_ID,
            }))],
        }
    }

    pub fn has_new_line(&self) -> bool {
        self.leading_trivia.contains(&Trivia::NewLine)
    }

    pub fn range(&self) -> Range {
        self.token.as_ref().map(|token| token.range).unwrap_or_default()
    }

    /// Returns the source range covering this node and its nested children.
    pub fn node_range(&self) -> Range {
        let children = self.all_children();
        let first = children.first().map(|child| child.node_range());
        let last = children.last().map(|child| child.node_range());
        let own = self.token.as_ref().map(|token| token.range);
        Range {
            start: match (own, first) {
                (Some(own), Some(first)) => own.start.min(first.start),
                (Some(own), None) => own.start,
                (None, Some(first)) => first.start,
                (None, None) => Range::default().start,
            },
            end: match (own, last) {
                (Some(own), Some(last)) => own.end.max(last.end),
                (Some(own), None) => own.end,
                (None, Some(last)) => last.end,
                (None, None) => Range::default().end,
            },
        }
    }

    /// All immediate child nodes as a flat, source-ordered list (structural tokens
    /// included), for generic tree walkers that don't care about a node's shape.
    pub fn all_children(&self) -> Cow<'_, [Shared<Node>]> {
        match &self.kind {
            NodeKind::Call { args } => Cow::Borrowed(args.as_slice()),
            NodeKind::BinaryOp { lhs, rhs, .. } | NodeKind::Assign { lhs, rhs, .. } => {
                Cow::Owned(vec![Shared::clone(lhs), Shared::clone(rhs)])
            }
            NodeKind::UnaryOp { operand, .. } => Cow::Owned(vec![Shared::clone(operand)]),
            NodeKind::If {
                args,
                colon,
                then_branch,
                elifs,
                else_branch,
            } => {
                let mut v: Vec<Shared<Node>> = args.to_vec();
                v.extend(colon.iter().cloned());
                v.push(Shared::clone(then_branch));
                v.extend(elifs.iter().cloned());
                v.extend(else_branch.iter().cloned());
                Cow::Owned(v)
            }
            NodeKind::Elif {
                args,
                colon,
                then_branch,
            }
            | NodeKind::Unless {
                args,
                colon,
                then_branch,
            } => {
                let mut v: Vec<Shared<Node>> = args.to_vec();
                v.extend(colon.iter().cloned());
                v.push(Shared::clone(then_branch));
                Cow::Owned(v)
            }
            NodeKind::Else { colon, then_branch } => {
                let mut v: Vec<Shared<Node>> = colon.iter().cloned().collect();
                v.push(Shared::clone(then_branch));
                Cow::Owned(v)
            }
            NodeKind::Block { program } => Cow::Borrowed(program.as_slice()),
            NodeKind::Def {
                name,
                params,
                colon_or_do,
                program,
            } => {
                let mut v: Vec<Shared<Node>> = vec![Shared::clone(name)];
                if let Some(p) = params {
                    v.extend(p.iter().cloned());
                }
                v.extend(colon_or_do.iter().cloned());
                v.extend(program.iter().cloned());
                Cow::Owned(v)
            }
            NodeKind::Fn {
                params,
                colon_or_do,
                program,
            } => {
                let mut v: Vec<Shared<Node>> = params.to_vec();
                v.extend(colon_or_do.iter().cloned());
                v.extend(program.iter().cloned());
                Cow::Owned(v)
            }
            NodeKind::Let { lhs, eq_token, rhs } | NodeKind::Var { lhs, eq_token, rhs } => {
                Cow::Owned(vec![Shared::clone(lhs), Shared::clone(eq_token), Shared::clone(rhs)])
            }
            NodeKind::Array { items } => Cow::Borrowed(items.as_slice()),
            NodeKind::Dict { entries } => Cow::Borrowed(entries.as_slice()),
            NodeKind::DictEntry { key, colon, value } => {
                Cow::Owned(vec![Shared::clone(key), Shared::clone(colon), Shared::clone(value)])
            }
            NodeKind::Spread { operand } => Cow::Owned(vec![Shared::clone(operand)]),
            NodeKind::Match {
                args,
                colon_or_do,
                arms,
                end_token,
            } => {
                let mut v: Vec<Shared<Node>> = args.to_vec();
                v.extend(colon_or_do.iter().cloned());
                v.extend(arms.iter().cloned());
                v.extend(end_token.iter().cloned());
                Cow::Owned(v)
            }
            NodeKind::MatchArm {
                pipe,
                pattern,
                if_token,
                guard_args,
                colon,
                body,
            } => {
                let mut v: Vec<Shared<Node>> = vec![Shared::clone(pipe), Shared::clone(pattern)];
                v.extend(if_token.iter().cloned());
                if let Some(args) = guard_args {
                    v.extend(args.iter().cloned());
                }
                v.push(Shared::clone(colon));
                v.push(Shared::clone(body));
                Cow::Owned(v)
            }
            NodeKind::OrPattern { items }
            | NodeKind::Pattern { items }
            | NodeKind::QualifiedAccess { items }
            | NodeKind::Selector { items }
            | NodeKind::SelectorCall { args: items } => Cow::Borrowed(items.as_slice()),
            NodeKind::Foreach {
                args,
                colon_or_do,
                program,
            }
            | NodeKind::While {
                args,
                colon_or_do,
                program,
            }
            | NodeKind::Until {
                args,
                colon_or_do,
                program,
            } => {
                let mut v: Vec<Shared<Node>> = args.to_vec();
                v.extend(colon_or_do.iter().cloned());
                v.extend(program.iter().cloned());
                Cow::Owned(v)
            }
            NodeKind::Loop { colon_or_do, program } => {
                let mut v: Vec<Shared<Node>> = colon_or_do.iter().cloned().collect();
                v.extend(program.iter().cloned());
                Cow::Owned(v)
            }
            NodeKind::Try {
                colon_or_do,
                body,
                catch,
            } => {
                let mut v: Vec<Shared<Node>> = colon_or_do.iter().cloned().collect();
                v.push(Shared::clone(body));
                v.extend(catch.iter().cloned());
                Cow::Owned(v)
            }
            NodeKind::Catch {
                params,
                colon_or_do,
                body,
            } => {
                let mut v: Vec<Shared<Node>> = params.iter().flatten().cloned().collect();
                v.extend(colon_or_do.iter().cloned());
                v.push(Shared::clone(body));
                Cow::Owned(v)
            }
            NodeKind::As { expr, name } => Cow::Owned(vec![Shared::clone(expr), Shared::clone(name)]),
            NodeKind::Break { colon, value } | NodeKind::Yield { colon, value } => {
                Cow::Owned(colon.iter().chain(value.iter()).cloned().collect())
            }
            NodeKind::CallDynamic { callee, args } => {
                let mut v = vec![Shared::clone(callee)];
                v.extend(args.iter().cloned());
                Cow::Owned(v)
            }
            NodeKind::Group { lparen, expr, rparen } => {
                Cow::Owned(vec![Shared::clone(lparen), Shared::clone(expr), Shared::clone(rparen)])
            }
            NodeKind::Import { path, as_token, alias } => {
                let mut v = vec![Shared::clone(path)];
                v.extend(as_token.iter().cloned());
                v.extend(alias.iter().cloned());
                Cow::Owned(v)
            }
            NodeKind::Include { path } => Cow::Owned(vec![Shared::clone(path)]),
            NodeKind::Module {
                name,
                colon_or_do,
                program,
            } => {
                let mut v = vec![Shared::clone(name)];
                v.extend(colon_or_do.iter().cloned());
                v.extend(program.iter().cloned());
                Cow::Owned(v)
            }
            NodeKind::Ident { attr } | NodeKind::Self_ { attr } => Cow::Owned(attr.iter().cloned().collect()),
            NodeKind::Param {
                asterisk,
                eq_token,
                default,
            } => Cow::Owned(asterisk.iter().chain(eq_token).chain(default).cloned().collect()),
            NodeKind::Symbol { colon, name } => Cow::Owned(vec![Shared::clone(colon), Shared::clone(name)]),
            _ => Cow::Borrowed(&[]),
        }
    }

    pub fn name(&self) -> Option<SmolStr> {
        self.token.as_ref().map(|token| token.to_string().into())
    }

    pub fn is_token(&self) -> bool {
        matches!(self.kind, NodeKind::Token)
    }

    pub fn is_eof(&self) -> bool {
        matches!(self.kind, NodeKind::Eof)
    }

    pub fn is_fn(&self) -> bool {
        matches!(self.kind, NodeKind::Fn { .. })
    }

    pub fn is_def(&self) -> bool {
        matches!(self.kind, NodeKind::Def { .. })
    }

    pub fn is_pipe(&self) -> bool {
        self.token
            .as_ref()
            .is_some_and(|token| matches!(token.kind, TokenKind::Pipe))
    }

    pub fn comments(&self) -> Vec<Comment> {
        self.leading_trivia
            .iter()
            .filter_map(|trivia| trivia.comment().map(|c| (trivia.range(), c.to_string())))
            .collect::<Vec<_>>()
    }

    pub fn children_without_token(&self) -> Vec<Shared<Node>> {
        self.all_children()
            .iter()
            .filter(|child| !child.is_token())
            .cloned()
            .collect::<Vec<_>>()
    }

    pub fn split_cond_and_program(&self) -> (Vec<Shared<Node>>, Vec<Shared<Node>>) {
        match &self.kind {
            NodeKind::Def {
                name,
                params,
                colon_or_do,
                program,
            } => {
                let mut cond = vec![Shared::clone(name)];
                if let Some(p) = params {
                    cond.extend(p.iter().filter(|c| !c.is_token()).cloned());
                }
                let mut prog: Vec<Shared<Node>> = colon_or_do.iter().filter(|c| !c.is_token()).cloned().collect();
                prog.extend(program.iter().filter(|c| !c.is_token()).cloned());
                return (cond, prog);
            }
            NodeKind::Fn {
                params,
                colon_or_do,
                program,
            } => {
                let cond: Vec<Shared<Node>> = params.iter().filter(|c| !c.is_token()).cloned().collect();
                let mut prog: Vec<Shared<Node>> = colon_or_do.iter().filter(|c| !c.is_token()).cloned().collect();
                prog.extend(program.iter().filter(|c| !c.is_token()).cloned());
                return (cond, prog);
            }
            _ => {}
        }

        let children = self.all_children();
        let colon_index = children.iter().position(|child| {
            child
                .token
                .as_ref()
                .map(|token| matches!(token.kind, TokenKind::Colon))
                .unwrap_or(false)
        });

        // If there's no colon, split before the right parenthesis
        let index = match colon_index {
            Some(index) => index,
            None => children
                .iter()
                .position(|child| {
                    child
                        .token
                        .as_ref()
                        .map(|token| matches!(token.kind, TokenKind::RParen))
                        .unwrap_or(false)
                })
                .map(|index| index + 1)
                .unwrap_or_default(),
        };

        (
            children
                .iter()
                .take(index)
                .filter(|child| !child.is_token())
                .cloned()
                .collect::<Vec<_>>(),
            children
                .iter()
                .skip(index)
                .filter(|child| !child.is_token())
                .cloned()
                .collect::<Vec<_>>(),
        )
    }

    pub fn binary_op(&self) -> Option<(Shared<Node>, Shared<Node>)> {
        match &self.kind {
            NodeKind::BinaryOp { lhs, rhs, .. } | NodeKind::Assign { lhs, rhs, .. } => {
                Some((Shared::clone(lhs), Shared::clone(rhs)))
            }
            _ => None,
        }
    }

    pub fn unary_op(&self) -> Option<Shared<Node>> {
        match &self.kind {
            NodeKind::UnaryOp { operand, .. } => Some(Shared::clone(operand)),
            _ => None,
        }
    }

    /// Returns the name of an identifier or definition node.
    pub fn get_identifier(&self) -> Option<String> {
        match &self.kind {
            NodeKind::Ident { .. } => self.token.as_ref().map(ToString::to_string),
            NodeKind::Def { name, .. } => name.get_identifier(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {

    use rstest::rstest;

    use super::*;
    use crate::arena::ArenaId;

    #[test]
    fn test_node_range_includes_binary_operands_and_nested_call() {
        let (nodes, errors) = crate::parse_recovery("1 + foo(2)");
        assert!(!errors.has_errors(), "{errors}");
        assert_eq!(nodes[0].node_range().start, crate::Position::new(1, 1));
        assert_eq!(nodes[0].node_range().end, crate::Position::new(1, 11));
    }

    #[test]
    fn test_node_range_includes_grouped_expression() {
        let (nodes, errors) = crate::parse_recovery("(1 + 2)");
        assert!(!errors.has_errors(), "{errors}");
        assert_eq!(nodes[0].node_range().start, crate::Position::new(1, 1));
        assert_eq!(nodes[0].node_range().end, crate::Position::new(1, 8));
    }

    #[test]
    fn test_def_identifier_is_name() {
        let (nodes, errors) = crate::parse_recovery("def answer(): 42 end");
        assert!(!errors.has_errors(), "{errors}");
        assert_eq!(nodes[0].get_identifier(), Some("answer".to_string()));
    }

    #[rstest]
    #[case(
        Trivia::Whitespace(Shared::new(Token {
            kind: TokenKind::Whitespace(1),
            range: Range::default(),
            module_id: ArenaId::new(0),
        })),
        " "
    )]
    #[case(Trivia::NewLine, "\n")]
    #[case(
        Trivia::Tab(Shared::new(Token {
            kind: TokenKind::Tab(1),
            range: Range::default(),
            module_id: ArenaId::new(0),
        })),
        "\t"
    )]
    #[case(
        Trivia::Comment(Shared::new(Token {
            kind: TokenKind::Comment("comment".to_string()),
            range: Range::default(),
            module_id: ArenaId::new(0),
        })),
        "# comment"
    )]
    fn test_trivia_display(#[case] trivia: Trivia, #[case] expected: &str) {
        assert_eq!(format!("{}", trivia), expected);
    }

    #[rstest]
    #[case(
        Trivia::Whitespace(Shared::new(Token {
            kind: TokenKind::Whitespace(1),
            range: Range::default(),
            module_id: ArenaId::new(0),
        })),
        true, false, false, false
    )]
    #[case(Trivia::NewLine, false, true, false, false)]
    #[case(
        Trivia::Tab(Shared::new(Token {
            kind: TokenKind::Tab(1),
            range: Range::default(),
            module_id: ArenaId::new(0),
        })),
        false, false, true, false
    )]
    #[case(
        Trivia::Comment(Shared::new(Token {
            kind: TokenKind::Comment("comment".to_string()),
            range: Range::default(),
            module_id: ArenaId::new(0),
        })),
        false, false, false, true
    )]
    fn test_trivia_type_checks(
        #[case] trivia: Trivia,
        #[case] is_whitespace: bool,
        #[case] is_new_line: bool,
        #[case] is_tab: bool,
        #[case] is_comment: bool,
    ) {
        assert_eq!(trivia.is_whitespace(), is_whitespace);
        assert_eq!(trivia.is_new_line(), is_new_line);
        assert_eq!(trivia.is_tab(), is_tab);
        assert_eq!(trivia.is_comment(), is_comment);
    }

    #[rstest]
    #[case(
        Trivia::Comment(Shared::new(Token {
            kind: TokenKind::Comment("test comment".to_string()),
            range: Range::default(),
            module_id: ArenaId::new(0),
        })),
        Some("test comment")
    )]
    #[case(
        Trivia::Whitespace(Shared::new(Token {
            kind: TokenKind::Whitespace(1),
            range: Range::default(),
            module_id: ArenaId::new(0),
        })),
        None
    )]
    fn test_trivia_comment(#[case] trivia: Trivia, #[case] expected: Option<&str>) {
        assert_eq!(trivia.comment(), expected);
    }

    #[rstest]
    #[case(NodeKind::Token, true)]
    #[case(NodeKind::Call { args: ArgList::new() }, false)]
    #[case(NodeKind::Env, false)]
    fn test_node_is_token(#[case] kind: NodeKind, #[case] expected: bool) {
        let node = Node {
            kind,
            token: None,
            leading_trivia: Vec::new(),
            trailing_trivia: Vec::new(),
        };
        assert_eq!(node.is_token(), expected);
    }

    #[test]
    fn test_node_has_new_line() {
        let node = Node {
            kind: NodeKind::Call { args: ArgList::new() },
            token: None,
            leading_trivia: vec![Trivia::NewLine],
            trailing_trivia: Vec::new(),
        };
        assert!(node.has_new_line());

        let node_without_newline = Node {
            kind: NodeKind::Call { args: ArgList::new() },
            token: None,
            leading_trivia: Vec::new(),
            trailing_trivia: Vec::new(),
        };
        assert!(!node_without_newline.has_new_line());
    }

    #[test]
    fn test_children_without_token() {
        let token_node = Shared::new(Node {
            kind: NodeKind::Token,
            token: None,
            leading_trivia: Vec::new(),
            trailing_trivia: Vec::new(),
        });

        let call_node = Shared::new(Node {
            kind: NodeKind::Call { args: ArgList::new() },
            token: None,
            leading_trivia: Vec::new(),
            trailing_trivia: Vec::new(),
        });

        let parent = Node {
            kind: NodeKind::Array {
                items: vec![token_node, call_node.clone()].into(),
            },
            token: None,
            leading_trivia: Vec::new(),
            trailing_trivia: Vec::new(),
        };

        let result = parent.children_without_token();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], call_node);
    }
}
