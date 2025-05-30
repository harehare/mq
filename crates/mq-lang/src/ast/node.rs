use std::{
    fmt::{self, Display, Formatter},
    hash::{Hash, Hasher},
    rc::Rc,
};

use compact_str::CompactString;
use smallvec::SmallVec;

use crate::{Token, arena::Arena, lexer, number::Number, range::Range};
use crate::ast::expr_ref::ExprRef; // Added

use super::{IdentName, Program, TokenId};

type Depth = u8;
type Index = usize;
type Optional = bool;
type Lang = CompactString;
pub type Params = SmallVec<[ExprRef; 4]>; // Changed from Rc<Node>
pub type Args = SmallVec<[ExprRef; 4]>; // Changed from Rc<Node>
pub type Cond = (Option<ExprRef>, ExprRef); // Changed from Rc<Node>
pub type Branches = SmallVec<[Cond; 4]>; // Uses updated Cond

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
    Ident(Ident),
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
    Number(Number),
    Bool(bool),
    None,
}

#[derive(PartialEq, PartialOrd, Debug, Clone)]
pub enum Expr {
    Call(Ident, Args, Optional), // Args is now SmallVec<[ExprRef; 4]>
    Def(Ident, Params, Vec<ExprRef>), // Params is SmallVec<[ExprRef; 4]>, Program is Vec<ExprRef>
    Fn(Params, Vec<ExprRef>),      // Params is SmallVec<[ExprRef; 4]>, Program is Vec<ExprRef>
    Let(Ident, ExprRef),           // Changed from Rc<Node>
    Literal(Literal),
    Ident(Ident),
    InterpolatedString(Vec<StringSegment>),
    Selector(Selector),
    While(ExprRef, Vec<ExprRef>),   // Changed from Rc<Node> and Program
    Until(ExprRef, Vec<ExprRef>),   // Changed from Rc<Node> and Program
    Foreach(Ident, ExprRef, Vec<ExprRef>), // Changed from Rc<Node> and Program
    If(Branches),                  // Branches uses ExprRef
    Include(Literal),
    Self_,
    Nodes,
}
