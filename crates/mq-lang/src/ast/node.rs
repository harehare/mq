use std::{
    fmt::{self, Display, Formatter},
    hash::{Hash, Hasher},
    // Rc is no longer used directly for AST nodes
};

use compact_str::CompactString;
use smallvec::SmallVec;
use typed_arena::Arena as TypedArena; // Renamed to avoid conflict

use crate::{Token, arena::Arena as TokenArena, lexer, number::Number, range::Range}; // TokenArena for clarity

use super::{IdentName, TokenId}; // Program is now Vec<NodeId>

// Define NodeId
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub usize);

// Define AstArena
pub type AstArena<'ast> = TypedArena<NodeData<'ast>>;

// Define NodeData
#[derive(PartialEq, PartialOrd, Debug, Clone)]
pub struct NodeData<'ast> {
    pub token_id: TokenId,
    pub expr: Expr<'ast>,
}

type Depth = u8;
type Index = usize;
type Optional = bool;
type Lang = CompactString;

// Updated type aliases using NodeId
pub type Program<'ast> = Vec<NodeId>;
pub type Params<'ast> = SmallVec<[NodeId; 4]>;
pub type Args<'ast> = SmallVec<[NodeId; 4]>;
pub type Cond<'ast> = (Option<NodeId>, NodeId);
pub type Branches<'ast> = SmallVec<[Cond<'ast>; 4]>;

// The old Node struct is removed. NodeData in AstArena takes its place.

// NodeData methods (e.g., range)
impl<'ast> NodeData<'ast> {
    pub fn range(&self, node_id: NodeId, ast_arena: &AstArena<'ast>, token_arena: &TokenArena<Rc<Token>>) -> Range {
        match &self.expr {
            Expr::Def(_, _, program)
            | Expr::Fn(_, program)
            | Expr::While(_, program)
            | Expr::Until(_, program)
            | Expr::Foreach(_, _, program) => {
                let start = program
                    .first()
                    .map(|id| ast_arena[*id].range(*id, ast_arena, token_arena).start)
                    .unwrap_or_else(|| token_arena[self.token_id].range.start.clone()); // Fallback for empty programs
                let end = program
                    .last()
                    .map(|id| ast_arena[*id].range(*id, ast_arena, token_arena).end)
                    .unwrap_or_else(|| token_arena[self.token_id].range.end.clone()); // Fallback for empty programs
                Range { start, end }
            }
            Expr::Call(_, args, _) => {
                let start = args
                    .first()
                    .map(|id| ast_arena[*id].range(*id, ast_arena, token_arena).start)
                    .unwrap_or_else(|| token_arena[self.token_id].range.start.clone()); // Fallback for empty args
                let end = args
                    .last()
                    .map(|id| ast_arena[*id].range(*id, ast_arena, token_arena).end)
                    .unwrap_or_else(|| token_arena[self.token_id].range.end.clone()); // Fallback for empty args
                Range { start, end }
            }
            Expr::Let(_, node_id_val) => ast_arena[*node_id_val].range(*node_id_val, ast_arena, token_arena),
            Expr::If(nodes) => {
                // Ensure nodes is not empty before unwrapping
                if nodes.is_empty() {
                    return token_arena[self.token_id].range.clone(); // Default range if no branches
                }
                let first_branch_node_id = nodes.first().unwrap().1;
                let last_branch_node_id = nodes.last().unwrap().1;
                let start = ast_arena[first_branch_node_id].range(first_branch_node_id, ast_arena, token_arena).start;
                let end = ast_arena[last_branch_node_id].range(last_branch_node_id, ast_arena, token_arena).end;
                Range { start, end }
            }
            Expr::Literal(_)
            | Expr::Ident(_)
            | Expr::Selector(_)
            | Expr::Include(_)
            | Expr::InterpolatedString(_)
            | Expr::Nodes
            | Expr::Self_ => token_arena[self.token_id].range.clone(),
        }
    }

    pub fn is_nodes(&self) -> bool {
        matches!(self.expr, Expr::Nodes)
    }
}

