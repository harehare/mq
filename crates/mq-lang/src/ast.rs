use std::rc::Rc;

use compact_str::CompactString;
use node::Node;

use crate::{Token, arena::ArenaId};

pub mod error;
pub mod node;
pub mod parser;

pub type Program = Vec<Rc<Node>>;
pub type IdentName = CompactString;
pub type TokenId = ArenaId<Rc<Token>>;
