use std::{
    fmt::{self, Display, Formatter},
    hash::{Hash, Hasher},
    rc::Rc,
};

use compact_str::CompactString;

use crate::{
    Token,
    arena::{Arena, ArenaId},
    number::Number,
    range::Range,
};

use super::{IdentName, Params, Program};

type Depth = u8;
type Index = usize;
type Optional = bool;
type Lang = CompactString;
pub type Args = Vec<Rc<Node>>;
pub type Cond = (Option<Rc<Node>>, Rc<Node>);
pub type TokenId = ArenaId<Rc<Token>>;

#[derive(PartialEq, PartialOrd, Debug, Clone)]
pub struct Node {
    pub token_id: TokenId,
    pub expr: Rc<Expr>,
}

impl Node {
    pub fn range(&self, arena: Rc<Arena<Rc<Token>>>) -> Range {
        match &*self.expr {
            Expr::Def(_, _, args)
            | Expr::While(_, args)
            | Expr::Until(_, args)
            | Expr::Foreach(_, _, args) => {
                let start = args
                    .first()
                    .map(|node| node.range(Rc::clone(&arena)).start)
                    .unwrap_or_default();
                let end = args
                    .last()
                    .map(|node| node.range(Rc::clone(&arena)).end)
                    .unwrap_or_default();
                Range { start, end }
            }
            Expr::Call(_, args, _) => {
                let start = args
                    .first()
                    .map(|node| node.range(Rc::clone(&arena)).start)
                    .unwrap_or_default();
                let end = args
                    .last()
                    .map(|node| node.range(Rc::clone(&arena)).end)
                    .unwrap_or_default();
                Range { start, end }
            }
            Expr::Let(_, node) => node.range(Rc::clone(&arena)),
            Expr::If(nodes) => {
                let start = nodes.first().unwrap().1.range(Rc::clone(&arena));
                let end = nodes.last().unwrap().1.range(Rc::clone(&arena));
                Range {
                    start: start.start,
                    end: end.end,
                }
            }
            Expr::Literal(_)
            | Expr::Ident(_)
            | Expr::Selector(_)
            | Expr::Include(_)
            | Expr::Self_ => arena[self.token_id].range.clone(),
        }
    }
}

#[derive(PartialEq, Debug, Eq, Clone)]
pub struct Ident {
    pub name: IdentName,
    pub token: Option<Rc<Token>>,
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
    List(Option<Index>, Option<bool>),
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
    Code(Option<Lang>),
    Math,
    Heading(Option<Depth>),
    Table(Option<usize>, Option<usize>),
    HorizontalRule,
    Definition,
    MdxFlowExpression,
    MdxTextExpression,
    MdxJsEsm,
    MdxJsxFlowElement,
}

#[derive(PartialEq, PartialOrd, Debug, Clone)]
pub enum Literal {
    String(String),
    Number(Number),
    Bool(bool),
    None,
}

#[derive(PartialEq, PartialOrd, Debug, Clone)]
pub enum Expr {
    Call(Ident, Args, Optional),
    Def(Ident, Params, Program),
    Let(Ident, Rc<Node>),
    Literal(Literal),
    Ident(Ident),
    Selector(Selector),
    While(Rc<Node>, Program),
    Until(Rc<Node>, Program),
    Foreach(Ident, Rc<Node>, Program),
    If(Vec<Cond>),
    Include(Literal),
    Self_,
}