#[derive(PartialEq, Debug, Eq, Clone)]
pub struct Ident {
    pub name: IdentName,
    pub token: Option<Rc<Token>>, // This remains as Rc<Token> as tokens are in their own arena
}

impl Hash for Ident {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl Ord for Ident {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name.cmp(&other.name)
    }
}

impl PartialOrd for Ident {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ident {
    pub fn new(name: &str) -> Self {
        Self::new_with_token(name, None)
    }

    pub fn new_with_token(name: &str, token: Option<Rc<Token>>) -> Self {
        Self {
            name: CompactString::from(name),
            token,
        }
    }
}

impl Display for Ident {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", self.name)
    }
}

#[derive(PartialEq, PartialOrd, Debug, Eq, Clone)]
pub enum Selector {
    Blockquote,
    Footnote,
    List(Option<Index>, Option<bool>), // Index is usize, bool is Optional
    Toml,
    Yaml,
    Break,
    InlineCode,
    InlineMath,
    Delete,
    Emphasis,
    FootnoteRef,
    Html,
    Image,
    ImageRef,
    MdxJsxTextElement,
    Link,
    LinkRef,
    Strong,
    Code(Option<Lang>), // Lang is CompactString
    Math,
    Heading(Option<Depth>), // Depth is u8
    Table(Option<usize>, Option<usize>),
    Text,
    HorizontalRule,
    Definition,
    MdxFlowExpression,
    MdxTextExpression,
    MdxJsEsm,
    MdxJsxFlowElement,
}

#[derive(Debug, Clone, PartialOrd, PartialEq, Eq)]
pub enum StringSegment {
    Text(String),
    Ident(Ident), // Ident itself doesn't need lifetime as its members are owned or Rc
    Self_,
}

impl From<&lexer::token::StringSegment> for StringSegment {
    fn from(segment: &lexer::token::StringSegment) -> Self {
        match segment {
            lexer::token::StringSegment::Text(text, _) => StringSegment::Text(text.to_owned()),
            lexer::token::StringSegment::Ident(ident, _) if ident == "self" => StringSegment::Self_,
            lexer::token::StringSegment::Ident(ident, _) => StringSegment::Ident(Ident::new(ident)),
        }
    }
}

#[derive(PartialEq, PartialOrd, Debug, Clone)]
pub enum Literal {
    String(String),
    Number(Number), // Number is a struct with owned data
    Bool(bool),
    None,
}

// Expr enum updated to use NodeId
#[derive(PartialEq, PartialOrd, Debug, Clone)]
pub enum Expr<'ast> {
    Call(Ident, Args<'ast>, Optional),
    Def(Ident, Params<'ast>, Program<'ast>),
    Fn(Params<'ast>, Program<'ast>),
    Let(Ident, NodeId),
    Literal(Literal),
    Ident(Ident),
    InterpolatedString(Vec<StringSegment>), // StringSegment doesn't need lifetime
    Selector(Selector),                    // Selector is an enum with no NodeId
    While(NodeId, Program<'ast>),
    Until(NodeId, Program<'ast>),
    Foreach(Ident, NodeId, Program<'ast>),
    If(Branches<'ast>),
    Include(Literal), // Literal doesn't need lifetime
    Self_,
    Nodes,
}

#[cfg(test)]
// #[ignore] // Removing ignore to enable tests
mod tests {
    use std::rc::Rc;
    use rstest::rstest;
    use smallvec::{SmallVec, smallvec};

    use crate::{Position, Token, TokenKind, arena::ArenaId, range::Range, ast::node::NodeId}; // Corrected path for Token

    use super::*; // This will import the new NodeId, NodeData, AstArena etc.

    // Helper to create a dummy TokenId for tests.
    // In real scenarios, TokenId comes from a TokenArena.
    fn create_dummy_token_id(token_arena: &mut TokenArena<Rc<Token>>, range: Range) -> TokenId {
        let token = Rc::new(Token {
            range,
            kind: TokenKind::Eof, // Or any kind, doesn't matter much for these tests
            module_id: ArenaId::new(0), // Dummy module_id
        });
        token_arena.alloc(token)
    }
    
    // Helper to allocate a NodeData into the AstArena for tests
    // The lifetime 'node_arena is for the AstArena itself, and 'ast_lifetime is the lifetime
    // parameter for types like Expr<'ast_lifetime> stored in NodeData.
    // When we allocate, the allocated NodeData will have its lifetime 'ast_lifetime tied to 'node_arena.
    fn alloc_node<'node_arena, 'ast_lifetime>(
        ast_arena: &'node_arena TypedArena<NodeData<'ast_lifetime>>,
        token_id: TokenId,
        expr: Expr<'ast_lifetime>,
    ) -> NodeId
    where
        'node_arena: 'ast_lifetime, // Ensures arena outlives the data it contains
    {
        let node_data_ref = ast_arena.alloc(NodeData { token_id, expr });
        NodeId(node_data_ref as *const _ as usize)
    }
    
    // Helper to retrieve NodeData using the usize ID. This is inherently unsafe.
    // The lifetime 'a should correspond to the lifetime of the AstArena from which the node is retrieved.
    unsafe fn get_node_data<'a>(node_id: NodeId) -> &'a NodeData<'a> {
        &*(node_id.0 as *const NodeData<'a>)
    }


    #[test]
    fn test_node_range_literal() {
        let ast_arena = TypedArena::new(); // AstArena<'ast> where 'ast is a new anonymous lifetime for this arena
        let mut token_arena = TokenArena::new(10);
        let range = Range {
            start: Position::new(1, 1),
            end: Position::new(2, 2),
        };
        let token_id = create_dummy_token_id(&mut token_arena, range.clone());

        let node_id = alloc_node(
            &ast_arena, // Pass as &'arena TypedArena<NodeData<'arena_tied_lifetime>>
            token_id,
            Expr::Literal(Literal::String("test".to_string())),
        );
        
        let node_data = unsafe { get_node_data(node_id) }; // Use unsafe getter

        assert_eq!(node_data.range(node_id, &ast_arena, &token_arena), range);
    }

    #[test]
    fn test_node_range_def_with_program() {
        let ast_arena = TypedArena::new();
        let mut token_arena = TokenArena::new(10);

        let stmt1_range = Range { start: Position::new(1, 1), end: Position::new(1, 10) };
        let stmt1_token_id = create_dummy_token_id(&mut token_arena, stmt1_range.clone());
        let stmt1_node_id = alloc_node(
            &ast_arena, 
            stmt1_token_id, 
            Expr::Literal(Literal::String("statement1".to_string()))
        );

        let stmt2_range = Range { start: Position::new(2, 1), end: Position::new(2, 15) };
        let stmt2_token_id = create_dummy_token_id(&mut token_arena, stmt2_range.clone());
        let stmt2_node_id = alloc_node(
            &ast_arena, 
            stmt2_token_id, 
            Expr::Literal(Literal::String("statement2".to_string()))
        );
        
        let def_token_id = create_dummy_token_id(&mut token_arena, Range::default());
        let def_node_id = alloc_node(
            &ast_arena, 
            def_token_id, 
            Expr::Def(
                Ident::new("test_func"),
                SmallVec::new(), // Params are NodeId based, but empty here
                vec![stmt1_node_id, stmt2_node_id], // Program is Vec<NodeId>
        ));

        let node_data = unsafe { get_node_data(def_node_id) };
        assert_eq!(
            node_data.range(def_node_id, &ast_arena, &token_arena),
            Range { start: Position::new(1, 1), end: Position::new(2, 15) }
        );
    }
    
    #[test]
    fn test_node_range_while_loop() {
        let ast_arena = TypedArena::new();
        let mut token_arena = TokenArena::new(10);

        let cond_expr_token_id = create_dummy_token_id(&mut token_arena, Range::default());
        let cond_node_id = alloc_node(
            &ast_arena,
            cond_expr_token_id,
            Expr::Literal(Literal::Bool(true)),
        );

        let stmt1_range = Range { start: Position::new(3, 2), end: Position::new(3, 8) };
        let stmt1_token_id = create_dummy_token_id(&mut token_arena, stmt1_range.clone());
        let stmt1_node_id = alloc_node(&ast_arena, stmt1_token_id, Expr::Literal(Literal::String("loop1".to_string())));

        let stmt2_range = Range { start: Position::new(4, 2), end: Position::new(4, 12) };
        let stmt2_token_id = create_dummy_token_id(&mut token_arena, stmt2_range.clone());
        let stmt2_node_id = alloc_node(&ast_arena, stmt2_token_id, Expr::Literal(Literal::String("loop2".to_string())));

        let while_token_id = create_dummy_token_id(&mut token_arena, Range::default());
        let while_node_id = alloc_node(
            &ast_arena,
            while_token_id,
            Expr::While(cond_node_id, vec![stmt1_node_id, stmt2_node_id]),
        );
        
        let node_data = unsafe { get_node_data(while_node_id) };
        assert_eq!(
            node_data.range(while_node_id, &ast_arena, &token_arena),
            Range { start: Position::new(3, 2), end: Position::new(4, 12) }
        );
    }

    #[test]
    fn test_node_range_until_loop() {
        let ast_arena = TypedArena::new();
        let mut token_arena = TokenArena::new(10);
        
        let cond_expr_token_id = create_dummy_token_id(&mut token_arena, Range::default());
        let cond_node_id = alloc_node(
            &ast_arena,
            cond_expr_token_id,
            Expr::Literal(Literal::Bool(false)),
        );

        let stmt1_range = Range { start: Position::new(5, 4), end: Position::new(5, 9) };
        let stmt1_token_id = create_dummy_token_id(&mut token_arena, stmt1_range.clone());
        let stmt1_node_id = alloc_node(&ast_arena, stmt1_token_id, Expr::Literal(Literal::String("until1".to_string())));

        let stmt2_range = Range { start: Position::new(6, 4), end: Position::new(6, 15) };
        let stmt2_token_id = create_dummy_token_id(&mut token_arena, stmt2_range.clone());
        let stmt2_node_id = alloc_node(&ast_arena, stmt2_token_id, Expr::Literal(Literal::String("until2".to_string())));

        let until_token_id = create_dummy_token_id(&mut token_arena, Range::default());
        let until_node_id = alloc_node(
            &ast_arena,
            until_token_id,
            Expr::Until(cond_node_id, vec![stmt1_node_id, stmt2_node_id]),
        );

        let node_data = unsafe { get_node_data(until_node_id) };
        assert_eq!(
            node_data.range(until_node_id, &ast_arena, &token_arena),
            Range { start: Position::new(5, 4), end: Position::new(6, 15) }
        );
    }

    #[test]
    fn test_node_range_foreach_loop() {
        let ast_arena = TypedArena::new();
        let mut token_arena = TokenArena::new(10);

        let iterable_token_id = create_dummy_token_id(&mut token_arena, Range::default());
        let iterable_node_id = alloc_node(
            &ast_arena,
            iterable_token_id,
            Expr::Literal(Literal::String("items".to_string())),
        );
        
        let stmt1_range = Range { start: Position::new(10, 2), end: Position::new(10, 20) };
        let stmt1_token_id = create_dummy_token_id(&mut token_arena, stmt1_range.clone());
        let stmt1_node_id = alloc_node(&ast_arena, stmt1_token_id, Expr::Literal(Literal::String("foreach1".to_string())));

        let stmt2_range = Range { start: Position::new(11, 2), end: Position::new(11, 20) };
        let stmt2_token_id = create_dummy_token_id(&mut token_arena, stmt2_range.clone());
        let stmt2_node_id = alloc_node(&ast_arena, stmt2_token_id, Expr::Literal(Literal::String("foreach2".to_string())));
        
        let foreach_token_id = create_dummy_token_id(&mut token_arena, Range::default());
        let foreach_node_id = alloc_node(
            &ast_arena,
            foreach_token_id,
            Expr::Foreach(
                Ident::new("item"),
                iterable_node_id,
                vec![stmt1_node_id, stmt2_node_id],
            ),
        );
        
        let node_data = unsafe { get_node_data(foreach_node_id) };
        assert_eq!(
            node_data.range(foreach_node_id, &ast_arena, &token_arena),
            Range { start: Position::new(10, 2), end: Position::new(11, 20) }
        );
    }

    #[test]
    fn test_node_range_call_with_args() {
        let ast_arena = TypedArena::new();
        let mut token_arena = TokenArena::new(10);

        let arg1_range = Range { start: Position::new(2, 2), end: Position::new(2, 2) };
        let arg1_token_id = create_dummy_token_id(&mut token_arena, arg1_range.clone());
        let arg1_node_id = alloc_node(&ast_arena, arg1_token_id, Expr::Literal(Literal::String("arg1".to_string())));

        let arg2_range = Range { start: Position::new(3, 3), end: Position::new(3, 3) };
        let arg2_token_id = create_dummy_token_id(&mut token_arena, arg2_range.clone());
        let arg2_node_id = alloc_node(&ast_arena, arg2_token_id, Expr::Literal(Literal::String("arg2".to_string())));
        
        let call_node_token_id = create_dummy_token_id(&mut token_arena, Range { start: Position::new(1,1), end: Position::new(1,1) });
        let call_node_id = alloc_node(
            &ast_arena,
            call_node_token_id,
            Expr::Call(
                Ident::new("test_func"),
                smallvec![arg1_node_id, arg2_node_id],
                false,
            ),
        );

        let node_data = unsafe { get_node_data(call_node_id) };
        assert_eq!(
            node_data.range(call_node_id, &ast_arena, &token_arena),
            Range { start: Position::new(2, 2), end: Position::new(3, 3) }
        );
    }

    #[test]
    fn test_node_range_if_expression() {
        let ast_arena = TypedArena::new();
        let mut token_arena = TokenArena::new(10);

        let cond_range = Range { start: Position::new(1, 1), end: Position::new(1, 1) };
        let cond_token_id = create_dummy_token_id(&mut token_arena, cond_range.clone());
        let cond_node_id = alloc_node(&ast_arena, cond_token_id, Expr::Literal(Literal::Bool(true)));
        
        let then_range = Range { start: Position::new(2, 2), end: Position::new(2, 2) };
        let then_token_id = create_dummy_token_id(&mut token_arena, then_range.clone());
        let then_node_id = alloc_node(&ast_arena, then_token_id, Expr::Literal(Literal::String("then".to_string())));

        let else_range = Range { start: Position::new(3, 3), end: Position::new(3, 3) };
        let else_token_id = create_dummy_token_id(&mut token_arena, else_range.clone());
        let else_node_id = alloc_node(&ast_arena, else_token_id, Expr::Literal(Literal::String("else".to_string())));

        let if_node_token_id = create_dummy_token_id(&mut token_arena, Range::default());
        let if_node_id = alloc_node(
            &ast_arena,
            if_node_token_id,
            Expr::If(smallvec![
                (Some(cond_node_id), then_node_id),
                (None, else_node_id),
            ]),
        );
        
        let node_data = unsafe { get_node_data(if_node_id) };
        assert_eq!(
            node_data.range(if_node_id, &ast_arena, &token_arena),
            Range { start: Position::new(2, 2), end: Position::new(3, 3) }
        );
    }

    #[rstest]
    #[case("abc", "def", std::cmp::Ordering::Less)]
    #[case("def", "abc", std::cmp::Ordering::Greater)]
    #[case("abc", "abc", std::cmp::Ordering::Equal)]
    #[case("0", "abc", std::cmp::Ordering::Less)]
    #[case("xyz", "abc", std::cmp::Ordering::Greater)]
    fn test_ident_ordering(
        #[case] name1: &str,
        #[case] name2: &str,
        #[case] expected: std::cmp::Ordering,
    ) {
        // This test should still pass as Ident structure is mostly unchanged
        // except for how it might be referenced.
        let ident1 = Ident::new(name1);
        let ident2 = Ident::new(name2);
        assert_eq!(ident1.partial_cmp(&ident2), Some(expected));
    }
}
